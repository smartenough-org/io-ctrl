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

    /// Schedule transmission of a interconnect message.
    async fn transmit(&self) {}

    /// Run task that receives messages and pushes relevant into queue.
    async fn run(&self) {}
}

/* Fixme, this should not depend on FDCAN1 */
#[embassy_executor::task]
pub async fn spawn(can: &'static Interconnect) {
    can.run().await
}
