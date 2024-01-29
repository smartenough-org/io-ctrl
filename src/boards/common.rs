use embassy_stm32::{
    usart,
    time::mhz,
    bind_interrupts,
    peripherals,
    Config,
    gpio::{Pin as _, Level, Output, Speed}
};

/// Chip specific clock configuration.
pub fn config_stm32g4() -> Config {
    use embassy_stm32::rcc::{Clock48MhzSrc, Hsi48Config, Pll, PllM, PllN, PllQ, PllR, PllSource};
    let mut config = Config::default();

    // Change this to `false` to use the HSE clock source for the USB. This example assumes an 8MHz HSE.
    const USE_HSI48: bool = false;

    let pllq_div = if USE_HSI48 { None } else { Some(PllQ::DIV6) };
    config.rcc.pll = Some(Pll {
        source: PllSource::HSE(mhz(8)),
        prediv_m: PllM::DIV2,
        mul_n: PllN::MUL72,
        div_p: None,
        div_q: pllq_div,
        // Main system clock at 144 MHz
        div_r: Some(PllR::DIV2),
    });

    if USE_HSI48 {
        // Sets up the Clock Recovery System (CRS) to use the USB SOF to trim the HSI48 oscillator.
        config.rcc.clock_48mhz_src = Some(Clock48MhzSrc::Hsi48(Hsi48Config {
            sync_from_usb: true,
        }));
    } else {
        config.rcc.clock_48mhz_src = Some(Clock48MhzSrc::PllQ);
    }

    //config.enable_debug_during_sleep = true;
    config
}
