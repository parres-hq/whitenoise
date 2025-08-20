use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::relays::{Relay, RelayType};
use crate::whitenoise::Whitenoise;
use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct User {
    pub id: Option<i64>,
    pub pubkey: PublicKey,
    pub metadata: Metadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(pubkey: PublicKey) -> Self {
        User {
            id: None,
            pubkey,
            metadata: Metadata::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub(crate) async fn update_metadata(&mut self, whitenoise: &Whitenoise) -> Result<()> {
        let relays_to_query = self.get_query_relays(whitenoise).await?;
        let metadata = whitenoise
            .nostr
            .fetch_metadata_from(&relays_to_query, self.pubkey)
            .await?;
        if let Some(metadata) = metadata {
            if self.metadata != metadata {
                self.metadata = metadata;
                self.save(&whitenoise.database).await?;
            }
        }
        Ok(())
    }

    /// Fetches the latest relay lists for this user from Nostr and updates the local database
    ///
    /// # Arguments
    /// * `whitenoise` - The Whitenoise instance for network and database operations
    ///
    /// # Returns
    /// `Ok(())` if successful, or `Err(WhitenoiseError)` if critical operations fail
    ///
    /// # Error Handling
    /// Errors in updating individual relay types are logged but don't prevent other updates
    pub(crate) async fn update_relay_lists(&self, whitenoise: &Whitenoise) -> Result<()> {
        let initial_query_relays = self.get_query_relays(whitenoise).await?;

        tracing::info!(
            target: "whitenoise::users::update_relay_lists",
            "Updating relay lists for user {} using {} query relays",
            self.pubkey,
            initial_query_relays.len()
        );

        let updated_query_relays = self
            .update_nip65_relays(whitenoise, &initial_query_relays)
            .await?;

        self.update_secondary_relay_types(whitenoise, &updated_query_relays)
            .await?;

        tracing::info!(
            target: "whitenoise::users::update_relay_lists",
            "Successfully completed relay list updates for user {}",
            self.pubkey
        );

        Ok(())
    }

    async fn get_query_relays(&self, whitenoise: &Whitenoise) -> Result<Vec<Relay>> {
        let stored_relays = self.relays(RelayType::Nip65, &whitenoise.database).await?;

        if stored_relays.is_empty() {
            tracing::debug!(
                target: "whitenoise::users::get_query_relays",
                "User {} has no stored NIP-65 relays, using default relays",
                self.pubkey,
            );
            Ok(Relay::defaults())
        } else {
            Ok(stored_relays)
        }
    }

    async fn update_nip65_relays(
        &self,
        whitenoise: &Whitenoise,
        query_relays: &[Relay],
    ) -> Result<Vec<Relay>> {
        match self
            .sync_relays_for_type(whitenoise, RelayType::Nip65, query_relays)
            .await
        {
            Ok(true) => {
                let refreshed_relays = self.relays(RelayType::Nip65, &whitenoise.database).await?;
                tracing::info!(
                    target: "whitenoise::users::update_nip65_relays",
                    "Updated NIP-65 relays for user {}, now using {} relays for other types",
                    self.pubkey,
                    refreshed_relays.len()
                );
                Ok(refreshed_relays)
            }
            Ok(false) => {
                tracing::debug!(
                    target: "whitenoise::users::update_nip65_relays",
                    "NIP-65 relays unchanged for user {}",
                    self.pubkey
                );
                Ok(query_relays.to_vec())
            }
            Err(e) => {
                tracing::warn!(
                    target: "whitenoise::users::update_nip65_relays",
                    "Failed to update NIP-65 relays for user {}: {}, continuing with original relays",
                    self.pubkey,
                    e
                );
                Ok(query_relays.to_vec())
            }
        }
    }

    async fn update_secondary_relay_types(
        &self,
        whitenoise: &Whitenoise,
        query_relays: &[Relay],
    ) -> Result<()> {
        const SECONDARY_RELAY_TYPES: &[RelayType] = &[RelayType::Inbox, RelayType::KeyPackage];

        for &relay_type in SECONDARY_RELAY_TYPES {
            if let Err(e) = self
                .sync_relays_for_type(whitenoise, relay_type, query_relays)
                .await
            {
                tracing::warn!(
                    target: "whitenoise::users::update_secondary_relay_types",
                    "Failed to update {:?} relays for user {}: {}",
                    relay_type,
                    self.pubkey,
                    e
                );
                // Continue with other relay types - individual failures shouldn't stop the process
            }
        }

        Ok(())
    }

    /// Synchronizes relays for a specific type with the network state
    ///
    /// Returns `true` if changes were made, `false` if no changes needed
    async fn sync_relays_for_type(
        &self,
        whitenoise: &Whitenoise,
        relay_type: RelayType,
        query_relays: &[Relay],
    ) -> Result<bool> {
        let network_relay_urls = whitenoise
            .nostr
            .fetch_user_relays(self.pubkey, relay_type, query_relays)
            .await
            .map_err(|e| {
                tracing::warn!(
                    target: "whitenoise::users::sync_relays_for_type",
                    "Failed to fetch {:?} relays for user {}: {}",
                    relay_type, self.pubkey, e
                );
                e
            })?;

        let stored_relays = self.relays(relay_type, &whitenoise.database).await?;
        let network_relay_urls_vec: Vec<_> = network_relay_urls.into_iter().collect();

        // Check if there are any changes needed
        let stored_urls: HashSet<&RelayUrl> = stored_relays.iter().map(|r| &r.url).collect();
        let network_urls_set = network_relay_urls_vec.iter().collect();

        if stored_urls == network_urls_set {
            tracing::debug!(
                target: "whitenoise::users::sync_relays_for_type",
                "No changes needed for {:?} relays for user {}",
                relay_type,
                self.pubkey
            );
            return Ok(false);
        }

        // Apply changes
        tracing::info!(
            target: "whitenoise::users::sync_relays_for_type",
            "Updating {:?} relays for user {}: {} existing -> {} new",
            relay_type,
            self.pubkey,
            stored_urls.len(),
            network_urls_set.len()
        );

        // Remove relays that are no longer needed
        for existing_relay in &stored_relays {
            if !network_urls_set.contains(&existing_relay.url) {
                if let Err(e) = self
                    .remove_relay(existing_relay, relay_type, &whitenoise.database)
                    .await
                {
                    tracing::warn!(
                        target: "whitenoise::users::sync_relays_for_type",
                        "Failed to remove {:?} relay {} for user {}: {}",
                        relay_type,
                        existing_relay.url,
                        self.pubkey,
                        e
                    );
                }
            }
        }

        // Add new relays
        for new_relay_url in &network_relay_urls_vec {
            if !stored_urls.contains(new_relay_url) {
                let new_relay = whitenoise.find_or_create_relay_by_url(new_relay_url).await?;
                if let Err(e) = self
                    .add_relay(&new_relay, relay_type, &whitenoise.database)
                    .await
                {
                    tracing::warn!(
                        target: "whitenoise::users::sync_relays_for_type",
                        "Failed to add {:?} relay {} for user {}: {}",
                        relay_type,
                        new_relay_url,
                        self.pubkey,
                        e
                    );
                }
            }
        }

        Ok(true)
    }

    pub async fn key_package_event(&self, whitenoise: &Whitenoise) -> Result<Option<Event>> {
        let key_package_relays = self
            .relays(RelayType::KeyPackage, &whitenoise.database)
            .await?;
        let key_package_event = whitenoise
            .nostr
            .fetch_user_key_package(self.pubkey, &key_package_relays)
            .await?;
        Ok(key_package_event)
    }
}

impl Whitenoise {
    /// Retrieves a user by their public key.
    ///
    /// This method looks up a user in the database using their Nostr public key.
    /// The user may have been discovered through various means such as:
    /// - Following lists from accounts
    /// - Message interactions
    /// - Direct user lookups
    /// - Metadata events
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The Nostr public key of the user to retrieve
    ///
    /// # Returns
    ///
    /// Returns a `Result<User>` containing:
    /// - `Ok(User)` - The user with the specified public key, including their metadata
    /// - `Err(WhitenoiseError)` - If the user is not found or there's a database error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nostr_sdk::PublicKey;
    /// use whitenoise::Whitenoise;
    ///
    /// # async fn example(whitenoise: &Whitenoise) -> Result<(), Box<dyn std::error::Error>> {
    /// let pubkey = PublicKey::parse("npub1...")?;
    /// let user = whitenoise.find_user_by_pubkey(&pubkey).await?;
    /// println!("Found user: {:?}", user.metadata.name);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The user with the specified public key doesn't exist in the database
    /// - There's a database connection or query error
    /// - The public key format is invalid (though this is typically caught at the type level)
    pub async fn find_user_by_pubkey(&self, pubkey: &PublicKey) -> Result<User> {
        User::find_by_pubkey(pubkey, &self.database).await
    }

    pub async fn find_or_create_user_by_pubkey(&self, pubkey: &PublicKey) -> Result<(User, bool)> {
        User::find_or_create_by_pubkey(pubkey, &self.database).await
    }

    /// Retrieves the relay list for a specific user and relay type.
    ///
    /// This method fetches the configured relays for a user based on the specified relay type.
    /// Different relay types serve different purposes in the Nostr ecosystem:
    /// - **Nostr relays** (NIP-65): General purpose relays for reading/writing events
    /// - **Inbox relays** (NIP-65): Specialized relays for receiving private messages
    /// - **Key package relays**: Relays that store MLS key packages for encrypted group messaging
    ///
    /// The relay lists are typically published by users as relay list events (kinds 10002, 10050, 10051)
    /// and cached locally in the database for efficient access.
    ///
    /// # Arguments
    ///
    /// * `user` - The user whose relay list to retrieve
    /// * `relay_type` - The type of relays to fetch (Nostr, Inbox, or KeyPackage)
    ///
    /// # Returns
    ///
    /// Returns a `Result<Vec<Relay>>` containing:
    /// - `Ok(Vec<Relay>)` - A vector of relays configured for the user and relay type
    /// - `Err(WhitenoiseError)` - If there's a database error or user lookup fails
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nostr_sdk::PublicKey;
    /// use whitenoise::{RelayType, Whitenoise};
    ///
    /// # async fn example(whitenoise: &Whitenoise) -> Result<(), Box<dyn std::error::Error>> {
    /// let pubkey = PublicKey::parse("npub1...")?;
    /// let user = whitenoise.find_user_by_pubkey(&pubkey).await?;
    ///
    /// // Get user's inbox relays for private messaging
    /// let inbox_relays = whitenoise.user_relays(&user, RelayType::Inbox).await?;
    /// println!("User has {} inbox relays", inbox_relays.len());
    ///
    /// // Get user's key package relays for MLS group messaging
    /// let kp_relays = whitenoise.user_relays(&user, RelayType::KeyPackage).await?;
    /// for relay in kp_relays {
    ///     println!("Key package relay: {}", relay.url);
    /// }
    ///
    /// // Get user's general Nostr relays
    /// let nip65_relays = whitenoise.user_relays(&user, RelayType::Nip65).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Relay Types
    ///
    /// - `RelayType::Nip65` - General purpose relays from NIP-65 relay lists (kind 10002)
    /// - `RelayType::Inbox` - Inbox relays for private messages (kind 10050)
    /// - `RelayType::KeyPackage` - Relays storing MLS key packages (kind 10051)
    ///
    /// # Notes
    ///
    /// - If a user hasn't published relay lists, the returned vector may be empty
    /// - Relay lists are cached locally and updated when new relay list events are received
    /// - The method returns database records that include additional metadata like read/write permissions
    /// - For users that haven't configured specific relay types, consider falling back to default relays
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - There's a database connection or query error
    /// - The user object contains invalid data (shouldn't happen with valid User instances)
    pub async fn user_relays(&self, user: &User, relay_type: RelayType) -> Result<Vec<Relay>> {
        user.relays(relay_type, &self.database).await
    }

    pub async fn user_metadata(&self, pubkey: &PublicKey) -> Result<Metadata> {
        let user = self.find_user_by_pubkey(pubkey).await?;
        Ok(user.metadata.clone())
    }

    pub(crate) async fn background_fetch_user_data(&self, user: &User) -> Result<()> {
        let user_clone = user.clone();
        let mut mut_user_clone = user.clone();

        tokio::spawn(async move {
            let whitenoise = Whitenoise::get_instance()?;
            // Parallel network requests
            let relay_task = user_clone.update_relay_lists(whitenoise);
            let metadata_task = mut_user_clone.update_metadata(whitenoise);

            let (relay_result, metadata_result) = tokio::join!(relay_task, metadata_task);

            // Log errors but don't fail
            if let Err(e) = relay_result {
                tracing::warn!(
                    "Failed to fetch relay lists for {}: {}",
                    user_clone.pubkey,
                    e
                );
            }
            if let Err(e) = metadata_result {
                tracing::warn!("Failed to fetch metadata for {}: {}", user_clone.pubkey, e);
            }

            Ok::<(), WhitenoiseError>(())
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::create_mock_whitenoise;
    use chrono::Utc;
    use std::collections::HashSet;

    #[test]
    fn test_basic_relay_url_equality() {
        let url1 = RelayUrl::parse("wss://relay1.example.com").unwrap();
        let url2 = RelayUrl::parse("wss://relay1.example.com").unwrap();
        let url3 = RelayUrl::parse("wss://relay2.example.com").unwrap();

        assert_eq!(url1, url2);
        assert_ne!(url1, url3);

        let mut url_set = HashSet::new();
        url_set.insert(&url1);
        url_set.insert(&url2); // Should not increase size since url1 == url2
        url_set.insert(&url3);

        assert_eq!(url_set.len(), 2);
        assert!(url_set.contains(&url1));
        assert!(url_set.contains(&url3));
    }

    #[tokio::test]
    async fn test_update_relay_lists_success() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new().name("Test User"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let saved_user = user.save(&whitenoise.database).await.unwrap();
        let initial_relay_url = RelayUrl::parse("wss://initial.example.com").unwrap();
        let initial_relay = whitenoise
            .find_or_create_relay_by_url(&initial_relay_url)
            .await
            .unwrap();

        saved_user
            .add_relay(&initial_relay, RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();

        saved_user.update_relay_lists(&whitenoise).await.unwrap();
        let relays = saved_user
            .relays(RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(relays.len(), 1);
        assert_eq!(relays[0].url, initial_relay_url);
    }

    #[tokio::test]
    async fn test_update_relay_lists_with_no_initial_relays() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new().name("Test User No Relays"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let saved_user = user.save(&whitenoise.database).await.unwrap();

        saved_user.update_relay_lists(&whitenoise).await.unwrap();
        assert!(saved_user
            .relays(RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_get_query_relays_with_stored_relays() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let saved_user = user.save(&whitenoise.database).await.unwrap();

        // Add a relay
        let relay_url = RelayUrl::parse("wss://test.example.com").unwrap();
        let relay = whitenoise.find_or_create_relay_by_url(&relay_url).await.unwrap();
        saved_user
            .add_relay(&relay, RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();

        // Test get_query_relays
        let query_relays = saved_user.get_query_relays(&whitenoise).await.unwrap();

        assert_eq!(query_relays.len(), 1);
        assert_eq!(query_relays[0].url, relay_url);
    }

    #[tokio::test]
    async fn test_get_query_relays_with_no_stored_relays() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User::new(test_pubkey);
        let saved_user = user.save(&whitenoise.database).await.unwrap();
        let query_relays = saved_user.get_query_relays(&whitenoise).await.unwrap();

        assert_eq!(
            query_relays.into_iter().map(|r| r.url).collect::<Vec<_>>(),
            Relay::defaults()
                .into_iter()
                .map(|r| r.url)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_update_metadata_with_working_relays() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new().name("Original Name"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let mut saved_user = user.save(&whitenoise.database).await.unwrap();

        for default_relay in &Relay::defaults() {
            let relay = whitenoise
                .find_or_create_relay_by_url(&default_relay.url)
                .await
                .unwrap();
            saved_user
                .add_relay(&relay, RelayType::Nip65, &whitenoise.database)
                .await
                .unwrap();
        }

        let original_metadata = saved_user.metadata.clone();
        let result = saved_user.update_metadata(&whitenoise).await;

        assert!(result.is_ok());

        let user_after = User::find_by_pubkey(&test_pubkey, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(user_after.metadata.name, original_metadata.name);
        assert_eq!(user_after.pubkey, test_pubkey);
    }

    #[tokio::test]
    async fn test_update_metadata_with_no_nip65_relays() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new().name("Test User"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let mut saved_user = user.save(&whitenoise.database).await.unwrap();
        let result = saved_user.update_metadata(&whitenoise).await;

        assert!(result.is_ok());

        let user_after = User::find_by_pubkey(&test_pubkey, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(user_after.metadata.name, Some("Test User".to_string()));
        assert_eq!(user_after.pubkey, test_pubkey);
    }

    #[tokio::test]
    async fn test_update_metadata_preserves_user_state() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new().name("Test User").about("Test description"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let mut saved_user = user.save(&whitenoise.database).await.unwrap();

        let relay_url = RelayUrl::parse("ws://localhost:7777").unwrap();
        let relay = whitenoise.find_or_create_relay_by_url(&relay_url).await.unwrap();
        saved_user
            .add_relay(&relay, RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();

        let original_id = saved_user.id;
        let result = saved_user.update_metadata(&whitenoise).await;

        assert!(result.is_ok());

        let final_user = User::find_by_pubkey(&test_pubkey, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(final_user.id, original_id);
        assert_eq!(final_user.pubkey, test_pubkey);
        assert_eq!(final_user.metadata.name, Some("Test User".to_string()));
        assert_eq!(
            final_user.metadata.about,
            Some("Test description".to_string())
        );
    }
}
