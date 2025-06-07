//! Nostr API for Whitenoise
//! This module provides methods for interacting with the Nostr network, including
//! loading metadata, relays, and contact lists for users.
//!
//! All methods in this module only load data from the nostr cache. They do not perform any network requests.
//! The cache is updated by the nostr manager in the backaground.

use crate::error::Result;
use crate::relays::RelayType;
use crate::{OnboardingState, Whitenoise};
use nostr_sdk::prelude::*;
use std::collections::HashMap;

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
    pub async fn load_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
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
    pub async fn load_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self.nostr.query_user_relays(pubkey, relay_type).await?;
        Ok(relays)
    }

    /// Loads a user's contact list from the Nostr network.
    ///
    /// This method retrieves the user's contact list, which contains the public keys
    /// of other users they follow. For each contact, it also includes their metadata
    /// if available.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose contact list should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(HashMap<PublicKey, Option<Metadata>>)` where the keys are the public keys
    /// of contacts and the values are their associated metadata (if available).
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the contact list query fails.
    pub async fn load_contact_list(
        &self,
        pubkey: PublicKey,
    ) -> Result<HashMap<PublicKey, Option<Metadata>>> {
        let contacts = self.nostr.query_user_contact_list(pubkey).await?;
        Ok(contacts)
    }

    pub async fn load_key_package(&self, pubkey: PublicKey) -> Result<Option<Event>> {
        let key_package = self.nostr.query_user_key_package(pubkey).await?;
        Ok(key_package)
    }

    pub async fn load_onboarding_state(&self, pubkey: PublicKey) -> Result<OnboardingState> {
        let mut onboarding_state = OnboardingState::default();

        let inbox_relays = self.load_relays(pubkey, RelayType::Inbox).await?;
        let key_package_relays = self.load_relays(pubkey, RelayType::KeyPackage).await?;
        let key_package_published = self.load_key_package(pubkey).await?;

        onboarding_state.inbox_relays = !inbox_relays.is_empty();
        onboarding_state.key_package_relays = !key_package_relays.is_empty();
        onboarding_state.key_package_published = key_package_published.is_some();

        Ok(onboarding_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Whitenoise, WhitenoiseConfig};
    use nostr_sdk::Keys;
    use tempfile::TempDir;

    // Helper function to create a test Whitenoise instance
    async fn create_test_whitenoise() -> (Whitenoise, TempDir, TempDir) {
        let data_temp_dir = TempDir::new().expect("Failed to create temp data dir");
        let logs_temp_dir = TempDir::new().expect("Failed to create temp logs dir");

        let config = WhitenoiseConfig::new(data_temp_dir.path(), logs_temp_dir.path());

        let whitenoise = Whitenoise::initialize_whitenoise(config)
            .await
            .expect("Failed to initialize Whitenoise");

        (whitenoise, data_temp_dir, logs_temp_dir)
    }

    // Helper function to create test keys
    fn create_test_keys() -> Keys {
        Keys::generate()
    }

    #[tokio::test]
    async fn test_load_metadata_success() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        // In test environment, this should return None since no metadata is cached
        let result = whitenoise.load_metadata(pubkey).await;
        assert!(result.is_ok());

        // Since we're running in test mode and no actual data is cached, we expect None
        let metadata = result.unwrap();
        assert!(metadata.is_none());
    }

    #[tokio::test]
    async fn test_load_metadata_error_handling() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        // Test that the method properly handles the Result type
        let result = whitenoise.load_metadata(pubkey).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_relays_all_types() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        // Test loading different relay types
        let relay_types = [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage];

        for relay_type in relay_types {
            let result = whitenoise.load_relays(pubkey, relay_type).await;
            assert!(result.is_ok());

            // In test environment, this should return an empty vector
            let relays = result.unwrap();
            assert!(relays.is_empty());
        }
    }

    #[tokio::test]
    async fn test_load_relays_inbox() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_relays(pubkey, RelayType::Inbox).await;
        assert!(result.is_ok());

        let relays = result.unwrap();
        assert!(relays.is_empty()); // Empty in test environment
    }

    #[tokio::test]
    async fn test_load_relays_key_package() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_relays(pubkey, RelayType::KeyPackage).await;
        assert!(result.is_ok());

        let relays = result.unwrap();
        assert!(relays.is_empty()); // Empty in test environment
    }

    #[tokio::test]
    async fn test_load_contact_list_empty() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_contact_list(pubkey).await;
        assert!(result.is_ok());

        // In test environment, this should return an empty HashMap
        let contacts = result.unwrap();
        assert!(contacts.is_empty());
    }

    #[tokio::test]
    async fn test_load_contact_list_structure() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_contact_list(pubkey).await;
        assert!(result.is_ok());

        let contacts = result.unwrap();
        // Verify the return type is HashMap<PublicKey, Option<Metadata>>
        assert_eq!(contacts.len(), 0);
    }

    #[tokio::test]
    async fn test_load_key_package_none() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_key_package(pubkey).await;
        assert!(result.is_ok());

        // In test environment, this should return None
        let key_package = result.unwrap();
        assert!(key_package.is_none());
    }

    #[tokio::test]
    async fn test_load_onboarding_state_default() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_onboarding_state(pubkey).await;
        assert!(result.is_ok());

        let onboarding_state = result.unwrap();

        // In test environment, all relays should be empty and key package should be None
        assert!(!onboarding_state.inbox_relays);
        assert!(!onboarding_state.key_package_relays);
        assert!(!onboarding_state.key_package_published);
    }

    #[tokio::test]
    async fn test_load_onboarding_state_structure() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_onboarding_state(pubkey).await;
        assert!(result.is_ok());

        let onboarding_state = result.unwrap();

        // Verify the structure matches OnboardingState
        assert!(!onboarding_state.inbox_relays);
        assert!(!onboarding_state.key_package_relays);
        assert!(!onboarding_state.key_package_published);
    }

    #[tokio::test]
    async fn test_multiple_pubkeys() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;

        // Test with multiple different public keys
        let keys1 = create_test_keys();
        let keys2 = create_test_keys();
        let keys3 = create_test_keys();

        let pubkeys = [keys1.public_key(), keys2.public_key(), keys3.public_key()];

        for pubkey in pubkeys {
            // Test metadata loading
            let metadata_result = whitenoise.load_metadata(pubkey).await;
            assert!(metadata_result.is_ok());

            // Test relay loading
            let relays_result = whitenoise.load_relays(pubkey, RelayType::Inbox).await;
            assert!(relays_result.is_ok());

            // Test contact list loading
            let contacts_result = whitenoise.load_contact_list(pubkey).await;
            assert!(contacts_result.is_ok());

            // Test key package loading
            let key_package_result = whitenoise.load_key_package(pubkey).await;
            assert!(key_package_result.is_ok());

            // Test onboarding state loading
            let onboarding_result = whitenoise.load_onboarding_state(pubkey).await;
            assert!(onboarding_result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_api_methods_return_types() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        // Test that all methods return the expected types

        // load_metadata should return Result<Option<Metadata>>
        let metadata = whitenoise.load_metadata(pubkey).await;
        assert!(metadata.is_ok());

        // load_relays should return Result<Vec<RelayUrl>>
        let relays = whitenoise.load_relays(pubkey, RelayType::Inbox).await;
        assert!(relays.is_ok());

        // load_contact_list should return Result<HashMap<PublicKey, Option<Metadata>>>
        let contacts = whitenoise.load_contact_list(pubkey).await;
        assert!(contacts.is_ok());

        // load_key_package should return Result<Option<Event>>
        let key_package = whitenoise.load_key_package(pubkey).await;
        assert!(key_package.is_ok());

        // load_onboarding_state should return Result<OnboardingState>
        let onboarding = whitenoise.load_onboarding_state(pubkey).await;
        assert!(onboarding.is_ok());
    }

    #[tokio::test]
    async fn test_onboarding_state_logic() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        let result = whitenoise.load_onboarding_state(pubkey).await;
        assert!(result.is_ok());

        let onboarding_state = result.unwrap();

        // Test the logic: inbox_relays should be true if relays are not empty
        // In test environment, relays will be empty, so should be false
        assert!(!onboarding_state.inbox_relays);

        // Test the logic: key_package_relays should be true if relays are not empty
        // In test environment, relays will be empty, so should be false
        assert!(!onboarding_state.key_package_relays);

        // Test the logic: key_package_published should be true if key package exists
        // In test environment, key package will be None, so should be false
        assert!(!onboarding_state.key_package_published);
    }

    #[tokio::test]
    async fn test_relay_type_enum_usage() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        // Test that all RelayType enum variants work
        let result_nostr = whitenoise.load_relays(pubkey, RelayType::Nostr).await;
        assert!(result_nostr.is_ok());

        let result_inbox = whitenoise.load_relays(pubkey, RelayType::Inbox).await;
        assert!(result_inbox.is_ok());

        let result_key_package = whitenoise.load_relays(pubkey, RelayType::KeyPackage).await;
        assert!(result_key_package.is_ok());
    }

    #[test]
    fn test_onboarding_state_default() {
        let onboarding_state = OnboardingState::default();

        assert!(!onboarding_state.inbox_relays);
        assert!(!onboarding_state.key_package_relays);
        assert!(!onboarding_state.key_package_published);
    }

    #[tokio::test]
    async fn test_concurrent_api_calls() {
        let (whitenoise, _data_temp, _logs_temp) = create_test_whitenoise().await;
        let test_keys = create_test_keys();
        let pubkey = test_keys.public_key();

        // Test that multiple concurrent API calls work correctly
        let metadata_future = whitenoise.load_metadata(pubkey);
        let relays_future = whitenoise.load_relays(pubkey, RelayType::Inbox);
        let contacts_future = whitenoise.load_contact_list(pubkey);
        let key_package_future = whitenoise.load_key_package(pubkey);
        let onboarding_future = whitenoise.load_onboarding_state(pubkey);

        let results = tokio::join!(
            metadata_future,
            relays_future,
            contacts_future,
            key_package_future,
            onboarding_future
        );

        assert!(results.0.is_ok());
        assert!(results.1.is_ok());
        assert!(results.2.is_ok());
        assert!(results.3.is_ok());
        assert!(results.4.is_ok());
    }
}
