/* WIP CONCEPT */

use crate::boards::ctrl_board::Board;
use crate::components::debouncer::SwitchState;

/// App component parses inputs and turns them into various Actions
pub struct IORouter {
    board: &'static Board,
}

impl IORouter {
    pub fn new(board: &'static Board) -> Self {
        Self {
            board
        }
    }

    pub fn add_direct() {
    }

    pub async fn run(&self) -> ! {
        /* How long is too long for short press */
        const MAX_SHORT_MS: u32 = 300;

        /* All initially disabled (in low-state enabled devices) */
        let mut output_state: [bool; 16] = [true; 16];
        for n in 0..16 {
            self.board.hardware.set_output(n, output_state[n]);
        }

        loop {
            let event = self.board.hardware.debouncer.read_events().await;
            defmt::info!("Got some event {:?}", event);

            match event.state {
                SwitchState::Activated => {
                    /* Ignoring long-held events yet */
                }
                SwitchState::Active(ms) => {
                    /* Ignoring long-held events yet */
                }
                SwitchState::Deactivated(ms) => {
                    let switch = event.switch as usize;
                    if ms <= MAX_SHORT_MS {
                        /* Toggle correlated output */
                        output_state[switch] = !output_state[switch];
                        self.board.hardware.set_output(switch, output_state[switch]);
                        defmt::info!("Set output {} to {}", switch, output_state[switch]);
                    }
                }
            }
        }
    }
}


#[embassy_executor::task(pool_size = 1)]
pub async fn task(io_router: &'static IORouter) {
    io_router.run().await;
}
