use core::cell::RefCell;
use embassy_stm32::{peripherals, can, pac};
use embassy_time::{Duration, Timer};
use defmt::info;

pub struct Interconnect<I>
where
    I: can::Instance
{
    can: RefCell<can::Fdcan<'static, I, fdcan::NormalOperationMode>>
}


impl<I: can::Instance> Interconnect<I> {
    pub fn new(can: can::Fdcan<'static, I, fdcan::NormalOperationMode>) -> Self {
        Self {
            can: RefCell::new(can),
        }
    }

    async fn run(&self) {
        let mut can = self.can.borrow_mut();
        let mut i: u32 = 0;
        loop {
            let interrupt_reg = pac::FDCAN1.ir().read();
            let cccr_reg = pac::FDCAN1.cccr().read();
            let psr_reg = pac::FDCAN1.cccr().read();
            defmt::info!("LOOPLY LOOP overruns={} ir={:b} cccr={:b} psr={:b}", can.overruns, interrupt_reg.0, cccr_reg.0, psr_reg.0);

            //defmt::info!("Can LOOPS! overruns={}", can.overruns);
            // Timer::after(Duration::from_millis(500)).await;

            let frame = can::TxFrame::new(
                can::TxFrameHeader {
                    len: 4,
                    frame_format: can::FrameFormat::Standard,
                    id: can::StandardId::new(0x123).unwrap().into(),
                    bit_rate_switching: false,
                    marker: None,
                },
                &i.to_le_bytes(),
            ).unwrap();
            info!("Writing frame");
            _ = can.write(&frame).await;

            /* Wait and read some frame */
            match can.read().await {
                Ok(rx_frame) => {
                    let data = rx_frame.data();
                    let cnt = u32::from_le_bytes(data[..4].try_into().unwrap());
                    info!("Rx: {}", cnt)
                },
                Err(err) => info!("Nothing received {:?}", err),
            }
            i = i + 1;
        }
    }

}


/* Fixme, this should not depend on FDCAN1 */
#[embassy_executor::task(pool_size = 1)]
pub async fn spawn(can: &'static Interconnect<peripherals::FDCAN1>) {
    can.run().await
}
