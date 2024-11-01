use crate::io::events::{ButtonEvent, InputEventChannel, SwitchEvent, SwitchState, Trigger};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

pub type HighLevelChannel = Channel<ThreadModeRawMutex, ButtonEvent, 6>;

/// Convert low-level switch states into higher-level button events.
pub struct EventConverter {
    input_q: InputEventChannel,
    output_q: HighLevelChannel,
}

impl EventConverter {
    /// Max time [ms] until which the activation ends in ShortClick.
    const MAX_SHORT_MS: u32 = 300;

    pub fn new() -> Self {
        Self {
            input_q: InputEventChannel::new(),
            output_q: HighLevelChannel::new(),
        }
    }

    pub async fn send(&self, event: SwitchEvent) {
        self.input_q.send(event).await;
    }

    pub async fn receive(&self) -> ButtonEvent {
        self.output_q.receive().await
    }

    pub fn try_read_events(&self) -> Option<ButtonEvent> {
        let ret = self.output_q.try_receive();
        match ret {
            Ok(event) => Some(event),
            Err(err) => {
                defmt::info!("Error while reading channel {:?}", err);
                None
            }
        }
    }

    /// Used by external readers.
    pub async fn read_events(&self) -> ButtonEvent {
        self.output_q.receive().await
    }

    pub async fn run(&self) -> ! {
        loop {
            let input_event = self.input_q.receive().await;
            match input_event.state {
                SwitchState::Activated => {
                    self.output_q
                        .send(ButtonEvent {
                            switch_id: input_event.switch_id,
                            trigger: Trigger::Activated,
                        })
                        .await;
                }
                SwitchState::Active(ms) => {
                    // We were activated and are still active. For a some period of time.
                    if ms >= Self::MAX_SHORT_MS {
                        /* TODO: Should this be repeated... or deduplicated? */
                        self.output_q
                            .send(ButtonEvent {
                                switch_id: input_event.switch_id,
                                trigger: Trigger::LongActivated,
                            })
                            .await;
                    }
                }
                SwitchState::Deactivated(ms) => {
                    // We were activated, maybe longactivated, now we deactivate.
                    if ms <= Self::MAX_SHORT_MS {
                        self.output_q
                            .send(ButtonEvent {
                                switch_id: input_event.switch_id,
                                trigger: Trigger::ShortClick,
                            })
                            .await;
                    } else {
                        self.output_q
                            .send(ButtonEvent {
                                switch_id: input_event.switch_id,
                                trigger: Trigger::LongClick,
                            })
                            .await;

                        self.output_q
                            .send(ButtonEvent {
                                switch_id: input_event.switch_id,
                                trigger: Trigger::LongDeactivated,
                            })
                            .await;
                    }

                    self.output_q
                        .send(ButtonEvent {
                            switch_id: input_event.switch_id,
                            trigger: Trigger::Deactivated,
                        })
                        .await;
                }
            }
        }
    }
}
