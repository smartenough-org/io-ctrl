use crate::components::message::MessageRaw;
use crate::config::LOCAL_ADDRESS;
use defmt::*;
use embassy_stm32::can;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;

use super::message::Message;

pub struct Interconnect
//where
//I: can::Instance
{
    // can: RefCell<can::Fdcan<'static, I, fdcan::NormalOperationMode>>,
    can_tx: Mutex<NoopRawMutex, can::CanTx<'static>>,
    can_rx: Mutex<NoopRawMutex, can::CanRx<'static>>,
}

static USE_LOOPBACK: bool = true;

impl Interconnect {
    pub fn new(mut can: can::CanConfigurator<'static>) -> Self {
        let mode = if USE_LOOPBACK {
            can::OperatingMode::InternalLoopbackMode
        } else {
            can::OperatingMode::NormalOperationMode
        };

        can.properties().set_extended_filter(
            can::filter::ExtendedFilterSlot::_0,
            can::filter::ExtendedFilter::accept_all_into_fifo1(),
        );
        can.set_bitrate(250_000);
        let can = can.start(mode);
        let (can_tx, can_rx, _props) = can.split();
        Self {
            can_tx: Mutex::new(can_tx),
            can_rx: Mutex::new(can_rx),
        }
    }

    pub async fn receive(&self) -> Result<MessageRaw, ()> {
        let start = embassy_time::Instant::now();
        let mut can = self.can_rx.lock().await;
        match can.read().await {
            Ok(envelope) => {
                let (ts, rx_frame) = (envelope.ts, envelope.frame);
                let header = rx_frame.header();
                let addr: u16 = match header.id() {
                    embedded_can::Id::Extended(_id) => {
                        defmt::info!("Got extended CAN frame - ignoring");
                        return Err(());
                    }
                    embedded_can::Id::Standard(id) => id.as_raw(),
                };

                let delta = (ts - start).as_millis();
                info!(
                    "Rx: addr={:#02x} len={} {:02x} --- {}ms",
                    addr,
                    header.len(),
                    rx_frame.data()[0..rx_frame.header().len() as usize],
                    delta,
                );
                Ok(MessageRaw::from_can(addr, rx_frame.data()))
            }
            Err(_err) => {
                error!("Error in frame");
                Err(())
            }
        }
    }

    async fn transmit_standard(&self, raw: &MessageRaw) {
        let mut can = self.can_tx.lock().await;
        // RTR False
        let standard_id =
            embedded_can::StandardId::new(raw.to_can_addr()).expect("This should create a message");
        let id = embedded_can::Id::Standard(standard_id);
        let hdr = can::frame::Header::new(id, raw.length(), false);
        let frame = can::frame::Frame::new(hdr, raw.data_as_array()).unwrap();
        info!(
            "Trnsmitting {:?} {:#02x} {:?}",
            raw,
            raw.to_can_addr(),
            frame
        );
        _ = can.write(&frame).await;
    }

    /// Schedule transmission of a interconnect message - from this node.
    pub async fn transmit_response(&self, msg: &Message) {
        let raw = msg.to_raw(LOCAL_ADDRESS);
        self.transmit_standard(&raw).await;
    }

    pub async fn transmit_request(&self, dst_addr: u8, msg: &Message) {
        let raw = msg.to_raw(dst_addr);
        self.transmit_standard(&raw).await;
    }

    /// Run task that receives messages and pushes relevant into queue.
    pub async fn run(&self) {}
}
