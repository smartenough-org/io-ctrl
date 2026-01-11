use core::cell::UnsafeCell;
use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::rtc::{DateTime, DayOfWeek};
use embassy_stm32::uid;
use embassy_time::{Duration, Timer};

use crate::boards::ctrl_board::Board;
use crate::components::message::{Message, args};
use crate::components::status;

use crate::buttonsmash::consts::BINDINGS_COUNT;
use crate::buttonsmash::{Event, EventChannel, Executor, Opcode};
use crate::config;
use crate::io::event_converter::run_event_converter;

/// High-level command queue that are consumed by executor.
static EVENT_CHANNEL: EventChannel = EventChannel::new();

/// Main application/business logic entrypoint.
pub struct CtrlApp {
    /// For all IO needs (and comm peripherals like CAN and USB)
    pub board: &'static Board,
    executor: UnsafeCell<Executor<BINDINGS_COUNT>>,
}

impl CtrlApp {
    pub async fn new(board: &'static Board) -> Self {
        // TODO: Pass interconnect? Or a queue?
        let mut executor = Executor::new(
            board,
            &board.interconnect,
            &board.shutters_channel,
        );
        Self::configure(&mut executor).await;

        Self {
            board,
            executor: UnsafeCell::new(executor),
        }
    }

    /// Returns hard-configured Executor. TODO: This is temporary. Code should
    /// be programmable and read from flash on start.
    async fn configure(executor: &mut Executor<BINDINGS_COUNT>) {
        let program = [
            // Setup proc.
            Opcode::Start(0),
            // Basic usable program for initial setup.
            Opcode::LayerDefault,
            Opcode::BindShortToggle(1, 1),
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
            // Test proc.
            Opcode::Start(1),
            Opcode::Activate(100),
            Opcode::Activate(101),
            Opcode::Deactivate(110),
            Opcode::Stop,
            Opcode::Start(2),
            Opcode::Noop,
            Opcode::Stop,
        ];

        executor.load_static(&program).await;
    }

    fn spawn_tasks(&'static self, spawner: &Spawner) {
        let executor = unsafe { &mut *self.executor.get() };
        spawner.spawn(unwrap!(task_pump_switch_events_to_microvm(executor)));
        spawner.spawn(unwrap!(run_event_converter(
            self.board.input_q,
            &EVENT_CHANNEL
        )));
        spawner.spawn(unwrap!(task_read_interconnect(self.board)));
    }

    pub async fn main(&'static mut self, spawner: &Spawner) -> ! {
        defmt::info!("Starting app on chip {}", uid::uid());

        let welcome_message = Message::Info {
            code: args::InfoCode::Started.to_bytes(),
            arg: 0,
        };

        self.board
            .interconnect
            .transmit_response(&welcome_message, false)
            .await;

        // This might fail within tasks on i²c/CAN communication with expanders.
        self.spawn_tasks(spawner);

        let mut cnt = 0;
        loop {
            // Prevent deep sleep to allow easy remote debugging.
            // TODO: Remove for production.
            Timer::after(Duration::from_millis(20)).await;
            cnt += 1;
            if cnt % 300 == 0 {
                defmt::info!("Tick: {:?}", status::COUNTERS);
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
pub async fn task_read_interconnect(board: &'static Board) {
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
                board.shutters_channel.send((shutter_idx, cmd)).await;
            }

            Message::RequestStatus => {
                if !to_us {
                    continue;
                }
                defmt::info!("TODO: Send our status");
                // TODO: This needs access to inputs / outputs and uptime. MicroVM has it?
            }

            Message::Ping { body } => {
                if !to_us {
                    continue;
                }
                let msg = Message::Pong { body };
                // NOTE: Should this be blocking? We just got message so CAN should be operational.
                board.interconnect.transmit_response(&msg, true).await;
            }

            // Those are not required on endpoints.
            Message::Error { .. }
            | Message::Info { .. }
            | Message::OutputChanged { .. }
            | Message::StatusIO { .. }
            | Message::InputTriggered { .. }
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
