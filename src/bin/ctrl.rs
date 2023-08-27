#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]

use {defmt_rtt as _, panic_probe as _};

use defmt::unwrap;
use static_cell::make_static;
use embassy_executor::Spawner;
use embassy_stm32::{
    usart,
    time::mhz,
    bind_interrupts,
    peripherals,
    Config,
    gpio::{Pin as _, Level, Output, Speed}
};

use dskctrl::usb_comm::UsbSerial;
use dskctrl::status::{Message, Status};
use dskctrl::intercom::UartIntercom;
use embassy_time::{Duration, Timer};

bind_interrupts!(struct Irqs {
    USART1 => usart::BufferedInterruptHandler<peripherals::USART1>;
});


/// Chip specific clock configuration.
pub fn config_stm32g4() -> Config {
    use embassy_stm32::rcc::{Clock48MhzSrc, CrsConfig, CrsSyncSource, Pll, PllM, PllN, PllQ, PllR, PllSrc};
    let mut config = Config::default();

    // Change this to `false` to use the HSE clock source for the USB. This example assumes an 8MHz HSE.
    const USE_HSI48: bool = true;

    let pllq_div = if USE_HSI48 { None } else { Some(PllQ::Div6) };
    config.rcc.pll = Some(Pll {
        source: PllSrc::HSE(mhz(8)),
        prediv_m: PllM::Div2,
        mul_n: PllN::Mul72,
        div_p: None,
        div_q: pllq_div,
        // Main system clock at 144 MHz
        div_r: Some(PllR::Div2),
    });

    if USE_HSI48 {
        // Sets up the Clock Recovery System (CRS) to use the USB SOF to trim the HSI48 oscillator.
        config.rcc.clock_48mhz_src = Some(Clock48MhzSrc::Hsi48(Some(CrsConfig {
            sync_src: CrsSyncSource::Usb,
        })));
    } else {
        config.rcc.clock_48mhz_src = Some(Clock48MhzSrc::PllQ);
    }

    //config.enable_debug_during_sleep = true;
    return config;
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {

    let config = config_stm32g4();

    let p = embassy_stm32::init(config);

    // TODO: Is using so many statics ok? Semantically, not a problem. Can this
    // be made easily non-static at all?

    // Create status
    let led = Output::new(p.PC6.degrade(), Level::High, Speed::Low);
    let status: &'static Status = make_static!(Status::new(led));
    unwrap!(spawner.spawn(status_runner(status)));

    let mut serial_config = usart::Config::default();

    /* Interface has an asymetrical optoisolator with 36µs falling edge latency,
     * and immediate rising edge. 2400 baud seem to work. For 9600 that's 34%
     * error. */
    serial_config.baudrate = 2400;
    defmt::info!("Serial config baudrate {:?}", serial_config.baudrate);
    let tx_buf = make_static!([0u8; 32]);
    let rx_buf = make_static!([0u8; 32]);
    let usart = usart::BufferedUart::new(p.USART1, Irqs, p.PB7 /* rx */, p.PB6 /* tx */,
                                         tx_buf, rx_buf, serial_config);
    let intercom = make_static!(UartIntercom::new(usart, status));
    unwrap!(spawner.spawn(intercom_runner(intercom)));

    // Create USB side
    let usbserial = UsbSerial::new(status, p.USB, p.PA12, p.PA11);
    unwrap!(spawner.spawn(usb_runner(usbserial, intercom)));

    status.set_state(Message::Init, 1).await;

    let mut actuator_ctrl = {
        use dskctrl::actuator::{ActuatorCtrl, Actuator, PinType};
        use embassy_stm32::gpio::{Pull, OutputOpenDrain};
        let do_pin = |pin| {
            OutputOpenDrain::new(pin, Level::High, Speed::Low, Pull::None)
        };
        ActuatorCtrl::new([
            // Maybe pass AnyPin instead? Can't... that's not a Trait. Makes it dependent on STM32.
            Actuator::new(do_pin(p.PA0.degrade()), PinType::ActiveLow),
            Actuator::new(do_pin(p.PA2.degrade()), PinType::ActiveLow),
            Actuator::new(do_pin(p.PA4.degrade()), PinType::ActiveLow),
            Actuator::new(do_pin(p.PA6.degrade()), PinType::ActiveLow),
        ])
    };

    use dskctrl::actuator::{Action, Command};
    actuator_ctrl.execute(Command::new(0, Action::On));

    loop {
        defmt::info!("ON");
        actuator_ctrl.execute(Command::new(0, Action::On));
        actuator_ctrl.execute(Command::new(1, Action::On));
        actuator_ctrl.execute(Command::new(2, Action::On));
        actuator_ctrl.execute(Command::new(3, Action::On));

        Timer::after(Duration::from_millis(1000)).await;

        defmt::info!("OFF");
        actuator_ctrl.execute(Command::new(0, Action::Off));
        actuator_ctrl.execute(Command::new(1, Action::Off));
        actuator_ctrl.execute(Command::new(2, Action::Off));
        actuator_ctrl.execute(Command::new(3, Action::Off));
        Timer::after(Duration::from_millis(1000)).await;
    }
}

type BufferedIntercom<'a> = UartIntercom<usart::BufferedUart<'a, peripherals::USART1>>;

#[embassy_executor::task]
async fn intercom_runner(intercom: &'static BufferedIntercom<'static>) {
    intercom.tx_loop().await;
}

#[embassy_executor::task]
async fn status_runner(status: &'static Status) {
    status.update_loop().await;
}

#[embassy_executor::task]
pub async fn usb_runner(serial: UsbSerial, intercom: &'static BufferedIntercom<'static>) {
    serial.run(intercom).await;
}
