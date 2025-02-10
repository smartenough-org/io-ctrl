use crate::components::message::MessageRaw;
use crate::components::status;
use crate::config::LOCAL_ADDRESS;
use defmt::*;
use embassy_stm32::can::frame::Envelope;
use embassy_stm32::can::{self, BufferedCanSender};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::DynamicReceiver;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;

use super::message::Message;

pub struct Interconnect {
    can_tx: Mutex<NoopRawMutex, BufferedCanSender>,
    can_rx: DynamicReceiver<'static, Result<Envelope, embassy_stm32::can::enums::BusError>>,
}

// NOTE: Use loopback for single-device tests.
static USE_LOOPBACK: bool = false;

static TX_BUF: StaticCell<can::TxBuf<4>> = StaticCell::new();
static RX_BUF: StaticCell<can::RxBuf<4>> = StaticCell::new();
// I only keep this around so that can keeps working.
static BUFFERED_CAN: StaticCell<embassy_stm32::can::BufferedCan<'static, 4, 4>> = StaticCell::new();

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

        let tx_buf = TX_BUF.init(can::TxBuf::<4>::new());
        let rx_buf = RX_BUF.init(can::RxBuf::<4>::new());

        let buffered = can.buffered(tx_buf, rx_buf);
        let writer = buffered.writer();
        let reader = buffered.reader();
        BUFFERED_CAN.init(buffered);

        Self {
            can_tx: Mutex::new(writer),
            can_rx: reader,
        }
    }

    /// Will block until a message is read.
    pub async fn receive(&self) -> Result<MessageRaw, ()> {
        let start = embassy_time::Instant::now();
        let can = &self.can_rx;
        match can.receive().await {
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
                defmt::trace!(
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

    pub async fn transmit_standard(&self, raw: &MessageRaw, block: bool) {
        // RTR False
        let frame = raw.to_can_frame();
        defmt::debug!(
            "CAN TX: Transmitting {:?} {:#02x} {:?}",
            raw,
            raw.to_can_addr(),
            frame
        );
        let mut tx = self.can_tx.lock().await;
        let ret = tx.try_write(frame);
        if ret.is_err() {
            status::COUNTERS.can_queue_full.inc();
            if !block {
                defmt::warn!("Output CAN buffer is full - not blocking. Message will be dropped");
            } else {
                defmt::warn!("Output CAN buffer is full - will block and wait.");
                let frame = raw.to_can_frame();
                tx.write(frame).await;
            }
        }
    }

    /// Schedule transmission of a interconnect message - from this node.
    /// TODO: Nicer API than bool?
    pub async fn transmit_response(&self, msg: &Message, block: bool) {
        let raw = msg.to_raw(LOCAL_ADDRESS);
        self.transmit_standard(&raw, block).await;
    }

    pub async fn transmit_request(&self, dst_addr: u8, msg: &Message, block: bool) {
        let raw = msg.to_raw(dst_addr);
        self.transmit_standard(&raw, block).await;
    }
}
