use core::cell::RefCell;
use defmt::info;
use embassy_stm32::{can, pac, peripherals, uid};
use embassy_time::{Duration, Timer};

pub struct Interconnect
//where
//I: can::Instance
{
    // can: RefCell<can::Fdcan<'static, I, fdcan::NormalOperationMode>>,
}

impl Interconnect {
    pub fn new() -> Self {
        Self {}
    }
    /*
    pub fn new(can: can::Fdcan<'static, I, fdcan::NormalOperationMode>) -> Self {
        Self {
            can: RefCell::new(can),
        }
    }
    */

    async fn run(&self) {}
}

/* Fixme, this should not depend on FDCAN1 */
#[embassy_executor::task]
pub async fn spawn(can: &'static Interconnect) {
    can.run().await
}
