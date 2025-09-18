//! This module contains functions for publishing Nostr events and handling the publish tracking process.

use nostr_sdk::prelude::*;

use crate::{
    nostr_manager::{NostrManager, NostrManagerError, Result},
    whitenoise::{
        accounts::Account,
        relays::{Relay, RelayType},
    },
};

impl NostrManager {
    /// Publishes an MLS commit event to the specified relays.
    ///
    /// This method allows publishing an MLS commit event (which is already signed) to a list of relay URLs. It ensures that the client
    /// is connected to all specified relays before attempting to publish the event and then tracks that we published the event to the database.
    /// This is the public crate interface to ensure you're using the correct publishing method.
    pub(crate) async fn publish_mls_commit_to(
        &self,
        event: Event,
        account: &Account,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<Output<EventId>> {
        self.publish_event_to(event, account, relays).await
    }

    /// Publishes an MLS message event to the specified relays.
    ///
    /// This method allows publishing an MLS message event (which is already signed) to a list of relay URLs. It ensures that the client
    /// is connected to all specified relays before attempting to publish the event and then tracks that we published the event to the database.
    /// This is the public crate interface to ensure you're using the correct publishing method.
    pub(crate) async fn publish_mls_message_to(
        &self,
        event: Event,
        account: &Account,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<Output<EventId>> {
        self.publish_event_to(event, account, relays).await
    }

    /// Constructs and publishes a Nostr gift wrap event using a temporary signer.
    ///
    /// This method creates a gift-wrapped Nostr event and publishes it to specified relays using a
    /// temporary signer. Gift wrapping provides privacy by encrypting the inner event (rumor) and
    /// hiding the recipient's identity from relay operators and other observers.
    ///
    /// The signer is set before publishing and automatically unset immediately after the operation
    /// completes, ensuring it doesn't persist in the client state. This method also ensures that
    /// the client is connected to all specified relays before attempting to publish.
    pub(crate) async fn publish_gift_wrap_to(
        &self,
        receiver: &PublicKey,
        rumor: UnsignedEvent,
        extra_tags: &[Tag],
        account: &Account,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let wrapped_event =
            EventBuilder::gift_wrap(&signer, receiver, rumor, extra_tags.to_vec()).await?;
        self.publish_event_to(wrapped_event, account, relays).await
    }

    /// Publishes a Nostr metadata event using a passed signer.
    pub(crate) async fn publish_metadata_with_signer(
        &self,
        metadata: &Metadata,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let event_builder = EventBuilder::metadata(metadata);
        self.publish_event_builder_with_signer(event_builder, relays, signer)
            .await
    }

    /// Publishes a Nostr relay list event using a passed signer.
    pub(crate) async fn publish_relay_list_with_signer(
        &self,
        relay_list: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        relay_type: RelayType,
        target_relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        let tags: Vec<Tag> = match relay_type {
            RelayType::Nip65 => relay_list
                .iter()
                .map(|relay| Tag::reference(relay.url.to_string()))
                .collect(),
            RelayType::Inbox | RelayType::KeyPackage => relay_list
                .iter()
                .map(|relay| Tag::custom(TagKind::Relay, [relay.url.to_string()]))
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

    /// Publishes a Nostr follow list event using a passed signer.
    pub(crate) async fn publish_follow_list_with_signer(
        &self,
        follow_list: &[PublicKey],
        target_relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
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

    /// Publishes a Nostr key package event using a passed signer.
    pub(crate) async fn publish_key_package_with_signer(
        &self,
        encoded_key_package: &str,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        tags: &[Tag],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package).tags(tags.to_vec());

        self.publish_event_builder_with_signer(key_package_event_builder, relays, signer)
            .await
    }

    /// Publishes a Nostr event deletion event using a passed signer.
    pub(crate) async fn publish_event_deletion_with_signer(
        &self,
        event_id: &EventId,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let event_deletion_event_builder =
            EventBuilder::delete(EventDeletionRequest::new().id(*event_id));
        self.publish_event_builder_with_signer(event_deletion_event_builder, relays, signer)
            .await
    }

    /// Publishes a Nostr event (which is already signed) to the specified relays.
    ///
    /// This method allows publishing an already signed event to a list of relay URLs. It ensures that the client
    /// is connected to all specified relays before attempting to publish the event and then tracks that we published the event to the database.
    async fn publish_event_to(
        &self,
        event: Event,
        account: &Account,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<Output<EventId>> {
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();

        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(&urls).await?;
        let result = self.client.send_event_to(urls, &event).await?;

        // Track the published event if we have a successful result (best-effort)
        if !result.success.is_empty() {
            self.event_tracker
                .track_published_event(result.id(), &account.pubkey)
                .await
                .map_err(|e| NostrManagerError::FailedToTrackPublishedEvent(e.to_string()))?;
        }
        Ok(result)
    }

    /// Publishes a Nostr event using a temporary signer.
    ///
    /// This method allows publishing an event with a signer that is only used for this specific operation.
    /// The signer is set before publishing and unset immediately after. This method also ensures that
    /// the client is connected to all specified relays before attempting to publish.
    ///
    /// Automatically tracks published events in the database by looking up the account from the signer's public key.
    async fn publish_event_builder_with_signer(
        &self,
        event_builder: EventBuilder,
        relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        // Get the public key from the signer for account lookup
        let pubkey = signer.get_public_key().await?;
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();

        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(&urls).await?;
        let result = self
            .with_signer(signer, || async {
                self.client
                    .send_event_builder_to(urls, event_builder)
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
        let relays: Vec<crate::whitenoise::relays::Relay> = vec![];
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
            crate::whitenoise::relays::Relay {
                id: None,
                url: RelayUrl::parse("ws://localhost:8080").unwrap(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            crate::whitenoise::relays::Relay {
                id: None,
                url: RelayUrl::parse("ws://localhost:7777").unwrap(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
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

        let test_relay_urls: Vec<RelayUrl> = test_relays.iter().map(|r| r.url.clone()).collect();
        let fetch_result = nostr_manager
            .fetch_metadata_from(&test_relay_urls, keys.public_key())
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
        let relays = vec![crate::whitenoise::relays::Relay {
            id: None,
            url: test_relay_url,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];
        let keys = Keys::generate();

        let result = nostr_manager
            .publish_follow_list_with_signer(&follow_list, &relays, keys)
            .await;

        assert!(result.is_ok(), "Should succeed without sending an event when follow_list is empty but relays are provided");
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
        let relays: Vec<crate::whitenoise::relays::Relay> = vec![];
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
        let relays: Vec<crate::whitenoise::relays::Relay> = vec![];
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
}
