use defmt::{unwrap, info, panic};
use core::cell::RefCell;
use embassy_time::{Instant, Duration, Timer, with_timeout};
use embassy_stm32::gpio::{AnyPin, Output};

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;

#[derive(Debug, PartialEq, Eq)]
pub enum Message {
    /// Just started
    Init,
    /// We are idle
    Idle,
    /// Transferring data
    Transfer,
    /// eg. Just connected
    Attention,
}

impl Message {
    fn to_time(&self) -> (Duration, Duration) {
        let map_tuple = |(on, off)| {
            (Duration::from_millis(on), Duration::from_millis(off))
        };
        map_tuple(match self {
            Message::Init => (200, 200),
            Message::Idle => (50, 3000),
            Message::Transfer => (10, 50),
            Message::Attention => (100, 100),
        })
    }
}

#[derive(Debug)]
struct MessageOrder {
    message: Message,
    until: Instant,
}

impl Default for MessageOrder {
    fn default() -> Self {
        MessageOrder {
            message: Message::Idle,
            until: Instant::now()
        }
    }
}

/// Controls status LED.
pub struct Status {
    led: RefCell<Output<'static, AnyPin>>,
    channel: Channel<NoopRawMutex, MessageOrder, 3>,
}

impl Status {
    pub fn new(led: Output<'static, AnyPin>) -> Self {
        let channel = Channel::<NoopRawMutex, MessageOrder, 3>::new();
        Status {
            led: RefCell::new(led),
            channel
        }
    }

    /// Set state to be displayed. Might block if queue full.
    pub async fn set_state(&self, message: Message, seconds: u32) {
        let until = Instant::now() + Duration::from_secs(seconds as u64);
        self.channel.send(MessageOrder { message, until }).await;
    }

    pub fn try_set_state(&self, message: Message, seconds: u32) {
        let until = Instant::now() + Duration::from_secs(seconds as u64);
        let _ = self.channel.try_send(MessageOrder { message, until });
    }

    async fn read_wait(&self, timeout: Duration,
                       on_t: &mut Duration, off_t: &mut Duration, until: &mut Instant) {
        let result = with_timeout(timeout, self.channel.recv()).await;
        match result {
            // Data or timeout interrupted with data.
            Ok(incoming) => {
                let (new_on_t, new_off_t) = incoming.message.to_time();
                *on_t = new_on_t;
                *off_t = new_off_t;
                *until = incoming.until;

                info!("Status: Change {:?} {:?} - until {:?}",
                      new_on_t, new_off_t, incoming.until);
            }
            // Timeout.
            Err(_) => ()
        }
    }

    pub async fn update_loop(&self) {
        let mut until = Instant::now();
        let (mut on_t, mut off_t) = Message::Idle.to_time();
        let mut cnt = 0;
        let mut led = self.led.borrow_mut();
        loop {
            led.set_low();
            self.read_wait(on_t, &mut on_t, &mut off_t, &mut until).await;

            led.set_high();
            self.read_wait(off_t, &mut on_t, &mut off_t, &mut until).await;

            if Instant::now() > until {
                (on_t, off_t) = Message::Idle.to_time();
                until = Instant::MAX;
                info!("Status: Going back to idle");
            }

            cnt += 1;
            if cnt % 10 == 0 {
                info!("Heartbeat {}", cnt);
            }
        }
    }
}
