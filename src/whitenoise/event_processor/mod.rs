use nostr_sdk::prelude::*;
use tokio::sync::mpsc::Receiver;

use crate::{
    types::{ProcessableEvent, RetryInfo},
    whitenoise::{
        error::{Result, WhitenoiseError},
        Whitenoise,
    },
};

mod account_event_processor;
mod event_handlers;
mod global_event_processor;

impl Whitenoise {
    /// Start the event processing loop in a background task
    pub(crate) async fn start_event_processing_loop(
        whitenoise: &'static Whitenoise,
        receiver: Receiver<ProcessableEvent>,
        shutdown_receiver: Receiver<()>,
    ) {
        tokio::spawn(async move {
            Self::process_events(whitenoise, receiver, shutdown_receiver).await;
        });
    }

    /// Shutdown event processing gracefully
    pub(crate) async fn shutdown_event_processing(&self) -> Result<()> {
        match self.shutdown_sender.send(()).await {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Expected if processor already shut down
        }
    }

    /// Main event processing loop
    async fn process_events(
        whitenoise: &'static Whitenoise,
        mut receiver: Receiver<ProcessableEvent>,
        mut shutdown: Receiver<()>,
    ) {
        tracing::debug!(
            target: "whitenoise::event_processor::process_events",
            "Starting event processing loop"
        );

        let mut shutting_down = false;

        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    tracing::debug!(
                        target: "whitenoise::event_processor::process_events",
                        "Received event for processing"
                    );

                    // Process the event
                    match event {
                        ProcessableEvent::NostrEvent { event, subscription_id, retry_info } => {
                            let sub_id = match &subscription_id {
                                Some(s) => s.clone(),
                                None => {
                                    tracing::warn!(
                                        target: "whitenoise::event_processor::process_events",
                                        "Event received without subscription ID, skipping"
                                    );
                                    continue;
                                }
                            };
                            if whitenoise.is_event_global(&sub_id) {
                                whitenoise.process_global_event(event, sub_id, retry_info).await;
                            } else {
                                whitenoise.process_account_event(event, sub_id, retry_info).await;
                            }
                        }
                        ProcessableEvent::RelayMessage(relay_url, message) => {
                            whitenoise.process_relay_message(relay_url, message).await;
                        }
                    }
                }
                Some(_) = shutdown.recv(), if !shutting_down => {
                    tracing::info!(
                        target: "whitenoise::event_processor::process_events",
                        "Received shutdown signal, finishing current queue..."
                    );
                    shutting_down = true;
                    // Continue processing remaining events in queue, but don't wait for new shutdown signals
                }
                else => {
                    if shutting_down {
                        tracing::debug!(
                            target: "whitenoise::event_processor::process_events",
                            "Queue flushed, shutting down event processor"
                        );
                    } else {
                        tracing::debug!(
                            target: "whitenoise::event_processor::process_events",
                            "All channels closed, exiting event processing loop"
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Process relay messages for logging/monitoring
    async fn process_relay_message(&self, relay_url: RelayUrl, message_type: String) {
        tracing::debug!(
            target: "whitenoise::event_processor::process_relay_message",
            "Processing message from {}: {}",
            relay_url,
            message_type
        );
    }

    fn is_event_global(&self, subscription_id: &str) -> bool {
        subscription_id.starts_with("global_users_")
    }

    /// Schedule a retry for a failed event processing attempt
    fn schedule_retry(
        &self,
        event: Event,
        subscription_id: String,
        retry_info: RetryInfo,
        error: WhitenoiseError,
    ) {
        if let Some(next_retry) = retry_info.next_attempt() {
            let delay_ms = next_retry.delay_ms();
            tracing::warn!(
                target: "whitenoise::event_processor::schedule_retry",
                "Event processing failed (attempt {}/{}), retrying in {}ms: {}",
                next_retry.attempt,
                next_retry.max_attempts,
                delay_ms,
                error
            );

            let retry_event = ProcessableEvent::NostrEvent {
                event,
                subscription_id: Some(subscription_id),
                retry_info: next_retry,
            };
            let sender = self.event_sender.clone();

            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                if let Err(send_err) = sender.send(retry_event).await {
                    tracing::error!(
                        target: "whitenoise::event_processor::schedule_retry",
                        "Failed to requeue event for retry: {}",
                        send_err
                    );
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::whitenoise::test_utils::*;
    use std::time::Duration;
    #[tokio::test]
    async fn test_shutdown_event_processing() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let result = whitenoise.shutdown_event_processing().await;
        assert!(result.is_ok());

        // Test that multiple shutdowns don't cause errors
        let result2 = whitenoise.shutdown_event_processing().await;
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_queue_operations_after_shutdown() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        whitenoise.shutdown_event_processing().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Test that shutdown completed successfully without errors
        // (We can't test queuing operations since those methods were removed)
    }
}
