use crate::components::message::MessageRaw;
use crate::config::LOCAL_ADDRESS;
use defmt::*;
use embassy_stm32::can;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;

use super::message::Message;

pub struct Interconnect {
    can_tx: Mutex<NoopRawMutex, can::CanTx<'static>>,
    can_rx: Mutex<NoopRawMutex, can::CanRx<'static>>,
}

// NOTE: Use loopback for single-device tests.
static USE_LOOPBACK: bool = false;

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

    /// Will block until a message is read.
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

                let length: usize = rx_frame.header().len().into();

                let delta = if ts > start {
                    // This panics on start > ts
                    (ts - start).as_millis()
                } else {
                    // Message was already buffered when we were called.
                    0
                };
                info!(
                    "CAN RX: can_addr={:#02x} len={} {:02x} --- {}ms",
                    addr,
                    header.len(),
                    rx_frame.data()[0..length],
                    delta,
                );
                Ok(MessageRaw::from_can(addr, &rx_frame.data()[0..length]))
            }
            Err(_err) => {
                // FIXME: This can start looping wildly on gate.
                /*
                 * 17251.164398 ERROR Error in frame
                 * └─ io_ctrl::components::interconnect::{impl#0}::receive::{async_fn#0} @ src/components/interconnect.rs:74
                 * 17251.164398 INFO  Interconnect: Received message Err(()). Pushing to USB.
                 * └─ io_ctrl::app::gate_app::__task_read_interconnect_task::{async_fn#0} @ src/app/gate_app.rs:69
                 * 17251.164428 WARN  Error while reading a message Err(())
                 * └─ io_ctrl::app::gate_app::__task_read_interconnect_task::{async_fn#0} @ src/app/gate_app.rs:83
                 * 17251.164459 ERROR Error in frame
                 * └─ io_ctrl::components::interconnect::{impl#0}::receive::{async_fn#0} @ src/components/interconnect.rs:74
                 * 17251.164459 INFO  Interconnect: Received message Err(()). Pushing to USB.
                 * └─ io_ctrl::app::gate_app::__task_read_interconnect_task::{async_fn#0} @ src/app/gate_app.rs:69
                 * 17251.164489 WARN  Error while reading a message Err(())
                 * └─ io_ctrl::app::gate_app::__task_read_interconnect_task::{async_fn#0} @ src/app/gate_app.rs:83
                 */
                error!("Error in frame");
                Err(())
            }
        }
    }

    pub async fn transmit_standard(&self, raw: &MessageRaw) {
        let mut can = self.can_tx.lock().await;
        // RTR False
        let standard_id =
            embedded_can::StandardId::new(raw.to_can_addr()).expect("This should create a message");
        let id = embedded_can::Id::Standard(standard_id);
        let hdr = can::frame::Header::new(id, raw.length(), false);
        let frame = can::frame::Frame::new(hdr, raw.data_as_slice()).unwrap();
        info!(
            "CAN TX: Transmitting {:?} {:#02x} {:?}",
            raw,
            raw.to_can_addr(),
            frame
        );
        // FIXME: This can hang. We should hide it behind our own queue.
        let removed_frame = can.write(&frame).await;
        if removed_frame.is_some() {
            defmt::warn!("CAN output queue is full. We've removed lower-priority message {:?}",
                         removed_frame);
            status::COUNTERS.can_queue_full.inc();
        }
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
}
