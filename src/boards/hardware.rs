use core::cell::{
    RefCell,
    UnsafeCell
};
use defmt::{info, unwrap};
use crate::boards::shared::Shared;

use embassy_stm32::gpio::{Level, Output, AnyPin, Pin, Pull, Speed};
use crate::components::{
    io,
    interconnect,
    debouncer,
};

use embedded_hal::digital::{
    InputPin,
    OutputPin
};

use embassy_stm32::pac;
use embassy_stm32::dma::NoDma;
use embassy_stm32::i2c::{Error, I2c};
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, can, i2c, peripherals};
use port_expander::{Pcf8575, dev::pcf8575, write_multiple};
use static_cell::make_static;

bind_interrupts!(struct CanIrqs {
    FDCAN1_IT0 => can::IT0InterruptHandler<peripherals::FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<peripherals::FDCAN1>;
});

bind_interrupts!(struct I2CIrqs {
    I2C3_EV => i2c::EventInterruptHandler<peripherals::I2C3>;
    I2C3_ER => i2c::ErrorInterruptHandler<peripherals::I2C3>;
});

use embassy_stm32::peripherals::I2C3;

type BusProxy = shared_bus::I2cProxy<'static, shared_bus::NullMutex<I2c<'static, I2C3>>>;
type Expander = shared_bus::NullMutex<pcf8575::Driver<BusProxy>>;
type ExpanderPin = port_expander::Pin<'static, port_expander::mode::QuasiBidirectional, Expander>;
type Debouncer = debouncer::Debouncer<16, ExpanderPin>;

/*
 * Hardware is shared between components and requires some internal mutability.
 */


/// Represents our µC hardware interface.
pub struct Hardware
{
    // ? UnsafeCell? For led maybe ok.
    led: UnsafeCell<Output<'static>>,

    /// Handle physical outputs - relays, SSRs, etc.
    pub outputs: RefCell<io::IOIndex<32, ExpanderPin>>,
    /// Handle physical switches - inputs.
    pub debouncer: Debouncer,

    // pub interconnect: interconnect::Interconnect<peripherals::FDCAN1>,
    pub interconnect: interconnect::Interconnect,
}

impl Hardware
{
    pub fn new(
        p: embassy_stm32::Peripherals,
        _shared_resource: &'static Shared,
    ) -> Self {
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
            NoDma,
            NoDma,
            Hertz(400_000),
            Default::default(),
        );
        /* Our operations are short and not async - can't be interrupted. So maybe Simple manager is enough. */
        // let bus = make_static!(shared_bus::BusManagerCortexM::new(i2c));
        let bus = make_static!(shared_bus::BusManagerSimple::new(i2c));

        /* TODO: Assumption we have up to 3 expanders. One for outputs, one for inputs */
        /* Inputs */
        let exp1 = make_static!(Pcf8575::new(bus.acquire_i2c(), false, false, false));
        /* Outputs */
        let exp2 = make_static!(Pcf8575::new(bus.acquire_i2c(), true, true, true));
        /* Unknown yet! */
        let exp3 = make_static!(Pcf8575::new(bus.acquire_i2c(), false, true, false));

        let exp1_pins = exp1.split();
        let exp2_pins = exp2.split();
        let exp3_pins = exp3.split();

        /* TODO: The expander reading could be improved with INT and reading
         * multiple IOs at the time, but we need a caching layer so that the IOs
         * can still be properly abstracted
         */

        /* Gather all operable pins on the device. Some on expanders, some not. Assign them IDs */
        let outputs = io::IOIndex::new([
            /* IO_OUTS_0 Header */
            /*
            io::UniPin::Native(p.PB3.degrade()),
            io::UniPin::Native(p.PB6.degrade()),
            io::UniPin::Native(p.PC4.degrade()),
            io::UniPin::Native(p.PB15.degrade()),

            io::UniPin::Native(p.PB4.degrade()),
            io::UniPin::Native(p.PB7.degrade()),
            io::UniPin::Native(p.PB12.degrade()),
            io::UniPin::Native(p.PB11.degrade()),
            */

            // First expander - assumed outputs
            io::UniPin::Expander(exp1_pins.p00),
            io::UniPin::Expander(exp1_pins.p01),
            io::UniPin::Expander(exp1_pins.p02),
            io::UniPin::Expander(exp1_pins.p03),
            io::UniPin::Expander(exp1_pins.p04),
            io::UniPin::Expander(exp1_pins.p05),
            io::UniPin::Expander(exp1_pins.p06),
            io::UniPin::Expander(exp1_pins.p07),
            io::UniPin::Expander(exp1_pins.p10),
            io::UniPin::Expander(exp1_pins.p11),
            io::UniPin::Expander(exp1_pins.p12),
            io::UniPin::Expander(exp1_pins.p13),
            io::UniPin::Expander(exp1_pins.p14),
            io::UniPin::Expander(exp1_pins.p15),
            io::UniPin::Expander(exp1_pins.p16),
            io::UniPin::Expander(exp1_pins.p17),

            // Third expander - to be decided what is it
            io::UniPin::Expander(exp3_pins.p00),
            io::UniPin::Expander(exp3_pins.p01),
            io::UniPin::Expander(exp3_pins.p02),
            io::UniPin::Expander(exp3_pins.p03),
            io::UniPin::Expander(exp3_pins.p04),
            io::UniPin::Expander(exp3_pins.p05),
            io::UniPin::Expander(exp3_pins.p06),
            io::UniPin::Expander(exp3_pins.p07),
            io::UniPin::Expander(exp3_pins.p10),
            io::UniPin::Expander(exp3_pins.p11),
            io::UniPin::Expander(exp3_pins.p12),
            io::UniPin::Expander(exp3_pins.p13),
            io::UniPin::Expander(exp3_pins.p14),
            io::UniPin::Expander(exp3_pins.p15),
            io::UniPin::Expander(exp3_pins.p16),
            io::UniPin::Expander(exp3_pins.p17),
        ]);

        let inputs = io::IOIndex::new([
            /*
            // IO_COLS_0 Header: TT_EXT 0 to 7 (Assumed to be inputs)
            io::UniPin::Native(p.PA0.degrade()),
            io::UniPin::Native(p.PA1.degrade()),
            io::UniPin::Native(p.PA2.degrade()),
            io::UniPin::Native(p.PA3.degrade()),
            io::UniPin::Native(p.PA4.degrade()),
            io::UniPin::Native(p.PA5.degrade()),
            io::UniPin::Native(p.PA6.degrade()),
            io::UniPin::Native(p.PA7.degrade()),

            // IO_ROWS_AN1 - maybe inputs? Unsure
            io::UniPin::Native(p.PC10.degrade()),
            io::UniPin::Native(p.PA15.degrade()),
            io::UniPin::Native(p.PB10.degrade()),
            io::UniPin::Native(p.PB1.degrade()),

            io::UniPin::Native(p.PC11.degrade()),
            io::UniPin::Native(p.PB13.degrade()),
            io::UniPin::Native(p.PB2.degrade()),
            io::UniPin::Native(p.PB0.degrade()),
            */

            // Second expander - Assumed inputs
            io::UniPin::Expander(exp2_pins.p00),
            io::UniPin::Expander(exp2_pins.p01),
            io::UniPin::Expander(exp2_pins.p02),
            io::UniPin::Expander(exp2_pins.p03),
            io::UniPin::Expander(exp2_pins.p04),
            io::UniPin::Expander(exp2_pins.p05),
            io::UniPin::Expander(exp2_pins.p06),
            io::UniPin::Expander(exp2_pins.p07),
            io::UniPin::Expander(exp2_pins.p10),
            io::UniPin::Expander(exp2_pins.p11),
            io::UniPin::Expander(exp2_pins.p12),
            io::UniPin::Expander(exp2_pins.p13),
            io::UniPin::Expander(exp2_pins.p14),
            io::UniPin::Expander(exp2_pins.p15),
            io::UniPin::Expander(exp2_pins.p16),
            io::UniPin::Expander(exp2_pins.p17),
        ]);
        /* 72ios -> 572B, 32ios -> 256B */

        // let router = Router::new();
        // Salon, Dining room, Kitchen, Office, Hall, Garage, Terrace, Bathroom,

        /*
        let size = core::mem::size_of::<io::Inputs<32, ExpanderPin>>();
        defmt::info!("Size of inputs is {}", size);
        */

        let debouncer = Debouncer::new(inputs);

        Self {
            led: UnsafeCell::new(Output::new(p.PC6.degrade(), Level::Low, Speed::Low)),
            outputs: RefCell::new(outputs),
            debouncer,
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
        let mut outs = self.outputs.borrow_mut();
        outs.set(idx, state);
    }
}


/* Set of hardware tasks */
#[embassy_executor::task(pool_size = 1)]
pub async fn spawn_debouncer(debouncer: &'static Debouncer) {
    debouncer.run().await;
}
