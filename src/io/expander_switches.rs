use crate::io::{
    events::{self, IoIdx},
    event_converter::EventConverter,
};
use crate::io::pcf8575::Pcf8575;
use core::cell::RefCell;
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
}

impl<BUS: I2c> ExpanderSwitches<BUS> {
    pub fn new(expander: Pcf8575<BUS>, io_indices: [IoIdx; 16],
               event_converter: &'static EventConverter) -> Self {
        Self {
            io_indices,
            expander: RefCell::new(expander),
            event_converter,
            // channel: events::InputEventChannel::new(),
        }
    }

    /// Active scanner loop that observes the expander and generates events when input changes.
    pub async fn run(&self) -> ! {
        /*
         * Let's start with a generic NO switches. So we set outputs to HIGH and
         * watch for LOW state which is active.
         */
        let mut expander = self.expander.borrow_mut();

        defmt::info!("Starting expander scanning loop");

        // Initialize pins to outputs.
        expander.write(0xffff).await.unwrap();

        const LOOP_WAIT_MS: u32 = 30;
        const MIN_TIME: u16 = 2;
        const ACTIVE_LEVEL: bool = false;

        /* Amount of time the switch is active */
        let mut state = [0u16; 16];

        let mut errors = 0;

        loop {
            Timer::after(Duration::from_millis(LOOP_WAIT_MS.into())).await;

            let bytes = if let Ok(bytes) = expander.read().await {
                errors = 0;
                bytes
            } else {
                // Reading failed. If intermittent, we can accept it.
                errors += 1;
                defmt::error!("Unable to read expander. Errors={}", errors);
                if errors > 60 {
                    defmt::panic!("Expander connection seems dead after {} errors", errors);
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
