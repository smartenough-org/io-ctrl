use crate::components::status::{self, Status};
use crate::io::events::{self, InputChannel, IoIdx};
use crate::io::pcf8575::Pcf8575;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::{AtomicU16, Ordering};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;

/// Read inputs (switches) and generate events.
pub struct ExpanderInputs<BUS: I2c> {
    /// Indices of connected PINs
    io_indices: [IoIdx; 16],

    /// Expander address for identification
    id: u8,

    /// shared i2c bus
    expander: Mutex<NoopRawMutex, Pcf8575<BUS>>,

    // We output events into this queue.
    queue: &'static InputChannel,

    /// Internal error counter that will cause panic if unreachable for too long.
    errors: AtomicU16,

    /// True if expander responds
    expander_online: AtomicBool,

    /// Last read value from expander.
    last_input: AtomicU16,

    /// For notifing about problems with expander,
    status: &'static Status,

    /// Is this expander required? Or it might be absent?
    required: bool,
}

impl<BUS: I2c> ExpanderInputs<BUS> {
    pub fn new(
        expander: Pcf8575<BUS>,
        id: u8,
        io_indices: [IoIdx; 16],
        queue: &'static InputChannel,
        status: &'static Status,
        required: bool,
    ) -> Self {
        Self {
            io_indices,
            expander: Mutex::new(expander),
            id,
            queue,
            errors: AtomicU16::new(0),
            expander_online: AtomicBool::new(false),
            last_input: AtomicU16::new(0),
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

    pub fn get_indices(&self) -> &[u8; 16] {
        &self.io_indices
    }

    pub fn get_id(&self) -> u8 {
        self.id
    }

    pub fn get_inputs(&self) -> Option<[(u8, bool); 16]> {
        let input = self.last_input.load(Ordering::Relaxed);
        if !self.expander_online.load(Ordering::Relaxed) {
            return None;
        }

        let mut data: [(u8, bool); 16] = [(0, false); 16];
        for (pos, index) in self.io_indices.iter().enumerate() {
            data[pos] = (*index, (input & (1 << pos)) != 0);
        }
        Some(data)
    }

    /// Active scanner loop that observes the expander and generates events when input changes.
    pub async fn run(&self) -> ! {
        /*
         * Let's start with a generic NO switches. So we set outputs to HIGH and
         * watch for LOW state which is active.
         */
        let mut initialized = false;
        let mut expander = self.expander.lock().await;

        defmt::info!("Starting expander scanning loop");

        const LOOP_WAIT_MS: u32 = 30;
        const MIN_TIME: u16 = 2;
        const ACTIVE_LEVEL: bool = false;

        /* Amount of time the switch is active */
        let mut state = [0u16; 16];

        loop {
            if !initialized {
                // Initialize as high to use them as inputs.
                if expander.write(0xffff).await.is_ok() {
                    initialized = true;
                } else {
                    if self.required {
                        status::COUNTERS.expander_input_error.inc();
                        self.status.is_warning();
                        let errs = self.errors.fetch_add(1, Ordering::Relaxed);
                        defmt::error!("Unable to configure expander {}. Errors={}", self.id, errs);
                        if errs > 60 {
                            defmt::panic!(
                                "Expander {} connection seems dead after {} errors",
                                self.id,
                                errs
                            );
                        }
                    }
                    self.expander_online.store(false, Ordering::Relaxed);
                    Timer::after(Duration::from_millis(1000)).await;
                    continue;
                }
            }

            Timer::after(Duration::from_millis(LOOP_WAIT_MS.into())).await;

            let bytes = if let Ok(bytes) = expander.read().await {
                if self.errors.load(Ordering::Relaxed) > 0 {
                    self.errors.fetch_sub(1, Ordering::Relaxed);
                }
                self.last_input.store(bytes, Ordering::Relaxed);
                self.expander_online.store(true, Ordering::Relaxed);
                bytes
            } else {
                // Reading failed. If intermittent, we can accept it.
                let errs = self.errors.load(Ordering::Relaxed) + 1;
                self.errors.store(errs, Ordering::Relaxed);

                self.last_input.store(0, Ordering::Relaxed);
                self.expander_online.store(false, Ordering::Relaxed);

                // TODO: After failure we might need to reinitialize as inputs.
                // TODO: initialized = false; Test it.

                if self.required {
                    status::COUNTERS.expander_input_error.inc();
                    self.status.is_warning();
                    defmt::error!("Unable to read expander {}. Errors={}", self.id, errs);
                    if errs > 60 {
                        defmt::panic!(
                            "Expander {} connection seems dead after {} errors",
                            self.id,
                            errs
                        );
                    }
                }
                continue;
            };

            for (pos, entry) in state.iter_mut().enumerate() {
                let value = (bytes & (1 << pos)) != 0;

                if value == ACTIVE_LEVEL {
                    /* Switch is pressed (or maybe noise/contact bouncing) */
                    *entry = entry.saturating_add(1);

                    match (*entry).cmp(&MIN_TIME) {
                        core::cmp::Ordering::Equal => {
                            /* Just activated */
                            self.transmit(events::SwitchEvent {
                                switch_id: self.io_indices[pos],
                                state: events::SwitchState::Activated,
                            })
                            .await;
                        }
                        core::cmp::Ordering::Greater => {
                            /* Was activated and still is active */
                            let time_active = LOOP_WAIT_MS * (*entry as u32);
                            self.transmit(events::SwitchEvent {
                                switch_id: self.io_indices[pos],
                                state: events::SwitchState::Active(time_active),
                            })
                            .await;
                        }
                        _ => {
                            /* Not yet active */
                            defmt::info!(
                                "new active level state id={} idx={} state={}",
                                self.id,
                                pos,
                                entry
                            );
                        }
                    }
                } else {
                    if *entry >= MIN_TIME {
                        /* Was active, now it just got deactivated */
                        let time_active = LOOP_WAIT_MS * (*entry as u32);
                        self.transmit(events::SwitchEvent {
                            switch_id: self.io_indices[pos],
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
