use crate::components::message::MessageRaw;
use crate::components::status;
use crate::config::LOCAL_ADDRESS;
use defmt::*;
use embassy_stm32::can::{self, BufferedCanReceiver, BufferedCanSender};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

use super::message::Message;

pub struct Interconnect {
    can_tx: Mutex<NoopRawMutex, BufferedCanSender>,
    can_rx: BufferedCanReceiver,
}

// NOTE: Use loopback for single-device tests.
static USE_LOOPBACK: bool = false;

static TX_BUF: StaticCell<can::TxBuf<4>> = StaticCell::new();
static RX_BUF: StaticCell<can::RxBuf<4>> = StaticCell::new();
// I only keep this around so that can keeps working.
static BUFFERED_CAN: StaticCell<embassy_stm32::can::BufferedCan<'static, 4, 4>> = StaticCell::new();

pub enum WhenFull {
    /// Output queue is full and can't immediately schedule message? Drop message.
    Drop,
    /// Output queue is full? Block until it's free. Might block indefinetely if CAN failed.
    Block,
    /// Wait a bit and retry, but don't block forever.
    Wait,
}

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

    pub async fn transmit_standard(&self, raw: &MessageRaw, when_full: WhenFull) -> bool {
        // RTR False
        let frame = raw.to_can_frame();

        // Happy path.
        let ret = {
            let mut tx = self.can_tx.lock().await;
            tx.try_write(frame)
        };
        if ret.is_err() {
            status::COUNTERS.can_queue_full.inc();
            match when_full {
                WhenFull::Drop => {
                    defmt::warn!(
                        "Output CAN buffer is full - not blocking. Message will be dropped"
                    );
                    status::COUNTERS.can_drop.inc();
                    false
                }
                WhenFull::Block => {
                    defmt::warn!("Output CAN buffer is full - will block and wait.");
                    let frame = raw.to_can_frame();
                    let mut tx = self.can_tx.lock().await;
                    tx.write(frame).await;
                    true
                }
                WhenFull::Wait => {
                    // Longest frame should be under 150 bits with some stuffed
                    // bits. With 250kbps that's 0.6ms transmission time. If the
                    // CAN works at all, then within around 0.5ms we should be
                    // able to store the new frame.
                    let mut wait_time = 1;
                    for _ in 0..8 {
                        Timer::after(Duration::from_micros(600 + wait_time * 500)).await;
                        let mut tx = self.can_tx.lock().await;
                        let ret = tx.try_write(frame);
                        if ret.is_ok() {
                            return true;
                        }
                        wait_time += 1;
                    }
                    defmt::error!("Dropping CAN message after waiting {:?}", frame);
                    status::COUNTERS.can_drop.inc();
                    false
                }
            }
        } else {
            defmt::info!("Message to {:#02x} scheduled {:?}", raw.to_can_addr(), raw);
            true
        }
    }

    /// Schedule transmission of a interconnect message - from this node.
    /// TODO: Nicer API than bool?
    pub async fn transmit_response(&self, msg: &Message, when_full: WhenFull) -> bool {
        let raw = msg.to_raw(LOCAL_ADDRESS);
        self.transmit_standard(&raw, when_full).await
    }

    pub async fn transmit_request(&self, dst_addr: u8, msg: &Message, when_full: WhenFull) -> bool {
        let raw = msg.to_raw(dst_addr);
        self.transmit_standard(&raw, when_full).await
    }
}
