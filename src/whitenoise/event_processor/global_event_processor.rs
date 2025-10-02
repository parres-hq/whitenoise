use nostr_sdk::prelude::*;

use crate::{
    types::RetryInfo,
    whitenoise::{
        Whitenoise,
        error::{Result, WhitenoiseError},
    },
};

impl Whitenoise {
    pub(super) async fn process_global_event(
        &self,
        event: Event,
        subscription_id: String,
        retry_info: RetryInfo,
    ) {
        if self
            .validate_batched_subscription_id(&subscription_id)
            .is_err()
        {
            tracing::error!(
                target: "whitenoise::event_processor::process_global_event",
                "Invalid batched subscription ID: {}", subscription_id
            );
            return;
        }

        match self.should_skip_global_event_processing(&event).await {
            Some(skip_reason) => {
                tracing::debug!(
                    target: "whitenoise::event_processor::process_global_event",
                    "Skipping event {}: {} (kind {})",
                    event.id.to_hex(),
                    skip_reason,
                    event.kind.as_u16()
                );
                return;
            }
            None => {
                // Continue processing
            }
        }

        let result = self.route_global_event_for_processing(&event).await;

        match result {
            Ok(()) => {
                if let Err(e) = self
                    .nostr
                    .event_tracker
                    .track_processed_global_event(&event)
                    .await
                {
                    tracing::error!(target: "whitenoise::event_processor::process_global_event", "Failed to track processed global event: {}", e);
                }
            }
            Err(e) => {
                // Handle retry logic for actual processing errors
                if retry_info.should_retry() {
                    self.schedule_retry(event, subscription_id, retry_info, e);
                } else {
                    tracing::error!(
                        target: "whitenoise::event_processor::process_global_event",
                        "Event processing failed after {} attempts, giving up: {}",
                        retry_info.max_attempts,
                        e
                    );
                }
            }
        }
    }

    /// Check if a global event should be skipped (not processed)
    /// Returns Some(reason) if should skip, None if should process
    async fn should_skip_global_event_processing(&self, event: &Event) -> Option<&'static str> {
        let already_processed = match self
            .nostr
            .event_tracker
            .already_processed_global_event(&event.id)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::event_processor::should_skip_global_event_processing",
                    "Already processed check failed for {}: {}",
                    event.id.to_hex(),
                    e
                );
                false
            }
        };
        if already_processed {
            return Some("already processed");
        }

        // For global events, check if WE published this event
        let should_skip = match self
            .nostr
            .event_tracker
            .global_published_event(&event.id)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::event_processor::should_skip_global_event_processing",
                    "Global published check failed for {}: {}",
                    event.id.to_hex(),
                    e
                );
                false
            }
        };
        if should_skip {
            return Some("self-published event");
        }

        None
    }

    async fn route_global_event_for_processing(&self, event: &Event) -> Result<()> {
        match event.kind {
            Kind::Metadata => self.handle_metadata(event.clone()).await,
            Kind::RelayList | Kind::InboxRelays | Kind::MlsKeyPackageRelays => {
                self.handle_relay_list(event.clone()).await
            }
            _ => {
                tracing::debug!(target: "whitenoise::event_processor::route_global_event_for_processing",
                "Received unhandled global event of kind: {:?} - add handler if needed", event.kind);
                Ok(()) // Unhandled events are not errors
            }
        }
    }

    fn validate_batched_subscription_id(&self, subscription_id: &str) -> Result<()> {
        // Simple validation format: global_users_abc123_0
        // we could have a more robust validation here but this is good enough for now
        if subscription_id.starts_with("global_users_") && subscription_id.matches('_').count() == 3
        {
            Ok(())
        } else {
            Err(WhitenoiseError::InvalidEvent(format!(
                "Invalid batched subscription ID: {}",
                subscription_id
            )))
        }
    }
}
