///
/// Represents the Hardware. Pretty much everything in this file is static,
/// initialized once and available through the lifetime of a program.
///
use crate::boards::common;
use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::rtc::{DateTime, Rtc, RtcConfig, RtcError, RtcTimeProvider};

use crate::components::{interconnect::Interconnect, status::Status, usb_connect};

use defmt::info;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;

use embassy_stm32::gpio::{Level, Output, Speed};

use crate::io::{
    events::InputChannel, events::IoIdx, expander_inputs, expander_outputs,
    indexed_outputs::IndexedOutputs, pcf8575::Pcf8575,
};

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::i2c::{Config, I2c};
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

type AsyncI2C = I2c<'static, embassy_stm32::mode::Async, embassy_stm32::i2c::Master>;
type SharedI2C = I2cDevice<'static, NoopRawMutex, AsyncI2C>;
type ExpanderInputs = expander_inputs::ExpanderInputs<SharedI2C>;
type ExpanderOutputs = expander_outputs::ExpanderOutputs<SharedI2C>;

static I2C_BUS: StaticCell<Mutex<NoopRawMutex, AsyncI2C>> = StaticCell::new();

static STATUS: StaticCell<Status> = StaticCell::new();

/// A queue that aggregates all hardware event sources (expanders, native IOs, etc).
/// It's later consumed by EventConverter.
static INPUT_CHANNEL: InputChannel = InputChannel::new();

/// Usb bidirectional comms
static USB_UP: usb_connect::CommChannel = usb_connect::CommChannel::new();
static USB_DOWN: usb_connect::CommChannel = usb_connect::CommChannel::new();

const INDICES_N: usize = 24;

/// Represents our µC hardware interface. It's 'static and shared by most code.
pub struct Board {
    // FIXME: ? UnsafeCell? For led maybe ok.
    // led: UnsafeCell<Output<'static>>,
    pub status: &'static Status,

    /// Handle physical switches - inputs.
    pub expander_switches: ExpanderInputs,

    /// Handle physical sensors (window, door contactrons mostly) - inputs.
    pub expander_sensors: ExpanderInputs,

    /// Queue of input events (from expanders, native IOs, etc.)
    pub input_q: &'static InputChannel,

    /// Physical outputs.
    indexed_outputs:
        Mutex<NoopRawMutex, IndexedOutputs<INDICES_N, 1, 8, ExpanderOutputs, Output<'static>>>,
    /// CAN communication between the layers.
    pub interconnect: Interconnect,

    /// Usb group, used by gate.
    pub usb_connect: Mutex<NoopRawMutex, usb_connect::UsbConnect>,
    pub usb_up: &'static usb_connect::CommChannel,
    pub usb_down: &'static usb_connect::CommChannel,

    /// On board RTC.
    pub rtc: Mutex<NoopRawMutex, Rtc>,
    pub time_provider: RtcTimeProvider,
}

impl Board {
    pub fn init() -> Self {
        let config = common::config_stm32g4();
        let peripherals = embassy_stm32::init(config);

        common::ensure_boot0_configuration();
        Self::assign_peripherals(peripherals)
    }

    pub fn assign_peripherals(p: embassy_stm32::Peripherals) -> Self {
        /* Basics */
        let led = Output::new(p.PC6, Level::Low, Speed::Low);
        let status = STATUS.init(Status::new(led));

        /* Initialize CAN */
        let can = can::CanConfigurator::new(p.FDCAN1, p.PB8, p.PB9, CanIrqs);
        let interconnect = Interconnect::new(can);

        let mut cfg: Config = Default::default();
        cfg.frequency = Hertz(400_000);

        /* Initialize I²C and 16-bit port expanders */
        let i2c = I2c::new(p.I2C3, p.PA8, p.PB5, I2CIrqs, p.DMA1_CH6, p.DMA1_CH1, cfg);
        let i2c_bus = I2C_BUS.init(Mutex::new(i2c));

        /* There can be multiple combinations of expanders. The base is pretty ubiquitous:
         * - One for inputs.
         * - One for controlled outputs.
         *
         * And then in one place I've got other box for sensors so I'd go for
         * another outputs, but in most other places I'd need inputs
         * (window/door sensors). Inputs are often longer lines without
         * additional protection. Outputs are shorter, within the box and with
         * some protection.
         *
         * For now I can go 1x outputs, 2x inputs. And use STM32 IOs for additional inputs.
         *
         * In future: We need I/O configurable by the VM.
         */
        // Inputs - light switches.
        let io_ex_inputs = Pcf8575::new(I2cDevice::new(i2c_bus), true, true, true);

        // Inputs - sensors.
        let io_sensors = Pcf8575::new(I2cDevice::new(i2c_bus), true, true, false);

        // Outputs
        let io_ex_outputs = Pcf8575::new(I2cDevice::new(i2c_bus), false, false, false);

        let expander_switches = ExpanderInputs::new(
            io_ex_inputs,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            &INPUT_CHANNEL,
            status,
            true, /* required */
        );

        let expander_sensors = ExpanderInputs::new(
            io_sensors,
            [
                21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36,
            ],
            &INPUT_CHANNEL,
            status,
            false, /* optional - at least for now */
        );

        let main_outputs = ExpanderOutputs::new(io_ex_outputs);

        #[rustfmt::skip]
        let indexed_outputs = Mutex::new(IndexedOutputs::new(
            [main_outputs],
            [
                // That's just example on how to add native IOs to outputs.
                Output::new(p.PB3, Level::High, Speed::Low),
                Output::new(p.PB4, Level::High, Speed::Low),
                Output::new(p.PB6, Level::High, Speed::Low),
                Output::new(p.PB7, Level::High, Speed::Low),
                Output::new(p.PC4, Level::High, Speed::Low),
                Output::new(p.PB12, Level::High, Speed::Low),
                Output::new(p.PB15, Level::High, Speed::Low),
                Output::new(p.PB11, Level::High, Speed::Low),
            ],
            // IDs for outputs in order, starting with expander outputs.
            [
                1,  2,  3,  4,  5,  6,  7,  8,
                9, 10, 11, 12, 13, 14, 15, 16,
                /* Native Pins start here */
                51, 52, 53, 54, 55, 56, 57, 58,
            ],
        ));

        let (rtc, time_provider) = Rtc::new(p.RTC, RtcConfig::default());

        let usb_connect = usb_connect::UsbConnect::new(p.USB, p.PA12, p.PA11);

        info!("Board initialized");
        Self {
            expander_switches,
            expander_sensors,
            indexed_outputs,
            interconnect,
            status,
            usb_connect: Mutex::new(usb_connect),
            usb_up: &USB_UP,
            usb_down: &USB_DOWN,
            rtc: Mutex::new(rtc),
            time_provider,
            input_q: &INPUT_CHANNEL,
        }
    }

    /// Spawn main common tasks.
    pub fn spawn_tasks(&'static self, spawner: &Spawner) {
        spawner.spawn(unwrap!(task_status(self.status)));
        spawner.spawn(unwrap!(task_usb_transceiver(self)));
    }

    /// Spawn tasks related to IO handling.
    pub fn spawn_io_tasks(&'static self, spawner: &Spawner) {
        spawner.spawn(unwrap!(task_expander_inputs(&self.expander_switches)));
        spawner.spawn(unwrap!(task_expander_inputs(&self.expander_sensors)));
    }

    pub async fn set_output(&self, idx: IoIdx, state: bool) -> Result<(), ()> {
        // TODO: Try few times; count errors; panic after some threshold.
        self.indexed_outputs.lock().await.set(idx, state).await
    }

    pub async fn toggle_output(&self, idx: IoIdx) -> Result<bool, ()> {
        // TODO: Try few times; count errors; panic after some threshold.
        self.indexed_outputs.lock().await.toggle(idx).await
    }

    pub async fn get_output(&self, idx: IoIdx) -> Option<bool> {
        self.indexed_outputs.lock().await.get(idx)
    }

    pub async fn get_output_status(&self) -> [(u8, bool); INDICES_N] {
        self.indexed_outputs.lock().await.get_all()
    }

    /// Read time from RTC.
    pub async fn read_time(&self) -> DateTime {
        match self.time_provider.now() {
            Ok(dt) => dt,
            Err(_rtc_err) => {
                defmt::error!("Error while reading RTC.");

                /*
                // This serves to get an increasing time in case of RTC failure
                // But requires some base that is not there yet.
                let elapsed = self.boot_time.elapsed().as_secs();
                let years = elapsed / (365 * 24 * 60 * 60);
                let elapsed = elapsed - years * (365 * 24 * 60 * 60);

                let months = elapsed / (31 * 24 * 60 * 60); /* This is very rough */
                let elapsed = elapsed - months * (31 * 24 * 60 * 60);

                let days = elapsed / (24 * 60 * 60);
                let elapsed = elapsed - days * (24 * 60 * 60);

                let hours = elapsed / (60 * 60);
                let elapsed = elapsed - hours * (60 * 60);

                let minutes = elapsed / 60;
                let seconds = elapsed - minutes * 60;
                */

                DateTime::from(
                    2025,
                    1, /* month */
                    1, /* day */
                    embassy_stm32::rtc::DayOfWeek::Wednesday,
                    0, /* hour */
                    0, /* minute */
                    0, /* second */
                    0, /* usecond */
                )
                .expect("This should work")
            }
        }
    }

    /// Set time to RTC.
    pub async fn set_time(&self, dt: DateTime) -> Result<(), RtcError> {
        let mut rtc = self.rtc.lock().await;
        rtc.set_datetime(dt)
    }
}

#[embassy_executor::task]
pub async fn task_expander_inputs(switches: &'static ExpanderInputs) {
    switches.run().await;
}

#[embassy_executor::task]
pub async fn task_status(status: &'static Status) {
    status.update_loop().await
}

#[embassy_executor::task]
pub async fn task_usb_transceiver(board: &'static Board) {
    let mut usb_connect = board.usb_connect.lock().await;
    usb_connect.run(board.usb_up, board.usb_down).await
}
