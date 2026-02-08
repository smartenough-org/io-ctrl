use crate::buttonsmash::shutters;
use crate::components::interconnect::WhenFull;
use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::rtc::{DateTime, DayOfWeek};
use embassy_stm32::uid;
use embassy_time::{Duration, Instant, Timer};
use static_cell::StaticCell;

use crate::boards::ctrl_board::Board;
use crate::components::message::{Message, args};
use crate::components::status;

use crate::buttonsmash::consts::BINDINGS_COUNT;
use crate::buttonsmash::{Event, EventChannel, Executor, Opcode};
use crate::config;
use crate::io::event_converter::run_event_converter;

/// High-level command queue that are consumed by executor.
static EVENT_CHANNEL: EventChannel = EventChannel::new();
static EXECUTOR: StaticCell<Executor<BINDINGS_COUNT>> = StaticCell::new();

/// Main application/business logic entrypoint.
pub struct CtrlApp {
    /// For all IO needs (and comm peripherals like CAN and USB)
    pub board: &'static Board,
    pub shutters: shutters::ShutterChannel,
    pub executor: Option<&'static mut Executor<BINDINGS_COUNT>>,
}

impl CtrlApp {
    /// Create CtrlApp as much as you can, but watchout for growing too large.
    /// When used with .awaits, the future grew once to 5kB. That's why it's
    /// split currently between new, configure, spawn_tasks and uses statics.
    pub fn new(board: &'static Board, spawner: &Spawner) -> Self {
        let shutters_channel: shutters::ShutterChannel = ector::actor!(
            spawner,
            shutters,
            shutters::Manager,
            shutters::Manager::new(board)
        )
        .into();

        let executor = EXECUTOR.init(Executor::new(board, shutters_channel));
        Self {
            board,
            executor: Some(executor),
            shutters: shutters_channel,
        }
    }

    pub fn spawn_tasks(&mut self, spawner: &Spawner) {
        // let executor = unsafe { &mut *self.executor.get() };
        let executor = self.executor.take().expect("This needs to be defined");
        spawner.spawn(unwrap!(task_pump_switch_events_to_microvm(executor)));
        spawner.spawn(unwrap!(run_event_converter(
            self.board.input_q,
            &EVENT_CHANNEL
        )));
        spawner.spawn(unwrap!(task_read_interconnect(self.board, self.shutters)));
    }

    /// Returns hard-configured Executor. TODO: This is temporary. Code should
    /// be programmable and read from flash on start.
    pub async fn configure(&mut self) {
        const PROGRAM: [Opcode; 34] = [
            // Setup proc.
            Opcode::Start(0),
            // Basic usable program for initial setup.
            Opcode::LayerDefault,
            Opcode::BindShortToggle(1, 1),
            // Opcode::BindShortCall(1, 1), // Testing shutters via procedure 1.
            Opcode::BindShortToggle(2, 2),
            Opcode::BindShortToggle(3, 3),
            Opcode::BindShortToggle(4, 4),
            Opcode::BindShortToggle(5, 5),
            Opcode::BindShortToggle(6, 6),
            Opcode::BindShortToggle(7, 7),
            Opcode::BindShortToggle(8, 8),
            Opcode::BindShortToggle(9, 9),
            Opcode::BindShortToggle(10, 10),
            Opcode::BindShortToggle(11, 11),
            Opcode::BindShortToggle(12, 12),
            Opcode::BindShortToggle(13, 13),
            Opcode::BindShortToggle(14, 14),
            Opcode::BindShortToggle(15, 15),
            Opcode::BindShortToggle(16, 16),
            // Configure shutter down/up. Don't use unconfigured shutters.
            Opcode::BindShutter(0, 13, 14),
            Opcode::BindShutter(1, 15, 16),
            // Opcode::BindLongActivate(1, 2),

            // Send the complete status on initialization.
            Opcode::SendStatus,
            Opcode::Stop,
            /*
            Opcode::BindShortToggle(1, 10),
            Opcode::BindShortToggle(2, 11),
            Opcode::BindLongToggle(3, 20),
            Opcode::BindShortToggle(3, 21),
            Opcode::BindShortCall(4, 1),
            Opcode::BindLayerHold(5, 66),
            Opcode::LayerPush(66),
            Opcode::BindShortToggle(1, 13),
            */
            Opcode::Stop,

            // Shutter control - Tilt.
            Opcode::Start(1),
            Opcode::ShutterCmd(0, shutters::Cmd::TiltReverse),
            Opcode::Stop,

            // Test procedure 2
            Opcode::Start(2),
            Opcode::Activate(100),
            Opcode::Activate(101),
            Opcode::Deactivate(110),
            Opcode::Stop,
            // Test procedure 3.
            Opcode::Start(3),
            Opcode::Noop,
            Opcode::Stop,
        ];

        let executor = self.executor.take().expect("This needs to be defined");
        executor.load_static(&PROGRAM).await;
        self.executor = Some(executor);
    }

    pub async fn main(&'static mut self) -> ! {
        defmt::info!("Starting app on chip {}", uid::uid());

        let welcome_message = Message::Info {
            code: args::InfoCode::Started.to_bytes(),
            arg: 0,
        };

        if !self.board.init_outputs().await.is_ok() {
            defmt::info!("Error while initializing outputs. Expander error?");
        }

        if !self
            .board
            .interconnect
            .transmit_response(&welcome_message, WhenFull::Wait)
            .await
        {
            defmt::info!("Unable to schedule sent of initial CAN message");
        }

        let mut cnt = 0;
        let mut last_tick = Instant::now();

        if cfg!(feature = "deep-sleep") {
            loop {
                // Prevent deep sleep to allow easy remote debugging.
                // TODO: Remove for production.
                Timer::after(Duration::from_secs(10)).await;
                defmt::info!("Tick: {:?}", status::COUNTERS);
            }
        } else {
            loop {
                Timer::after(Duration::from_millis(1)).await;
                cnt += 1;
                if cnt == 300 {
                    let now = Instant::now();
                    let passed = (now - last_tick).as_millis();
                    if passed > 10000 {
                        defmt::info!("Tick: {:?}", status::COUNTERS);
                        last_tick = now;
                    }
                    cnt = 0;
                }
            }
            // embassy_futures::yield_now().await;

            /*
            let ir_reg = pac::FDCAN1.ir().read();
            let cccr_reg = pac::FDCAN1.cccr().read();
            let psr_reg = pac::FDCAN1.cccr().read();

            defmt::info!("APP cccr={:b} DAR={} init={} | ir={:b} psr={:b} pea={} ped={} bo={} ew={} ep={} tcf={} mraf={}",
                         cccr_reg.0, cccr_reg.dar(), cccr_reg.init(),

                         ir_reg.0, psr_reg.0, ir_reg.pea(), ir_reg.ped(), ir_reg.bo(),
                         ir_reg.ew(), ir_reg.ep(), ir_reg.tcf(), ir_reg.mraf());
            */
        }
    }
}

#[embassy_executor::task(pool_size = 1)]
pub async fn task_pump_switch_events_to_microvm(executor: &'static mut Executor<BINDINGS_COUNT>) {
    executor.listen_events(&EVENT_CHANNEL).await;
}

#[embassy_executor::task(pool_size = 1)]
pub async fn task_read_interconnect(
    board: &'static Board,
    shutters_channel: shutters::ShutterChannel,
) {
    loop {
        let raw = board.interconnect.receive().await;
        defmt::info!("Received raw message {}", raw);

        // CAN level parsing.
        let raw = if let Ok(raw) = raw {
            raw
        } else {
            // Error in frame. Duhno how to handle. Might need hard restart maybe?
            status::COUNTERS.can_frame_error.inc();
            continue;
        };

        // Semantic message parsing.
        let message = if let Some(message) = Message::from_raw(&raw) {
            message
        } else {
            defmt::warn!("Error while reading a message {:?}", raw);
            continue;
        };

        // Are we the addressee?
        let to_us = match raw.addr_type().0 {
            config::LOCAL_ADDRESS => {
                defmt::warn!("Message is addressed to us - {}", config::LOCAL_ADDRESS);
                true
            }
            config::BROADCAST_ADDRESS => {
                defmt::warn!(
                    "Message is addressed to broadcast {}.",
                    config::BROADCAST_ADDRESS
                );
                true
            }
            addr => {
                defmt::warn!(
                    "Message is not addressed to us. (addr {} != local {})",
                    addr,
                    config::LOCAL_ADDRESS
                );
                false
            }
        };

        match message {
            Message::CallProcedure { proc_id } => {
                if !to_us {
                    continue;
                }
                defmt::warn!("TODO: Call procedure {}", proc_id);
            }

            Message::TriggerInput { input, trigger } => {
                if !to_us {
                    continue;
                }
                defmt::warn!("TODO: Emulate input trigger {} as {:?}", input, trigger);
            }

            Message::SetOutput { output, state } => {
                if !to_us {
                    continue;
                }
                let event = match state {
                    args::OutputChangeRequest::On => Event::RemoteActivate(output),
                    args::OutputChangeRequest::Off => Event::RemoteDeactivate(output),
                    args::OutputChangeRequest::Toggle => Event::RemoteToggle(output),
                };
                defmt::warn!("Trigger output {} to {:?} -> {:?}", output, state, event);
                EVENT_CHANNEL.send(event).await;
            }

            Message::TimeAnnouncement {
                year,
                month,
                day,
                hour,
                minute,
                second,
                day_of_week,
            } => {
                // This one is a broadcast. We don't send those.
                if to_us {
                    defmt::warn!("Message error. TimeAnnouncement sent... from us?");
                    continue;
                }
                let dow = match day_of_week {
                    0 => DayOfWeek::Monday,
                    1 => DayOfWeek::Tuesday,
                    2 => DayOfWeek::Wednesday,
                    3 => DayOfWeek::Thursday,
                    4 => DayOfWeek::Friday,
                    5 => DayOfWeek::Saturday,
                    6 => DayOfWeek::Sunday,
                    _ => {
                        defmt::warn!(
                            "Invalid date of week specified in time announcement {}",
                            day_of_week
                        );
                        continue;
                    }
                };
                let dt = DateTime::from(year, month, day, dow, hour, minute, second, 0);

                match dt {
                    Ok(dt) => {
                        if board.set_time(dt).await.is_err() {
                            defmt::error!("RTC returned an error - unable to set time");
                        } else {
                            defmt::info!("Time was set.");
                        }
                    }
                    Err(_err) => {
                        defmt::error!(
                            "Unable to decode time from {}-{}-{} {} {}:{}:{}.",
                            year,
                            month,
                            day,
                            day_of_week,
                            hour,
                            minute,
                            second
                        );
                    }
                }
            }
            Message::ShutterCmd { shutter_idx, cmd } => {
                defmt::warn!("Remote shutter cmd to {}: {:?}", shutter_idx, cmd);
                shutters_channel.send((shutter_idx, cmd)).await;
            }

            Message::RequestStatus => {
                if !to_us {
                    continue;
                }
                let event = Event::RemoteStatusRequest;
                EVENT_CHANNEL.send(event).await;
            }

            Message::Ping { body } => {
                if !to_us {
                    continue;
                }
                let msg = Message::Pong { body };
                // NOTE: Should this be blocking? We just got message so CAN should be operational.
                board
                    .interconnect
                    .transmit_response(&msg, WhenFull::Wait)
                    .await;
            }

            // Those are not required on endpoints.
            Message::Error { .. }
            | Message::Info { .. }
            | Message::OutputChanged { .. }
            | Message::StatusIO { .. }
            | Message::InputChanged { .. }
            | Message::Pong { .. }
            | Message::Status { .. } => {
                if to_us {
                    defmt::warn!("Unhandled message was addressed to us: {:?}", message);
                } else {
                    defmt::debug!("Ignoring unhandled message: {:?}", message);
                }
            }
        }
    }
}
