use core::cell::RefCell;
use embassy_time::{Duration, Timer};
use embedded_hal_02::digital::v2::{
    InputPin,
    OutputPin
};

use crate::components::io::IOIndex;


pub struct Debouncer<const N: usize, P>
where
    P: OutputPin + InputPin + Sized
{
    // inputs: ()
    pub inputs: RefCell<IOIndex<N, P>>,
}


impl<const N: usize, P> Debouncer<N, P>
where
    P: InputPin + OutputPin + Sized
{
    pub fn new(inputs: IOIndex<N, P>) -> Self {
        Self {
            inputs: RefCell::new(inputs),
        }
    }

    pub async fn run(&self) -> ! {
        /*
         * Let's start with a generic NO switches. So we set outputs to HIGH and
         * watch for LOW state which is active.
         */
        let mut inputs = self.inputs.borrow_mut();

        for idx in 0..N {
            inputs.set_high(idx);
        }

        const MIN_TIME: u8 = 2;
        const ACTIVE_LEVEL: bool = false;

        let mut state: [u8; N] = [0u8; N];
        loop {
            Timer::after(Duration::from_millis(30)).await;
            for idx in 0..N {
                let value = inputs.get(idx);

                if value != ACTIVE_LEVEL {
                    if state[idx] == MIN_TIME {
                        /* TODO Deactivate input */
                        defmt::info!("DEACTIVATED {}", idx);
                    }
                    state[idx] = 0;
                    continue;
                } else {
                    if state[idx] == MIN_TIME {
                        /* Steady active */
                        defmt::info!("active {}", idx);
                        continue;
                    }
                    state[idx] += 1;
                    if state[idx] == MIN_TIME {
                        /* TODO Activated */
                        defmt::info!("ACTIVATED {}", idx);
                    } else {
                        defmt::info!("not yet active idx={} state={}", idx, state[idx]);
                    }
                }
            }
        }
    }
}
