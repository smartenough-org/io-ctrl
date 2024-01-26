#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

// TODO: Temporarily
#![allow(unused_imports)]

use {defmt_rtt as _, panic_probe as _};

use static_cell::make_static;
use embassy_executor::Spawner;
use embassy_stm32::{
    usart,
    time::mhz,
    bind_interrupts,
    peripherals,
    Config,
    gpio::{Pin as _, Level, Output, Speed}
};

use io_ctrl::components::{
    status::{Message, Status},
    intercom::UartIntercom,
    usb_comm::UsbSerial,
};
use embassy_time::{Duration, Timer};

/// Select HW version here.
use io_ctrl::boards::ctrl_board;

/// Main testable app logic is here.
use io_ctrl::app::CtrlApp;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
    defmt::info!("Preinit");

    // Create board peripherals (early init)
    let board: &'static mut ctrl_board::Board = make_static!(ctrl_board::Board::init());

    // Wait for stabilization of power, peripherals, etc.
    Timer::after(Duration::from_millis(50)).await;

    // TODO Some initializations?

    defmt::info!("Starting board");

    // Start board tasks.
    board.spawn_tasks(&spawner);

    let mut app = CtrlApp::new(board);
    app.main().await;
}
