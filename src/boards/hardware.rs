use crate::boards::shared::Shared;
use core::cell::UnsafeCell;
use defmt::info;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;

use crate::components::{
    // io,
    interconnect,
    // debouncer,
};
use embassy_stm32::gpio::{AnyPin, Level, Output, Pin, Pull, Speed};

use crate::io::{expander_reader, pcf8575};

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::i2c::I2c;
use embassy_stm32::pac;
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, can, i2c, peripherals};
// use port_expander::{Pcf8575, dev::pcf8575, write_multiple};
use static_cell::make_static;

bind_interrupts!(struct CanIrqs {
    FDCAN1_IT0 => can::IT0InterruptHandler<peripherals::FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<peripherals::FDCAN1>;
});

bind_interrupts!(struct I2CIrqs {
    I2C3_EV => i2c::EventInterruptHandler<peripherals::I2C3>;
    I2C3_ER => i2c::ErrorInterruptHandler<peripherals::I2C3>;
});

type AsyncI2C = I2c<'static, embassy_stm32::mode::Async>;
type SharedI2C = I2cDevice<'static, NoopRawMutex, AsyncI2C>;
type ExpanderReader = expander_reader::ExpanderReader<SharedI2C>;

/*
 * Hardware is shared between components and requires some internal mutability.
 */
/// Represents our µC hardware interface.
pub struct Hardware {
    // ? UnsafeCell? For led maybe ok.
    led: UnsafeCell<Output<'static>>,

    /* FIXME: Would be better if all Refcells were private and accessible within a func-call */
    /// Handle physical outputs - relays, SSRs, etc.
    // pub outputs: RefCell<io::IOIndex<32, ExpanderPin>>,
    /// Handle physical switches - inputs.
    // pub debouncer: Debouncer,
    pub expander_reader: ExpanderReader,
    // pub interconnect: interconnect::Interconnect<peripherals::FDCAN1>,
    pub interconnect: interconnect::Interconnect,
}

impl Hardware {
    pub fn new(p: embassy_stm32::Peripherals, _shared_resource: &'static Shared) -> Self {
        /* Initialize CAN */
        // let mut can = can::Fdcan::new(p.FDCAN1, p.PB8, p.PB9, CanIrqs);

        // 250k bps
        // can.set_bitrate(250_000);

        let dar1 = pac::FDCAN1.cccr().read().dar();
        pac::FDCAN1.cccr().read().set_dar(false);
        let dar2 = pac::FDCAN1.cccr().read().dar();

        info!("BEF {} AFT {}", dar1, dar2);
        // let can = can.into_normal_mode();

        let dar1 = pac::FDCAN1.cccr().read().dar();
        pac::FDCAN1.cccr().read().set_dar(false);
        let dar2 = pac::FDCAN1.cccr().read().dar();

        info!("BEF {} AFT {}", dar1, dar2);
        // let interconnect = interconnect::Interconnect::new(can);
        let interconnect = interconnect::Interconnect::new();

        /* Initialize I²C and 16-bit port expanders */
        let i2c = I2c::new(
            p.I2C3,
            p.PA8,
            p.PB5,
            I2CIrqs,
            p.DMA1_CH6,
            p.DMA1_CH1,
            Hertz(400_000),
            Default::default(),
        );
        // let i2c_bus = make_static!(NoopMutex::new(RefCell::new(i2c)));
        let i2c_bus = make_static!(Mutex::new(i2c));
        // let i2c_bus = I2C_BUS.init(i2c_bus);

        /* TODO: Assumption we have up to 3 expanders. One for outputs, one for inputs */
        // Inputs
        let inputs = pcf8575::Pcf8575::new(I2cDevice::new(i2c_bus), false, false, false);
        // Outputs
        let outputs = pcf8575::Pcf8575::new(I2cDevice::new(i2c_bus), true, true, true);

        let expander_reader = ExpanderReader::new(
            inputs,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        );

        /* TODO: The expander reading could be improved with INT and reading
         * multiple IOs at the time, but we need a caching layer so that the IOs
         * can still be properly abstracted
         */

        // let router = Router::new();
        // Salon, Dining room, Kitchen, Office, Hall, Garage, Terrace, Bathroom,

        /*
        let size = core::mem::size_of::<io::Inputs<32, ExpanderPin>>();
        defmt::info!("Size of inputs is {}", size);
        */

        // let debouncer = Debouncer::new(inputs);

        Self {
            led: UnsafeCell::new(Output::new(p.PC6.degrade(), Level::Low, Speed::Low)),
            // outputs: RefCell::new(outputs),
            // debouncer,
            expander_reader,
            interconnect,
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

    pub fn set_output(&self, idx: usize, state: bool) {
        // let mut outs = self.outputs.borrow_mut();
        // outs.set(idx, state);
    }
}

/* Set of hardware tasks */
#[embassy_executor::task(pool_size = 1)]
pub async fn spawn_readers(reader: &'static ExpanderReader) {
    reader.run().await;
}
