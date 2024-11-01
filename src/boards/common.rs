use embassy_stm32::{time::Hertz, Config};

/// Chip specific clock configuration.
pub fn config_stm32g4() -> Config {
    use embassy_stm32::rcc::{
        mux, Hse, HseMode, Hsi48Config, Pll, PllMul, PllPreDiv, PllQDiv, PllRDiv, PllSource, Sysclk,
    };
    let mut config = Config::default();

    // Change this to `false` to use the HSE clock source for the USB. This example assumes an 8MHz HSE.
    const USE_HSI48: bool = false;

    let pllq_div = if USE_HSI48 { None } else { Some(PllQDiv::DIV6) };
    config.rcc.hse = Some(Hse {
        freq: Hertz(8_000_000),
        mode: HseMode::Oscillator,
    });
    config.rcc.pll = Some(Pll {
        source: PllSource::HSE,
        prediv: PllPreDiv::DIV2,
        mul: PllMul::MUL72,
        divp: None,
        // TODO: FDCan might require 42.5Mhz (from example). Unsure about USB.
        divq: pllq_div,
        // Main system clock at 144 MHz
        divr: Some(PllRDiv::DIV2),
    });

    /* ??? */
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.boost = true; // BOOST!

    if USE_HSI48 {
        // Sets up the Clock Recovery System (CRS) to use the USB SOF to trim the HSI48 oscillator.
        config.rcc.mux.clk48sel = mux::Clk48sel::HSI48;
        config.rcc.hsi48 = Some(Hsi48Config {
            sync_from_usb: true,
        });
    } else {
        // config.rcc.mux.clk48sel = mux::Clk48sel::Pll;
        // config.rcc.clock_48mhz_src = Some(Clock48MhzSrc::PllQ);
    }

    //config.enable_debug_during_sleep = true;
    config
}
