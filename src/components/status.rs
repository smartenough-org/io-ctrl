use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};
use defmt::info;
use embassy_stm32::gpio::Output;
use embassy_time::{with_timeout, Duration, Instant};

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;

/// Simplify API of atomics for this usecase.
pub struct Counter(AtomicU32);
impl Counter {
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    pub fn inc(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }

    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

impl defmt::Format for Counter {
    fn format(&self, fmt: defmt::Formatter) {
        let data = self.0.load(Ordering::Relaxed);
        defmt::write!(fmt, "{}", data);
    }
}

#[derive(defmt::Format)]
pub struct Counters {
    /// Input event queue was full.
    pub input_queue_full: Counter,
    /// Output queue was full.
    pub output_queue_full: Counter,

    /// Error while reading input IO expander
    pub expander_input_error: Counter,
    /// Error while reading output IO expander
    pub expander_output_error: Counter,
    /// Error from CAN firmware while reading frames.
    pub can_frame_error: Counter,
    /// Output CAN queue is full.
    pub can_queue_full: Counter,
}

pub static COUNTERS: Counters = Counters {
    input_queue_full: Counter::new(),
    output_queue_full: Counter::new(),
    expander_input_error: Counter::new(),
    expander_output_error: Counter::new(),
    can_frame_error: Counter::new(),
    can_queue_full: Counter::new(),
};

impl Counters {
    /// Has any problem been detected?
    pub fn has_problem(&self) -> bool {
        self.input_queue_full.get() > 0
            || self.output_queue_full.get() > 0
            || self.expander_input_error.get() > 0
            || self.expander_output_error.get() > 0
            || self.can_frame_error.get() > 0
            || self.can_queue_full.get() > 0
    }
}

#[derive(Debug, PartialEq, Eq, defmt::Format)]
pub enum Blink {
    /// Just started
    Init,
    /// We are idle and OK.
    Idle,
    /// Executing some actions data
    Active,
    /// Recent error/warning occured - temporary situation.
    Warning,
    /// We are mostly IDLE, but some error happened.
    Attention,
}

impl Blink {
    fn to_time(&self) -> (Duration, Duration, usize) {
        let (on, off, count) = match self {
            // Externally triggered
            Blink::Active => (10, 50, 8),
            Blink::Warning => (100, 100, 10),

            // Special internal
            Blink::Init => (200, 200, 3),
            Blink::Idle => (10, 3000, 0),
            Blink::Attention => (300, 3000, 0),
        };
        (Duration::from_millis(on), Duration::from_millis(off), count)
    }
}

/// Controls status LED.
pub struct Status {
    led: UnsafeCell<Output<'static>>,
    channel: Channel<NoopRawMutex, Blink, 3>,

    pub boot_time: Instant,
}

impl Status {
    pub fn new(led: Output<'static>) -> Self {
        let channel = Channel::<NoopRawMutex, Blink, 3>::new();
        Status {
            led: UnsafeCell::new(led),
            channel,
            boot_time: Instant::now(),
        }
    }

    /// Set state to be displayed. Might block if queue full.
    pub async fn set_state(&self, blink: Blink) {
        self.channel.send(blink).await;
    }

    /// Don't block and ignore failures.
    pub fn try_set_state(&self, blink: Blink) {
        let _ = self.channel.try_send(blink);
    }

    /// Set state to active errorlessly.
    pub fn is_active(&self) {
        self.try_set_state(Blink::Active);
    }

    /// Set state to active errorlessly.
    pub fn is_warning(&self) {
        self.try_set_state(Blink::Warning);
    }

    async fn read_wait(
        &self,
        timeout: Duration,
        on_t: &mut Duration,
        off_t: &mut Duration,
        count: &mut usize,
    ) {
        let result = with_timeout(timeout, self.channel.receive()).await;
        if let Ok(incoming) = result {
            // Data or timeout interrupted with data.
            let (new_on_t, new_off_t, new_count) = incoming.to_time();
            info!("System status: {:?}", incoming);
            *on_t = new_on_t;
            *off_t = new_off_t;
            *count = new_count;
        } else {
            // Timeout.
        }
    }

    pub async fn update_loop(&self) {
        // That's safe if there's only one update loop running.
        let led = unsafe { &mut *self.led.get() };
        let (mut on_t, mut off_t, mut count) = Blink::Init.to_time();
        let mut cnt = 0;
        loop {
            led.set_high();
            self.read_wait(on_t, &mut on_t, &mut off_t, &mut count)
                .await;

            led.set_low();
            self.read_wait(off_t, &mut on_t, &mut off_t, &mut count)
                .await;

            // When we reach count 0 - get back to blinking the idle/attention time. Count 0 means forever.
            if count == 0 {
                if COUNTERS.has_problem() {
                    (on_t, off_t, count) = Blink::Attention.to_time();
                } else {
                    (on_t, off_t, count) = Blink::Idle.to_time();
                }
            } else {
                count -= 1;
            }

            cnt += 1;
            if cnt % 10 == 0 {
                info!("Heartbeat {}", cnt);
            }
        }
    }
}
