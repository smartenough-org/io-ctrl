///
/// Represents the Hardware. Pretty much everything in this file is static,
/// initialized once and available through the lifetime of a program.
///
use core::cell::UnsafeCell;

use crate::boards::{common, io_router};
use defmt::unwrap;
use embassy_executor::Spawner;

use crate::components::interconnect::Interconnect;

use defmt::info;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;

use embassy_stm32::gpio::{Level, Output, Pin, Speed};

use crate::io::{
    events::InputChannel, events::IoIdx, expander_outputs, expander_switches,
    indexed_outputs::IndexedOutputs, pcf8575::Pcf8575,
};

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::i2c::I2c;
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, can, i2c, peripherals};
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

/// A queue that aggregates all hardware event sources (expanders, native IOs, etc).
/// It's later consumed by EventConverter.
static INPUT_CHANNEL: InputChannel = InputChannel::new();

/// Queue of output-controlling events that are handled by IORouter.
static OUTPUT_CHANNEL: io_router::OutputChannel = io_router::OutputChannel::new();

/// Represents our µC hardware interface. It's 'static and shared by most code.
pub struct Board {
    // ? UnsafeCell? For led maybe ok.
    led: UnsafeCell<Output<'static>>,

    /* FIXME: Would be better if all Refcells were private and accessible within a func-call */
    /// Handle physical outputs - relays, SSRs, etc.
    // pub outputs: RefCell<io::IOIndex<32, ExpanderPin>>,
    // pub expander_outputs: ExpanderOutputs,
    /// Handle physical switches - inputs.
    pub expander_switches: ExpanderSwitches,

    /// Queue of input events (from expanders, native IOs, etc.)
    pub input_q: &'static InputChannel,
    pub io_command_q: &'static io_router::OutputChannel,

    /// Physical outputs.
    indexed_outputs:
        Mutex<NoopRawMutex, IndexedOutputs<18, 1, 2, ExpanderOutputs, Output<'static>>>,

    /// CAN communication between the layers.
    pub interconnect: Interconnect,
}

impl Board {
    pub fn init() -> Self {
        let config = common::config_stm32g4();
        let peripherals = embassy_stm32::init(config);

        common::ensure_boot0_configuration();

        Self::assign_peripherals(peripherals)
    }

    pub fn assign_peripherals(p: embassy_stm32::Peripherals) -> Self {
        /* Initialize CAN */
        let can = can::CanConfigurator::new(p.FDCAN1, p.PB8, p.PB9, CanIrqs);
        let interconnect = Interconnect::new(can);

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

        /* TODO: Assumption we have up to 3 expanders. One for outputs, second
         * for inputs (light, switches), third one for sensors */
        // Inputs - light switches.
        let inputs = Pcf8575::new(I2cDevice::new(i2c_bus), true, true, true);

        // Inputs - sensors.
        let _sensors = Pcf8575::new(I2cDevice::new(i2c_bus), true, true, false);

        // Outputs
        let outputs = Pcf8575::new(I2cDevice::new(i2c_bus), false, false, false);

        let expander_switches = ExpanderSwitches::new(
            inputs,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            &INPUT_CHANNEL,
        );

        let expander_outputs = ExpanderOutputs::new(outputs);

        let indexed_outputs = Mutex::new(IndexedOutputs::new(
            [expander_outputs],
            [
                Output::new(p.PB3, Level::High, Speed::Low),
                Output::new(p.PB4, Level::High, Speed::Low),
            ],
            // IDs for outputs in order, starting with expander outputs.
            [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                /* Native Pins start here */
                17, 18,
            ],
        ));

        info!("Board initialized");
        Self {
            led: UnsafeCell::new(Output::new(p.PC6.degrade(), Level::Low, Speed::Low)),
            expander_switches,
            indexed_outputs,
            interconnect,
            input_q: &INPUT_CHANNEL,
            io_command_q: &OUTPUT_CHANNEL,
        }
    }

    pub fn spawn_tasks(&'static self, spawner: &Spawner) {
        unwrap!(spawner.spawn(task_interconnect(&self.interconnect)));
        unwrap!(spawner.spawn(task_expander_switches(&self.expander_switches)));
        unwrap!(spawner.spawn(io_router::task_io_router(self, self.io_command_q)));
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

#[embassy_executor::task(pool_size = 1)]
pub async fn task_expander_switches(switches: &'static ExpanderSwitches) {
    switches.run().await;
}

#[embassy_executor::task]
pub async fn task_interconnect(interconnect: &'static Interconnect) {
    interconnect.run().await
}
