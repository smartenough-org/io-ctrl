/*
 * Requirements / use cases:
 * - Estimate position and track synchronization status.
 * - Interruptible. If we are going down, and someones sends different command - stop motion.
 * - Report state changes during movement.
 */
use ector;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};

use crate::boards::ctrl_board_v1::Board;
use crate::buttonsmash::consts::{OutIdx, ShutterIdx};
use crate::config::MAX_SHUTTERS;

use defmt::Format;
use defmt::info;

// TODO: Maybe that should be time hysteresis for both cases?
/// Accuracy of position that's considered good enough. In percentage points.
const HYSTERESIS: f32 = 5.0;
/// Accuracy of tilt position.
const HYSTERESIS_TILT: f32 = 15.0;
/// Time after movement stops before we can start another one.
const COOLDOWN: Duration = Duration::from_millis(500);
/// When in motion, how often should we report position change.
const UPDATE_PERIOD: Duration = Duration::from_millis(1000);
/// If completely nothing happens, how often?
const NOOP_UPDATE_PERIOD: Duration = Duration::from_millis(10000);

/// Internal commands handled by a shutter driver.
#[derive(Format, Eq, PartialEq, Clone, Copy, Debug)]
#[repr(u8)]
pub enum Cmd {
    /// Full analog control: change height and tilt to given values 0-100.
    /// This is a two-step operation: ride (rise or drop) + tilt.
    Go(TargetPosition),

    /// Uncover/open completely. Tilt time + rise time + over_time up.
    Open,
    /// Cover/close completely. Tilt time + drop time + over_time down.
    Close,

    /// Keep height and change tilt to given 0-100.
    Tilt(u8),

    // Tilt helpers.
    /// Tilt(100) - completely closed.
    TiltClose,
    /// Tilt(0) - completely open.
    TiltOpen,
    /// 45 deg.
    TiltHalf,
    /// Open if not completely open; otherwise - close.
    TiltReverse,

    /// Shutters are configured with commands.
    SetIO(/* down */ OutIdx, /* up */ OutIdx),
    // TODO SetRiseDropTime(u16, u16),
    // TODO SetTiltOverTime(u16, u16),
}

mod codes {
    pub const GO: u8 = 0x01;
    pub const OPEN: u8 = 0x02;
    pub const CLOSE: u8 = 0x03;
    pub const TILT: u8 = 0x04;
    pub const TILT_CLOSE: u8 = 0x05;
    pub const TILT_OPEN: u8 = 0x06;
    pub const TILT_HALF: u8 = 0x07;
    pub const TILT_REVERSE: u8 = 0x08;
    pub const SET_IO: u8 = 0x10;
}

impl Cmd {
    pub fn from_raw(raw: &[u8; 5]) -> Option<Self> {
        Some(match raw[0] {
            codes::GO => Cmd::Go(TargetPosition::new(raw[1], raw[2])),
            codes::OPEN => Cmd::Open,
            codes::CLOSE => Cmd::Close,
            codes::TILT => Cmd::Tilt(raw[1]),
            codes::TILT_CLOSE => Cmd::Close,
            codes::TILT_OPEN => Cmd::TiltOpen,
            codes::TILT_HALF => Cmd::TiltHalf,
            codes::TILT_REVERSE => Cmd::TiltReverse,
            codes::SET_IO => Cmd::SetIO(raw[1], raw[2]),
            _ => {
                return None;
            }
        })
    }

    pub fn to_raw(&self, raw: &mut [u8]) {
        raw.fill(0);
        assert!(raw.len() >= 5);
        match self {
            Cmd::Go(position) => {
                raw[0] = codes::GO;
                raw[1] = position.height;
                raw[2] = position.tilt;
            }
            Cmd::Open => {
                raw[0] = codes::OPEN;
            }
            Cmd::Close => {
                raw[0] = codes::CLOSE;
            }
            Cmd::Tilt(tilt) => {
                raw[0] = codes::TILT;
                raw[1] = *tilt;
            }
            Cmd::TiltClose => {
                raw[0] = codes::TILT_CLOSE;
            }
            Cmd::TiltOpen => {
                raw[0] = codes::TILT_OPEN;
            }
            Cmd::TiltHalf => {
                raw[0] = codes::TILT_HALF;
            }
            Cmd::TiltReverse => {
                raw[0] = codes::TILT_REVERSE;
            }
            Cmd::SetIO(down, up) => {
                raw[0] = codes::SET_IO;
                raw[1] = *down;
                raw[2] = *up;
            }
        }
    }
}

/// Current shutter position, or partial position during computation.
#[derive(Format, Debug, Clone, Copy, PartialEq)]
struct Position {
    // Accuracy should allow for 1ms resolution of time. Height 0-100 in 60s
    // would mean 1% takes 600ms. 65535 would have 0.92ms resolution, but we
    // would have to convert. f32 is fine on stm32g4.

    // TODO: Height/Tilt should be an Enum - Known / Guessed. To mark when the position is not synchronized.
    /// Position of shutters. 0 (open) - 100% (closed)
    height: f32,
    /// 0 (open) - 100% (closed)
    tilt: f32,
}

/// Planned target shutter position.
#[derive(Format, Debug, Clone, Copy, Eq, PartialEq)]
pub struct TargetPosition {
    // We stick to 0-100% by 1% accuracy.
    /// Position of shutters. 0 (open) - 100% (closed)
    height: u8,
    /// 0 (open) - 100% (closed)
    tilt: u8,
}

impl TargetPosition {
    pub fn new(height: u8, tilt: u8) -> Self {
        Self { height, tilt }
    }

    fn as_position(&self) -> Position {
        Position {
            height: self.height as f32,
            tilt: self.tilt as f32,
        }
    }
}

/// Shutter configuration.
#[derive(Format)]
pub struct Config {
    /// Output to open/raise the shutter.
    pub up: OutIdx,
    /// Output to close/lower the shutter.
    pub down: OutIdx,

    /// Time it takes to raise the shutter completely [ms].
    pub rise_time: Duration,
    /// Time it takes to lower completely [ms].
    pub drop_time: Duration,
    /// Time it takes to tilt the shutter between open/close positions when
    /// switching directions.
    pub tilt_time: Duration,

    /// When reaching 0 or 100% how much time to spend on the limit switch to
    /// synchronize position information.
    pub over_time: Duration,
}

/// Internal state machine for changing state in asynchronous manner.
#[derive(Format, Debug, Eq, PartialEq)]
enum Action {
    /// Nothing is happening. But maybe should start happening.
    Idle,

    /// Nothing is happening and won't start happening by itself.
    Sleep,

    /// Currently moving up since Instant to open or decrease tilt.
    Up(Instant),

    /// Currently moving down since Instant to close or increase tilt.
    Down(Instant),

    /// Waiting between changing directions
    Cooldown(Instant),
}

/// Single shutter parameters.
pub struct Shutter {
    /// Output channel for commands
    board: &'static Board,
    /// Shutter config.
    cfg: Config,
    /// Current estimated shutter position.
    position: Position,
    /// Our target position - used if we are in motion, or equal to `position`.
    target: Position,
    /// Current shutter action.
    action: Action,
    /// If we restarted, the shutter position is unknown. We can fix it by
    /// overshooting first movement a bit. Sometimes.
    in_sync: bool,
}

impl Format for Shutter {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "Shutter {{height={} tilt={} target={:?} action={:?}}}",
            self.position.height,
            self.position.tilt,
            self.target,
            self.action
        );
    }
}

impl Config {
    pub fn new(up: OutIdx, down: OutIdx) -> Self {
        Self {
            up,
            down,
            rise_time: Duration::from_millis(57260), // Measured 57.32s
            drop_time: Duration::from_millis(57260), // Measured 57.26
            tilt_time: Duration::from_millis(1500),  // Measured 1.5s.
            over_time: Duration::from_secs(2),
        }
    }

    /// Time it will take to move from position to position.
    fn travel_as_time(&self, from: f32, to: f32) -> Duration {
        // 0% - open, 100% - closed
        // from > to -> down.
        let cost = if from > to {
            self.drop_time
        } else {
            self.rise_time
        }
        .as_millis() as f32;

        let diff = (from - to).abs();
        Duration::from_millis((cost * diff / 100.0) as u64)
    }

    /// Time it will take to tilt.
    fn tilt_as_time(&self, from: f32, to: f32) -> Duration {
        let change = (from - to).abs();
        Duration::from_millis((self.tilt_time.as_millis() as f32 * change / 100.0) as u64)
    }

    /// How much tilted in given time.
    fn time_as_tilt(&self, elapsed: Duration) -> f32 {
        let tilt = 100.0 * elapsed.as_millis() as f32 / self.tilt_time.as_millis() as f32;
        tilt.clamp(0.0, 100.0)
    }

    fn time_as_travel(&self, dir: i8, elapsed: Duration) -> f32 {
        let movement = match dir {
            // Going down (towards higher height)
            1 => 100.0 * elapsed.as_millis() as f32 / self.drop_time.as_millis() as f32,
            // Going up (towards lower height)
            -1 => 100.0 * elapsed.as_millis() as f32 / self.rise_time.as_millis() as f32,
            _ => {
                // TODO: enum?
                panic!("Bad direction argument");
            }
        };
        movement.clamp(0.0, 100.0)
    }
}

impl Position {
    pub fn new(height: u8, tilt: u8) -> Self {
        assert!(height <= 100);
        assert!(tilt <= 100);
        Self {
            height: height as f32,
            tilt: tilt as f32,
        }
    }

    pub fn new_zero() -> Self {
        Self {
            height: 0.0,
            tilt: 0.0,
        }
    }
}

impl Shutter {
    pub fn new(up: OutIdx, down: OutIdx, board: &'static Board) -> Self {
        Self {
            board,
            cfg: Config::new(up, down),
            position: Position::new_zero(),
            target: Position::new_zero(),
            action: Action::Sleep,
            in_sync: false,
        }
    }

    /// We want to tilt from start position to the target one, and some time passed.
    /// Return current tilt (movement in one direction for x ms) and residual ms
    /// time that changed the height.
    /// Returns (current tilt, rest of time for consumption)
    fn consume_tilt(&mut self, now: Instant) -> (f32, Duration) {
        let (since, dir, max_tilt) = match self.action {
            Action::Up(since) => {
                // Up, opens. Towards 0.
                (since, -1.0, 0.0)
            }
            Action::Down(since) => {
                // Down closes, towards 100.
                (since, 1.0, 100.0)
            }
            _ => {
                // Nothing will change
                return (self.position.tilt, Duration::from_secs(0));
            }
        };

        // Max time that will be taken by tilt in current direction.
        let max_time = self.cfg.tilt_as_time(self.position.tilt, max_tilt);
        // True time taken.
        let elapsed = now.duration_since(since);

        if elapsed >= max_time {
            // We reached the final tilt in max_time. Rest of elapsed time
            // should be used for changing height.
            (max_tilt, elapsed - max_time)
        } else {
            // We are within the tilt movement still.

            // How much did we tilt already?
            let tilted = self.cfg.time_as_tilt(elapsed);

            // If tilt-time conversion was not perfect, we might not be able to
            // consume exactly the time that passed. But with f32 that should be
            // accurate enough to assume we consume everything.

            let mut final_tilt = self.position.tilt;
            final_tilt += dir * tilted;
            assert!((0.0..=100.0).contains(&final_tilt)); // TODO: Dev time only.

            let final_tilt = final_tilt.clamp(0.0, 100.0);
            (final_tilt, Duration::from_secs(0))
        }
    }

    // Consume time for movement. Tilt should be calculated first.
    fn consume_height(&self, elapsed: Duration) -> f32 {
        let (dir, _conf_time) = match self.action {
            Action::Up(_since) => {
                // Up, opens. Towards 0.
                (-1i8, self.cfg.rise_time.as_millis())
            }
            Action::Down(_since) => {
                // Down closes, towards 100.
                (1, self.cfg.drop_time.as_millis())
            }
            _ => {
                // Nothing will change
                return self.position.height;
            }
        };

        let height_delta = dir as f32 * self.cfg.time_as_travel(dir, elapsed);

        let mut height = self.position.height;
        height += height_delta;
        height = height.clamp(0.0, 100.0);
        height
    }

    /// Stop movement.
    async fn go_idle(&self) {
        // Report error?
        let _ = self.board.set_output(self.cfg.up, false).await;
        let _ = self.board.set_output(self.cfg.down, false).await;
    }

    /// Start movement UP.
    async fn go_up(&self) {
        // NOTE: Should not be needed. Just for security.
        if self.board.set_output(self.cfg.down, false).await.is_err() {
            // Security - don't enable if can't disable the other one.
            return;
        }
        let _ = self.board.set_output(self.cfg.up, true).await;
    }

    /// Start movement DOWN.
    async fn go_down(&self) {
        // NOTE: Should not be needed. Just for security.
        if self.board.set_output(self.cfg.up, false).await.is_err() {
            // Security - don't enable if can't disable the other one.
            return;
        }
        let _ = self.board.set_output(self.cfg.down, true).await;
    }

    /// This is an universal state 'tick':
    /// - Update current state according to actions in progress.
    /// - Advance the action (finish, switch, do nothing).
    /// - Return the duration after which update should again be called.
    async fn update(&mut self, now: Instant) -> Duration {
        // Step I: Update tilt / height if we are in motion.
        let (tilt, elapsed) = self.consume_tilt(now);
        let height = self.consume_height(elapsed);
        info!(
            "Update: from h{}t{} -> h{}t{} delta h{}t{} residual tilt time {}ms",
            self.position.height,
            self.position.tilt,
            self.target.height,
            self.target.tilt,
            height,
            tilt,
            elapsed.as_millis(),
        );

        self.position.tilt = tilt;
        self.position.height = height;

        // Step II: Check for finishing currently pending actions or starting
        // new ones.
        match &self.action {
            Action::Idle | Action::Sleep => {
                // We are inactive, maybe a new action can be started if target
                // position is not reached yet.
                let height_diff = (self.target.height - self.position.height).abs();
                let tilt_diff = (self.target.tilt - self.position.tilt).abs();

                if height_diff > HYSTERESIS {
                    if self.target.height < self.position.height {
                        // We should move up.
                        info!("INIT: Idle -> Up (Height)");
                        self.action = Action::Up(now);
                        self.go_up().await;
                        // Return 0 to we got called again shortly and calculate proper time.
                        Duration::from_secs(0)
                    } else {
                        // We should move down.
                        info!("INIT: Idle -> Down (Height)");
                        self.action = Action::Down(now);
                        self.go_down().await;
                        Duration::from_secs(0)
                    }
                } else if tilt_diff > HYSTERESIS_TILT {
                    if self.target.tilt < self.position.tilt {
                        // Tilt is too high, we should move `up` to open the shutters angle.
                        info!("INIT: Idle -> Up (Tilt)");
                        self.action = Action::Up(now);
                        self.go_up().await;
                        Duration::from_secs(0)
                    } else {
                        // Tilt is too low (we are too open), move down a bit.
                        info!("INIT: Idle -> Down (Tilt)");
                        self.action = Action::Down(now);
                        self.go_down().await;
                        Duration::from_secs(0)
                    }
                } else {
                    // Nothing is happening and won't until we get new command -
                    // target position is reached.
                    info!("Idle/Sleep -> Sleep (10s) {:?}", self);
                    self.action = Action::Sleep;
                    NOOP_UPDATE_PERIOD
                }
            }
            Action::Cooldown(since) => {
                let elapsed = now.duration_since(*since);
                if elapsed >= COOLDOWN {
                    self.action = Action::Idle;
                    // We are inactive now and new action can be started.
                    Duration::from_secs(0)
                } else {
                    // Wait until the cooldown ends
                    COOLDOWN - elapsed
                }
            }
            Action::Up(_) => {
                // We are going UP - to a smaller height values and smaller tilt values.
                self.action = Action::Up(now);
                if self.position.height <= self.target.height {
                    // Height achieved! What about the tilt? In UP, the tilt decreases.
                    if self.position.tilt <= self.target.tilt {
                        // Tilt achieved! Stop movement.
                        self.go_idle().await;
                        self.action = Action::Cooldown(now);
                        COOLDOWN
                    } else {
                        // We're still in motion until the tilt is fine.
                        self.cfg.tilt_as_time(self.position.tilt, self.target.tilt)
                    }
                } else {
                    // The movement should continue.
                    self.cfg
                        .travel_as_time(self.position.height, self.target.height)
                }
            }
            Action::Down(_) => {
                // We are going DOWN - to a larger height values and larger tilt values.
                self.action = Action::Down(now);
                if self.position.height >= self.target.height {
                    // Height achieved! What about the tilt?
                    if self.position.tilt >= self.target.tilt {
                        // Tilt achieved! Stop movement.
                        self.go_idle().await;
                        self.action = Action::Cooldown(now);
                        COOLDOWN
                    } else {
                        // We're still in motion until the tilt is fine.
                        self.cfg.tilt_as_time(self.position.tilt, self.target.tilt)
                    }
                } else {
                    // The movement should continue.
                    self.cfg
                        .travel_as_time(self.position.height, self.target.height)
                }
            }
        }
    }

    /// Finish current action. Return Some(time to wait until it finishes) or
    /// None if we are idle. We assume positions are already updated.
    async fn finish(&mut self, now: Instant) {
        match &self.action {
            Action::Idle | Action::Sleep => {}
            Action::Cooldown(_) => { /* Update can finish a cooldown. We don't have to. */ }
            Action::Up(_) | Action::Down(_) => {
                self.go_idle().await;
                self.action = Action::Cooldown(now);
            }
        }
    }

    // Initiate new movement.
    async fn set_target(&mut self, now: Instant, target: Position) -> Duration {
        match self.action {
            Action::Sleep => {
                // Wake us up, so update() will get called soon.
                self.action = Action::Idle;
            }
            Action::Idle => { /* Ok */ }
            Action::Cooldown(_) => { /* Ok */ }
            _ => {
                panic!(
                    "Set target with Go action called when we're active {:?}. Finish first.",
                    self.action
                );
            }
        }
        self.target = target;
        self.update(now).await
    }

    /// Receives a command that starts/interrupts shutter state.
    async fn command(&mut self, cmd: Cmd, now: Instant) {
        // New command invalidates any previous ones.
        // TODO: Don't stop sending UP signal only to send it in a second?

        info!("Shutter command {:?} at state {:?}", cmd, self);
        if self.action != Action::Sleep {
            // Update state (our current position).
            self.update(now).await;
            // Finish previous movement... TODO: Or not? If the direction matches?
            self.finish(now).await;
        }

        info!("Shutter after finishing previous actions: {:?}", self);

        let target = match cmd {
            Cmd::Go(target) => target.as_position(),
            Cmd::Open => {
                if !self.in_sync {
                    // That's simplification
                    self.position = Position::new(100, 100);
                    self.in_sync = true;
                }
                Position::new_zero()
            }
            Cmd::Close => {
                if !self.in_sync {
                    self.position = Position::new_zero();
                    self.in_sync = true;
                }
                Position {
                    height: 100.0,
                    tilt: 100.0,
                }
            }

            Cmd::TiltClose => Position {
                height: self.position.height,
                tilt: 100.0,
            },
            Cmd::TiltOpen => Position {
                height: self.position.height,
                tilt: 0.0,
            },
            Cmd::TiltHalf => Position {
                height: self.position.height,
                tilt: 50.0,
            },
            Cmd::TiltReverse => Position {
                height: self.position.height,
                tilt: if self.position.tilt >= 50.0 {
                    0.0
                } else {
                    100.0
                },
            },
            Cmd::Tilt(tilt) => Position {
                height: self.position.height,
                tilt: tilt as f32,
            },
            Cmd::SetIO(down_idx, up_idx) => {
                assert_eq!(self.action, Action::Sleep);
                self.cfg.down = down_idx;
                self.cfg.up = up_idx;
                return;
            }
        };
        self.set_target(now, target).await;
    }
}

pub struct Manager {
    shutters: [Shutter; MAX_SHUTTERS],
}

impl Manager {
    pub fn new(board: &'static Board) -> Self {
        Self {
            shutters: [
                // Shutters start unconfigured, and can later be set dynamically with commands.
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
                Shutter::new(OutIdx::MAX, OutIdx::MAX, board),
            ],
        }
    }
}

pub type ShutterChannel = ector::DynamicAddress<(ShutterIdx, Cmd)>;

impl ector::Actor for Manager {
    type Message = (ShutterIdx, Cmd);

    async fn on_mount<M>(&mut self, _: ector::DynamicAddress<Self::Message>, mut inbox: M) -> !
    where
        M: ector::Inbox<Self::Message>,
    {
        loop {
            let mut min_duration = NOOP_UPDATE_PERIOD;
            let mut all_sleep = true;
            for shutter in self.shutters.iter_mut() {
                let duration = if shutter.action == Action::Sleep {
                    NOOP_UPDATE_PERIOD
                } else {
                    all_sleep = false;
                    shutter.update(Instant::now()).await
                };
                if duration < min_duration {
                    min_duration = duration;
                }
            }
            if !all_sleep && min_duration > UPDATE_PERIOD {
                // When something is happening the minimal state-update time is
                // UPDATE_PERIOD, not NOOP_UPDATE_PERIOD to update shutter state
                // correctly.
                min_duration = UPDATE_PERIOD;
            }
            if min_duration != NOOP_UPDATE_PERIOD {
                defmt::info!(
                    "Will wait for {:?}ms and revisit shutters",
                    min_duration.as_millis()
                );
            }
            let inbox_future = inbox.next();
            let max_time_future = Timer::after(min_duration);
            match select(inbox_future, max_time_future).await {
                Either::First((shutter_idx, cmd)) => {
                    defmt::info!("Shutter: cmd={:?} idx={:?}", cmd, shutter_idx);
                    let shutter = &mut self.shutters[shutter_idx as usize];
                    shutter.command(cmd, Instant::now()).await;
                }
                Either::Second(()) => {
                    // Timeout happened - Will rescan to see what needs an update.
                }
            }
        }
    }
}

// How to build only when cfg test?
/*

After simplification and migration from output queue to simply calling Board the
tests are invalid. I'd probably need a Board mock.

pub mod tests {
    use super::*;

    pub async fn single_shutter() {
        let channel = OutputChannel::new();
        let mut shutter = Shutter::new(1, 2, &channel);

        // Let's assume it thinks it's synced
        shutter.in_sync = true;

        defmt::info!("Initial test shutter: {:?}", shutter);
        assert_eq!(shutter.cfg.tilt_as_time(0, 100), shutter.cfg.tilt_time);
        assert_eq!(shutter.cfg.tilt_as_time(100, 0), shutter.cfg.tilt_time);
        assert_eq!(shutter.cfg.tilt_as_time(50, 0), shutter.cfg.tilt_time / 2);
        assert_eq!(shutter.cfg.tilt_as_time(0, 50), shutter.cfg.tilt_time / 2);

        assert_eq!(
            shutter.cfg.travel_as_time(0, 100), // down
            shutter.cfg.drop_time
        );
        assert_eq!(
            shutter.cfg.travel_as_time(100, 0), // down
            shutter.cfg.rise_time
        );
        assert_eq!(shutter.action, Action::Idle);
        assert!(channel.try_receive().is_err());

        // It's already up, should be a noop and no commands sent.
        let mut now = Instant::now();
        shutter.command(Cmd::Open, now).await;
        assert_eq!(shutter.action, Action::Idle);
        assert!(channel.try_receive().is_err());

        // Closing will make it go down.
        shutter.command(Cmd::Close, now).await;
        assert_eq!(shutter.action, Action::Down(now));

        // Cleanup output queue.
        assert!(channel.try_receive().is_ok());
        assert!(channel.try_receive().is_ok());

        // Nothing should change. Time didn't pass.
        shutter.update(now).await;
        assert_eq!(shutter.position.tilt, 0);
        assert_eq!(shutter.position.height, 0);

        now += shutter.cfg.tilt_time;

        shutter.update(now).await;
        assert_eq!(shutter.position.tilt, 100);
        assert_eq!(shutter.position.height, 0);

        // Let's wait 50% of drop time.
        now += shutter.cfg.drop_time / 2;

        shutter.update(now).await;
        assert_eq!(shutter.position.tilt, 100);
        assert_eq!(shutter.position.height, 50);
        assert_ne!(shutter.action, Action::Idle);
        assert_ne!(
            core::mem::discriminant(&shutter.action),
            core::mem::discriminant(&Action::Cooldown(now))
        );
        assert!(channel.try_receive().is_err());

        // Let's wait another 50% of time.
        now += shutter.cfg.drop_time / 2;

        shutter.update(now).await;
        assert_eq!(shutter.position.tilt, 100);
        assert_eq!(shutter.position.height, 100);
        // Idle commands were sent.
        assert!(channel.try_receive().is_ok());
        assert!(channel.try_receive().is_ok());

        let cooldown_start = now;
        assert_eq!(shutter.action, Action::Cooldown(cooldown_start));

        // Finish half of a cooldown period.
        now += COOLDOWN / 2;
        shutter.update(now).await;
        assert_eq!(shutter.position.tilt, 100);
        assert_eq!(shutter.position.height, 100);
        assert!(channel.try_receive().is_err());
        assert_eq!(shutter.action, Action::Cooldown(cooldown_start));

        // Move to 50% height, but set tilt to 45deg. Still half cooldown period to go.
        shutter.command(Cmd::Go(Position::new(50, 50)), now).await;
        assert_eq!(shutter.action, Action::Cooldown(cooldown_start));

        // Still idle after rest of cooldown.
        now += COOLDOWN / 2;
        shutter.update(now).await;
        assert_eq!(shutter.action, Action::Idle);

        // Will immediately start motion if Idle.
        shutter.update(now).await;
        assert_eq!(shutter.action, Action::Up(now));
        assert!(channel.try_receive().is_ok());
        assert!(channel.try_receive().is_ok());
        assert_eq!(shutter.position.tilt, 100);
        assert_eq!(shutter.position.height, 100);

        // First we consume tilt.
        now += shutter.cfg.tilt_time;
        shutter.update(now).await;
        assert_eq!(shutter.position.tilt, 0);
        assert_eq!(shutter.position.height, 100);
        info!("Should be tilted {:?}", shutter);

        // Should reach height and switch to cooldown.
        now += shutter.cfg.rise_time / 2;
        shutter.update(now).await;
        info!("Should be in the middle {:?}", shutter);
        assert_eq!(shutter.action, Action::Cooldown(now));
        assert!(channel.try_receive().is_ok());
        assert!(channel.try_receive().is_ok());

        now += COOLDOWN;
        let time = shutter.update(now).await;
        assert_eq!(time, Duration::from_millis(0));
        assert_eq!(shutter.action, Action::Idle);
        info!("Should be idle {:?}", shutter);

        // Immediately after cooldown, should go down to close tilt.
        let start_time = shutter.update(now).await;
        assert_eq!(start_time, Duration::from_millis(0));
        assert_eq!(shutter.action, Action::Down(now));
        assert!(channel.try_receive().is_ok());
        assert!(channel.try_receive().is_ok());

        // Next update (same moment) has proper time.
        let time = shutter.update(now).await;
        assert_eq!(time, shutter.cfg.tilt_time / 2);

        now += time;
        let time = shutter.update(now).await;
        assert!(channel.try_receive().is_ok());
        assert!(channel.try_receive().is_ok());
        assert_eq!(time, COOLDOWN);
        assert_eq!(shutter.action, Action::Cooldown(now));
        assert_eq!(shutter.position.tilt, 50);
        assert_eq!(shutter.position.height, 50);

        now += COOLDOWN;
        shutter.update(now).await;
        assert_eq!(shutter.action, Action::Idle);
        assert!(channel.try_receive().is_err());
    }
}
*/
