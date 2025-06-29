use nostr_sdk::prelude::*;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::Receiver;

use crate::types::ProcessableEvent;
use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;

impl Whitenoise {
    // ============================================================================
    // EVENT PROCESSING
    // ============================================================================

    // Private Helper Methods =====================================================

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
    pub(crate) async fn extract_pubkey_from_subscription_id(
        &self,
        subscription_id: &str,
    ) -> Option<PublicKey> {
        if let Some(underscore_pos) = subscription_id.find('_') {
            let hash_str = &subscription_id[..underscore_pos];
            // Get all accounts and find the one whose hash matches
            let accounts = self.accounts.read().await;
            for pubkey in accounts.keys() {
                let mut hasher = Sha256::new();
                hasher.update(self.nostr.session_salt());
                hasher.update(pubkey.to_bytes());
                let hash = hasher.finalize();
                let pubkey_hash = format!("{:x}", hash)[..12].to_string();
                if pubkey_hash == hash_str {
                    return Some(*pubkey);
                }
            }
        }
        None
    }

    /// Main event processing loop
    async fn process_events(
        whitenoise: &'static Whitenoise,
        mut receiver: Receiver<ProcessableEvent>,
        mut shutdown: Receiver<()>,
    ) {
        tracing::debug!(
            target: "whitenoise::process_events",
            "Starting event processing loop"
        );

        let mut shutting_down = false;

        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    tracing::debug!(
                        target: "whitenoise::process_events",
                        "Received event for processing"
                    );

                    // Process the event
                    match event {
                        ProcessableEvent::NostrEvent { event, subscription_id, retry_info } => {
                            // Filter and route nostr events based on kind
                            let result = match event.kind {
                                Kind::GiftWrap => {
                                    whitenoise.process_giftwrap(event.clone(), subscription_id.clone()).await
                                }
                                Kind::MlsGroupMessage => {
                                    whitenoise.process_mls_message(event.clone(), subscription_id.clone()).await
                                }
                                _ => {
                                    // TODO: Add more event types as needed
                                    tracing::debug!(
                                        target: "whitenoise::process_events",
                                        "Received unhandled event of kind: {:?} - add handler if needed",
                                        event.kind
                                    );
                                    Ok(()) // Unhandled events are not errors
                                }
                            };

                            // Handle retry logic
                            if let Err(e) = result {
                                if retry_info.should_retry() {
                                    if let Some(next_retry) = retry_info.next_attempt() {
                                        let delay_ms = next_retry.delay_ms();
                                        tracing::warn!(
                                            target: "whitenoise::process_events",
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
                                                    target: "whitenoise::process_events",
                                                    "Failed to requeue event for retry: {}",
                                                    send_err
                                                );
                                            }
                                        });
                                    }
                                } else {
                                    tracing::error!(
                                        target: "whitenoise::process_events",
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
                        target: "whitenoise::process_events",
                        "Received shutdown signal, finishing current queue..."
                    );
                    shutting_down = true;
                    // Continue processing remaining events in queue, but don't wait for new shutdown signals
                }
                else => {
                    if shutting_down {
                        tracing::debug!(
                            target: "whitenoise::process_events",
                            "Queue flushed, shutting down event processor"
                        );
                    } else {
                        tracing::debug!(
                            target: "whitenoise::process_events",
                            "All channels closed, exiting event processing loop"
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Process giftwrap events with account awareness
    async fn process_giftwrap(&self, event: Event, subscription_id: Option<String>) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::process_giftwrap",
            "Processing giftwrap: {:?}",
            event
        );

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

        tracing::debug!(
            target: "whitenoise::process_giftwrap",
            "Processing giftwrap for target account: {} (author: {})",
            target_pubkey.to_hex(),
            event.pubkey.to_hex()
        );

        // Validate that this matches the subscription_id if available
        if let Some(sub_id) = subscription_id {
            if let Some(sub_pubkey) = self.extract_pubkey_from_subscription_id(&sub_id).await {
                if target_pubkey != sub_pubkey {
                    return Err(WhitenoiseError::InvalidEvent(format!(
                        "Giftwrap target pubkey {} does not match subscription pubkey {} - possible routing error",
                        target_pubkey.to_hex(),
                        sub_pubkey.to_hex()
                    )));
                }
            }
        }

        let target_account = self.read_account_by_pubkey(&target_pubkey).await?;

        tracing::info!(
            target: "whitenoise::process_giftwrap",
            "Giftwrap received for account: {} - processing not yet implemented",
            target_pubkey.to_hex()
        );

        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&target_pubkey)?;

        let unwrapped = extract_rumor(&keys, &event).await.map_err(|e| {
            WhitenoiseError::Configuration(format!("Failed to decrypt giftwrap: {}", e))
        })?;

        match unwrapped.rumor.kind {
            Kind::MlsWelcome => {
                self.process_welcome(&target_account, event, unwrapped.rumor)
                    .await?;
            }
            Kind::PrivateDirectMessage => {
                tracing::debug!(
                    target: "whitenoise::process_giftwrap",
                    "Received private direct message: {:?}",
                    unwrapped.rumor
                );
            }
            _ => {
                tracing::debug!(
                    target: "whitenoise::process_giftwrap",
                    "Received unhandled giftwrap of kind {:?}",
                    unwrapped.rumor.kind
                );
            }
        }

        Ok(())
    }

    async fn process_welcome(
        &self,
        account: &Account,
        event: Event,
        rumor: UnsignedEvent,
    ) -> Result<()> {
        // Process the welcome message - lock scope is minimal
        {
            let nostr_mls_guard = account.nostr_mls.lock().await;
            if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
                nostr_mls
                    .process_welcome(&event.id, &rumor)
                    .map_err(WhitenoiseError::NostrMlsError)?;
                tracing::debug!(target: "whitenoise::process_welcome", "Processed welcome event");
            } else {
                tracing::error!(target: "whitenoise::process_welcome", "Nostr MLS not initialized");
                return Err(WhitenoiseError::NostrMlsNotInitialized);
            }
        } // nostr_mls lock released here

        let key_package_event_id: Option<EventId> = rumor
            .tags
            .iter()
            .find(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .and_then(|tag| tag.content())
            .and_then(|content| EventId::parse(content).ok());

        if let Some(key_package_event_id) = key_package_event_id {
            self.delete_key_package_from_relays_for_account(
                account,
                &key_package_event_id,
                false, // For now we don't want to delete the key packages from MLS storage
            )
            .await?;
            tracing::debug!(target: "whitenoise::process_welcome", "Deleted used key package from relays");

            self.publish_key_package_for_account(account).await?;
            tracing::debug!(target: "whitenoise::process_welcome", "Published new key package");
        } else {
            tracing::warn!(target: "whitenoise::process_welcome", "No key package event id found in welcome event");
        }

        Ok(())
    }

    /// Process MLS group messages with account awareness
    async fn process_mls_message(
        &self,
        event: Event,
        subscription_id: Option<String>,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::process_mls_message",
            "Processing MLS message: {:?}",
            event
        );

        let sub_id = subscription_id.ok_or_else(|| {
            WhitenoiseError::InvalidEvent(
                "MLS message received without subscription ID".to_string(),
            )
        })?;

        let target_pubkey = self
            .extract_pubkey_from_subscription_id(&sub_id)
            .await
            .ok_or_else(|| {
                WhitenoiseError::InvalidEvent(format!(
                    "Cannot extract pubkey from subscription ID: {}",
                    sub_id
                ))
            })?;

        tracing::debug!(
            target: "whitenoise::process_mls_message",
            "Processing MLS message for account: {}",
            target_pubkey.to_hex()
        );

        let target_account = self.read_account_by_pubkey(&target_pubkey).await?;

        let nostr_mls_guard = target_account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            match nostr_mls.process_message(&event) {
                Ok(result) => {
                    tracing::debug!(target: "whitenoise::process_mls_message", "Processed MLS message - Result: {:?}", result);
                    Ok(())
                }
                Err(e) => {
                    tracing::error!(target: "whitenoise::process_mls_message", "MLS message processing failed for account {}: {}", target_pubkey.to_hex(), e);
                    Err(WhitenoiseError::NostrMlsError(e))
                }
            }
        } else {
            tracing::error!(target: "whitenoise::process_mls_message", "Nostr MLS not initialized");
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }

    /// Process relay messages for logging/monitoring
    async fn process_relay_message(&self, relay_url: RelayUrl, message_type: String) {
        tracing::debug!(
            target: "whitenoise::process_relay_message",
            "Processing message from {}: {}",
            relay_url,
            message_type
        );
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
    async fn test_extract_pubkey_from_subscription_id() {
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let subscription_id = "abc123_user_events";
        let extracted = whitenoise
            .extract_pubkey_from_subscription_id(subscription_id)
            .await;
        assert!(extracted.is_none());

        let invalid_case = "no_underscore";
        let extracted = whitenoise
            .extract_pubkey_from_subscription_id(invalid_case)
            .await;
        assert!(extracted.is_none());

        let multi_underscore_id = "abc123_user_events_extra";
        let result = whitenoise
            .extract_pubkey_from_subscription_id(multi_underscore_id)
            .await;
        assert!(result.is_none());
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
