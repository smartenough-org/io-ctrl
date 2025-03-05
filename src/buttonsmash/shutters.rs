use ector;
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};

use crate::boards::{IOCommand, OutputChannel};
use crate::buttonsmash::consts::{OutIdx, ShutterIdx};
use crate::config::MAX_SHUTTERS;

use defmt::Format;

const MAX_TILT: u8 = 90;

#[derive(Format)]
enum Cmd {
    Lift,
    Drop,
    Go(u8),

    TiltClose,
    TiltOpen,
    TiltHalf,
}

#[derive(Format, Clone)]
struct Position {
    /// Position of shutters. 0 (open) - 100% (closed)
    height: u8,
    /// 0 (open) - 90 (closed)
    tilt: u8,
}

pub struct Config {
    /// Output to raise the shutter.
    pub up: OutIdx,
    /// Output to lower the shutter.
    pub down: OutIdx,

    /// Time it takes to raise the shutter completely [ms].
    pub rise_time: u32,
    /// Time it takes to lower completely [ms].
    pub drop_time: u32,
    /// Time it takes to tilt the shutter between open/close positions when
    /// switching directions.
    pub tilt_time: u32,

    /// When reaching 0 or 100% how much time to spend on the limit switch to
    /// synchronize position information.
    pub over_time: u32,
}

enum Action {
    /// Nothing is happening
    Inactive,
    /// We are going up since `Instant` starting from State
    Up(Duration, Position, Instant),
    /// We are going down since `Instant`.
    Down(Duration, Position, Instant),
}

pub struct Shutter<'a> {
    output_channel: &'a OutputChannel,
    cfg: Config,
    position: Position,
    action: Action,
}

impl Config {
    pub fn new(up: OutIdx, down: OutIdx) -> Self {
        Self {
            up,
            down,
            rise_time: 60 * 1000,
            drop_time: 60 * 1000,
            tilt_time: 500,
            over_time: 2 * 1000,
        }
    }
}

impl Position {
    pub fn new() -> Self {
        Self { height: 0, tilt: 0 }
    }
}

impl<'a> Shutter<'a> {
    pub fn new(up: OutIdx, down: OutIdx, output_channel: &'a OutputChannel) -> Self {
        Self {
            output_channel,
            cfg: Config::new(up, down),
            position: Position { height: 0, tilt: 0 },
            action: Action::Inactive,
        }
    }

    /// Update current state according to actions in progress.
    /// ie. either leave as is, or calculate from action starting position.
    async fn update_state(&mut self) {
        let now = Instant::now();
        let tilt_time = self.cfg.tilt_time as i32;
        let (direction, initial_pos, tilt_cost, since, full_time) = match &self.action {
            Action::Inactive => return,
            Action::Up(_duration, initial_pos, since) => {
                // Tilt decreases when going up, so we pay cost if it was closed.
                let tilt_cost = initial_pos.tilt as i32 * tilt_time as i32 / MAX_TILT as i32;
                (-1, initial_pos, tilt_cost, since, self.cfg.rise_time as i32)
            }
            Action::Down(_duration, initial_pos, since) => {
                // Tilt increases when going down, so we pay cost if it was open.
                let tilt_cost = (MAX_TILT - initial_pos.tilt) as i32 * tilt_time / MAX_TILT as i32;
                (1, initial_pos, tilt_cost, since, self.cfg.drop_time as i32)
            }
        };
        // TODO: Handle tilt. It should eat some time, but only in certain cases.
        let mut in_motion_ms = now.duration_since(*since).as_millis() as i32;
        if in_motion_ms > tilt_cost {
            // We tilted and are changing height now.
            if direction == -1 {
                self.position.tilt = 0;
            } else {
                self.position.tilt = MAX_TILT;
            }
            in_motion_ms -= tilt_cost;
        } else {
            // Only tilt changed.
            let tilted = direction * MAX_TILT as i32 * in_motion_ms / tilt_time;
            self.position.tilt += tilted as u8;
        }
        let prcnt_moved: i32 = direction * 100 * in_motion_ms / full_time;
        let position = initial_pos.height as i32 + prcnt_moved;
        let position: u8 = position.clamp(0, 100) as u8;
        self.position.height = position;
    }

    /// Finish current action.
    async fn finish(&mut self) {
        match &self.action {
            Action::Inactive => {}
            Action::Up(_duration, _pos, _since) => {
                self.output_channel
                    .send(IOCommand::DeactivateOutput(self.cfg.up))
                    .await;
            }
            Action::Down(_duration, _pos, _since) => {
                self.output_channel
                    .send(IOCommand::DeactivateOutput(self.cfg.down))
                    .await;
            }
        }
        self.update_state().await;
        self.action = Action::Inactive;
    }

    async fn handle_command(&mut self, cmd: Cmd) {
        // New command invalidates the previous ones.

        match cmd {
            Cmd::Lift => {
                // Calculate time to go up.
            }

            Cmd::Drop => {}

            Cmd::Go(position) => {}

            Cmd::TiltClose => {}

            Cmd::TiltOpen => {}

            Cmd::TiltHalf => {}
        }
    }
}

struct Manager<'a> {
    output_channel: &'a OutputChannel,
    shutters: [Shutter<'a>; MAX_SHUTTERS],
}

impl<'a> Manager<'a> {
    async fn handle_command(&mut self, cmd: Cmd, shutter: ShutterIdx) {
        // New command invalidates the previous ones.

        self.shutters[shutter as usize].handle_command(cmd).await;
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

    pub fn it_builds() {
        let channel = OutputChannel::new();
        let shutter = Shutter::new(1, 2, &channel);
        defmt::info!("Hello, log!");
        assert!(true)
    }
}
