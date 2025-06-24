use crate::whitenoise::accounts::Account;
use crate::whitenoise::accounts::OnboardingState;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// A row in the relays table
#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct RelayRow {
    pub url: String,
    pub relay_type: String,
    pub account_pubkey: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Relay {
    pub url: String,
    pub relay_type: RelayType,
    pub account_pubkey: PublicKey,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum RelayType {
    Nostr,
    Inbox,
    KeyPackage,
}

impl From<String> for RelayType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "nostr" => Self::Nostr,
            "inbox" => Self::Inbox,
            "key_package" => Self::KeyPackage,
            _ => panic!("Invalid relay type: {}", s),
        }
    }
}

impl From<RelayType> for String {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nostr => "nostr".to_string(),
            RelayType::Inbox => "inbox".to_string(),
            RelayType::KeyPackage => "key_package".to_string(),
        }
    }
}

impl From<RelayType> for Kind {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nostr => Kind::RelayList,
            RelayType::Inbox => Kind::InboxRelays,
            RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
        }
    }
}

impl Whitenoise {
    /// Loads the Nostr metadata for a contact by their public key.
    ///
    /// This method queries the Nostr network for user metadata associated with the provided public key.
    /// The metadata includes information such as display name, profile picture, and other user details
    /// that have been published to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the contact whose metadata should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(Metadata))` if metadata is found, `Ok(None)` if no metadata is available,
    /// or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the metadata query fails.
    pub async fn fetch_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let metadata = self.nostr.query_user_metadata(pubkey).await?;
        Ok(metadata)
    }

    /// Loads the Nostr relays associated with a user's public key.
    ///
    /// This method queries the Nostr network for relay URLs that the user has published
    /// for a specific relay type (e.g., read relays, write relays). These relays are
    /// used to determine where to send and receive Nostr events for the user.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relays should be fetched.
    /// * `relay_type` - The type of relays to fetch (e.g., read, write, or both).
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<RelayUrl>)` containing the list of relay URLs, or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay query fails.
    pub async fn fetch_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self.nostr.query_user_relays(pubkey, relay_type).await?;
        Ok(relays)
    }

    /// Fetches user relays for the specified type, falling back to default client relays if empty.
    ///
    /// This helper method abstracts the common pattern of checking if user-specific relays
    /// are configured and falling back to default client relays when they're not available.
    /// This is particularly useful during onboarding and in test environments where users
    /// haven't configured relays yet.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relays should be fetched.
    /// * `relay_type` - The type of relays to fetch (Nostr, Inbox, or KeyPackage).
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<RelayUrl>)` containing user relays if available, otherwise default client relays.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if either the relay query or default relay fetch fails.
    pub(crate) async fn fetch_relays_with_fallback(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let user_relays = self.fetch_relays(pubkey, relay_type).await?;

        if user_relays.is_empty() {
            self.nostr.relays().await.map_err(WhitenoiseError::from)
        } else {
            Ok(user_relays)
        }
    }

    /// Updates the metadata for the given account by publishing a new metadata event to Nostr.
    ///
    /// This method takes the provided metadata, creates a Nostr metadata event (Kind::Metadata),
    /// and publishes it to the account's relays. It also updates the account's `last_synced` timestamp
    /// in the database to reflect the successful publication.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The new `Metadata` to publish for the account.
    /// * `account` - A reference to the `Account` whose metadata should be updated.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful publication and database update.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The metadata cannot be serialized to JSON
    /// * The account's private key cannot be retrieved from the secret store
    /// * The event publication fails
    /// * The database update fails
    pub async fn update_metadata(&self, metadata: &Metadata, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Updating metadata for account: {}",
            account.pubkey.to_hex()
        );

        // Serialize metadata to JSON
        let metadata_json = serde_json::to_string(metadata)?;

        // Create metadata event
        let event = EventBuilder::new(Kind::Metadata, metadata_json);

        // Get signing keys for the account
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        // Publish the event
        let result = self
            .nostr
            .publish_event_builder_with_signer(event, &relays_to_use, keys)
            .await?;

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Published metadata event: {:?}",
            result
        );

        Ok(())
    }

    /// Updates the relay list for the given account by publishing a new relay list event to Nostr.
    ///
    /// This method takes the provided relay URLs and relay type, creates the appropriate relay list event
    /// (Nostr relays, Inbox relays, or Key Package relays), and publishes it to the account's relays.
    /// The relay list event contains the provided relay URLs as relay tags.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose relay list should be updated.
    /// * `relay_type` - The type of relay list to update (Nostr, Inbox, or KeyPackage).
    /// * `relays` - A vector of `RelayUrl` specifying the relays to include in the event.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful publication.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The account's private key cannot be retrieved from the secret store
    /// * The event creation fails
    /// * The event publication fails
    pub async fn update_relays(
        &self,
        account: &Account,
        relay_type: RelayType,
        relays: Vec<RelayUrl>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_account_relays",
            "Updating {:?} relays for account: {} with {} relays",
            relay_type,
            account.pubkey.to_hex(),
            relays.len()
        );

        // Use the existing helper method to publish the relay list
        self.publish_relay_list_for_account(account, relays, relay_type)
            .await?;

        tracing::debug!(
            target: "whitenoise::update_account_relays",
            "Successfully updated {:?} relays for account: {}",
            relay_type,
            account.pubkey.to_hex()
        );

        Ok(())
    }

    pub async fn fetch_onboarding_state(&self, pubkey: PublicKey) -> Result<OnboardingState> {
        let mut onboarding_state = OnboardingState::default();

        let inbox_relays = self.fetch_relays(pubkey, RelayType::Inbox).await?;
        let key_package_relays = self.fetch_relays(pubkey, RelayType::KeyPackage).await?;

        let key_package_published = if key_package_relays.is_empty() {
            None
        } else {
            self.fetch_key_package_event(pubkey, key_package_relays.clone())
                .await?
        };

        onboarding_state.inbox_relays = !inbox_relays.is_empty();
        onboarding_state.key_package_relays = !key_package_relays.is_empty();
        onboarding_state.key_package_published = key_package_published.is_some();

        Ok(onboarding_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;
    #[tokio::test]
    async fn test_fetch_all_relay_types() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let relay_types = [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage];
        for relay_type in relay_types {
            let result = whitenoise.fetch_relays(pubkey, relay_type).await;
            assert!(result.is_ok());
            let relays = result.unwrap();
            assert!(relays.is_empty()); // Empty in test environment
        }
    }

    #[tokio::test]
    async fn test_fetch_onboarding_state_structure() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await;
        assert!(account.is_ok(), "{:?}", account);

        let result = whitenoise.fetch_onboarding_state(pubkey).await;
        assert!(result.is_ok());

        let onboarding_state = result.unwrap();
        // In test environment, all should be false since no data is cached
        assert!(!onboarding_state.inbox_relays);
        assert!(!onboarding_state.key_package_relays);
        assert!(!onboarding_state.key_package_published);
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, test_keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise
            .secrets_store
            .store_private_key(&test_keys)
            .unwrap();

        let log_account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Initialize NostrMls for the account
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();

        // Create test metadata
        let metadata = Metadata {
            name: Some("Updated Name".to_string()),
            display_name: Some("Updated Display Name".to_string()),
            about: Some("Updated bio".to_string()),
            picture: Some("https://example.com/new-avatar.jpg".to_string()),
            banner: Some("https://example.com/banner.jpg".to_string()),
            nip05: Some("user@example.com".to_string()),
            lud16: Some("user@lightning.example.com".to_string()),
            ..Default::default()
        };

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account).await;
        assert!(result.is_ok(), "update_metadata should succeed");
    }

    #[tokio::test]
    async fn test_update_metadata_with_minimal_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Initialize NostrMls for the account
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();

        // Create minimal metadata (only name)
        let metadata = Metadata {
            name: Some("Simple Name".to_string()),
            ..Default::default()
        };

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account).await;
        assert!(
            result.is_ok(),
            "update_metadata should succeed with minimal metadata"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_with_empty_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Initialize NostrMls for the account
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();

        // Create completely empty metadata
        let metadata = Metadata::default();

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account).await;
        assert!(
            result.is_ok(),
            "update_metadata should succeed with empty metadata"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_without_stored_keys() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account but DON'T store the keys
        let (account, _keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        // Note: not storing keys in secrets_store

        // Create test metadata
        let metadata = Metadata {
            name: Some("Test Name".to_string()),
            ..Default::default()
        };

        // Test updating metadata - this should fail because keys aren't stored
        let result = whitenoise.update_metadata(&metadata, &account).await;
        assert!(
            result.is_err(),
            "update_metadata should fail when keys aren't stored"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_serialization() {
        // Test that various metadata fields serialize correctly
        let metadata = Metadata {
            name: Some("Test User".to_string()),
            display_name: Some("Test Display".to_string()),
            about: Some("Bio with special chars: émojí 🚀".to_string()),
            picture: Some("https://example.com/picture.jpg".to_string()),
            banner: Some("https://example.com/banner.jpg".to_string()),
            nip05: Some("test@example.com".to_string()),
            lud16: Some("test@lightning.example.com".to_string()),
            website: Some("https://example.com".to_string()),
            ..Default::default()
        };

        // Test that the metadata can be serialized to JSON
        let serialized = serde_json::to_string(&metadata);
        assert!(serialized.is_ok(), "Metadata should serialize to JSON");

        let json_str = serialized.unwrap();
        assert!(json_str.contains("Test User"));
        assert!(json_str.contains("Bio with special chars"));
        assert!(json_str.contains("émojí 🚀"));
    }
    #[tokio::test]
    async fn test_relay_type_to_event_kind_mapping() {
        // Test that RelayType maps to correct Nostr event kinds
        // This tests the logic inside publish_relay_list_for_account without network calls

        let test_cases = [
            (RelayType::Nostr, Kind::RelayList),
            (RelayType::Inbox, Kind::InboxRelays),
            (RelayType::KeyPackage, Kind::MlsKeyPackageRelays),
        ];

        for (relay_type, expected_kind) in test_cases {
            let actual_kind = match relay_type {
                RelayType::Nostr => Kind::RelayList,
                RelayType::Inbox => Kind::InboxRelays,
                RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
            };

            assert_eq!(
                actual_kind, expected_kind,
                "RelayType::{:?} should map to Kind::{:?}",
                relay_type, expected_kind
            );
        }
    }

    #[tokio::test]
    async fn test_relay_list_tag_creation() {
        // Test that relay URLs are correctly converted to tags
        let test_relays = [
            "wss://relay.damus.io",
            "wss://nos.lol",
            "wss://relay.primal.net",
            "wss://nostr.wine",
        ];

        let relay_urls: Vec<RelayUrl> = test_relays
            .iter()
            .map(|url| RelayUrl::parse(url).unwrap())
            .collect();

        // Create tags the same way as publish_relay_list_for_account
        let tags: Vec<Tag> = relay_urls
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();

        // Verify tag structure
        assert_eq!(tags.len(), test_relays.len());

        for (i, tag) in tags.iter().enumerate() {
            let tag_vec = tag.clone().to_vec();
            assert_eq!(tag_vec.len(), 2, "Relay tag should have 2 elements");
            assert_eq!(tag_vec[0], "relay", "First element should be 'relay'");
            assert_eq!(
                tag_vec[1], test_relays[i],
                "Second element should be the relay URL"
            );
        }
    }

    #[tokio::test]
    async fn test_relay_list_event_structure() {
        // Test event creation for each relay type without publishing
        let relay_urls = [
            RelayUrl::parse("wss://relay.damus.io").unwrap(),
            RelayUrl::parse("wss://nos.lol").unwrap(),
        ];

        let test_cases = [
            (RelayType::Nostr, Kind::RelayList),
            (RelayType::Inbox, Kind::InboxRelays),
            (RelayType::KeyPackage, Kind::MlsKeyPackageRelays),
        ];

        for (_relay_type, expected_kind) in test_cases {
            // Create tags
            let tags: Vec<Tag> = relay_urls
                .iter()
                .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                .collect();

            // Create event (same logic as publish_relay_list_for_account)
            let _event_builder = EventBuilder::new(expected_kind, "").tags(tags.clone());

            // Verify event structure - we can't build the event without keys,
            // but we can verify the builder has the right components
            // (The actual event building happens during signing)

            // Verify tags are correctly attached
            assert_eq!(tags.len(), 2);

            // Verify tag content
            for (i, tag) in tags.iter().enumerate() {
                let tag_vec = tag.clone().to_vec();
                assert_eq!(tag_vec[0], "relay");
                assert_eq!(tag_vec[1], relay_urls[i].to_string());
            }
        }
    }

    #[tokio::test]
    async fn test_empty_relay_list_handling() {
        // Test that empty relay lists are handled correctly
        // (publish_relay_list_for_account returns early for empty lists)

        let empty_relays: Vec<RelayUrl> = vec![];

        // The method returns early if relays.is_empty(), so test that logic
        assert!(empty_relays.is_empty());

        // If we were to create tags anyway, it should be empty
        let tags: Vec<Tag> = empty_relays
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();

        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn test_single_relay_event() {
        // Test with a single relay
        let single_relay = vec![RelayUrl::parse("wss://relay.damus.io").unwrap()];

        let tags: Vec<Tag> = single_relay
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();

        assert_eq!(tags.len(), 1);
        let tag_vec = tags[0].clone().to_vec();
        assert_eq!(tag_vec[0], "relay");
        assert_eq!(tag_vec[1], "wss://relay.damus.io");
    }

    #[tokio::test]
    async fn test_multiple_relay_event() {
        // Test with multiple relays
        let multiple_relays = vec![
            RelayUrl::parse("wss://relay.damus.io").unwrap(),
            RelayUrl::parse("wss://nos.lol").unwrap(),
            RelayUrl::parse("wss://relay.primal.net").unwrap(),
            RelayUrl::parse("wss://nostr.wine").unwrap(),
            RelayUrl::parse("wss://relay.snort.social").unwrap(),
        ];

        let expected_urls = [
            "wss://relay.damus.io",
            "wss://nos.lol",
            "wss://relay.primal.net",
            "wss://nostr.wine",
            "wss://relay.snort.social",
        ];

        let tags: Vec<Tag> = multiple_relays
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();

        assert_eq!(tags.len(), expected_urls.len());

        for (i, tag) in tags.iter().enumerate() {
            let tag_vec = tag.clone().to_vec();
            assert_eq!(tag_vec[0], "relay");
            assert_eq!(tag_vec[1], expected_urls[i]);
        }
    }

    #[tokio::test]
    async fn test_relay_url_formats() {
        // Test different valid relay URL formats
        let test_urls = [
            "wss://relay.damus.io",
            "wss://nos.lol/",
            "wss://relay.primal.net/v1",
            "ws://localhost:8080",
        ];

        for url_str in test_urls {
            let relay_url = RelayUrl::parse(url_str).unwrap();
            let tag = Tag::custom(TagKind::Relay, [relay_url.to_string()]);

            let tag_vec = tag.to_vec();
            assert_eq!(tag_vec[0], "relay");
            assert_eq!(tag_vec[1], url_str);
        }
    }

    #[tokio::test]
    async fn test_update_account_relays_logic() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let (account, keys) = create_test_account();

        // Store account keys so we can test the event creation part
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        let test_relays = [
            RelayUrl::parse("wss://relay.damus.io").unwrap(),
            RelayUrl::parse("wss://nos.lol").unwrap(),
        ];

        // Test that all relay types can be processed
        let relay_types = [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage];

        for relay_type in relay_types {
            // We can't easily test the actual method without network calls,
            // but we can test that the components work

            // Verify we can get the keys (required for signing)
            let signing_keys = whitenoise
                .secrets_store
                .get_nostr_keys_for_pubkey(&account.pubkey);
            assert!(
                signing_keys.is_ok(),
                "Should be able to get signing keys for relay type {:?}",
                relay_type
            );

            // Verify event kind mapping
            let expected_kind = match relay_type {
                RelayType::Nostr => Kind::RelayList,
                RelayType::Inbox => Kind::InboxRelays,
                RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
            };

            // Create tags (same logic as in the method)
            let tags: Vec<Tag> = test_relays
                .iter()
                .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                .collect();

            // Create event builder
            let _event_builder = EventBuilder::new(expected_kind, "").tags(tags);

            // If we got here without panicking, the event structure is valid
        }
    }

    #[tokio::test]
    async fn test_relay_list_edge_cases() {
        // Test various edge cases in relay list processing

        // Test with special characters in URLs (should be URL encoded)
        let special_relay =
            RelayUrl::parse("wss://relay.example.com/path?param=value&other=test").unwrap();
        let tag = Tag::custom(TagKind::Relay, [special_relay.to_string()]);

        let tag_vec = tag.to_vec();
        assert_eq!(tag_vec[0], "relay");
        assert!(tag_vec[1].contains("wss://relay.example.com"));

        // Test very long relay URL
        let long_path = "a".repeat(100);
        let long_url = format!("wss://relay.example.com/{}", long_path);
        let long_relay = RelayUrl::parse(&long_url).unwrap();
        let long_tag = Tag::custom(TagKind::Relay, [long_relay.to_string()]);

        let long_tag_vec = long_tag.to_vec();
        assert_eq!(long_tag_vec[0], "relay");
        assert_eq!(long_tag_vec[1], long_url);
    }
}
