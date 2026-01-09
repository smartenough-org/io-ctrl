use embassy_stm32::pac;
use embassy_stm32::{Config, time::Hertz};

/// Chip specific clock configuration.
pub fn config_stm32g4() -> Config {
    use embassy_stm32::rcc::{
        Hse, HseMode, Hsi48Config, Pll, PllMul, PllPreDiv, PllQDiv, PllRDiv, PllSource, Sysclk, mux,
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
    config.rcc.boost = true;

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
    assert!(config.enable_debug_during_sleep);
    config
}

/// Disable BOOT0 pin if it is enabled. It collides with CAN. We run from
/// main memory always.
pub fn ensure_boot0_configuration() {
    let n_boot0 = pac::FLASH.optr().read().n_boot0();
    let n_boot1 = pac::FLASH.optr().read().n_boot1();
    let n_swboot0 = pac::FLASH.optr().read().n_swboot0();
    defmt::info!("Boot config: {}, {}, {}", n_boot0, n_boot1, n_swboot0);
    if !n_boot0 || n_swboot0 {
        reconfigure_option_bytes_g4();
    } else {
        defmt::info!("Option bytes already configured, BOOT0 is disabled");
    }
}

fn reconfigure_option_bytes_g4() {
    // According to RM0440 (page 206)
    defmt::info!("Disabling BOOT0 (enable GPIO)");
    /*
    unsafe {
        flash::program_option_bytes(|| {
            pac::FLASH.optr().modify(|r| {
                r.set_n_boot0(true);
                r.set_n_swboot0(false);
            });
           let data = pac::FLASH.optr().read();
           assert_eq!(data.n_boot0(), true);
           assert_eq!(data.n_swboot0(), false);
        });
    }
    */

    // Wait, while the memory interface is busy.
    while pac::FLASH.sr().read().bsy() {}

    // Unlock flash
    if pac::FLASH.cr().read().lock() {
        defmt::info!("Flash is locked, unlocking");
        /* Magic bytes from embassy-stm32/src/flash/g.rs / RM */
        pac::FLASH.keyr().write_value(0x4567_0123);
        pac::FLASH.keyr().write_value(0xCDEF_89AB);
    }
    // Check: Should be unlocked.
    assert!(!pac::FLASH.cr().read().lock());

    // Unlock Option bytes
    if pac::FLASH.cr().read().optlock() {
        defmt::info!("Option bytes locked, unlocking");

        /* Source: RM / original HAL */
        pac::FLASH.optkeyr().write_value(0x0819_2A3B);
        pac::FLASH.optkeyr().write_value(0x4C5D_6E7F);
    }
    // Check: Should be unlocked
    assert!(!pac::FLASH.cr().read().optlock());

    /* Program boot0 */
    pac::FLASH.optr().modify(|r| {
        r.set_n_boot0(true);
        r.set_n_swboot0(false);
    });

    // Check: Should have changed
    assert!(pac::FLASH.optr().read().n_boot0());
    assert!(!pac::FLASH.optr().read().n_swboot0());

    // Reload option bytes. This should in general cause RESET.
    pac::FLASH.cr().modify(|w| w.set_optstrt(true));
    while pac::FLASH.sr().read().bsy() {}

    pac::FLASH.cr().modify(|w| w.set_obl_launch(true));

    defmt::info!("Relocking");
    // Lock option bytes and flash
    pac::FLASH.cr().modify(|w| w.set_optlock(true));
    pac::FLASH.cr().modify(|w| w.set_lock(true));
}
