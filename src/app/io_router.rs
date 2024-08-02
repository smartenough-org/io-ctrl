use core::cell::UnsafeCell;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;

use crate::buttonsmash::{Opcode, CommandQueue, Event, Command};
use crate::boards::ctrl_board::Board;
use crate::io::events::{IoIdx, ButtonEvent, Trigger};

/// App component parses inputs and turns them into various Actions
pub struct IORouter {
    board: &'static Board,
    cmd_queue: &'static CommandQueue,
}

impl IORouter {
    pub fn new(board: &'static Board, cmd_queue: &'static CommandQueue) -> Self {
        Self {
            board,
            cmd_queue
        }
    }

    pub async fn run(&self) -> ! {
        /* All initially disabled (in low-state enabled devices) */
        let mut output_state: [bool; 32] = [true; 32];
        for n in 1..=16 {
            self.board.hardware.set_output(n as IoIdx, true).await.unwrap();
        }

        loop {
            // let event = self.board.hardware.event_converter.read_events().await;
            // defmt::info!("Got some event from expander/converter {:?}", event);
            let command = self.cmd_queue.receive().await;
            defmt::info!("IORouter got command: {:?}", command);

            // TODO: Unwraps - make it soft.
            match command {
                Command::ToggleOutput(idx) => {
                    output_state[idx as usize] = !output_state[idx as usize];
                    self.board.hardware.set_output(idx, output_state[idx as usize]).await.unwrap();
                },
                Command::ActivateOutput(idx) => {
                    // Low-state activate
                    self.board.hardware.set_output(idx, false).await.unwrap();
                    output_state[idx as usize] = false;
                },
                Command::DeactivateOutput(idx) => {
                    // Low-state activate
                    self.board.hardware.set_output(idx, true).await.unwrap();
                    output_state[idx as usize] = true;
                },
                Command::ActivateLayer(_layer) => {
                    todo!("No public activate layer");
                },
                Command::DeactivateLayer(_layer) => {
                    todo!("No public deactivate layer");
                },
                Command::Noop => {
                    // No operation
                },
            }


            /*
            match event.trigger {
                Trigger::Activated => {
                    // Ignoring long-held events yet
                }
                Trigger::ShortClick => {
                    // Toggle correlated output
                    let switch_id = event.switch_id as usize;
                    output_state[switch_id] = !output_state[switch_id];
                    self.board.hardware.set_output(event.switch_id, output_state[switch_id]).await.unwrap();
                    defmt::info!("Set output {} to {}", switch_id, output_state[switch_id]);
                }
                _ => {}
            }
            */
        }
    }
}
