/*
 * Requirements / use cases:
 * - Estimate position and track synchronization status.
 * - Interruptible. If we are going down, and someones sends different command - stop motion.
 * - Report state changes during movement.
 */
use ector;
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};

use crate::boards::{IOCommand, OutputChannel};
use crate::buttonsmash::consts::{OutIdx, ShutterIdx};
use crate::config::MAX_SHUTTERS;

use defmt::Format;
use defmt::{info, error};

// TODO: Maybe that should be time hysteresis for both cases?
/// Accuracy of position that's considered good enough. In percentage points.
const HYSTERESIS: u8 = 5;
/// Accuracy of tilt position.
const HYSTERESIS_TILT: u8 = 15;
/// Time after movement stops before we can start another one.
const COOLDOWN: Duration = Duration::from_millis(500);
/// When in motion, how often should we report position change.
const UPDATE_PERIOD: u32 = 1000;

/// Internal commands handled by a shutter driver.
#[derive(Format)]
enum Cmd {
    /// Full analog control: change height and tilt to given values 0-100.
    /// This is a two-step operation: ride + tilt.
    Go(Position),

    /// Uncover/open completely. Tilt time + rise time + over_time up.
    Open,
    /// Cover/close completely. Tilt time + drop time + over_time down.
    Close,

    /// Keep height and change tilt to given 0-100.
    Tilt(u8),

    // Tilt helpers.
    /// Tilt(100) - completely closed.
    TiltClose,
    // Tilt(0) - completely open.
    TiltOpen,
    // 45 deg.
    TiltHalf,
    /// Open if not completely open; otherwise - close.
    TiltReverse,
}

/// Current or planned shutter position.
#[derive(Format, Debug, Clone, Eq, PartialEq)]
struct Position {
    // TODO: Height/Tilt should be an Enum - Known / Guessed. To mark when the position is not synchronized.
    /// Position of shutters. 0 (open) - 100% (closed)
    height: u8,
    /// 0 (open) - 100% (closed)
    tilt: u8,
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
    /// Nothing is happening
    Idle,

    /// Currently moving up since Instant to open or decrease tilt.
    Up(Instant),

    /// Currently moving down since Instant to close or increase tilt.
    Down(Instant),

    /// Waiting between changing directions
    Cooldown(Instant),
}

/// Single shutter parameters.
pub struct Shutter<'a> {
    /// Output channel for commands
    output_channel: &'a OutputChannel,
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

impl Format for Shutter<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Shutter {{pos={:?} target={:?} action={:?}}}",
                      self.position, self.target, self.action);
    }
}

impl Config {
    pub fn new(up: OutIdx, down: OutIdx) -> Self {
        Self {
            up,
            down,
            rise_time: Duration::from_secs(60),
            drop_time: Duration::from_secs(60),
            tilt_time: Duration::from_millis(500),
            over_time: Duration::from_secs(2),
        }
    }

    /// Time it will take to move from position to position.
    fn travel_as_time(&self, from: u8, to: u8) -> Duration {
        // 0% - open, 100% - closed
        // from > to -> down.
        let cost = if from > to {
            self.drop_time
        } else {
            self.rise_time
        }.as_millis() as u64;

        let diff = from.abs_diff(to) as u64;
        return Duration::from_millis(cost * diff / 100);
    }

    /// Time it will take to tilt.
    fn tilt_as_time(&self, from: u8, to: u8) -> Duration {
        let change = from.abs_diff(to) as u64;
        return Duration::from_millis(self.tilt_time.as_millis() * change / 100);
    }

    /// How much tilted in given time.
    fn time_as_tilt(&self, elapsed: Duration) -> u8 {
        let tilt: u64 = 100 * elapsed.as_millis() / self.tilt_time.as_millis();
        let tilt = tilt.clamp(0, 100);
        tilt as u8
    }

    fn time_as_travel(&self, dir: i8, elapsed: Duration) -> u8 {
        let movement = match dir {
            // Going down (towards higher height)
            1 => {
                100 * elapsed.as_millis() / self.drop_time.as_millis()
            },
            // Going up (towards lower height)
            -1 => {
                100 * elapsed.as_millis() / self.rise_time.as_millis()
            }
            _ => {
                // TODO: enum?
                panic!("Bad direction argument");
            }
        };
        movement.clamp(0, 100) as u8
    }
}

impl Position {
    pub fn new(height: u8, tilt: u8) -> Self {
        assert!(height <= 100);
        assert!(tilt <= 100);
        Self { height, tilt }
    }

    pub fn new_zero() -> Self {
        Self { height: 0, tilt: 0 }
    }
}

impl<'a> Shutter<'a> {
    pub fn new(up: OutIdx, down: OutIdx, output_channel: &'a OutputChannel) -> Self {
        Self {
            output_channel,
            cfg: Config::new(up, down),
            position: Position::new_zero(),
            target: Position::new_zero(),
            action: Action::Idle,
            in_sync: false,
        }
    }

    /// We want to tilt from start position to the target one, and some time passed.
    /// Return current tilt (movement in one direction for x ms) and residual ms
    /// time that changed the height.
    /// Returns (current tilt, rest of time for consumption)
    fn consume_tilt(&mut self, now: Instant) -> (u8, Duration) {
        let (since, dir, max_tilt) = match self.action {
            Action::Up(since) => {
                // Up, opens. Towards 0.
                (since, -1, 0)
            }
            Action::Down(since) => {
                // Down closes, towards 100.
                (since, 1, 100)
            }
            _ => {
                // Nothing will change
                return (self.position.tilt, Duration::from_secs(0));
            }
        };

        let max_time = self.cfg.tilt_as_time(self.position.tilt, max_tilt);
        let elapsed = now.duration_since(since);

        if elapsed >= max_time {
            // We reached the final tilt in max_time
            return (max_tilt, elapsed - max_time);
        } else {
            // We are within the tilt movement still.

            let tilted = self.cfg.time_as_tilt(elapsed);
            let consumed_time = self.cfg.tilt_as_time(0, tilted);
            assert!(tilted < 100); // from other limit
            let mut tilt = self.position.tilt as i32;
            tilt += dir as i32 * self.cfg.time_as_tilt(elapsed) as i32;
            assert!(tilt >= 0 && tilt <= 100);
            return (tilt as u8, elapsed - consumed_time)
        }
    }

    // Consume time for movement. Tilt should be calculated first.
    fn consume_height(&self, elapsed: Duration) -> u8 {
        let (dir, _max_tilt, _conf_time) = match self.action {
            Action::Up(_since) => {
                // Up, opens. Towards 0.
                (-1i64, 0, self.cfg.rise_time.as_millis())
            }
            Action::Down(_since) => {
                // Down closes, towards 100.
                (1, 100, self.cfg.drop_time.as_millis())
            }
            _ => {
                // Nothing will change
                return self.position.height;
            }
        };

        let height_delta = dir * self.cfg.time_as_travel(dir as i8, elapsed) as i64;

        let mut height: i64 = self.position.height.into();
        height += height_delta;
        height = height.clamp(0, 100);
        height as u8
    }

    /// Stop movement.
    async fn go_idle(&self) {
        self.output_channel
            .send(IOCommand::DeactivateOutput(self.cfg.up))
            .await;

        self.output_channel
            .send(IOCommand::DeactivateOutput(self.cfg.down))
            .await;
    }

    /// Start movement UP.
    async fn go_up(&self) {
        // NOTE: Should not be needed. Just for security.
        self.output_channel
            .send(IOCommand::DeactivateOutput(self.cfg.down))
            .await;

        self.output_channel
            .send(IOCommand::ActivateOutput(self.cfg.up))
            .await;
    }

    /// Start movement DOWN.
    async fn go_down(&self) {
        // NOTE: Should not be needed. Just for security.
        self.output_channel
            .send(IOCommand::DeactivateOutput(self.cfg.up))
            .await;

        self.output_channel
            .send(IOCommand::ActivateOutput(self.cfg.down))
            .await;
    }

    /// This is an universal state 'tick':
    /// - Update current state according to actions in progress.
    /// - Advance the action (finish, switch, do nothing).
    /// - Return the duration after which update should again be called.
    async fn update(&mut self, now: Instant) -> Duration {
        // Step I: Update tilt / height if we are in motion.
        let (tilt, elapsed) = self.consume_tilt(now);
        let height = self.consume_height(elapsed);
        info!("Update: from h{}t{} -> h{}t{} delta h{}t{} in {}",
              self.position.height, self.position.tilt,
              self.target.height, self.target.tilt,
              tilt, height, elapsed);

        self.position.tilt = tilt;
        self.position.height = height;

        // Step II: Check for finishing currently pending actions or starting
        // new ones.
        match &self.action {
            Action::Idle => {
                // We are inactive, and new action can be started.
                let height_diff = self.target.height.abs_diff(self.position.height);
                let tilt_diff = self.target.tilt.abs_diff(self.position.tilt);

                if height_diff > HYSTERESIS {
                    if self.target.height < self.position.height {
                        // We should move up.
                        info!("INIT: Idle -> Up (Height)");
                        self.action = Action::Up(now);
                        self.go_up().await;
                        // Return 0 to we got called again shortly and calculate proper time.
                        return Duration::from_secs(0);
                    } else {
                        // We should move down.
                        info!("INIT: Idle -> Down (Height)");
                        self.action = Action::Down(now);
                        self.go_down().await;
                        return Duration::from_secs(0);
                    }
                } else if tilt_diff > HYSTERESIS_TILT {
                    if self.target.tilt < self.position.tilt {
                        // Tilt is too high, we should move `up` to open the shutters angle.
                        info!("INIT: Idle -> Up (Tilt)");
                        self.action = Action::Up(now);
                        self.go_up().await;
                        return Duration::from_secs(0);
                    } else {
                        // Tilt is too low (we are too open), move down a bit.
                        info!("INIT: Idle -> Down (Tilt)");
                        self.action = Action::Down(now);
                        self.go_down().await;
                        return Duration::from_secs(0);
                    }
                } else {
                    // Nothing is happening.
                    info!("Idle -> Idle (10s)");
                    return Duration::from_secs(10);
                }
            }
            Action::Cooldown(since) => {
                let elapsed = now.duration_since(*since);
                if elapsed >= COOLDOWN {
                    self.action = Action::Idle;
                    // We are inactive now and new action can be started.
                    return Duration::from_secs(0);
                } else {
                    // Wait until the cooldown ends
                    return COOLDOWN - elapsed;
                }
            }
            Action::Up(_) => {
                // We are going UP - to a smaller height values and smaller tilt values.
                if self.position.height <= self.target.height {
                    // Height achieved! What about the tilt? In UP, the tilt decreases.
                    if self.position.tilt <= self.target.tilt {
                        // Tilt achieved! Stop movement.
                        self.go_idle().await;
                        self.action = Action::Cooldown(now);
                        return COOLDOWN;
                    } else {
                        // We're still in motion until the tilt is fine.
                        return self.cfg.tilt_as_time(self.position.tilt, self.target.tilt);
                    }
                } else {
                    // The movement should continue.
                    return self.cfg.travel_as_time(self.position.height, self.target.height)
                }
            }
            Action::Down(_) => {
                // We are going DOWN - to a larger height values and larger tilt values.
                if self.position.height >= self.target.height {
                    // Height achieved! What about the tilt?
                    if self.position.tilt >= self.target.tilt {
                        // Tilt achieved! Stop movement.
                        self.go_idle().await;
                        self.action = Action::Cooldown(now);
                        return COOLDOWN;
                    } else {
                        // We're still in motion until the tilt is fine.
                        return self.cfg.tilt_as_time(self.position.tilt, self.target.tilt);
                    }
                } else {
                    // The movement should continue.
                    return self.cfg.travel_as_time(self.position.height, self.target.height)
                }
            }
        }
    }

    /// Finish current action. Return Some(time to wait until it finishes) or
    /// None if we are idle. We assume positions are already updated.
    async fn finish(&mut self, now: Instant) {
        match &self.action {
            Action::Idle => {}
            Action::Cooldown(_) => {
                /* Update can finish a cooldown. We don't have to. */
            }
            Action::Up(_) | Action::Down(_) => {
                self.go_idle().await;
                self.action = Action::Cooldown(now);
            }
        }
    }

    // Initiate new movement.
    async fn set_target(&mut self, now: Instant, target: Position) -> Duration {
        match self.action {
            Action::Idle => { /* Ok */ },
            Action::Cooldown(_) => { /* Ok */ },
            _ => {
                panic!("Go action called when we're active {:?}. Finish first.", self.action);
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
        // Update state (our current position).
        self.update(now).await;
        // Finish previous movement... TODO: Or not? If the direction matches?
        self.finish(now).await;

        info!("Shutter after finishing previous actions: {:?}", self);

        let target = match cmd {
            Cmd::Go(target) => {
                target
            }
            Cmd::Open => {
                if !self.in_sync {
                    // That's simplification
                    self.position = Position::new(100, 100);
                    self.in_sync = true;
                }
                Position {
                    height: 0,
                    tilt: 0,
                }
            }
            Cmd::Close => {
                if !self.in_sync {
                    self.position = Position::new(0, 0);
                    self.in_sync = true;
                }
                Position {
                    height: 100,
                    tilt: 100,
                }
            }

            Cmd::TiltClose => {
                Position {
                    height: self.position.height,
                    tilt: 100,
                }
            }
            Cmd::TiltOpen => {
                Position {
                    height: self.position.height,
                    tilt: 0,
                }
            }
            Cmd::TiltHalf => {
                Position {
                    height: self.position.height,
                    tilt: 50,
                }
            }
            Cmd::TiltReverse => {
                Position {
                    height: self.position.height,
                    tilt: if self.position.tilt > 0 { 0 } else { 100 },
                }
            }
            Cmd::Tilt(tilt) => {
                Position {
                    height: self.position.height,
                    tilt,
                }
            }
        };
        self.set_target(now, target).await;
    }
}

struct Manager<'a> {
    output_channel: &'a OutputChannel,
    shutters: [Shutter<'a>; MAX_SHUTTERS],
}

impl<'a> Manager<'a> {
    async fn command(&mut self, cmd: Cmd, shutter: ShutterIdx) {
        // New command invalidates the previous ones.
        let now = Instant::now();

        self.shutters[shutter as usize].command(cmd, now).await;
    }
}

impl ector::Actor for Manager<'static> {
    type Message = (Cmd, ShutterIdx);

    async fn on_mount<M>(&mut self, _: ector::DynamicAddress<Self::Message>, mut inbox: M) -> !
    where
        M: ector::Inbox<Self::Message>,
    {
        let schedules: heapless::Vec<Instant, 8> = heapless::Vec::new();

        loop {
            let inbox_future = inbox.next();
            let max_time_future = Timer::after(Duration::from_millis(20));
            match select(inbox_future, max_time_future).await {
                Either::First((cmd, shutter_idx)) => {
                    defmt::info!("Shutter: {:?} {:?}", cmd, shutter_idx);
                }
                Either::Second(()) => {
                    // TODO Something needs disabling
                }
            }
        }
    }
}

// How to build only when cfg test?
pub mod tests {
    use super::*;
    use crate::boards::OutputChannel;

    pub async fn single_shutter() {

        let channel = OutputChannel::new();
        let mut shutter = Shutter::new(1, 2, &channel);

        // Let's assume it thinks it's synced
        shutter.in_sync = true;

        defmt::info!("Initial test shutter: {:?}", shutter);
        assert_eq!(shutter.cfg.tilt_as_time(0, 100),
                   shutter.cfg.tilt_time);
        assert_eq!(shutter.cfg.tilt_as_time(100, 0),
                   shutter.cfg.tilt_time);
        assert_eq!(shutter.cfg.tilt_as_time(50, 0),
                   shutter.cfg.tilt_time / 2);
        assert_eq!(shutter.cfg.tilt_as_time(0, 50),
                   shutter.cfg.tilt_time / 2);

        assert_eq!(shutter.cfg.travel_as_time(0, 100), // down
                   shutter.cfg.drop_time);
        assert_eq!(shutter.cfg.travel_as_time(100, 0), // down
                   shutter.cfg.rise_time);
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
        assert_ne!(core::mem::discriminant(&shutter.action),
                   core::mem::discriminant(&Action::Cooldown(now)));
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
