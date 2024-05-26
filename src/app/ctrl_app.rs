use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::{pac, uid};
use embassy_time::{Duration, Timer};
use static_cell::make_static;

use crate::boards::ctrl_board::Board;
// use crate::app::io_router;

pub struct CtrlApp {
    pub board: &'static Board,
    // pub io_router: &'static io_router::IORouter,
}

impl CtrlApp {
    pub fn new(board: &'static Board) -> Self {
        // let io_router = make_static!(io_router::IORouter::new(&board));
        Self {
            // io_router,
            board,
        }
    }

    fn spawn_tasks(&'static self, spawner: &Spawner) {
        // unwrap!(spawner.spawn(io_router::task(&self.io_router)));
    }

    pub async fn main(&'static mut self, spawner: &Spawner) -> ! {
        self.spawn_tasks(&spawner);

        defmt::info!("Starting app on chip {}", uid::uid());
        loop {
            // defmt::info!("Main app tick");
            Timer::after(Duration::from_millis(1000)).await;
            self.board.hardware.led_on();
            Timer::after(Duration::from_millis(1000)).await;
            self.board.hardware.led_off();

            /*
            let ir_reg = pac::FDCAN1.ir().read();
            let cccr_reg = pac::FDCAN1.cccr().read();
            let psr_reg = pac::FDCAN1.cccr().read();

            defmt::info!("APP cccr={:b} DAR={} init={} | ir={:b} psr={:b} pea={} ped={} bo={} ew={} ep={} tcf={} mraf={}",
                         cccr_reg.0, cccr_reg.dar(), cccr_reg.init(),

                         ir_reg.0, psr_reg.0, ir_reg.pea(), ir_reg.ped(), ir_reg.bo(),
                         ir_reg.ew(), ir_reg.ep(), ir_reg.tcf(), ir_reg.mraf());
            */
        }
    }
}
