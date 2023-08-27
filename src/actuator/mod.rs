use defmt::{Format, unwrap, info, error};
use embedded_hal::digital::v2::OutputPin;
use embassy_time::Instant;

pub mod commands;

pub use commands::{Action, Command};

#[derive(Format)]
pub enum PinType {
    /* Bistable outputs */
    ActiveHigh,
    ActiveLow,

    /* Monostable outputs, with impulse activation for X ms */
    // ImpulseHigh(u16),
    // ImpulseLow(u16),

    /* Toggle on activation */
    // Toggle,
}

/// A single actuator (physical pin + configuration)
pub struct Actuator<T: OutputPin> {
    pin: T,
    pin_type: PinType,

    active_since: Option<Instant>,

    /// Max activation time in milliseconds.
    activation_limit: Option<u32>,
}

impl<T: OutputPin> Actuator<T> {
    pub fn new(pin: T, pin_type: PinType) -> Self {
        Self {
            pin,
            pin_type,

            active_since: None,
            activation_limit: None,
        }
    }

    fn enable(&mut self) {
        info!("Enabling pin {}", self.pin_type);
        match self.pin_type {
            PinType::ActiveHigh => {
                let _ = self.pin.set_high();
            },
            PinType::ActiveLow => {
                let _ = self.pin.set_low();
            },
        }
    }

    fn disable(&mut self) {
        match self.pin_type {
            PinType::ActiveHigh => {
                // FIXME Can't sensibly unwrap
                let _ = self.pin.set_low();
            },
            PinType::ActiveLow => {
                let _ = self.pin.set_high();
            },
        }
    }
}

/// Actuator controller; manages bunch of actuators.
pub struct ActuatorCtrl<T: OutputPin, const N: usize> {
    pub actuators: [Actuator<T>; N]
}

impl<T: OutputPin, const N: usize> ActuatorCtrl<T, N> {
    pub fn new(actuators: [Actuator<T>; N]) -> Self {
        // TODO: Certain pins might be active high or low. They need config.
        Self {
            actuators,
        }
    }

    pub fn execute(&mut self, cmd: Command) {
        info!("Executing command {}", cmd);
        let actuator = &mut self.actuators[cmd.actuator_idx];
        match cmd.action {
            Action::On => {
                actuator.active_since = Some(Instant::now());
                actuator.enable();
            },
            Action::Off => {
                actuator.active_since = None;
                actuator.disable();
            },
            _ => {
                panic!("Unhandled action");
            }
        }
    }

    pub async fn control(&mut self) {
        todo!();
    }
}
