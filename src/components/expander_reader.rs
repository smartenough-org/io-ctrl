use core::cell::RefCell;
use defmt::Format;
use embassy_time::{Duration, Timer};
use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embedded_hal_async::i2c::I2c;
use crate::components::pcf8575::Pcf8575;
use embedded_hal::digital::{
    InputPin,
    OutputPin
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

// use crate::components::io::IOIndex;

/// Input switch state
#[derive(Format)]
pub enum SwitchState {
    /// Just pressed
    Activated,
    // Still active
    Active(u32),
    /// Released with a time it was pressed (in quantified ms)
    Deactivated(u32)
}

/// Event transmitted over a channel
#[derive(Format)]
pub struct SwitchEvent {
    pub switch: u8,
    pub state: SwitchState,
}

pub type InputEventChannel = Channel<ThreadModeRawMutex, SwitchEvent, 8>;

/// Read inputs (switches) and generate events.
pub struct ExpanderReader<BUS: I2c>
{
    /// Indices of connceted PINs
    io_indices: [u8; 16],

    /// shared i2c bus
    expander: RefCell<Pcf8575<BUS>>,

    // Internal comm channel
    channel: InputEventChannel,
}

impl<BUS: I2c> ExpanderReader<BUS>
{
    pub fn new(expander: Pcf8575<BUS>, io_indices: [u8; 16]) -> Self {
        Self {
            io_indices,
            expander: RefCell::new(expander),
            channel: InputEventChannel::new(),
        }
    }

    /// Used by external readers
    pub fn try_read_events(&self) -> Option<SwitchEvent> {
        let ret = self.channel.try_receive();
        match ret {
            Ok(event) => return Some(event),
            Err(err) =>  {
                defmt::info!("Error while reading channel {:?}", err);
                return None;
            }
        }
    }

    /// Used by external readers.
    pub async fn read_events(&self) -> SwitchEvent {
        self.channel.receive().await
    }

    pub async fn run(&self) -> ! {
        /*
         * Let's start with a generic NO switches. So we set outputs to HIGH and
         * watch for LOW state which is active.
         */
        let mut expander = self.expander.borrow_mut();

        defmt::info!("Starting debouncer");

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
                /* FIXME: This is blocking operation that for expander is multiplied. Could be cached */
                /* This can be optimized with checking the INT too */
                let value = (bytes & (1<<idx)) != 0;

                if value == ACTIVE_LEVEL {
                    /* Switch is pressed (or maybe noise/contact bouncing) */
                    if state[idx] != u16::max_value() {
                        state[idx] += 1;
                    }

                    if state[idx] == MIN_TIME {
                        /* Just activated */
                        defmt::info!("ACTIVATED {}", idx);
                        self.channel.send(SwitchEvent {
                            switch: self.io_indices[idx],
                            state: SwitchState::Activated,
                        }).await;
                    } else if state[idx] > MIN_TIME {
                        /* Was activated and still is active */
                        let time_active = LOOP_WAIT_MS * (state[idx] as u32);
                        self.channel.send(SwitchEvent {
                            switch: self.io_indices[idx],
                            state: SwitchState::Active(time_active),
                        }).await;
                    } else {
                        /* Not yet active */
                        defmt::info!("active level state idx={} state={}", idx, state[idx]);
                    }
                } else {
                    if state[idx] >= MIN_TIME {
                        /* Deactivated */
                        let time_active = LOOP_WAIT_MS * (state[idx] as u32);
                        defmt::info!("DEACTIVATED {} after {}ms", idx, time_active);
                        self.channel.send(SwitchEvent {
                            switch: self.io_indices[idx],
                            state: SwitchState::Deactivated(time_active),
                        }).await;
                    }
                    state[idx] = 0;
                    continue;
                }
            }
        }
    }
}
