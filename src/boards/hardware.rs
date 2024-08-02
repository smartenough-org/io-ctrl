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
use embassy_stm32::gpio::{Level, Output, Pin, Speed};

use crate::io::{
    expander_switches,
    expander_outputs,
    event_converter::EventConverter,
    indexed_outputs::IndexedOutputs, events::IoIdx, pcf8575
};

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::i2c::I2c;
use embassy_stm32::pac;
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, can, i2c, peripherals};
// use port_expander::{Pcf8575, dev::pcf8575, write_multiple};
use static_cell::StaticCell;

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
type ExpanderSwitches = expander_switches::ExpanderSwitches<SharedI2C>;
type ExpanderOutputs = expander_outputs::ExpanderOutputs<SharedI2C>;

static I2C_BUS: StaticCell<Mutex<NoopRawMutex, AsyncI2C>> = StaticCell::new();
static EVENT_CONVERTER: StaticCell<EventConverter> = StaticCell::new();

/*
 * Hardware is shared between components and requires some internal mutability.
 */
/// Represents our µC hardware interface.
pub(crate) struct Hardware {
    // ? UnsafeCell? For led maybe ok.
    led: UnsafeCell<Output<'static>>,

    /* FIXME: Would be better if all Refcells were private and accessible within a func-call */
    /// Handle physical outputs - relays, SSRs, etc.
    // pub outputs: RefCell<io::IOIndex<32, ExpanderPin>>,
    // pub expander_outputs: ExpanderOutputs,
    /// Handle physical switches - inputs.
    // pub debouncer: Debouncer,
    pub expander_switches: ExpanderSwitches,

    pub event_converter: &'static EventConverter,

    indexed_outputs: Mutex<NoopRawMutex, IndexedOutputs<1, 2, ExpanderOutputs, Output<'static>>>,
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
        let i2c_bus = I2C_BUS.init(Mutex::new(i2c));

        let event_converter = EVENT_CONVERTER.init(EventConverter::new());

        /* TODO: Assumption we have up to 3 expanders. One for outputs, one for inputs */
        // Inputs
        let inputs = pcf8575::Pcf8575::new(I2cDevice::new(i2c_bus), true, true, true);

        // Outputs
        let outputs = pcf8575::Pcf8575::new(I2cDevice::new(i2c_bus), false, false, false);

        let expander_switches = ExpanderSwitches::new(
            inputs,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            event_converter,
        );

        let expander_outputs = ExpanderOutputs::new(outputs);

        let indexed_outputs = Mutex::new(IndexedOutputs::new(
            [expander_outputs],
            [Output::new(p.PB3, Level::High, Speed::Low),
             Output::new(p.PB4, Level::High, Speed::Low)],
            // IDs for outputs in order, starting with expander outputs.
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
             /* Native Pins start here */
             17, 18],
        ));

        Self {
            led: UnsafeCell::new(Output::new(p.PC6.degrade(), Level::Low, Speed::Low)),
            // outputs: RefCell::new(outputs),
            expander_switches,
            indexed_outputs,
            interconnect,

            // TODO: This is not a hardware.
            event_converter,
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

    pub async fn set_output(&self, idx: IoIdx, state: bool) -> Result<(), ()> {
        // TODO: Try few times; count errors; panic after some threshold.
        self.indexed_outputs.lock().await.set(idx, state).await
    }
}

/* Set of hardware tasks */
#[embassy_executor::task(pool_size = 1)]
pub async fn spawn_switches(switches: &'static ExpanderSwitches) {
    switches.run().await;
}

#[embassy_executor::task(pool_size = 1)]
pub async fn spawn_event_converter(ec: &'static EventConverter) {
    ec.run().await;
}
