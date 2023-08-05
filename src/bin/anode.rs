#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use {defmt_rtt as _, panic_probe as _};

use defmt::{unwrap, info, panic};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{AnyPin, Pin as _, Level, Output, Speed};
use embassy_stm32::{peripherals, Config};
use embassy_time::{Duration, Timer};
use embassy_stm32::time::mhz;
use embassy_stm32::peripherals::PA12;
use diode::usb_comm::{UsbSerial, run_usb};
use embassy_stm32::peripherals::PC13;
use static_cell::make_static;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
    info!("Hello World!");

    let mut config = Config::default();
    // TODO: Maybe required for USB?
    config.rcc.pll48 = true;
    config.rcc.sys_ck = Some(mhz(84));
    //config.rcc.sys_ck = Some(mhz(48));

    config.enable_debug_during_sleep = true;

    defmt::info!("Config is hse {:?} {:?} hclk {:?} sys_ck {:?} pclk {:?} {:?} pll48 {:?}",
                 &config.rcc.hse, &config.rcc.bypass_hse, &config.rcc.hclk,
                 &config.rcc.sys_ck, config.rcc.pclk1, config.rcc.pclk2,
                 &config.rcc.pll48);

    let p = embassy_stm32::init(config);

    let usbserial = UsbSerial::new(p.USB_OTG_FS, p.PA12, p.PA11);

    unwrap!(spawner.spawn(run_usb(usbserial)));
    // usbserial.run_loop().await;
    // blink::spawn(p.PC13).map_err(|_| ()).unwrap();

    let led = Output::new(p.PC13.degrade(), Level::High, Speed::Low);
    unwrap!(spawner.spawn(blinker(led)));
}



#[embassy_executor::task]
async fn blinker(mut led: Output<'static, AnyPin>) {

    loop {
        led.set_high();
        Timer::after(Duration::from_millis(3000)).await;

        led.set_low();
        Timer::after(Duration::from_millis(3000)).await;

        info!("bling.");
    }
}
