use crate::io::events::IoIdx;
use crate::{boards::ctrl_board::Board, components::status};
use defmt::Format;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

pub type OutIdx = u8;

#[derive(Debug, Eq, PartialEq, Format, Clone)]
pub enum IOCommand {
    /// Toggle output...
    ToggleOutput(OutIdx),
    /// Enable output of given ID - Local or remote.
    ActivateOutput(OutIdx),
    /// Deactivate output of given ID - Local or remote
    DeactivateOutput(OutIdx),
}

pub type OutputChannel = Channel<ThreadModeRawMutex, IOCommand, 3>;

/// Read events from command queue and alter our outputs.
#[embassy_executor::task(pool_size = 1)]
pub async fn task_io_router(board: &'static Board, cmd_queue: &'static OutputChannel) {
    /* All initially disabled (in low-state enabled devices) */
    let mut output_state: [bool; 32] = [true; 32];
    for n in 1..=16 {
        if board.set_output(n as IoIdx, true).await.is_err() {
            defmt::error!("Unable to initialize output IO. Expander failure?");
            status::COUNTERS.expander_output_error.inc();
            board.status.is_warning();
        }
    }

    loop {
        // let event = self.board.hardware.event_converter.read_events().await;
        // defmt::info!("Got some event from expander/converter {:?}", event);
        let command = cmd_queue.receive().await;
        defmt::info!("IORouter got command: {:?}", command);

        board.status.is_active();

        // TODO: Unwraps - make it soft.
        let result = match command {
            IOCommand::ToggleOutput(idx) => {
                output_state[idx as usize] = !output_state[idx as usize];
                board.set_output(idx, output_state[idx as usize]).await
            }
            IOCommand::ActivateOutput(idx) => {
                // Low-state activate
                output_state[idx as usize] = false;
                board.set_output(idx, false).await
            }
            IOCommand::DeactivateOutput(idx) => {
                // Low-state activate
                output_state[idx as usize] = true;
                board.set_output(idx, true).await
            }
        };
        if result.is_err() {
            defmt::error!(
                "Unable to configure output (expander failure?): {}",
                command
            );
            status::COUNTERS.expander_output_error.inc();
            board.status.is_warning();
        }
    }
}
