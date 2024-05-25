use core::cell::RefCell;
use defmt::Format;
use embassy_time::{Duration, Timer};
use embedded_hal::digital::{
    InputPin,
    OutputPin
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

use crate::components::io::IOIndex;

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
pub struct Debouncer<const N: usize, P>
where
    P: OutputPin + InputPin + Sized
{
    // inputs: ()
    inputs: RefCell<IOIndex<N, P>>,

    // Internal comm channel
    channel: InputEventChannel,
}


impl<const N: usize, P> Debouncer<N, P>
where
    P: InputPin + OutputPin + Sized
{
    pub fn new(inputs: IOIndex<N, P>) -> Self {
        Self {
            inputs: RefCell::new(inputs),
            channel: InputEventChannel::new(),
        }
    }

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

    pub async fn read_events(&self) -> SwitchEvent {
        self.channel.receive().await
    }

    pub async fn run(&self) -> ! {
        /*
         * Let's start with a generic NO switches. So we set outputs to HIGH and
         * watch for LOW state which is active.
         */
        let mut inputs = self.inputs.borrow_mut();

        defmt::info!("Starting debouncer");
        for idx in 0..N {
            inputs.set_high(idx);
        }

        const LOOP_WAIT_MS: u32 = 30;
        const MIN_TIME: u16 = 2;
        const ACTIVE_LEVEL: bool = false;

        /* Amount of time the switch is active */
        let mut state: [u16; N] = [0u16; N];

        loop {
            Timer::after(Duration::from_millis(LOOP_WAIT_MS.into())).await;

            for idx in 0..N {
                /* FIXME: This is blocking operation that for expander is multiplied. Could be cached */
                /* This can be optimized with checking the INT too */
                let value = inputs.get(idx);

                if value == ACTIVE_LEVEL {
                    /* Switch is pressed (or maybe noise/contact bouncing) */
                    if state[idx] != u16::max_value() {
                        state[idx] += 1;
                    }

                    if state[idx] == MIN_TIME {
                        /* Just activated */
                        defmt::info!("ACTIVATED {}", idx);
                        self.channel.send(SwitchEvent {
                            switch: idx as u8,
                            state: SwitchState::Activated,
                        }).await;
                    } else if state[idx] > MIN_TIME {
                        /* Was activated and still is active */
                        let time_active = LOOP_WAIT_MS * (state[idx] as u32);
                        self.channel.send(SwitchEvent {
                            switch: idx as u8,
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
                            switch: idx as u8,
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
