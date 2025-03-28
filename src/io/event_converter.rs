use crate::buttonsmash::{Event, EventChannel};
use crate::io::events::{InputChannel, SwitchState, Trigger};

/// Max time [ms] until which the activation ends in ShortClick.
const MAX_SHORT_MS: u32 = 400;

#[embassy_executor::task(pool_size = 1)]
pub async fn run_event_converter(input_q: &'static InputChannel, output_q: &'static EventChannel) {
    loop {
        let input_event = input_q.receive().await;
        match input_event.state {
            SwitchState::Activated => {
                output_q
                    .send(Event::new_button(input_event.switch_id, Trigger::Activated))
                    .await;
            }
            SwitchState::Active(ms) => {
                // We were activated and are still active. For a some period of time.
                if ms >= MAX_SHORT_MS {
                    /* TODO: Should this be repeated... or deduplicated? */
                    output_q
                        .send(Event::new_button(
                            input_event.switch_id,
                            Trigger::LongActivated,
                        ))
                        .await;
                }
            }
            SwitchState::Deactivated(ms) => {
                // We were activated, maybe longactivated, now we deactivate.
                if ms <= MAX_SHORT_MS {
                    output_q
                        .send(Event::new_button(
                            input_event.switch_id,
                            Trigger::ShortClick,
                        ))
                        .await;
                } else {
                    output_q
                        .send(Event::new_button(input_event.switch_id, Trigger::LongClick))
                        .await;

                    output_q
                        .send(Event::new_button(
                            input_event.switch_id,
                            Trigger::LongDeactivated,
                        ))
                        .await;
                }

                output_q
                    .send(Event::new_button(
                        input_event.switch_id,
                        Trigger::Deactivated,
                    ))
                    .await;
            }
        }
    }
}
