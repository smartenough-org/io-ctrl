use defmt::Format;

use super::shutters;
use crate::io::events::{ButtonEvent, Trigger};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
/*
 * Shared, common constants and trivial structures
 */

// Input IO index. `0` is reserved to simplify things.
pub type InIdx = u8;
pub type OutIdx = u8;
pub type ShutterIdx = u8;
pub type LayerIdx = u8;
pub type ProcIdx = u8;
pub const MAX_PROCEDURES: usize = 128;
pub const REGISTERS: usize = 32;
pub const MAX_LAYERS: usize = 128;
pub const MAX_LAYER_STACK: usize = 5;

pub const BINDINGS_COUNT: usize = 30;

/// Max call stack size.
pub const MAX_STACK: usize = 3;

// FIXME: Those required?
pub const MAX_INPUTS: usize = 128;
pub const MAX_OUTPUTS: usize = 128;

// TODO: Low/high active?
#[derive(Debug, Copy, Clone, Eq, PartialEq, Format)]
pub enum Command {
    /// Toggle output...
    ToggleOutput(OutIdx),
    /// Enable output of given ID - Local or remote.
    ActivateOutput(OutIdx),
    /// Deactivate output of given ID - Local or remote
    DeactivateOutput(OutIdx),

    /// Activate layer (public message)
    ActivateLayer(LayerIdx),
    /// Deactivate layer (public message)
    DeactivateLayer(LayerIdx),

    /// Shutter command
    Shutter(ShutterIdx, shutters::Cmd),

    /// No operation
    Noop,
}

#[derive(Format)]
pub enum LayerEvent {
    Activate(u8),
    Deactivate(u8),
}

/// Events handled as inputs to the Executor/MicroVM.
#[derive(Format)]
pub enum Event {
    /// Button event
    ButtonEvent(ButtonEvent),
    /*
    /// External information about layer change
    LayerEvent(LayerEvent),
    */
    RemoteProcedureCall(ProcIdx),
    RemoteToggle(OutIdx),
    RemoteActivate(OutIdx),
    RemoteDeactivate(OutIdx),
}

impl Event {
    pub fn new_button(in_idx: InIdx, trigger: Trigger) -> Self {
        Event::ButtonEvent(ButtonEvent {
            switch_id: in_idx,
            trigger,
        })
    }
}

/// Channel to tranport high-level events into the Executor.
pub type EventChannel = Channel<ThreadModeRawMutex, Event, 5>;
