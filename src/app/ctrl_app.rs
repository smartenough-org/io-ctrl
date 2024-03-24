use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_stm32::{pac, uid};
use static_cell::make_static;

use crate::boards::ctrl_board::Board;
use crate::app::io_router;
use port_expander::write_multiple;

pub struct CtrlApp
{
    pub board: &'static Board,

    pub io_router: &'static io_router::IORouter,
}

impl CtrlApp
{
    pub fn new(board: &'static Board) -> Self {
        let io_router = make_static!(io_router::IORouter::new(&board));
        Self {
            io_router,
            board
        }
    }

    fn spawn_tasks(&'static self, spawner: &Spawner) {
        unwrap!(spawner.spawn(io_router::task(&self.io_router)));
    }

    pub async fn main(&'static mut self, spawner: &Spawner) -> ! {
        self.spawn_tasks(&spawner);

        /*
        let mut pcf_pins = self.board.hardware.get_expander_pins();

        let mut bit: u16 = 0x01;
        for i in 0..100 {
            let val: u16 = !bit;
            write_multiple(
                [
                    &mut pcf_pins.p00,
                    &mut pcf_pins.p01,
                    &mut pcf_pins.p02,
                    &mut pcf_pins.p03,
                    &mut pcf_pins.p04,
                    &mut pcf_pins.p05,
                    &mut pcf_pins.p06,
                    &mut pcf_pins.p07,

                    &mut pcf_pins.p10,
                    &mut pcf_pins.p11,
                    &mut pcf_pins.p12,
                    &mut pcf_pins.p13,
                    &mut pcf_pins.p14,
                    &mut pcf_pins.p15,
                    &mut pcf_pins.p16,
                    &mut pcf_pins.p17,
                ],
                [
                    (val & (1<< 0)) != 0,
                    (val & (1<< 1)) != 0,
                    (val & (1<< 2)) != 0,
                    (val & (1<< 3)) != 0,
                    (val & (1<< 4)) != 0,
                    (val & (1<< 5)) != 0,
                    (val & (1<< 6)) != 0,
                    (val & (1<< 7)) != 0,
                    (val & (1<< 8)) != 0,
                    (val & (1<< 9)) != 0,
                    (val & (1<<10)) != 0,
                    (val & (1<<11)) != 0,
                    (val & (1<<12)) != 0,
                    (val & (1<<13)) != 0,
                    (val & (1<<14)) != 0,
                    (val & (1<<15)) != 0,
                ],
            ).unwrap();
            bit <<= 1;
            if bit == 0x00 {
                bit = 0x01;
            }
            defmt::info!("Set to {:b}", val);
            Timer::after(Duration::from_millis(500)).await;
        }
        */

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
