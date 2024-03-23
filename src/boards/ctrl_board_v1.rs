use embassy_stm32::{
    usart,
    time::mhz,
    bind_interrupts,
    peripherals,
    Config,
    flash,
    gpio::{Pin as _, Level, Output, Speed},
};

use embassy_stm32::pac;

use embassy_executor::Spawner;
use static_cell::make_static;
use crate::boards::common;
use crate::boards::hardware;
use crate::boards::shared::Shared;
use defmt::unwrap;

use crate::components::{
    interconnect,
    debouncer,
};


// TODO Desc
bind_interrupts!(struct Irqs {
    USART1 => usart::BufferedInterruptHandler<peripherals::USART1>;
});

pub struct Board {
    pub hardware: hardware::Hardware,
    pub shared: &'static Shared,
    // pub shared_resource: &'static SharedResource,
}


impl Board {
    pub fn init() -> Self {
        let config = common::config_stm32g4();
        let peripherals = embassy_stm32::init(config);

        /* Disable BOOT0 pin, as it collides with CAN. We run from main memory always. */
        let n_boot0 = pac::FLASH.optr().read().n_boot0();
        let n_boot1 = pac::FLASH.optr().read().n_boot1();
        let n_swboot0 = pac::FLASH.optr().read().n_swboot0();
        defmt::info!("Boot config: {}, {}, {}", n_boot0, n_boot1, n_swboot0);
        if !n_boot0 || n_swboot0 {
            Board::reconfigure_option_bytes_g4();
        } else {
            defmt::info!("Option bytes already configured, BOOT0 is disabled");
        }

        let shared: &'static Shared = make_static!(Shared::new());
        let hardware = hardware::Hardware::new(peripherals, shared);

        Board {
            hardware,
            shared
        }
    }

    pub fn spawn_tasks(&'static self, spawner: &Spawner) {
        unwrap!(spawner.spawn(interconnect::spawn(&self.hardware.interconnect)));
        unwrap!(spawner.spawn(hardware::spawn_debouncer(&self.hardware.debouncer)));
    }

    /// According to RM0440 (page 206)
    fn reconfigure_option_bytes_g4() {
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
}
