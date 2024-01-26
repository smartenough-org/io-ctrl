// use embassy_stm32;

use core::cell::UnsafeCell;
use defmt::unwrap;
use crate::boards::shared::Shared;

use embassy_stm32::gpio::{Input, Level, Output, AnyPin, Pin, Pull, Speed};

// EXAMPLE
//use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
//use embassy_sync::blocking_mutex::{NoopMutex, raw::NoopRawMutex};

// static ACTUATOR_CTRL: StaticCell<ActuatorCtrl<OutputOpenDrain<'static, AnyPin>, 4>> = StaticCell::new();
use embassy_stm32::dma::NoDma;
use embassy_stm32::i2c::{Error, I2c};
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, i2c, peripherals};
use port_expander::Pcf8575;

bind_interrupts!(struct I2CIrqs {
    I2C3_EV => i2c::EventInterruptHandler<peripherals::I2C3>;
    I2C3_ER => i2c::ErrorInterruptHandler<peripherals::I2C3>;
});

pub struct Hardware {
    // ? UnsafeCell?
    led: UnsafeCell<Output<'static, AnyPin>>,
}

impl Hardware {
    pub fn new(
        p: embassy_stm32::Peripherals,
        _shared_resource: &'static Shared,
    ) -> Self {

        /*
        let mut i2c = I2c::new(
            p.I2C3,
            p.PA8,
            p.PB5,
            I2CIrqs,
            NoDma,
            NoDma,
            Hertz(100_000),
            Default::default(),
        );
        let mut pcf = Pcf8575::new(i2c, true, false, false);
        let mut pcf_pins = pcf.split();
        pcf_pins.p00.set_high().unwrap();
        */

        // TODO!
        Self {
            led: UnsafeCell::new(Output::new(p.PC6.degrade(), Level::Low, Speed::Low)),
        }
    }

    pub fn led_on(&self) {
        /* Only this class uses it and we are single cpu */
        let led = unsafe { &mut *self.led.get() };
        led.set_high();
    }

    pub fn led_off(&self) {
        let led = unsafe { &mut *self.led.get() };
        led.set_low();
    }
}
