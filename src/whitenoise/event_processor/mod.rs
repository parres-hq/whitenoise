use nostr_sdk::prelude::*;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::Receiver;

use crate::{
    types::ProcessableEvent,
    whitenoise::{
        accounts::Account,
        error::{Result, WhitenoiseError},
        Whitenoise,
    },
};
mod event_handlers;

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

    /// Extract the account pubkey from a subscription_id
    /// Subscription IDs follow the format: {hashed_pubkey}_{subscription_type}
    /// where hashed_pubkey = SHA256(session salt || accouny_pubkey)[..12]
    async fn extract_pubkey_from_subscription_id(
        &self,
        subscription_id: &str,
    ) -> Result<PublicKey> {
        let underscore_pos = subscription_id.find('_');
        if underscore_pos.is_none() {
            return Err(WhitenoiseError::InvalidEvent(format!(
                "Invalid subscription ID: {}",
                subscription_id
            )));
        }

        let hash_str = &subscription_id[..underscore_pos.unwrap()];
        // Get all accounts and find the one whose hash matches
        let accounts = Account::all(&self.database).await?;
        for account in accounts.iter() {
            let mut hasher = Sha256::new();
            hasher.update(self.nostr.session_salt());
            hasher.update(account.pubkey.to_bytes());
            let hash = hasher.finalize();
            let pubkey_hash = format!("{:x}", hash)[..12].to_string();
            if pubkey_hash == hash_str {
                return Ok(account.pubkey);
            }
        }

        Err(WhitenoiseError::InvalidEvent(format!(
            "No account found for subscription hash: {}",
            hash_str
        )))
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
                            // Get the account from the subscription ID, log an error if we can't find it and skip the event
                            let account = match whitenoise.account_from_subscription_id(subscription_id.clone()).await {
                                Ok(account) => account,
                                Err(e) => {
                                    tracing::error!(
                                        target: "whitenoise::event_processor::process_events",
                                        "Error getting account from subscription ID: {}", e
                                    );
                                    continue;
                                }
                            };

                            // Check if we already processed this event - skip if so
                            let already_processed = match whitenoise.nostr.event_tracker.already_processed_event(&event.id, &account.pubkey).await {
                                Ok(v) => v,
                                Err(e) => {
                                    tracing::error!(
                                        target: "whitenoise::event_processor::process_events",
                                        "Already processed check failed for {}: {}",
                                        event.id.to_hex(),
                                        e
                                    );
                                    false
                                }
                            };
                            if already_processed {
                                tracing::debug!(
                                    target: "whitenoise::event_processor::process_events",
                                    "Already processed event: {} (kind {})",
                                    event.id.to_hex(),
                                    event.kind.as_u16()
                                );
                                continue; // Skip to next event
                            }

                            // For account-specific events, check if WE published this event
                            // We don't skip giftwraps and MLS messages because we need them to process in nostr-mls
                            let should_skip = match event.kind {
                                Kind::MlsGroupMessage => false,
                                Kind::GiftWrap => false,
                                _ => match whitenoise.nostr.event_tracker.account_published_event(&event.id, &account.pubkey).await {
                                    Ok(v) => v,
                                    Err(e) => {
                                        tracing::error!(
                                            target: "whitenoise::event_processor::process_events",
                                            "Account published check failed for {}: {}",
                                            event.id.to_hex(),
                                            e
                                        );
                                        false
                                    }
                                }
                            };

                            if should_skip {
                                tracing::debug!(
                                    target: "whitenoise::event_processor::process_events",
                                    "Skipping processing of our own event: {} (kind {})",
                                    event.id.to_hex(),
                                    event.kind.as_u16()
                                );
                                continue; // Skip to next event
                            }

                            // Filter and route nostr events based on kind
                            let result = match event.kind {
                                Kind::GiftWrap => {
                                    match validate_giftwrap_target(&account, &event) {
                                        Ok(()) => whitenoise.handle_giftwrap(&account, event.clone()).await,
                                        Err(e) => Err(e),
                                    }
                                }
                                Kind::MlsGroupMessage => {
                                    whitenoise.handle_mls_message(&account, event.clone()).await
                                }
                                Kind::Metadata => {
                                    whitenoise.handle_metadata(event.clone()).await
                                }
                                Kind::RelayList | Kind::InboxRelays | Kind::MlsKeyPackageRelays => {
                                    whitenoise.handle_relay_list(event.clone()).await
                                }
                                Kind::ContactList => {
                                    whitenoise.handle_contact_list(&account, event.clone()).await
                                }
                                _ => {
                                    // TODO: Add more event types as needed
                                    tracing::debug!(
                                        target: "whitenoise::event_processor::process_events",
                                        "Received unhandled event of kind: {:?} - add handler if needed",
                                        event.kind
                                    );
                                    Ok(()) // Unhandled events are not errors
                                }
                            };

                            // If processing was successful, record that we processed this event
                            if result.is_ok() {
                                if let Err(e) = whitenoise.nostr.event_tracker.track_processed_event(&event.id, &account.pubkey).await {
                                    tracing::warn!(
                                        target: "whitenoise::event_processor::process_events",
                                        "Failed to record processed event {}: {}",
                                        event.id.to_hex(),
                                        e
                                    );
                                }
                            }

                            // Handle retry logic
                            if let Err(e) = result {
                                if retry_info.should_retry() {
                                    if let Some(next_retry) = retry_info.next_attempt() {
                                        let delay_ms = next_retry.delay_ms();
                                        tracing::warn!(
                                            target: "whitenoise::event_processor::process_events",
                                            "Event processing failed (attempt {}/{}), retrying in {}ms: {}",
                                            next_retry.attempt,
                                            next_retry.max_attempts,
                                            delay_ms,
                                            e
                                        );

                                        let retry_event = ProcessableEvent::NostrEvent {
                                            event,
                                            subscription_id,
                                            retry_info: next_retry,
                                        };
                                        let sender = whitenoise.event_sender.clone();

                                        tokio::spawn(async move {
                                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                                            if let Err(send_err) = sender.send(retry_event).await {
                                                tracing::error!(
                                                    target: "whitenoise::event_processor::process_events",
                                                    "Failed to requeue event for retry: {}",
                                                    send_err
                                                );
                                            }
                                        });
                                    }
                                } else {
                                    tracing::error!(
                                        target: "whitenoise::event_processor::process_events",
                                        "Event processing failed after {} attempts, giving up: {}",
                                        retry_info.max_attempts,
                                        e
                                    );
                                }
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

    async fn account_from_subscription_id(
        &self,
        subscription_id: Option<String>,
    ) -> Result<Account> {
        let sub_id = subscription_id.ok_or_else(|| {
            WhitenoiseError::InvalidEvent(
                "MLS message received without subscription ID".to_string(),
            )
        })?;

        let target_pubkey = self
            .extract_pubkey_from_subscription_id(&sub_id)
            .await
            .map_err(|_| {
                WhitenoiseError::InvalidEvent(format!(
                    "Cannot extract pubkey from subscription ID: {}",
                    sub_id
                ))
            })?;

        tracing::debug!(
        target: "whitenoise::event_processor::process_mls_message",
        "Processing MLS message for account: {}",
        target_pubkey.to_hex()
        );

        Account::find_by_pubkey(&target_pubkey, &self.database).await
    }
}

fn validate_giftwrap_target(account: &Account, event: &Event) -> Result<()> {
    // Extract the target pubkey from the event's 'p' tag
    let target_pubkey = event
        .tags
        .iter()
        .find(|tag| tag.kind() == TagKind::p())
        .and_then(|tag| tag.content())
        .and_then(|pubkey_str| PublicKey::parse(pubkey_str).ok())
        .ok_or_else(|| {
            WhitenoiseError::InvalidEvent(
                "No valid target pubkey found in 'p' tag for giftwrap event".to_string(),
            )
        })?;

    if target_pubkey != account.pubkey {
        return Err(WhitenoiseError::InvalidEvent(format!(
            "Giftwrap target pubkey {} does not match account pubkey {} - possible routing error",
            target_pubkey.to_hex(),
            account.pubkey.to_hex()
        )));
    }

    Ok(())
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
    async fn test_extract_pubkey_from_subscription_id() {
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let subscription_id = "abc123_user_events";
        let extracted = whitenoise
            .extract_pubkey_from_subscription_id(subscription_id)
            .await;
        assert!(extracted.is_err());

        let invalid_case = "no_underscore";
        let extracted = whitenoise
            .extract_pubkey_from_subscription_id(invalid_case)
            .await;
        assert!(extracted.is_err());

        let multi_underscore_id = "abc123_user_events_extra";
        let result = whitenoise
            .extract_pubkey_from_subscription_id(multi_underscore_id)
            .await;
        assert!(result.is_err());
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
