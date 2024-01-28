use embassy_time::{Duration, Timer};

use crate::boards::ctrl_board::Board;
use port_expander::write_multiple;

pub struct CtrlApp
{
    pub board: &'static Board,
}

impl CtrlApp
{
    pub fn new(board: &'static Board) -> Self {
        Self { board }
    }

    pub async fn main(&mut self) -> ! {
        let mut outputs = self.board.hardware.outputs.borrow_mut();
        outputs.set_low(24);
        outputs.set_low(25);

        loop {
            defmt::info!("Main app tick");
            Timer::after(Duration::from_millis(1000)).await;
            self.board.hardware.led_on();
            Timer::after(Duration::from_millis(1000)).await;
            self.board.hardware.led_off();
        }
    }
}
