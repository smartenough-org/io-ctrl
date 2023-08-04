#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]


use {defmt_rtt as _, panic_probe as _};

#[rtic::app(device = embassy_stm32, peripherals = false, dispatchers = [EXTI2, EXTI3, EXTI4])]
mod app {
    use defmt::info;
    use embassy_stm32::gpio::{Level, Output, Speed};
    use embassy_stm32::{peripherals, Config};
    use embassy_time::{Duration, Timer};
    use diode::usb_comm::UsbSerial;
    use embassy_stm32::peripherals::PA12;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {}

    #[init]
    fn init(_: init::Context) -> (Shared, Local) {
        info!("Hello World!");

        let mut config = Config::default();
        config.enable_debug_during_sleep = true;
        defmt::info!("Config is hse {:?} {:?} hclk {:?} sys_ck {:?} pclk {:?} {:?} pll48 {:?}",
                     &config.rcc.hse, &config.rcc.bypass_hse, &config.rcc.hclk,
                     &config.rcc.sys_ck, config.rcc.pclk1, config.rcc.pclk2,
                     &config.rcc.pll48);


        let p = embassy_stm32::init(config);

        let _usbserial = UsbSerial::new(p.USB_OTG_FS, p.PA12, p.PA11);

        blink::spawn(p.PC13).map_err(|_| ()).unwrap();

        (Shared {}, Local {})
    }

    #[task(priority = 1)]
    async fn blink(_cx: blink::Context, pin: peripherals::PC13) {
        let mut led = Output::new(pin, Level::Low, Speed::Low);

        loop {
            info!("off!");
            led.set_high();
            Timer::after(Duration::from_millis(300)).await;
            info!("on!");
            led.set_low();
            Timer::after(Duration::from_millis(300)).await;
        }
    }
}
