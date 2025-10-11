//! This module contains functions for publishing Nostr events and handling the publish tracking process.

use nostr_sdk::prelude::*;

use crate::{
    RelayType,
    nostr_manager::{NostrManager, NostrManagerError, Result},
    whitenoise::accounts::Account,
};

impl NostrManager {
    /// Publishes an event to the specified relays in a background task.
    ///
    /// This is a fire-and-forget operation that spawns a background task to publish the event
    /// without blocking the caller. Errors are logged but not returned. This is useful for
    /// scenarios where you want to queue a publish operation but don't need to wait for completion.
    ///
    /// The method clones the necessary data to ensure `'static` lifetime for the spawned task.
    /// The event is tracked in the database if published successfully to at least one relay.
    pub(crate) fn background_publish_event_to(
        &self,
        event: Event,
        account: &Account,
        relays: Vec<RelayUrl>,
    ) {
        let nostr = self.clone();
        let account = account.clone();

        tokio::spawn(async move {
            match nostr.publish_event_to(event, &account, &relays).await {
                Ok(output) => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::background_publish_event_to",
                        "Successfully published message to {} relay(s)",
                        output.success.len()
                    );
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::background_publish_event_to",
                        "Failed to publish message in background task: {}",
                        e
                    );
                }
            }
        });
    }

    /// Constructs and publishes a Nostr gift wrap event using the provided signer.
    ///
    /// This method creates a gift-wrapped Nostr event and publishes it to specified relays.
    /// Gift wrapping provides privacy by encrypting the inner event (rumor) and hiding the
    /// recipient's identity from relay operators and other observers.
    ///
    /// The method ensures that the client is connected to all specified relays before attempting
    /// to publish. The published event is tracked in the database if successful.
    pub(crate) async fn publish_gift_wrap_to(
        &self,
        receiver: &PublicKey,
        rumor: UnsignedEvent,
        extra_tags: &[Tag],
        account: &Account,
        relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let wrapped_event =
            EventBuilder::gift_wrap(&signer, receiver, rumor, extra_tags.to_vec()).await?;
        self.publish_event_to(wrapped_event, account, relays).await
    }

    /// Publishes a Nostr metadata event using the provided signer.
    ///
    /// The event is automatically tracked in the database if published successfully.
    pub(crate) async fn publish_metadata_with_signer(
        &self,
        metadata: &Metadata,
        relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let event_builder = EventBuilder::metadata(metadata);
        self.publish_event_builder_with_signer(event_builder, relays, signer)
            .await
    }

    /// Publishes a Nostr relay list event using the provided signer.
    ///
    /// The event is automatically tracked in the database if published successfully.
    pub(crate) async fn publish_relay_list_with_signer(
        &self,
        relay_list: &[RelayUrl],
        relay_type: RelayType,
        target_relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        let tags: Vec<Tag> = match relay_type {
            RelayType::Nip65 => relay_list
                .iter()
                .map(|relay| Tag::reference(relay.to_string()))
                .collect(),
            RelayType::Inbox | RelayType::KeyPackage => relay_list
                .iter()
                .map(|relay| Tag::custom(TagKind::Relay, [relay.to_string()]))
                .collect(),
        };
        tracing::debug!(target: "whitenoise::nostr_manager::publish_relay_list_with_signer", "Publishing relay list tags {:?}", tags);
        let event = EventBuilder::new(relay_type.into(), "").tags(tags);
        let result = self
            .publish_event_builder_with_signer(event, target_relays, signer)
            .await?;
        tracing::debug!(target: "whitenoise::nostr_manager::publish_relay_list_with_signer", "Published relay list event to Nostr: {:?}", result);

        Ok(())
    }

    /// Publishes a Nostr follow list event using the provided signer.
    ///
    /// Returns early with `Ok(())` if the follow list is empty. Otherwise, publishes the
    /// contact list event which is automatically tracked in the database if successful.
    pub(crate) async fn publish_follow_list_with_signer(
        &self,
        follow_list: &[PublicKey],
        target_relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        if follow_list.is_empty() {
            tracing::debug!(
                target: "whitenoise::nostr_manager::publish_follow_list_with_signer",
                "Skipping publish: empty follow list"
            );
            return Ok(());
        }
        let tags: Vec<Tag> = follow_list
            .iter()
            .map(|pubkey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();
        let event = EventBuilder::new(Kind::ContactList, "").tags(tags);
        let result = self
            .publish_event_builder_with_signer(event, target_relays, signer)
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_manager::publish_follow_list_with_signer",
            "Published follow list event to Nostr: {:?}",
            result
        );
        Ok(())
    }

    /// Publishes a Nostr MLS key package event using the provided signer.
    ///
    /// The event is automatically tracked in the database if published successfully.
    pub(crate) async fn publish_key_package_with_signer(
        &self,
        encoded_key_package: &str,
        relays: &[RelayUrl],
        tags: &[Tag],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package).tags(tags.to_vec());

        self.publish_event_builder_with_signer(key_package_event_builder, relays, signer)
            .await
    }

    /// Publishes a Nostr event deletion event using the provided signer.
    ///
    /// The deletion event is automatically tracked in the database if published successfully.
    pub(crate) async fn publish_event_deletion_with_signer(
        &self,
        event_id: &EventId,
        relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let event_deletion_event_builder =
            EventBuilder::delete(EventDeletionRequest::new().id(*event_id));
        self.publish_event_builder_with_signer(event_deletion_event_builder, relays, signer)
            .await
    }

    /// Publishes an already signed Nostr event to the specified relays.
    ///
    /// This method publishes a pre-signed event to a list of relay URLs. It ensures that the client
    /// is connected to all specified relays before attempting to publish. The event is automatically
    /// tracked in the database if published successfully to at least one relay.
    pub(crate) async fn publish_event_to(
        &self,
        event: Event,
        account: &Account,
        relays: &[RelayUrl],
    ) -> Result<Output<EventId>> {
        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;
        let result = self.client.send_event_to(relays, &event).await?;

        // Track the published event if we have a successful result (best-effort)
        if !result.success.is_empty() {
            self.event_tracker
                .track_published_event(result.id(), &account.pubkey)
                .await
                .map_err(|e| NostrManagerError::FailedToTrackPublishedEvent(e.to_string()))?;
        }
        Ok(result)
    }

    /// Publishes a Nostr event builder using a temporary signer.
    ///
    /// This method signs and publishes an event builder using the provided signer within a scoped
    /// context via `with_signer`. The signer is only active for the duration of the publish operation.
    /// The method ensures that the client is connected to all specified relays before attempting to publish.
    ///
    /// Automatically tracks published events in the database using the signer's public key.
    async fn publish_event_builder_with_signer(
        &self,
        event_builder: EventBuilder,
        relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        // Get the public key from the signer for account lookup
        let pubkey = signer.get_public_key().await?;

        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;
        let result = self
            .with_signer(signer, || async {
                self.client
                    .send_event_builder_to(relays, event_builder)
                    .await
                    .map_err(NostrManagerError::Client)
            })
            .await?;

        // Track the published event if we have a successful result (best-effort)
        if !result.success.is_empty() {
            self.event_tracker
                .track_published_event(result.id(), &pubkey)
                .await
                .map_err(|e| NostrManagerError::FailedToTrackPublishedEvent(e.to_string()))?;
        }

        Ok(result)
    }
}

#[cfg(test)]
mod publish_tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_publish_metadata_with_signer_no_relays() {
        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(crate::whitenoise::event_tracker::NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        let metadata = Metadata::new().name("test_user").display_name("Test User");
        let relays: Vec<RelayUrl> = vec![];
        let keys = Keys::generate();

        let result = nostr_manager
            .publish_metadata_with_signer(&metadata, &relays, keys)
            .await;

        assert!(result.is_err(), "Should fail with empty relays");
        let error_message = format!("{:?}", result.unwrap_err());
        assert!(
            error_message.contains("NoRelaysSpecified"),
            "Expected NoRelaysSpecified error, got: {}",
            error_message
        );
    }

    #[tokio::test]
    async fn test_publish_and_fetch_metadata() {
        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(crate::whitenoise::event_tracker::NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(10))
                .await
                .unwrap();

        let test_relays = vec![
            RelayUrl::parse("ws://localhost:8080").unwrap(),
            RelayUrl::parse("ws://localhost:7777").unwrap(),
        ];

        let test_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = Metadata::new()
            .name(format!("test_user_{}", test_timestamp))
            .display_name(format!("Test User {}", test_timestamp))
            .about("Integration test for metadata publishing");

        let keys = Keys::generate();

        let publish_result = nostr_manager
            .publish_metadata_with_signer(&metadata, &test_relays, keys.clone())
            .await;

        publish_result.expect("Failed to publish metadata. Are test relays running on localhost:8080 and localhost:7777?");

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let fetch_result = nostr_manager
            .fetch_metadata_from(&test_relays, keys.public_key())
            .await
            .expect("Failed to fetch metadata from relays");

        if let Some(event) = fetch_result {
            let event_metadata = Metadata::from_json(&event.content).unwrap();
            assert_eq!(event_metadata.name, metadata.name);
            assert_eq!(event_metadata.display_name, metadata.display_name);
            assert_eq!(event_metadata.about, metadata.about);
        }
    }

    #[tokio::test]
    async fn test_publish_follow_list_with_signer_empty_follow_list_non_empty_relays() {
        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(crate::whitenoise::event_tracker::NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        let follow_list: Vec<PublicKey> = vec![];
        let test_relay_url = RelayUrl::parse("wss://relay.example.com").unwrap();
        let relays = vec![test_relay_url];
        let keys = Keys::generate();

        let result = nostr_manager
            .publish_follow_list_with_signer(&follow_list, &relays, keys)
            .await;

        assert!(
            result.is_ok(),
            "Should succeed without sending an event when follow_list is empty but relays are provided"
        );
    }

    #[tokio::test]
    async fn test_publish_follow_list_with_signer_empty_follow_list_empty_relays() {
        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(crate::whitenoise::event_tracker::NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        let follow_list: Vec<PublicKey> = vec![];
        let relays: Vec<RelayUrl> = vec![];
        let keys = Keys::generate();

        let result = nostr_manager
            .publish_follow_list_with_signer(&follow_list, &relays, keys)
            .await;

        assert!(
            result.is_ok(),
            "Should succeed when follow_list is empty, regardless of relays"
        );
    }

    #[tokio::test]
    async fn test_publish_follow_list_with_signer_non_empty_follow_list_empty_relays() {
        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(crate::whitenoise::event_tracker::NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        let follow_list = vec![Keys::generate().public_key()];
        let relays: Vec<RelayUrl> = vec![];
        let keys = Keys::generate();

        let result = nostr_manager
            .publish_follow_list_with_signer(&follow_list, &relays, keys)
            .await;

        assert!(
            result.is_err(),
            "Should fail with empty relays when follow_list is not empty"
        );
        let error_message = format!("{:?}", result.unwrap_err());
        assert!(
            error_message.contains("NoRelaysSpecified"),
            "Expected NoRelaysSpecified error, got: {}",
            error_message
        );
    }

    #[tokio::test]
    async fn test_giftwrap_ephemeral_key_security_issue() {
        use crate::whitenoise::test_utils::*;

        let (_whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test keys
        let real_keys = create_test_keys();
        let real_pubkey = real_keys.public_key();

        // Create a test rumor
        let rumor = UnsignedEvent::new(
            real_pubkey,
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "test message".to_string(),
        );

        // Create a test receiver
        let receiver_keys = create_test_keys();
        let receiver_pubkey = receiver_keys.public_key();

        // Test the current implementation - this should reveal the security issue
        let wrapped_event =
            EventBuilder::gift_wrap(&real_keys, &receiver_pubkey, rumor.clone(), vec![])
                .await
                .unwrap();

        // SECURITY TEST: Check if the giftwrap event's author (pubkey) is the real account pubkey
        // This should NOT be the case - it should be an ephemeral key
        println!("Real account pubkey: {}", real_pubkey.to_hex());
        println!("Giftwrap event author: {}", wrapped_event.pubkey.to_hex());
        println!("Giftwrap event ID: {}", wrapped_event.id.to_hex());
        println!("Giftwrap event kind: {}", wrapped_event.kind.as_u16());

        // CRITICAL SECURITY ISSUE: If these are equal, the giftwrap is not using ephemeral keys!
        if wrapped_event.pubkey == real_pubkey {
            panic!(
                "🚨 SECURITY VULNERABILITY CONFIRMED: Giftwrap event is signed with real keys!\n\
                Real pubkey: {}\n\
                Giftwrap author: {}\n\
                The outer giftwrap event should be signed with an ephemeral keypair, not the real account keys!",
                real_pubkey.to_hex(),
                wrapped_event.pubkey.to_hex()
            );
        }

        // If we reach here, the implementation is correct (using ephemeral keys)
        println!("✅ SECURITY OK: Giftwrap event is signed with ephemeral keys");
        assert_ne!(
            wrapped_event.pubkey, real_pubkey,
            "Giftwrap should use ephemeral keys, not real keys"
        );
    }

    #[tokio::test]
    async fn test_publish_event_to_empty_relays() {
        use crate::whitenoise::accounts::Account;
        use crate::whitenoise::event_tracker::NoEventTracker;

        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        // Create a test account and keys
        let keys = Keys::generate();
        let account = Account {
            id: Some(1),
            pubkey: keys.public_key(),
            user_id: 1,
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create a test event
        let event_builder = EventBuilder::text_note("test message");
        let event = event_builder.sign_with_keys(&keys).unwrap();

        // Attempt to publish to empty relay list
        let relays: Vec<RelayUrl> = vec![];
        let result = nostr_manager
            .publish_event_to(event, &account, &relays)
            .await;

        // Publishing to empty relays should fail - nostr-sdk returns an error for empty targets
        assert!(result.is_err(), "Publishing to empty relays should fail");
    }

    #[tokio::test]
    async fn test_publish_event_to_unreachable_relays() {
        use crate::whitenoise::accounts::Account;
        use crate::whitenoise::event_tracker::NoEventTracker;

        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        // Create a test account and keys
        let keys = Keys::generate();
        let account = Account {
            id: Some(1),
            pubkey: keys.public_key(),
            user_id: 1,
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create a test event
        let event_builder = EventBuilder::text_note("test message");
        let event = event_builder.sign_with_keys(&keys).unwrap();

        // Use unreachable relays
        let relays = vec![
            RelayUrl::parse("ws://localhost:1").unwrap(), // Invalid port
            RelayUrl::parse("ws://localhost:2").unwrap(), // Invalid port
        ];

        let result = nostr_manager
            .publish_event_to(event, &account, &relays)
            .await;

        // The publish should succeed at the API level (returns Output)
        // but have no successful relay sends
        match result {
            Ok(output) => {
                // All relay publishes should have failed
                assert!(
                    output.success.is_empty(),
                    "No relays should have succeeded with unreachable endpoints"
                );
            }
            Err(_) => {
                // It's also acceptable for the entire operation to fail
                // due to connection issues
            }
        }
    }

    #[tokio::test]
    async fn test_publish_event_to_success() {
        use crate::whitenoise::accounts::Account;
        use crate::whitenoise::event_tracker::NoEventTracker;

        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(10))
                .await
                .unwrap();

        // Create a test account and keys
        let keys = Keys::generate();
        let account = Account {
            id: Some(1),
            pubkey: keys.public_key(),
            user_id: 1,
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create a unique test event to avoid conflicts
        let test_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let event_builder = EventBuilder::text_note(format!("test message {}", test_timestamp));
        let event = event_builder.sign_with_keys(&keys).unwrap();

        // Use test relays
        let test_relays = vec![
            RelayUrl::parse("ws://localhost:8080").unwrap(),
            RelayUrl::parse("ws://localhost:7777").unwrap(),
        ];

        let result = nostr_manager
            .publish_event_to(event.clone(), &account, &test_relays)
            .await;

        // Should succeed with at least some relay sends
        match result {
            Ok(output) => {
                // Should have the correct event ID
                assert_eq!(*output.id(), event.id);
                tracing::debug!(
                    "Published to {} successful relays, {} failed",
                    output.success.len(),
                    output.failed.len()
                );
            }
            Err(e) => {
                panic!(
                    "Failed to publish event: {:?}. Are test relays running on localhost:8080 and localhost:7777?",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_background_publish_event_to_completes() {
        use crate::whitenoise::accounts::Account;
        use crate::whitenoise::event_tracker::NoEventTracker;

        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(10))
                .await
                .unwrap();

        // Create a test account and keys
        let keys = Keys::generate();
        let account = Account {
            id: Some(1),
            pubkey: keys.public_key(),
            user_id: 1,
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create a test event
        let event_builder = EventBuilder::text_note("background test message");
        let event = event_builder.sign_with_keys(&keys).unwrap();

        // Use test relays
        let test_relays = vec![
            RelayUrl::parse("ws://localhost:8080").unwrap(),
            RelayUrl::parse("ws://localhost:7777").unwrap(),
        ];

        // Call background publish (fire-and-forget)
        nostr_manager.background_publish_event_to(event, &account, test_relays);

        // Give the background task time to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // No assertions - just verify it doesn't panic or hang
        // The background task logs success/failure internally
    }

    #[tokio::test]
    async fn test_background_publish_event_to_with_unreachable_relays() {
        use crate::whitenoise::accounts::Account;
        use crate::whitenoise::event_tracker::NoEventTracker;

        let (sender, _receiver) = mpsc::channel(100);
        let event_tracker = Arc::new(NoEventTracker);
        let nostr_manager =
            NostrManager::new(sender, event_tracker, std::time::Duration::from_secs(5))
                .await
                .unwrap();

        // Create a test account and keys
        let keys = Keys::generate();
        let account = Account {
            id: Some(1),
            pubkey: keys.public_key(),
            user_id: 1,
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create a test event
        let event_builder = EventBuilder::text_note("background test with unreachable relays");
        let event = event_builder.sign_with_keys(&keys).unwrap();

        // Use unreachable relays
        let relays = vec![
            RelayUrl::parse("ws://localhost:1").unwrap(),
            RelayUrl::parse("ws://localhost:2").unwrap(),
        ];

        // Call background publish - should not panic even with unreachable relays
        nostr_manager.background_publish_event_to(event, &account, relays);

        // Give the background task time to complete and log the error
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // No assertions - verify it handles errors gracefully without panicking
    }
}
