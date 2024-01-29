use core::cell::RefCell;
use embassy_stm32::{peripherals, can, pac, uid};
use embassy_time::{Duration, Timer};
use defmt::info;

pub struct Interconnect<I>
where
    I: can::Instance
{
    can: RefCell<can::Fdcan<'static, I, fdcan::NormalOperationMode>>,
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
        let message_id: u16 = if uid::uid()[0] == 62 {
            info!("Initial pause");
            Timer::after(Duration::from_millis(5000)).await;
            0x0234
        } else {
            0x07ee
        };
        let message_id = 0x0234;
        loop {
            /*
            let interrupt_reg = pac::FDCAN1.ir().read();
            let cccr_reg = pac::FDCAN1.cccr().read();
            let psr_reg = pac::FDCAN1.cccr().read();
            defmt::info!("LOOPLY LOOP overruns={} ir={:b} cccr={:b} psr={:b}", can.overruns, interrupt_reg.0, cccr_reg.0, psr_reg.0);
            */

            //defmt::info!("Can LOOPS! overruns={}", can.overruns);
            // Timer::after(Duration::from_millis(500)).await;

            let frame = can::TxFrame::new(
                can::TxFrameHeader {
                    len: 4,
                    frame_format: can::FrameFormat::Standard,
                    id: can::StandardId::new(message_id).unwrap().into(),
                    bit_rate_switching: false,
                    marker: None,
                },
                &i.to_le_bytes(),
            ).unwrap();
            /*
            if i % 200 == 0 {
                info!("Writing frame id={} cnt={}", message_id, i);
            }
            */


            let ir_reg = pac::FDCAN1.ir().read();
            let cccr_reg = pac::FDCAN1.cccr().read();
            let psr_reg = pac::FDCAN1.cccr().read();

            defmt::info!("WRITE {} cccr={:b} DAR={} init={} | ir={:b} psr={:b} pea={} ped={} bo={} ew={} ep={} tcf={} mraf={}",
                         i, cccr_reg.0, cccr_reg.dar(), cccr_reg.init(),

                         ir_reg.0, psr_reg.0, ir_reg.pea(), ir_reg.ped(), ir_reg.bo(),
                         ir_reg.ew(), ir_reg.ep(), ir_reg.tcf(), ir_reg.mraf());


            // info!("Writing frame {}", i);
            _ = can.write(&frame).await;
            i += 1;
            Timer::after(Duration::from_millis(5)).await;

            /* Wait and read some frame */
            /*
            match can.read().await {
            Ok(rx_frame) => {
            let data = rx_frame.data();
            let cnt = u32::from_le_bytes(data[..4].try_into().unwrap());
            info!("Rx: {}", cnt)
        },
            Err(err) => info!("Nothing received {:?}", err),
        }
             */

            /*
            i = i + 1;
            if message_id == 0x0234 && i % 10000 == 0 {
            defmt::info!("PAUSE!");
            Timer::after(Duration::from_millis(10000)).await;

            }
            */
        }
    }
}

/* Fixme, this should not depend on FDCAN1 */
#[embassy_executor::task(pool_size = 1)]
pub async fn spawn(can: &'static Interconnect<peripherals::FDCAN1>) {
    can.run().await
}
