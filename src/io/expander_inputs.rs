use crate::components::status::{self, Status};
use crate::io::events::{self, InputChannel, IoIdx};
use crate::io::pcf8575::Pcf8575;
use core::{
    cell::RefCell,
    sync::atomic::{AtomicU16, Ordering},
};
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;

/// Read inputs (switches) and generate events.
pub struct ExpanderInputs<BUS: I2c> {
    /// Indices of connected PINs
    io_indices: [IoIdx; 16],

    /// shared i2c bus
    expander: RefCell<Pcf8575<BUS>>,

    // We output events into this queue.
    queue: &'static InputChannel,

    /// Internal error counter that will cause panic if unreachable for too long.
    errors: AtomicU16,

    /// Notifing about problems with expander,
    status: &'static Status,

    /// Is this expander required? Or it might be absent?
    required: bool,
}

impl<BUS: I2c> ExpanderInputs<BUS> {
    pub fn new(
        expander: Pcf8575<BUS>,
        io_indices: [IoIdx; 16],
        queue: &'static InputChannel,
        status: &'static Status,
        required: bool,
    ) -> Self {
        Self {
            io_indices,
            expander: RefCell::new(expander),
            queue,
            errors: AtomicU16::new(0),
            status,
            required,
        }
    }

    async fn transmit(&self, event: events::SwitchEvent) {
        // TODO: Update embassy-sync and use is_full()
        if self.queue.try_send(event.clone()).is_ok() {
            return;
        }
        self.status.is_warning();
        status::COUNTERS.input_queue_full.inc();
        defmt::error!("Input event queue is full! Might block");
        self.queue.send(event).await;
    }

    /// Active scanner loop that observes the expander and generates events when input changes.
    pub async fn run(&self) -> ! {
        /*
         * Let's start with a generic NO switches. So we set outputs to HIGH and
         * watch for LOW state which is active.
         */
        let mut initialized = false;
        let mut expander = self.expander.borrow_mut();

        defmt::info!("Starting expander scanning loop");

        const LOOP_WAIT_MS: u32 = 30;
        const MIN_TIME: u16 = 2;
        const ACTIVE_LEVEL: bool = false;

        /* Amount of time the switch is active */
        let mut state = [0u16; 16];

        loop {
            if !initialized {
                // Initialize pins to outputs.
                if expander.write(0xffff).await.is_ok() {
                    initialized = true;
                } else {
                    if self.required {
                        status::COUNTERS.expander_input_error.inc();
                        self.status.is_warning();
                        let errs = self.errors.fetch_add(1, Ordering::Relaxed);
                        defmt::error!("Unable to configure expander. Errors={}", errs);
                        if errs > 60 {
                            defmt::panic!("Expander connection seems dead after {} errors", errs);
                        }
                    }
                    Timer::after(Duration::from_millis(1000)).await;
                    continue;
                }
            }

            Timer::after(Duration::from_millis(LOOP_WAIT_MS.into())).await;

            let bytes = if let Ok(bytes) = expander.read().await {
                if self.errors.load(Ordering::Relaxed) > 0 {
                    self.errors.fetch_sub(1, Ordering::Relaxed);
                }
                bytes
            } else {
                // Reading failed. If intermittent, we can accept it.
                let errs = self.errors.load(Ordering::Relaxed) + 1;
                self.errors.store(errs, Ordering::Relaxed);
                if self.required {
                    status::COUNTERS.expander_input_error.inc();
                    self.status.is_warning();
                    defmt::error!("Unable to read expander. Errors={}", errs);
                    if errs > 60 {
                        defmt::panic!("Expander connection seems dead after {} errors", errs);
                    }
                }
                continue;
            };

            for (idx, entry) in state.iter_mut().enumerate() {
                let value = (bytes & (1 << idx)) != 0;

                if value == ACTIVE_LEVEL {
                    /* Switch is pressed (or maybe noise/contact bouncing) */
                    *entry = entry.saturating_add(1);

                    match (*entry).cmp(&MIN_TIME) {
                        core::cmp::Ordering::Equal => {
                            /* Just activated */
                            defmt::info!("ACTIVATED {}", idx);
                            self.transmit(events::SwitchEvent {
                                switch_id: self.io_indices[idx],
                                state: events::SwitchState::Activated,
                            })
                            .await;
                        }
                        core::cmp::Ordering::Greater => {
                            /* Was activated and still is active */
                            let time_active = LOOP_WAIT_MS * (*entry as u32);
                            self.transmit(events::SwitchEvent {
                                switch_id: self.io_indices[idx],
                                state: events::SwitchState::Active(time_active),
                            })
                            .await;
                        }
                        _ => {
                            /* Not yet active */
                            defmt::info!("active level state idx={} state={}", idx, entry);
                        }
                    }
                } else {
                    if *entry >= MIN_TIME {
                        /* Was active, now it just got deactivated */
                        let time_active = LOOP_WAIT_MS * (*entry as u32);
                        defmt::info!("DEACTIVATED {} after {}ms", idx, time_active);
                        self.transmit(events::SwitchEvent {
                            switch_id: self.io_indices[idx],
                            state: events::SwitchState::Deactivated(time_active),
                        })
                        .await;
                    }
                    *entry = 0;
                    continue;
                }
            }
        }
    }
}
