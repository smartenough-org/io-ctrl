use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::uid;
use embassy_time::{Duration, Timer};

use crate::boards::ctrl_board::Board;
use crate::components::interconnect::WhenFull;
use crate::components::{
    message::{Message, MessageRaw, args},
    status, usb_connect,
};

/// Main application/business logic entrypoint.
pub struct GateApp {
    /// For all IO needs (and comm peripherals like CAN and USB)
    pub board: &'static Board,
}

impl GateApp {
    pub async fn new(board: &'static Board) -> Self {
        Self { board }
    }

    fn spawn_tasks(&'static self, spawner: &Spawner) {
        spawner.spawn(unwrap!(task_read_interconnect(self.board)));
        spawner.spawn(unwrap!(task_read_usb(self.board)));
    }

    pub async fn main(&'static mut self, spawner: &Spawner) -> ! {
        defmt::info!("Starting gate app on chip {}", uid::uid());

        let welcome_message = Message::Info {
            code: args::InfoCode::Started.to_bytes(),
            arg: 0,
        };

        // Gate can block because it makes no sense without working CAN.
        self.board
            .interconnect
            .transmit_response(&welcome_message, WhenFull::Block)
            .await;

        self.spawn_tasks(spawner);

        let mut cnt = 0;
        loop {
            // Steady action to indicate we are alive and ok.
            Timer::after(Duration::from_millis(2)).await;
            if cnt % 3000 == 0 {
                defmt::info!("Tick: {:?}", status::COUNTERS);
            }
            cnt += 1;

            // If we sleep too much and probe doesn't work ok, we can reduce sleep using this:
            // embassy_futures::yield_now().await;
        }
    }
}

/// Read interconnect and pump into USB.
#[embassy_executor::task]
pub async fn task_read_interconnect(board: &'static Board) {
    loop {
        let raw = board.interconnect.receive().await;
        defmt::info!("Interconnect: Received message {}. Pushing to USB.", raw);

        if let Ok(msg) = raw {
            let mut buf = usb_connect::CommPacket::default();
            (buf.data[0], buf.data[1]) = msg.addr_type();
            buf.data[2] = msg.length();
            buf.data[3..3 + msg.length() as usize].copy_from_slice(msg.data_as_slice());
            buf.count = 3 + msg.length();

            if !board.usb_up.is_empty() {
                defmt::warn!(
                    "Non-empty queue (len={}) when sending to USB.",
                    board.usb_up.len()
                );
            }
            board.usb_up.send(buf).await;
        } else {
            defmt::warn!("Error while reading a message {:?}", raw);
            continue;
        };
    }
}

/// Read interconnect and pump into USB.
#[embassy_executor::task]
pub async fn task_read_usb(board: &'static Board) {
    loop {
        let raw = board.usb_down.receive().await;
        defmt::info!("USB RX: Received message {}", raw.as_slice());

        let length = raw.data[2] as usize;
        if length > 8 {
            defmt::error!("Received message is too big ({}), ignoring.", length);
            continue;
        }
        let body = &raw.data[3..3 + length];
        let raw = MessageRaw::from_bytes(raw.data[0], raw.data[1], body);

        if let Some(msg) = Message::from_raw(&raw) {
            defmt::info!("Parsed message is {:?} from raw {:?}.", msg, raw);
        } else {
            defmt::info!("Unable to parse message {:?}", raw)
        }

        board.interconnect.transmit_standard(&raw, WhenFull::Block).await;
    }
}
