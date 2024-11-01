use crate::io::pcf8575::Pcf8575;
use crate::io::{
    event_converter::EventConverter,
    events::{self, IoIdx},
};
use core::{
    cell::RefCell,
    sync::atomic::{AtomicU16, Ordering},
};
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;

/// Read inputs (switches) and generate events.
pub struct ExpanderSwitches<BUS: I2c> {
    /// Indices of connected PINs
    io_indices: [IoIdx; 16],

    /// shared i2c bus
    expander: RefCell<Pcf8575<BUS>>,

    // Converter reads our events and produces high-level combined events.
    event_converter: &'static EventConverter,

    errors: AtomicU16,
}

impl<BUS: I2c> ExpanderSwitches<BUS> {
    pub fn new(
        expander: Pcf8575<BUS>,
        io_indices: [IoIdx; 16],
        event_converter: &'static EventConverter,
    ) -> Self {
        Self {
            io_indices,
            expander: RefCell::new(expander),
            event_converter,
            errors: AtomicU16::new(0),
        }
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
                if let Ok(_) = expander.write(0xffff).await {
                    initialized = true;
                } else {
                    let errs = self.errors.load(Ordering::Relaxed) + 1;
                    self.errors.store(errs, Ordering::Relaxed);
                    defmt::error!("Unable to configure expander. Errors={}", errs);
                    if errs > 60 {
                        defmt::panic!("Expander connection seems dead after {} errors", errs);
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
                defmt::error!("Unable to read expander. Errors={}", errs);
                if errs > 60 {
                    defmt::panic!("Expander connection seems dead after {} errors", errs);
                }
                continue;
            };

            for idx in 0..16 {
                let value = (bytes & (1 << idx)) != 0;

                if value == ACTIVE_LEVEL {
                    /* Switch is pressed (or maybe noise/contact bouncing) */
                    if state[idx] != u16::MAX {
                        state[idx] += 1;
                    }

                    if state[idx] == MIN_TIME {
                        /* Just activated */
                        defmt::info!("ACTIVATED {}", idx);
                        self.event_converter
                            .send(events::SwitchEvent {
                                switch_id: self.io_indices[idx],
                                state: events::SwitchState::Activated,
                            })
                            .await;
                    } else if state[idx] > MIN_TIME {
                        /* Was activated and still is active */
                        let time_active = LOOP_WAIT_MS * (state[idx] as u32);
                        self.event_converter
                            .send(events::SwitchEvent {
                                switch_id: self.io_indices[idx],
                                state: events::SwitchState::Active(time_active),
                            })
                            .await;
                    } else {
                        /* Not yet active */
                        defmt::info!("active level state idx={} state={}", idx, state[idx]);
                    }
                } else {
                    if state[idx] >= MIN_TIME {
                        /* Was active, now it just got deactivated */
                        let time_active = LOOP_WAIT_MS * (state[idx] as u32);
                        defmt::info!("DEACTIVATED {} after {}ms", idx, time_active);
                        self.event_converter
                            .send(events::SwitchEvent {
                                switch_id: self.io_indices[idx],
                                state: events::SwitchState::Deactivated(time_active),
                            })
                            .await;
                    }
                    state[idx] = 0;
                    continue;
                }
            }
        }
    }
}
