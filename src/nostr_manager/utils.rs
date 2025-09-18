use std::collections::HashSet;

use nostr_sdk::prelude::*;

use crate::nostr_manager::NostrManager;

impl NostrManager {
    /// Extracts public keys from an event's tags.
    pub(crate) fn pubkeys_from_event(event: &Event) -> Vec<PublicKey> {
        event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)))
            .filter_map(|tag| tag.content().and_then(|c| c.parse::<PublicKey>().ok()))
            .collect()
    }

    /// Extracts relay URLs from an event's tags.
    pub(crate) fn relay_urls_from_event(event: &Event) -> HashSet<RelayUrl> {
        event
            .tags
            .iter()
            .filter(|tag| Self::is_relay_list_tag_for_event_kind(tag, event.kind))
            .filter_map(|tag| {
                tag.content()
                    .and_then(|content| RelayUrl::parse(content).ok())
            })
            .collect()
    }

    /// Determines if a tag is relevant for the given relay list event kind.
    /// Different relay list kinds use different tag types:
    /// - Kind::RelayList (10002) uses "r" tags (TagKind::SingleLetter)
    /// - Kind::InboxRelays (10050) and Kind::MlsKeyPackageRelays (10051) use "relay" tags (TagKind::Relay)
    pub(crate) fn is_relay_list_tag_for_event_kind(tag: &Tag, kind: Kind) -> bool {
        match kind {
            Kind::RelayList => Self::is_r_tag(tag),
            Kind::InboxRelays | Kind::MlsKeyPackageRelays => Self::is_relay_tag(tag),
            _ => Self::is_relay_tag(tag) || Self::is_r_tag(tag), // backward compatibility
        }
    }

    /// Checks if a tag is an "r" tag.
    pub(crate) fn is_r_tag(tag: &Tag) -> bool {
        tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R))
    }

    /// Checks if a tag is a "relay" tag.
    pub(crate) fn is_relay_tag(tag: &Tag) -> bool {
        tag.kind() == TagKind::Relay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_relay_urls_from_event_relay_list() {
        use nostr_sdk::prelude::*;

        // Test Kind::RelayList (10002) with "r" tags
        let keys = Keys::generate();

        let r_tags = vec![
            Tag::reference("wss://relay1.example.com"),
            Tag::reference("wss://relay2.example.com"),
            // Add a relay tag that should be ignored for RelayList
            Tag::custom(TagKind::Relay, ["wss://should-be-ignored.com"]),
        ];

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(r_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(&event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://relay1.example.com").unwrap()));
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://relay2.example.com").unwrap()));
        assert!(!parsed_relays.contains(&RelayUrl::parse("wss://should-be-ignored.com").unwrap()));
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_inbox_relays() {
        use nostr_sdk::prelude::*;

        // Test Kind::InboxRelays (10050) with "relay" tags
        let keys = Keys::generate();

        let relay_tags = vec![
            Tag::custom(TagKind::Relay, ["wss://inbox1.example.com"]),
            Tag::custom(TagKind::Relay, ["wss://inbox2.example.com"]),
            // Add an "r" tag that should be ignored for InboxRelays
            Tag::reference("wss://should-be-ignored.com"),
        ];

        let event = EventBuilder::new(Kind::InboxRelays, "")
            .tags(relay_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(&event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://inbox1.example.com").unwrap()));
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://inbox2.example.com").unwrap()));
        assert!(!parsed_relays.contains(&RelayUrl::parse("wss://should-be-ignored.com").unwrap()));
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_key_package_relays() {
        use nostr_sdk::prelude::*;

        // Test Kind::MlsKeyPackageRelays (10051) with "relay" tags
        let keys = Keys::generate();

        let relay_tags = vec![
            Tag::custom(TagKind::Relay, ["wss://keypackage1.example.com"]),
            Tag::custom(TagKind::Relay, ["wss://keypackage2.example.com"]),
            // Add an "r" tag that should be ignored for MlsKeyPackageRelays
            Tag::reference("wss://should-be-ignored.com"),
        ];

        let event = EventBuilder::new(Kind::MlsKeyPackageRelays, "")
            .tags(relay_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(&event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://keypackage1.example.com").unwrap()));
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://keypackage2.example.com").unwrap()));
        assert!(!parsed_relays.contains(&RelayUrl::parse("wss://should-be-ignored.com").unwrap()));
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_unknown_kind_backward_compatibility() {
        use nostr_sdk::prelude::*;

        // Test unknown kind with both "r" and "relay" tags (backward compatibility)
        let keys = Keys::generate();

        let mixed_tags = vec![
            Tag::reference("wss://r-tag-relay.example.com"),
            Tag::custom(TagKind::Relay, ["wss://relay-tag-relay.example.com"]),
        ];

        let event = EventBuilder::new(Kind::Custom(9999), "")
            .tags(mixed_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(&event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://r-tag-relay.example.com").unwrap()));
        assert!(
            parsed_relays.contains(&RelayUrl::parse("wss://relay-tag-relay.example.com").unwrap())
        );
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_invalid_urls_filtered() {
        use nostr_sdk::prelude::*;

        // Test that invalid URLs are filtered out
        let keys = Keys::generate();

        let tags = vec![
            Tag::reference("wss://valid-relay.example.com"),
            Tag::reference("not a valid url"),
            Tag::reference("wss://another-valid.example.com"),
        ];

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(&event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://valid-relay.example.com").unwrap()));
        assert!(
            parsed_relays.contains(&RelayUrl::parse("wss://another-valid.example.com").unwrap())
        );
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_empty_tags() {
        use nostr_sdk::prelude::*;

        // Test event with no relay tags
        let keys = Keys::generate();

        let tags = vec![
            Tag::custom(TagKind::Custom("alt".into()), ["Some description"]),
            Tag::custom(TagKind::Custom("d".into()), ["identifier"]),
        ];

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(&event);
        assert!(parsed_relays.is_empty());
    }

    // Existing tests below

    #[tokio::test]
    async fn test_pubkeys_from_event_with_valid_p_tags() {
        // Create test public keys
        let signer_keys = Keys::generate();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let pubkey1 = keys1.public_key();
        let pubkey2 = keys2.public_key();

        // Create an event with p tags containing valid public keys
        let event = EventBuilder::text_note("test content")
            .tags([Tag::public_key(pubkey1), Tag::public_key(pubkey2)])
            .sign(&signer_keys)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&pubkey1));
        assert!(result.contains(&pubkey2));
    }

    #[tokio::test]
    async fn test_pubkeys_from_event_with_empty_event() {
        // Create an event with no tags
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test content")
            .sign(&keys)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_pubkeys_from_event_with_non_p_tags() {
        let keys = Keys::generate();

        // Create an event with various non-p tags
        let event = EventBuilder::text_note("test content")
            .tags([
                Tag::hashtag("bitcoin"),
                Tag::identifier("test-id"),
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)),
                    vec!["event-id"],
                ),
            ])
            .sign(&keys)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_pubkeys_from_event_with_invalid_pubkey_content() {
        let keys = Keys::generate();

        // Create an event with p tags containing invalid public key content
        let event = EventBuilder::text_note("test content")
            .tags([
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                    vec!["invalid-pubkey"],
                ),
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                    vec!["also-invalid"],
                ),
            ])
            .sign(&keys)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_pubkeys_from_event_with_mixed_valid_and_invalid() {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let valid_pubkey = keys2.public_key();

        // Create an event with both valid and invalid p tags
        let event = EventBuilder::text_note("test content")
            .tags([
                Tag::public_key(valid_pubkey),
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                    vec!["invalid-pubkey"],
                ),
                Tag::hashtag("bitcoin"), // Non-p tag
            ])
            .sign(&keys1)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], valid_pubkey);
    }

    #[tokio::test]
    async fn test_pubkeys_from_event_with_duplicate_pubkeys() {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let pubkey = keys2.public_key();

        // Create an event with duplicate p tags
        let event = EventBuilder::text_note("test content")
            .tags([Tag::public_key(pubkey), Tag::public_key(pubkey)])
            .sign(&keys1)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        // Should contain duplicates as the method doesn't deduplicate
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], pubkey);
        assert_eq!(result[1], pubkey);
    }

    #[tokio::test]
    async fn test_pubkeys_from_event_with_empty_p_tag_content() {
        let keys = Keys::generate();

        // Create an event with p tag but no content
        let event = EventBuilder::text_note("test content")
            .tags([Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                Vec::<String>::new(),
            )])
            .sign(&keys)
            .await
            .unwrap();

        let result = NostrManager::pubkeys_from_event(&event);

        assert_eq!(result.len(), 0);
    }
}
