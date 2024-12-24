use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::rtc::{DateTime, DayOfWeek};
use embassy_stm32::uid;
use embassy_time::{Duration, Timer};

use crate::boards::ctrl_board::Board;
use crate::components::message::{args, Message};
use crate::components::status;

/// High-level command queue that are produced by executor.
// static EVENT_CHANNEL: EventChannel = EventChannel::new();

/// Main application/business logic entrypoint.
pub struct GateApp {
    /// For all IO needs (and comm peripherals like CAN and USB)
    pub board: &'static Board,
}

impl GateApp {
    pub async fn new(board: &'static Board) -> Self {
        Self {
            board,
        }
    }

    fn spawn_tasks(&'static self, spawner: &Spawner) {
        unwrap!(spawner.spawn(task_read_interconnect(&self.board)));
    }

    pub async fn main(&'static mut self, spawner: &Spawner) -> ! {
        defmt::info!("Starting gate app on chip {}", uid::uid());

        let welcome_message = Message::Info {
            code: args::InfoCode::Started.to_bytes(),
            arg: 0,
        };

        self.board
            .interconnect
            .transmit_response(&welcome_message)
            .await;

        // This might fail within tasks on i²c/CAN communication with expanders.
        self.spawn_tasks(spawner);

        defmt::info!("Starting app on chip {}", uid::uid());
        loop {
            // Steady blinking to indicate we are alive and ok.
            defmt::info!("Tick: {:?}", status::COUNTERS);
            Timer::after(Duration::from_millis(5000)).await;
        }
    }
}

#[embassy_executor::task(pool_size = 1)]
pub async fn task_read_interconnect(board: &'static Board) {
    loop {
        let raw = board.interconnect.receive().await;
        defmt::info!("Received raw message {}", raw);

        let message = if let Ok(raw) = raw {
            let maybe = Message::from_raw(raw);
            if let Ok(message) = maybe {
                message
            } else {
                continue;
            }
        } else {
            defmt::warn!("Error while reading a message {:?}", raw);
            continue;
        };

        match message {
            Message::CallProcedure { proc_id } => {
                defmt::warn!("TODO: Call procedure {}", proc_id);
            }

            Message::TriggerInput { input, trigger } => {
                defmt::warn!("TODO: Trigger input {} as {:?}", input, trigger);
            }

            Message::SetOutput { output, state } => {
                defmt::warn!("TODO: Trigger output {} to {:?}", output, state);
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
                let dt = DateTime::from(year, month, day, dow, hour, minute, second);

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

            Message::RequestStatus => {
                defmt::info!("TODO: Send our status");
            }

            // Those are not required on endpoints.
            Message::Error { .. }
            | Message::Info { .. }
            | Message::OutputChanged { .. }
            | Message::InputTriggered { .. }
            | Message::Status { .. } => {
                defmt::info!("Got unhandled message, ignoring: {:?}", message);
            }
        }
    }
}
