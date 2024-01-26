use embassy_stm32::{
    usart,
    time::mhz,
    bind_interrupts,
    peripherals,
    Config,
    gpio::{Pin as _, Level, Output, Speed}
};

use embassy_executor::Spawner;
use static_cell::make_static;
use crate::boards::common;
use crate::boards::hardware::Hardware;
use crate::boards::shared::Shared;

/* TODO Should use those */
use crate::components::*;


// TODO Desc
bind_interrupts!(struct Irqs {
    USART1 => usart::BufferedInterruptHandler<peripherals::USART1>;
});

pub struct Board {
    pub hardware: Hardware,
    pub shared: &'static Shared,
    // pub shared_resource: &'static SharedResource,
}


impl Board {
    pub fn init() -> Self {
        let config = common::config_stm32g4();
        let peripherals = embassy_stm32::init(config);

        let shared: &'static Shared = make_static!(Shared::new());
        let hardware = Hardware::new(peripherals, shared);
        Board {
            hardware,
            shared
        }
    }

    pub fn spawn_tasks(&'static self, _spawner: &Spawner) -> &Self {
        // self.hardware.start_tasks(spawner);
        self
    }
}
