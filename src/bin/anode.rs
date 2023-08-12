#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]

use {defmt_rtt as _, panic_probe as _};

use defmt::{unwrap, info};
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    peripherals,
    Config,
    gpio::{AnyPin, Pin as _, Level, Output, Speed}
};

use embassy_time::Duration;
use embassy_stm32::time::mhz;
use embassy_stm32::peripherals::PA12;
use embassy_stm32::peripherals::PC13;
use embassy_stm32::time::Hertz;
use static_cell::{StaticCell, make_static};

use embassy_sync::pipe::Pipe;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

use diode::usb_comm::UsbSerial;
use diode::status::{Message, Status};
use diode::intercom::UartIntercom;

use embassy_stm32::{
    Peripheral,
    usart,
};

bind_interrupts!(struct Irqs {
    USART1 => usart::BufferedInterruptHandler<peripherals::USART1>;
});

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
    info!("Hello World!");

    let mut config = Config::default();
    config.rcc.pll48 = true;
    config.rcc.sys_ck = Some(mhz(84));

    config.enable_debug_during_sleep = true;

    defmt::info!("Config is hse {:?} {:?} hclk {:?} sys_ck {:?} pclk {:?} {:?} pll48 {:?}",
                 &config.rcc.hse, &config.rcc.bypass_hse, &config.rcc.hclk,
                 &config.rcc.sys_ck, &config.rcc.pclk1, &config.rcc.pclk2,
                 &config.rcc.pll48);

    let p = embassy_stm32::init(config);

    // Create status
    let led = Output::new(p.PC13.degrade(), Level::High, Speed::Low);
    let status: &'static Status = make_static!(Status::new(led));
    unwrap!(spawner.spawn(status_runner(status)));

    let serial_config = usart::Config::default();
    let tx_buf = make_static!([0u8; 32]);
    let rx_buf = make_static!([0u8; 32]);
    let usart = usart::BufferedUart::new(p.USART1, Irqs, p.PB7, p.PB6,
                                         tx_buf, rx_buf, serial_config);
    let intercom = make_static!(UartIntercom::new(usart, status));
    unwrap!(spawner.spawn(intercom_runner(intercom)));

    // Create USB side
    let usbserial = UsbSerial::new(status, p.USB_OTG_FS, p.PA12, p.PA11);
    unwrap!(spawner.spawn(usb_runner(usbserial, intercom)));

    status.set_state(Message::Init, 1).await;
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
