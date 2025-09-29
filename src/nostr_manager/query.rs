//! This module contains functions for querying Nostr events from relays.

use nostr_sdk::prelude::*;

use crate::{
    nostr_manager::{utils::is_event_timestamp_valid, NostrManager, Result},
    RelayType,
};

impl NostrManager {
    pub(crate) async fn fetch_metadata_from(
        &self,
        nip65_relay_urls: &[RelayUrl],
        pubkey: PublicKey,
    ) -> Result<Option<Event>> {
        let filter: Filter = Filter::new().author(pubkey).kind(Kind::Metadata);
        let events: Events = self
            .client
            .fetch_events_from(nip65_relay_urls, filter, self.timeout)
            .await?;
        Self::latest_from_events(events)
    }

    pub(crate) async fn fetch_user_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
        nip65_relay_urls: &[RelayUrl],
    ) -> Result<Option<Event>> {
        let filter = Filter::new().author(pubkey).kind(relay_type.into());
        let events = self
            .client
            .fetch_events_from(nip65_relay_urls, filter, self.timeout)
            .await?;
        Self::latest_from_events(events)
    }

    pub(crate) async fn fetch_user_key_package(
        &self,
        pubkey: PublicKey,
        relays: &[RelayUrl],
    ) -> Result<Option<Event>> {
        let filter = Filter::new().kind(Kind::MlsKeyPackage).author(pubkey);
        let events = self
            .client
            .fetch_events_from(relays, filter, self.timeout)
            .await?;
        Self::latest_from_events(events)
    }

    fn latest_from_events(events: Events) -> Result<Option<Event>> {
        let latest = events
            .into_iter()
            .filter(is_event_timestamp_valid)
            .max_by_key(|e| (e.created_at, e.id));
        Ok(latest)
    }
}

#[cfg(test)]
mod contact_list_logic_tests {
    use super::*;
    use std::collections::HashMap;

    // Test data for problematic contact list
    fn get_test_contact_list_event() -> Event {
        let json = r#"{
            "kind": 3,
            "id": "ebdd64bb88ad560aaf949f9c2fc7a5a7bba82100f5767dd4a6422a4cef646951",
            "pubkey": "991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702",
            "created_at": 1752141958,
            "tags": [
              ["e", "25e5c82273a271cb1a840d0060391a0bf4965cafeb029d5ab55350b418953fbb"],
              ["e", "42224859763652914db53052103f0b744df79dfc4efef7e950fc0802fc3df3c5"],
              ["alt", "Follow List"],
              ["p", "e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad"],
              ["p", "c2827524936dedad5f623bcf8a04d201f3fd3ed7d4912a190dbeef685f45b2f7"],
              ["p", "eba7c2b111a28fa8e7cb07f1ae0feef490d49d897bd7b1fb5ce5d3f0d6739e8f"],
              ["p", "ef151c7a380f40a75d7d1493ac347b6777a9d9b5fa0aa3cddb47fc78fab69a8b"],
              ["p", "234c45ff85a31c19bf7108a747fa7be9cd4af95c7d621e07080ca2d663bb47d2"],
              ["p", "8664ff363efcd36a154efdcbc629a4d1e4c511f9114e1d35de73fff31cb783b3"],
              ["p", "6e468422dfb74a5738702a8823b9b28168abab8655faacb6853cd0ee15deee93"],
              ["p", "aac07d95089ce6adf08b9156d43c1a4ab594c6130b7dcb12ec199008c5819a2f"]
            ],
            "content": "{\"wss://nostr.bitcoiner.social/\":{\"read\":true,\"write\":true},\"wss://relay.nostr.bg/\":{\"read\":true,\"write\":true},\"wss://nostr.oxtr.dev/\":{\"read\":true,\"write\":true},\"wss://nostr.fmt.wiz.biz/\":{\"read\":true,\"write\":false},\"wss://relay.damus.io/\":{\"read\":true,\"write\":true},\"wss://nostr.mom/\":{\"read\":true,\"write\":true},\"wss://nos.lol/\":{\"read\":true,\"write\":true},\"wss://nostr.wine/\":{\"read\":true,\"write\":false},\"wss://relay.nostr.band/\":{\"read\":true,\"write\":false},\"wss://relay.noswhere.com/\":{\"read\":true,\"write\":false}}",
            "sig": "8c174dbb1d88065c3d34a4f40d15eda1160a3f041f29e87f881afb44058d8e5405fe02db63655903925f439f64445409b2acad62e059ac9c152e7442972f6ede"
        }"#;

        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn test_contact_list_with_mixed_tags() {
        let event = get_test_contact_list_event();

        // Count tags by type
        let e_tags = event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::e())
            .count();
        let p_tags = event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .count();
        let alt_tags = event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::Custom("alt".into()))
            .count();

        // Verify tag counts
        assert_eq!(e_tags, 2);
        assert_eq!(p_tags, 8);
        assert_eq!(alt_tags, 1);

        // Now extract contacts
        let contacts = NostrManager::pubkeys_from_event(&event);

        // Verify we only get the p tags as contacts
        assert_eq!(contacts.len(), 8);
    }

    #[test]
    fn test_contact_list_with_relay_preferences() {
        let event = get_test_contact_list_event();

        // Verify content contains relay preferences
        assert!(event.content.contains("wss://"));
        assert!(event.content.contains("read"));
        assert!(event.content.contains("write"));

        // Extract contacts - should work despite complex content
        let contacts = NostrManager::pubkeys_from_event(&event);
        assert_eq!(contacts.len(), 8);

        // Check specific contacts
        let expected_pubkey =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        assert!(contacts.contains(&expected_pubkey));
    }

    #[test]
    fn test_contact_list_with_future_timestamp() {
        let event = get_test_contact_list_event();
        let timestamp = Timestamp::from(1752141958);

        // The event timestamp was from the future when this test was written,
        // but it might not be in the future anymore as time passes
        // Uncomment to check if it's still in future:
        // let current_timestamp = Timestamp::now();
        // println!("Event timestamp: {}, Current time: {}", event.created_at, current_timestamp);

        // Check that we can parse and process events with timestamps from the far future
        // Regardless of whether that time has now passed
        let contacts = NostrManager::pubkeys_from_event(&event);
        assert_eq!(contacts.len(), 8);

        // Verify we extracted the correct timestamp from the event
        assert_eq!(event.created_at, timestamp);
    }

    #[tokio::test]
    async fn test_create_contact_list_hashmap() {
        let event = get_test_contact_list_event();
        let contacts_pubkeys = NostrManager::pubkeys_from_event(&event);
        assert_eq!(contacts_pubkeys.len(), 8);

        // Create the HashMap as done in fetch_user_contact_list
        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        for contact in contacts_pubkeys {
            contacts_metadata.insert(contact, None);
        }

        // Verify HashMap was created correctly
        assert_eq!(contacts_metadata.len(), 8);

        // Check specific contacts
        let test_pubkey =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        assert!(contacts_metadata.contains_key(&test_pubkey));
        assert!(contacts_metadata.get(&test_pubkey).unwrap().is_none());
    }

    #[tokio::test]
    async fn test_mock_query_user_contact_list() {
        // We don't need the temp dir and channels for this test, so we'll skip them

        // Mock the database query to return our test event
        let event = get_test_contact_list_event();

        // Simulate the logic of query_user_contact_list
        let contacts_pubkeys = if let Some(event) = Some(&event) {
            event
                .tags
                .iter()
                .filter(|tag| tag.kind() == TagKind::p())
                .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
                .collect::<Vec<PublicKey>>()
        } else {
            vec![]
        };

        // Create the contact metadata HashMap
        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        for contact in contacts_pubkeys {
            contacts_metadata.insert(contact, None);
        }

        // Verify results
        assert_eq!(contacts_metadata.len(), 8);

        // Check for specific contact
        let test_pubkey =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        assert!(contacts_metadata.contains_key(&test_pubkey));
    }

    #[tokio::test]
    async fn test_handle_duplicate_contacts() {
        // Create a contact list with duplicate p tags
        let contact1 =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        let contact2 =
            PublicKey::from_hex("c2827524936dedad5f623bcf8a04d201f3fd3ed7d4912a190dbeef685f45b2f7")
                .unwrap();

        // Create a mock event with duplicate contacts
        let event_json = format!(
            r#"{{
            "kind": 3,
            "id": "ebdd64bb88ad560aaf949f9c2fc7a5a7bba82100f5767dd4a6422a4cef646951",
            "pubkey": "991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702",
            "created_at": 1752141958,
            "tags": [
              ["p", "{}"],
              ["p", "{}"],
              ["p", "{}"],
              ["e", "25e5c82273a271cb1a840d0060391a0bf4965cafeb029d5ab55350b418953fbb"],
              ["alt", "Follow List"]
            ],
            "content": "{{}}",
            "sig": "8c174dbb1d88065c3d34a4f40d15eda1160a3f041f29e87f881afb44058d8e5405fe02db63655903925f439f64445409b2acad62e059ac9c152e7442972f6ede"
        }}"#,
            contact1.to_hex(),
            contact2.to_hex(),
            contact1.to_hex()
        );

        let event: Event = serde_json::from_str(&event_json).unwrap();

        // Extract contacts
        let contacts = NostrManager::pubkeys_from_event(&event);

        // Check for duplicate contacts
        let unique_contacts: std::collections::HashSet<_> = contacts.iter().cloned().collect();

        // We should have duplicates in the original list
        assert_eq!(contacts.len(), 3);
        assert_eq!(unique_contacts.len(), 2);

        // Count occurrences of each contact
        let contact1_count = contacts.iter().filter(|&c| *c == contact1).count();
        let contact2_count = contacts.iter().filter(|&c| *c == contact2).count();

        assert_eq!(contact1_count, 2); // Duplicate should be counted twice in the original list
        assert_eq!(contact2_count, 1);

        // Now create HashMap to check how duplicates are handled there
        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        for contact in contacts {
            contacts_metadata.insert(contact, None);
        }

        // Verify HashMap has the right count (deduplicated)
        assert_eq!(contacts_metadata.len(), 2);
        assert!(contacts_metadata.contains_key(&contact1));
        assert!(contacts_metadata.contains_key(&contact2));
    }

    #[test]
    fn test_contact_list_is_parseable() {
        // Test that we can correctly parse the event JSON
        let event_json = r#"{
            "kind": 3,
            "id": "ebdd64bb88ad560aaf949f9c2fc7a5a7bba82100f5767dd4a6422a4cef646951",
            "pubkey": "991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702",
            "created_at": 1752141958,
            "tags": [
              ["e", "25e5c82273a271cb1a840d0060391a0bf4965cafeb029d5ab55350b418953fbb"],
              ["e", "42224859763652914db53052103f0b744df79dfc4efef7e950fc0802fc3df3c5"],
              ["alt", "Follow List"],
              ["p", "e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad"],
              ["p", "c2827524936dedad5f623bcf8a04d201f3fd3ed7d4912a190dbeef685f45b2f7"]
            ],
            "content": "{\"wss://relay.example.com\":{\"read\":true,\"write\":true}}",
            "sig": "8c174dbb1d88065c3d34a4f40d15eda1160a3f041f29e87f881afb44058d8e5405fe02db63655903925f439f64445409b2acad62e059ac9c152e7442972f6ede"
        }"#;

        let event: Event = serde_json::from_str(event_json).unwrap();

        // Check that event fields are correctly parsed
        assert_eq!(event.kind, Kind::ContactList);
        assert_eq!(
            event.pubkey,
            PublicKey::from_hex("991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702")
                .unwrap()
        );
        assert_eq!(event.created_at.as_u64(), 1752141958);

        // Check that tags are correctly parsed
        assert_eq!(event.tags.len(), 5);

        // Extract contacts
        let contacts = NostrManager::pubkeys_from_event(&event);
        assert_eq!(contacts.len(), 2);
    }

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
