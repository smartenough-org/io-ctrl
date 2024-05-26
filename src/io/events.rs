use defmt::Format;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

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
    pub switch_id: u8,
    pub state: SwitchState,
}

/// Channel to transport IO events
pub type InputEventChannel = Channel<ThreadModeRawMutex, SwitchEvent, 8>;
