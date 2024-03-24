use embedded_hal_02::digital::v2::{
    InputPin,
    OutputPin
};

use embassy_stm32::gpio::low_level::Pin as _;

use embassy_stm32::gpio::{AnyPin, Input, Pull};

// TODO: Maybe in newer version the expander pin would support AnyPin or some common trait?
// Or maybe the NativePin could just use Output/Input/Sized traits? That's a different concrete type though.
pub enum UniPin<P>
where
    P: OutputPin + InputPin + Sized
{
    Native(AnyPin),
    Expander(P),
}

/** Allow to access pins by their IDs in runtime. */
pub struct IOIndex<const N: usize, P>
where
    P: OutputPin + InputPin + Sized
{
    pins: [UniPin<P>; N]
}


impl<const N: usize, P> IOIndex<N, P>
where
    P: InputPin + OutputPin + Sized
{
    pub fn new(pins: [UniPin<P>; N]) -> Self {
        Self {
            pins
        }
    }

    /// Set output state to high
    pub fn set_high(&mut self, idx: usize) {
        match &mut self.pins[idx] {
            UniPin::Native(pin) => {
                pin.set_high();
            }
            UniPin::Expander(pin) => {
                // TODO: Should use Result<>
                if let Err(_) = pin.set_high() {
                    defmt::error!("Error while setting expander IO {} high", idx);
                }
            }
        }
    }

    /// Set output state to low
    pub fn set_low(&mut self, idx: usize) {
        match &mut self.pins[idx] {
            UniPin::Native(pin) => {
                pin.set_low();
            }
            UniPin::Expander(pin) => {
                if let Err(_) = pin.set_low() {
                    defmt::error!("Error while setting expander IO {} low", idx);
                }
            }
        }
    }

    /// Set output assuming true -> high state, and false -> low state.
    pub fn set(&mut self, idx: usize, state: bool) {
        if state {
            self.set_high(idx);
        } else {
            self.set_low(idx);
        }
    }

    pub fn get(&mut self, idx: usize) -> bool {
        match &mut self.pins[idx] {
            UniPin::Native(pin) => {
                let inp = Input::new(pin, Pull::Up);
                inp.is_high()
            }
            UniPin::Expander(pin) => {
                if let Ok(value) = pin.is_high() {
                    value
                } else {
                    /* FIXME So-so. */
                    false
                }
            }
        }
    }
}
