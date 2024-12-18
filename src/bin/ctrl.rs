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

    // Sleep short time to give peripherals time to initialize.
    Timer::after(Duration::from_millis(50)).await;

    defmt::info!("Starting board");

    // Start board tasks.
    board.spawn_tasks(&spawner);

    let app = APP.init(CtrlApp::new(board).await);
    app.main(&spawner).await;
}
