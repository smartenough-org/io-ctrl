use defmt::{Format, unwrap, info, error};

#[derive(Format)]
pub enum Action {
    /// Enable output (do nothing if already enabled)
    On,
    /// Disable output (do nothing if already disabled)
    Off,
    /// Toggle output
    Toggle,

    /// On for a particular time in s. Will disable after a moment.
    OnBrief(u16),

    /// Successive presses rotate actions (eg. scenes)
    Rotate, // ???

    /*
    /// Reset MCU
    Reset,
    OutputTest,
     */
}


/// Local instruction to the ActuatorController
#[derive(Format)]
pub struct Command {
    pub actuator_idx: usize,
    pub action: Action,
}


impl Command {
    pub fn new(actuator_idx: usize, action: Action) -> Self {
        Self {
            actuator_idx,
            action
        }
    }
}
