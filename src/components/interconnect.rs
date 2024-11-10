use defmt::*;
use embassy_stm32::can;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use crate::config::LOCAL_ADDRESS;

/// Messages passed
pub enum MessageType {
    /// I'm doing something and maybe someone wants to know.
    Announcement,
    /// Erroneous situation happened.
    Error,
    /// Periodic not triggered by event status.
    Status,

    /// TODO: We will need something for OTA config updates.
    /// To whom this may concern (device ID), total length of OTA
    MicrocodeUpdateInit,
    /// Part of binary code for upgrade.
    MicrocodeUpdatePart,
    /// CRC, apply if matches.
    MicrocodeUpdateEnd,
}

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

    pub async fn receive(&self) -> u8 {
        let start = embassy_time::Instant::now();
        let mut can = self.can_rx.lock().await;
        match can.read().await {
            Ok(envelope) => {
                let (ts, rx_frame) = (envelope.ts, envelope.frame);
                let delta = (ts - start).as_millis();
                info!(
                    "Rx: {} {:02x} --- {}ms",
                    rx_frame.header().len(),
                    rx_frame.data()[0..rx_frame.header().len() as usize],
                    delta,
                )
            }
            Err(_err) => error!("Error in frame"),
        }

        69
    }

    /// Schedule transmission of a interconnect message.
    pub async fn transmit(&self, _msg_type: MessageType, _data: &[u8; 8]) {
        let mut can = self.can_tx.lock().await;
        let address = 0x123456F;
        let msg = [0; 8];
        let frame = can::frame::Frame::new_extended(address, &msg).unwrap();
        _ = can.write(&frame).await;
    }

    /// Run task that receives messages and pushes relevant into queue.
    pub async fn run(&self) {}
}
