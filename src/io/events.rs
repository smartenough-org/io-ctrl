use defmt::Format;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

pub type IoIdx = u8;

/// Input switch state
#[derive(Format)]
pub enum SwitchState {
    /// Just pressed
    Activated,
    // Still active
    Active(u32),
    /// Released with a time it was pressed (in quantified ms)
    Deactivated(u32),
}

/// Event transmitted over a channel
#[derive(Format)]
pub struct SwitchEvent {
    pub switch_id: IoIdx,
    pub state: SwitchState,
}

/// Channel to transport IO events
pub type InputEventChannel = Channel<ThreadModeRawMutex, SwitchEvent, 8>;


/// Any expanders that group multiple IOs together in batches of 16.
pub(crate) trait GroupedOutputs {
    async fn set_high(&mut self, idx: u8) -> Result<(), ()>;
    async fn set_low(&mut self, idx: u8) -> Result<(), ()>;
}
