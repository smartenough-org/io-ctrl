use crate::io::events::{ButtonEvent, TriggerChannel, RawEventChannel, SwitchState, Trigger};

/// Max time [ms] until which the activation ends in ShortClick.
const MAX_SHORT_MS: u32 = 300;

/*
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
*/

#[embassy_executor::task(pool_size = 1)]
pub async fn run_event_converter(
    input_q: &'static RawEventChannel,
    output_q: &'static TriggerChannel,
) {
    loop {
        let input_event = input_q.receive().await;
        match input_event.state {
            SwitchState::Activated => {
                output_q
                    .send(ButtonEvent {
                        switch_id: input_event.switch_id,
                        trigger: Trigger::Activated,
                    })
                    .await;
            }
            SwitchState::Active(ms) => {
                // We were activated and are still active. For a some period of time.
                if ms >= MAX_SHORT_MS {
                    /* TODO: Should this be repeated... or deduplicated? */
                    output_q
                        .send(ButtonEvent {
                            switch_id: input_event.switch_id,
                            trigger: Trigger::LongActivated,
                        })
                        .await;
                }
            }
            SwitchState::Deactivated(ms) => {
                // We were activated, maybe longactivated, now we deactivate.
                if ms <= MAX_SHORT_MS {
                    output_q
                        .send(ButtonEvent {
                            switch_id: input_event.switch_id,
                            trigger: Trigger::ShortClick,
                        })
                        .await;
                } else {
                    output_q
                        .send(ButtonEvent {
                            switch_id: input_event.switch_id,
                            trigger: Trigger::LongClick,
                        })
                        .await;

                    output_q
                        .send(ButtonEvent {
                            switch_id: input_event.switch_id,
                            trigger: Trigger::LongDeactivated,
                        })
                        .await;
                }

                output_q
                    .send(ButtonEvent {
                        switch_id: input_event.switch_id,
                        trigger: Trigger::Deactivated,
                    })
                    .await;
            }
        }
    }
}
