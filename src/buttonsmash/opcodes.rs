use defmt::Format;

use super::consts::{InIdx, LayerIdx, OutIdx, ProcIdx, ShutterIdx};
use super::shutters;

/// Opcodes of the internal micro vm.
/// Keep opcode argument length < 6B so it can be send completely
/// over a standard CAN message.
#[derive(Eq, PartialEq, Copy, Clone, Debug, Format)]
pub enum Opcode {
    /// No operation
    Noop,
    /// Start a procedure with ID.
    Start(u8),
    /// Return from procedure or end a program.
    Stop,
    /// Call a procedure
    Call(u8),

    /*
    /// Call first procedure if register is true, otherwise call second one.
    /// Can be used to implement grouping of lights that works independently
    /// from the current IO state. Eg. shortclick on a button causes a group
    /// of lights to toggle, even if some of them changed state in the meantime.
    /// FIXME: This can be implemented with CallRegister
    CallToggle(u8, ProcIdx, ProcIdx),
    */
    /// Call a procedure which ID is stored in given register. This allows a
    /// single button to iterate between multiple actions. Each action stores
    /// ID of the next procedure to be called.
    CallRegister(u8),
    SetRegister(u8, u8),

    /// Direct output control: Toggle IO
    Toggle(OutIdx),
    /// Direct output control: Activate IO (no matter state)
    Activate(OutIdx),
    /// Direct output control: Deactivate IO (no matter state)
    Deactivate(OutIdx),

    /// Generate a series of status events.
    SendStatus,

    /// Enable a layer (later: push layer onto a layer stack)
    LayerPush(LayerIdx),
    LayerPop,
    /// Set layer and clear any previously set layer stack.
    LayerSet(LayerIdx),
    /// Clear the layer stack - back to default layer.
    LayerDefault,

    /// Clear all bindings.
    BindClearAll,
    /// Map Input short click to a procedure (on current layer)
    BindShortCall(InIdx, ProcIdx),
    /// Map Input long click to a procedure (on current layer)
    BindLongCall(InIdx, ProcIdx),
    /// Map immediate activate of input to a procedure (on a current layer)
    BindActivateCall(InIdx, ProcIdx),
    /// Map immediate deactivation to a procedure (on a current layer)
    BindDeactivateCall(InIdx, ProcIdx),
    /// Map activate that takes longer than a short click to a procedure (on a current layer)
    BindLongActivate(InIdx, ProcIdx),
    /// Map deactivation after over short click time to a procedure (on a current layer)
    BindLongDeactivate(InIdx, ProcIdx),

    /*
     * Shortcuts
     */
    /// Bind short click to a toggle of an output
    BindShortToggle(InIdx, OutIdx),

    /// Bind long click to a toggle of an output
    BindLongToggle(InIdx, OutIdx),

    /// Bind layer to activate/deactivate triggers.
    BindLayerHold(InIdx, LayerIdx),
    /*
     * Native Shutter support. UP/DOWN control
     */
    /// Configure new shutter.
    /// - ID
    /// - DOWN/UP outputs
    /// - rise/fall total times, (maybe as x*10ms? u16 would allow for a 10min. time)
    /// - DOWN/UP min switch time,
    /// - from close to open on UP time,
    BindShutter(ShutterIdx, OutIdx, OutIdx), // Shutter ID, outputs: DOWN, UP.

    /// A command to a given shutter.
    ShutterCmd(ShutterIdx, shutters::Cmd),
    // Hypothetical?
    /*
    /// Read input value (local) into register
    ReadInput(InIdx),
    /// Read input value (local) into register
    ReadOutput(OutIdx),
    /// Call first if register is True, second one if False.
    CallConditionally(ProcIdx, ProcIdx),

    // WaitForRelease - maybe?
    // Procedure 0 is executed after loading and it can map the actions initially

    */
}
