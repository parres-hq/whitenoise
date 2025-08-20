//! Query functions for NostrManager
//! This handles fetching events from the database cache.

use crate::whitenoise::relays::Relay;
use crate::{
    nostr_manager::{NostrManager, Result},
    RelayType,
};
use nostr_sdk::prelude::*;
use std::collections::HashSet;

impl NostrManager {
    pub(crate) async fn fetch_metadata_from(
        &self,
        nip65_relays: &[Relay],
        pubkey: PublicKey,
    ) -> Result<Option<Metadata>> {
        let filter: Filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
        let urls: Vec<RelayUrl> = nip65_relays.iter().map(|r| r.url.clone()).collect();
        let events: Events = self
            .client
            .fetch_events_from(urls, filter, self.timeout)
            .await?;
        match events.first() {
            Some(event) => Ok(Some(Metadata::try_from(event)?)),
            None => Ok(None),
        }
    }

    pub(crate) async fn publish_metadata_with_signer(
        &self,
        metadata: &Metadata,
        relays: &[Relay],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        let event_builder = EventBuilder::metadata(metadata);
        self.publish_event_builder_with_signer(event_builder, relays, signer)
            .await
    }

    pub(crate) async fn publish_relay_list_with_signer(
        &self,
        relay_list: &[Relay],
        relay_type: RelayType,
        target_relays: &[Relay],
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

    pub(crate) async fn publish_follow_list_with_signer(
        &self,
        follow_list: &[PublicKey],
        target_relays: &[Relay],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        let tags: Vec<Tag> = follow_list
            .iter()
            .map(|pubkey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();
        let event = EventBuilder::new(Kind::ContactList, "").tags(tags);
        let result = self.publish_event_builder_with_signer(event, target_relays, signer).await?;
        tracing::debug!(target: "whitenoise::nostr_manager::publish_follow_list_with_signer", "Published follow list event to Nostr: {:?}", result);
        Ok(())
    }

    pub(crate) async fn fetch_user_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
        nip65_relays: &[Relay],
    ) -> Result<HashSet<RelayUrl>> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(relay_type.into())
            .limit(1);
        let urls: Vec<RelayUrl> = nip65_relays.iter().map(|r| r.url.clone()).collect();
        let relay_events = self
            .client
            .fetch_events_from(urls, filter.clone(), self.timeout)
            .await?;
        tracing::debug!("Fetched relay events {:?}", relay_events);

        match relay_events.first() {
            None => Ok(HashSet::new()),
            Some(event) => Ok(Self::relay_urls_from_event(event.clone())),
        }
    }

    pub(crate) async fn fetch_user_key_package(
        &self,
        pubkey: PublicKey,
        relays: &[Relay],
    ) -> Result<Option<Event>> {
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();
        let filter = Filter::new()
            .kind(Kind::MlsKeyPackage)
            .author(pubkey)
            .limit(1);
        let events = self
            .client
            .fetch_events_from(urls, filter.clone(), self.timeout)
            .await?;

        Ok(events.first_owned())
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use chrono::Utc;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_publish_metadata_with_signer_no_relays() {
        let (sender, _receiver) = mpsc::channel(100);
        let nostr_manager = NostrManager::new(sender, std::time::Duration::from_secs(5))
            .await
            .unwrap();

        let metadata = Metadata::new().name("test_user").display_name("Test User");
        let relays: Vec<Relay> = vec![];
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
        let nostr_manager = NostrManager::new(sender, std::time::Duration::from_secs(10))
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

        let fetch_result = nostr_manager
            .fetch_metadata_from(&test_relays, keys.public_key())
            .await
            .expect("Failed to fetch metadata from relays");

        if let Some(fetched_metadata) = fetch_result {
            assert_eq!(fetched_metadata.name, metadata.name);
            assert_eq!(fetched_metadata.display_name, metadata.display_name);
            assert_eq!(fetched_metadata.about, metadata.about);
        }
    }
}

#[cfg(test)]
mod contact_list_logic_tests {
    use super::*;
    use std::collections::HashMap;

    // Helper to create test metadata
    fn create_test_metadata(name: &str, display_name: &str, about: &str) -> Metadata {
        Metadata::new()
            .name(name)
            .display_name(display_name)
            .about(about)
    }

    #[test]
    fn test_parse_contact_list_tags_empty() {
        // Test parsing empty contact list
        let tags: Vec<Tag> = vec![];

        let contacts_pubkeys: Vec<PublicKey> = tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
            .collect();

        assert_eq!(contacts_pubkeys.len(), 0);
    }

    #[test]
    fn test_parse_contact_list_tags_single_contact() {
        // Test parsing contact list with one valid contact
        let test_pubkey = Keys::generate().public_key();
        let tags = vec![Tag::custom(TagKind::p(), [test_pubkey.to_hex()])];

        let contacts_pubkeys: Vec<PublicKey> = tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
            .collect();

        assert_eq!(contacts_pubkeys.len(), 1);
        assert_eq!(contacts_pubkeys[0], test_pubkey);
    }

    #[test]
    fn test_parse_contact_list_tags_multiple_contacts() {
        // Test parsing contact list with multiple valid contacts
        let test_pubkey1 = Keys::generate().public_key();
        let test_pubkey2 = Keys::generate().public_key();
        let test_pubkey3 = Keys::generate().public_key();

        let tags = vec![
            Tag::custom(TagKind::p(), [test_pubkey1.to_hex()]),
            Tag::custom(TagKind::p(), [test_pubkey2.to_hex()]),
            Tag::custom(TagKind::p(), [test_pubkey3.to_hex()]),
        ];

        let contacts_pubkeys: Vec<PublicKey> = tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
            .collect();

        assert_eq!(contacts_pubkeys.len(), 3);
        assert!(contacts_pubkeys.contains(&test_pubkey1));
        assert!(contacts_pubkeys.contains(&test_pubkey2));
        assert!(contacts_pubkeys.contains(&test_pubkey3));
    }

    #[test]
    fn test_parse_contact_list_tags_ignore_non_p_tags() {
        // Test that non-p tags are correctly ignored
        let test_pubkey = Keys::generate().public_key();
        let tags = vec![
            Tag::custom(TagKind::p(), [test_pubkey.to_hex()]),
            Tag::custom(TagKind::Custom("e".into()), vec!["some_event_id"]),
            Tag::custom(TagKind::Custom("d".into()), vec!["some_identifier"]),
            Tag::hashtag("nostr"),
        ];

        let contacts_pubkeys: Vec<PublicKey> = tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
            .collect();

        assert_eq!(contacts_pubkeys.len(), 1);
        assert_eq!(contacts_pubkeys[0], test_pubkey);
    }

    #[test]
    #[should_panic]
    fn test_parse_contact_list_tags_invalid_hex() {
        // Test that invalid hex strings cause panic (expected behavior with unwrap())
        let tags = vec![Tag::custom(TagKind::p(), vec!["invalid_hex_string"])];

        let _: Vec<PublicKey> = tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
            .collect();
    }

    #[test]
    fn test_metadata_association_consistency() {
        // Test that metadata is correctly associated with the right public keys
        // This simulates the core logic of query_user_contact_list

        let contact1 = Keys::generate().public_key();
        let contact2 = Keys::generate().public_key();
        let contact3 = Keys::generate().public_key();

        let metadata1 = Some(create_test_metadata(
            "alice",
            "Alice Smith",
            "Software developer",
        ));
        let metadata2 = None; // No metadata for contact2
        let metadata3 = Some(create_test_metadata(
            "carol",
            "Carol Brown",
            "Product manager",
        ));

        // Simulate the contacts and their corresponding metadata
        let contacts_and_metadata = vec![
            (contact1, metadata1.clone()),
            (contact2, metadata2.clone()),
            (contact3, metadata3.clone()),
        ];

        // Build the HashMap as the function would
        let mut contacts_metadata = HashMap::new();
        for (contact, metadata) in contacts_and_metadata {
            contacts_metadata.insert(contact, metadata);
        }

        // Verify correct associations
        assert_eq!(contacts_metadata.len(), 3);

        // Verify contact1 has the correct metadata
        let retrieved_meta1 = contacts_metadata.get(&contact1).unwrap();
        assert!(retrieved_meta1.is_some());
        let meta1 = retrieved_meta1.as_ref().unwrap();
        assert_eq!(meta1.name, Some("alice".to_string()));
        assert_eq!(meta1.display_name, Some("Alice Smith".to_string()));
        assert_eq!(meta1.about, Some("Software developer".to_string()));

        // Verify contact2 has no metadata
        let retrieved_meta2 = contacts_metadata.get(&contact2).unwrap();
        assert!(retrieved_meta2.is_none());

        // Verify contact3 has the correct metadata
        let retrieved_meta3 = contacts_metadata.get(&contact3).unwrap();
        assert!(retrieved_meta3.is_some());
        let meta3 = retrieved_meta3.as_ref().unwrap();
        assert_eq!(meta3.name, Some("carol".to_string()));
        assert_eq!(meta3.display_name, Some("Carol Brown".to_string()));
        assert_eq!(meta3.about, Some("Product manager".to_string()));

        // Critical test: Verify no metadata cross-contamination
        // Contact2 should not have metadata from contact1 or contact3
        assert_ne!(retrieved_meta2, retrieved_meta1);
        assert_ne!(retrieved_meta2, retrieved_meta3);

        // Contact1 and contact3 should have different metadata
        assert_ne!(retrieved_meta1, retrieved_meta3);
        if let (Some(m1), Some(m3)) = (retrieved_meta1.as_ref(), retrieved_meta3.as_ref()) {
            assert_ne!(m1.name, m3.name);
            assert_ne!(m1.display_name, m3.display_name);
            assert_ne!(m1.about, m3.about);
        }
    }

    #[test]
    fn test_metadata_association_all_none() {
        // Test case where all contacts have no metadata
        let contact1 = Keys::generate().public_key();
        let contact2 = Keys::generate().public_key();
        let contact3 = Keys::generate().public_key();

        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        contacts_metadata.insert(contact1, None);
        contacts_metadata.insert(contact2, None);
        contacts_metadata.insert(contact3, None);

        assert_eq!(contacts_metadata.len(), 3);
        assert!(contacts_metadata.get(&contact1).unwrap().is_none());
        assert!(contacts_metadata.get(&contact2).unwrap().is_none());
        assert!(contacts_metadata.get(&contact3).unwrap().is_none());
    }

    #[test]
    fn test_metadata_association_all_some() {
        // Test case where all contacts have unique metadata
        let contact1 = Keys::generate().public_key();
        let contact2 = Keys::generate().public_key();
        let contact3 = Keys::generate().public_key();

        let metadata1 = Some(create_test_metadata("user1", "User One", "First user"));
        let metadata2 = Some(create_test_metadata("user2", "User Two", "Second user"));
        let metadata3 = Some(create_test_metadata("user3", "User Three", "Third user"));

        let mut contacts_metadata = HashMap::new();
        contacts_metadata.insert(contact1, metadata1);
        contacts_metadata.insert(contact2, metadata2);
        contacts_metadata.insert(contact3, metadata3);

        assert_eq!(contacts_metadata.len(), 3);

        // Verify each contact has unique metadata
        let meta1 = contacts_metadata.get(&contact1).unwrap().as_ref().unwrap();
        let meta2 = contacts_metadata.get(&contact2).unwrap().as_ref().unwrap();
        let meta3 = contacts_metadata.get(&contact3).unwrap().as_ref().unwrap();

        // All should be different
        assert_eq!(meta1.name, Some("user1".to_string()));
        assert_eq!(meta2.name, Some("user2".to_string()));
        assert_eq!(meta3.name, Some("user3".to_string()));

        assert_ne!(meta1.name, meta2.name);
        assert_ne!(meta1.name, meta3.name);
        assert_ne!(meta2.name, meta3.name);
    }

    #[test]
    fn test_hashmap_key_integrity() {
        // Regression test to ensure PublicKey hash/equality works correctly
        let original_key = Keys::generate().public_key();
        let metadata = Some(create_test_metadata("test", "Test User", "Test metadata"));

        let mut contacts_metadata = HashMap::new();
        contacts_metadata.insert(original_key, metadata);

        // The same key should retrieve the same metadata
        let retrieved = contacts_metadata.get(&original_key);
        assert!(retrieved.is_some());
        assert!(retrieved.unwrap().is_some());

        // A different key should not retrieve anything
        let different_key = Keys::generate().public_key();
        let not_found = contacts_metadata.get(&different_key);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_duplicate_contacts_deduplication() {
        // Test scenario where the same contact appears multiple times in the list
        let contact1 = Keys::generate().public_key();
        let contact2 = Keys::generate().public_key();

        // Simulate a contact list with duplicates
        let contacts_with_duplicates = vec![contact1, contact2, contact1, contact2, contact1];

        // Build HashMap (which naturally deduplicates)
        let mut contacts_metadata = HashMap::new();
        for contact in contacts_with_duplicates {
            // Simulate different metadata for each insertion to test overwriting
            let metadata = if contact == contact1 {
                Some(create_test_metadata("alice", "Alice", "Alice's metadata"))
            } else {
                Some(create_test_metadata("bob", "Bob", "Bob's metadata"))
            };
            contacts_metadata.insert(contact, metadata);
        }

        // Should only have 2 unique contacts
        assert_eq!(contacts_metadata.len(), 2);
        assert!(contacts_metadata.contains_key(&contact1));
        assert!(contacts_metadata.contains_key(&contact2));

        // Each should have their respective metadata (last inserted wins)
        let meta1 = contacts_metadata.get(&contact1).unwrap().as_ref().unwrap();
        let meta2 = contacts_metadata.get(&contact2).unwrap().as_ref().unwrap();

        assert_eq!(meta1.name, Some("alice".to_string()));
        assert_eq!(meta2.name, Some("bob".to_string()));
    }
}
