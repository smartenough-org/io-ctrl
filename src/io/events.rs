use defmt::Format;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

pub type IoIdx = u8;

/// Debounced Input switch state
#[derive(Format)]
pub enum SwitchState {
    /// Just pressed (after debouncing period)
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

/// Higher level switch abstraction.
/// eg. Activated -> LongActivated -> LongClick -> LongDeactivated -> Deactivated.
/// Activated -> ShortClick -> Deactivated
#[derive(Copy, Clone, Eq, PartialEq, Format)]
pub enum Trigger {
    /// Short click activation; longer than debounce period, but shorter than a
    /// long click. Triggered on deactivation.
    ShortClick,
    /// Longer than a short click. Triggered on deactivation.
    LongClick,
    /// Triggered right after debouncing period is over.
    Activated,
    /// Triggered immediately on deactivation, no matter time.
    Deactivated,
    /// Activation that exceeds the shortclick time. A bit delayed.
    LongActivated,
    /// Deactivation after LongActivated was triggered
    LongDeactivated,
}

/// Event transmitted over a channel
#[derive(Format)]
pub struct ButtonEvent {
    pub switch_id: IoIdx,
    pub trigger: Trigger,
}

/// Channel to transport Raw, low-level IO events
pub type RawEventChannel = Channel<ThreadModeRawMutex, SwitchEvent, 8>;

/// Channel to tranport high-level debounced IO events.
pub type TriggerChannel = Channel<ThreadModeRawMutex, ButtonEvent, 6>;

/// Any expanders that group multiple IOs together in batches of 16.
pub(crate) trait GroupedOutputs {
    async fn set_high(&mut self, idx: u8) -> Result<(), ()>;
    async fn set_low(&mut self, idx: u8) -> Result<(), ()>;
}
