/*
 * Main entry point for IO controller boards.
 */

#![no_std]
#![no_main]
// TODO: Temporarily
#![allow(unused_imports)]

use panic_probe as _;

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
    rtt_target::rtt_init_defmt!();
    defmt::info!("Preinit");

    // Create board peripherals (early init)
    let board = BOARD.init(ctrl_board::Board::init());

    defmt::info!("Starting board");

    // Sleep short time to give peripherals time to initialize.
    Timer::after(Duration::from_millis(50)).await;

    // Start board tasks.
    board.spawn_tasks(&spawner);
    board.spawn_io_tasks(&spawner);

    let app = APP.init(CtrlApp::new(board, &spawner).await);
    app.main().await;
}
