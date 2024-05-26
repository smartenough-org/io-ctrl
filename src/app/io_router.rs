/* WIP CONCEPT */

use crate::boards::ctrl_board::Board;
use crate::io::events::{IoIdx, SwitchState};

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
            self.board.hardware.set_output(n as IoIdx, output_state[n]);
        }

        loop {
            // TODO: Wrap multiple expanders/native IOs into single queue.
            let event = self.board.hardware.expander_switches.read_events().await;
            defmt::info!("Got some event {:?}", event);

            match event.state {
                SwitchState::Activated => {
                    /* Ignoring long-held events yet */
                }
                SwitchState::Active(ms) => {
                    /* Ignoring long-held events yet */
                }
                SwitchState::Deactivated(ms) => {
                    if ms <= MAX_SHORT_MS {
                        /* Toggle correlated output */
                        let switch_id = event.switch_id as usize;
                        output_state[switch_id] = !output_state[switch_id];
                        self.board.hardware.set_output(event.switch_id, output_state[switch_id]);
                        defmt::info!("Set output {} to {}", switch_id, output_state[switch_id]);
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
