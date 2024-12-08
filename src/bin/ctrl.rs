#![no_std]
#![no_main]
// TODO: Temporarily
#![allow(unused_imports)]

use {defmt_rtt as _, panic_probe as _};

use embassy_executor::Spawner;
use static_cell::StaticCell;

use embassy_time::{Duration, Timer};

/// Select HW version here.
use io_ctrl::boards::ctrl_board;

/// Main testable app logic is here.
use io_ctrl::app::CtrlApp;

static BOARD: StaticCell<ctrl_board::Board> = StaticCell::new();
static APP: StaticCell<CtrlApp> = StaticCell::new();

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
    defmt::info!("Preinit");

    // Create board peripherals (early init)
    let board = BOARD.init(ctrl_board::Board::init());

    // Fast initial blink to indicate that the firmware started. This also
    // gives some time for power, peripherals, etc. to stabilize.
    for _ in 1..5 {
        Timer::after(Duration::from_millis(100)).await;
        board.led_on();
        Timer::after(Duration::from_millis(100)).await;
        board.led_off();
    }

    defmt::info!("Starting board");

    // Start board tasks.
    board.spawn_tasks(&spawner);

    let app = APP.init(CtrlApp::new(board).await);
    app.main(&spawner).await;
}
