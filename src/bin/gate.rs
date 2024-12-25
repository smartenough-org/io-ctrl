/*
 * Main entry point for CAN BUS <-> HA gate.
 */

#![no_std]
#![no_main]
// TODO: Temporarily
#![allow(unused_imports)]

use panic_probe as _;

use embassy_executor::Spawner;
use static_cell::StaticCell;

use embassy_time::{Duration, Timer};

use io_ctrl::boards::ctrl_board;

use io_ctrl::app::GateApp;

static BOARD: StaticCell<ctrl_board::Board> = StaticCell::new();
static GATE: StaticCell<GateApp> = StaticCell::new();

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
    rtt_target::rtt_init_defmt!();
    defmt::info!("Gate preinit");

    // Create board peripherals (early init)
    let board = BOARD.init(ctrl_board::Board::init());

    defmt::info!("Starting gate board");

    // Sleep short time to give peripherals time to initialize.
    Timer::after(Duration::from_millis(50)).await;

    // Start board tasks.
    board.spawn_tasks(&spawner);

    let gate = GATE.init(GateApp::new(board).await);
    gate.main(&spawner).await;
}
