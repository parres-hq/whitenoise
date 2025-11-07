use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    nostr_manager::NostrManager,
    whitenoise::{
        Whitenoise,
        database::processed_events::ProcessedEvent,
        error::{Result, WhitenoiseError},
        relays::{Relay, RelayType},
        utils::timestamp_to_datetime,
    },
};

/// TTL for user metadata before it's considered stale and needs refreshing
/// Set to 24 hours - metadata doesn't change frequently for most users
const METADATA_TTL_HOURS: i64 = 24;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct User {
    pub id: Option<i64>,
    pub pubkey: PublicKey,
    pub metadata: Metadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Checks if the user's metadata is stale and needs refreshing based on TTL.
    ///
    /// Returns `true` if the metadata was last updated more than `METADATA_TTL_HOURS` ago,
    /// or if this is a newly created user with default metadata.
    ///
    /// # Returns
    ///
    /// * `true` if metadata should be refreshed
    /// * `false` if metadata is still fresh
    fn needs_metadata_refresh(&self) -> bool {
        let now = Utc::now();
        let ttl_duration = Duration::hours(METADATA_TTL_HOURS);
        let stale_threshold = now - ttl_duration;

        // Always refresh if metadata is default (empty)
        if self.metadata == Metadata::new() {
            return true;
        }

        // Refresh if updated_at is older than TTL
        self.updated_at < stale_threshold
    }

    /// Syncs the user's metadata by fetching the latest version from Nostr relays.
    ///
    /// This method queries the user's configured relays (or default relays if none are configured)
    /// to fetch the most recent metadata event (kind 0) published by the user. If newer metadata
    /// is found that differs from the locally cached version, it updates the local record and
    /// saves the changes to the database.
    ///
    /// The method implements smart fetching by using the user's NIP-65 relay list when available,
    /// or falling back to default relays if the user hasn't published a relay list yet.
    ///
    /// # Arguments
    ///
    /// * `whitenoise` - The Whitenoise instance used to access the Nostr client and database
    pub async fn sync_metadata(&mut self, whitenoise: &Whitenoise) -> Result<()> {
        let relays_urls: Vec<_> = Relay::urls(&self.get_query_relays(whitenoise).await?);
        let metadata_event = whitenoise
            .nostr
            .fetch_metadata_from(&relays_urls, self.pubkey)
            .await?;
        if let Some(event) = metadata_event {
            let metadata = Metadata::from_json(&event.content)?;
            let should_update = self
                .should_update_metadata(&event, false, &whitenoise.database)
                .await?;

            if should_update {
                self.metadata = metadata;

                // Save the updated user metadata
                self.save(&whitenoise.database).await?;

                whitenoise
                    .nostr
                    .event_tracker
                    .track_processed_global_event(&event)
                    .await?;

                tracing::debug!(
                    target: "whitenoise::users::sync_metadata",
                    "Updated metadata for user {} with event timestamp {} via background sync",
                    self.pubkey,
                    event.created_at
                );
            }
        }
        Ok(())
    }

    /// Fetches the user's MLS key package event from their configured key package relays.
    ///
    /// This method retrieves the user's published MLS (Message Layer Security) key package
    /// from the Nostr network. Key packages are cryptographic objects that contain the user's
    /// public keys and credentials needed to add them to MLS group conversations.
    ///
    /// The method first retrieves the user's key package relay list (NIP-65 kind 10051 events),
    /// then fetches the most recent MLS key package event (kind 443) from those relays.
    ///
    /// # Arguments
    ///
    /// * `whitenoise` - The Whitenoise instance used to access the Nostr client and database
    pub async fn key_package_event(&self, whitenoise: &Whitenoise) -> Result<Option<Event>> {
        let key_package_relays = self
            .relays(RelayType::KeyPackage, &whitenoise.database)
            .await?;
        let mut key_package_relays_urls_set: HashSet<RelayUrl> =
            Relay::urls(&key_package_relays).into_iter().collect();
        if key_package_relays.is_empty() {
            tracing::warn!(
                target: "whitenoise::users::key_package_event",
                "User {} has no key package relays, using nip65 relays",
                self.pubkey
            );
            key_package_relays_urls_set.extend(Relay::urls(
                &self.relays(RelayType::Nip65, &whitenoise.database).await?,
            ));
        }
        if key_package_relays_urls_set.is_empty() {
            tracing::warn!(
                target: "whitenoise::users::key_package_event",
                "User {} has neither key package nor NIP-65 relays; returning None",
                self.pubkey
            );
            return Ok(None);
        }

        let key_package_relays_urls: Vec<RelayUrl> =
            key_package_relays_urls_set.into_iter().collect();
        let key_package_event = whitenoise
            .nostr
            .fetch_user_key_package(self.pubkey, &key_package_relays_urls)
            .await?;
        Ok(key_package_event)
    }

    pub async fn relays_by_type(
        &self,
        relay_type: RelayType,
        whitenoise: &Whitenoise,
    ) -> Result<Vec<Relay>> {
        self.relays(relay_type, &whitenoise.database).await
    }

    /// Fetches the latest relay lists for this user from Nostr and updates the local database
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

    /// Synchronizes stored relays with a new set of relay URLs
    ///
    /// This method compares the currently stored relays with a new set of relay URLs,
    /// removing relays that are no longer present and adding new ones. This is the
    /// core synchronization logic used by both network-fetched updates and direct
    /// event processing.
    ///
    /// Returns `true` if changes were made, `false` if no changes needed
    pub(crate) async fn sync_relay_urls(
        &self,
        whitenoise: &Whitenoise,
        relay_type: RelayType,
        new_relay_urls: &HashSet<RelayUrl>,
        event_created_at: Option<DateTime<Utc>>,
    ) -> Result<bool> {
        // First, check if we should process this event based on timestamp
        if let Some(new_timestamp) = event_created_at {
            let newest_stored_timestamp = ProcessedEvent::newest_relay_event_timestamp(
                &self.pubkey,
                relay_type,
                &whitenoise.database,
            )
            .await?;

            match newest_stored_timestamp {
                Some(stored_timestamp)
                    if new_timestamp.timestamp_millis() <= stored_timestamp.timestamp_millis() =>
                {
                    tracing::debug!(
                        target: "whitenoise::users::sync_relay_urls",
                        "Ignoring stale {:?} relay event for user {} (event: {}, stored: {})",
                        relay_type,
                        self.pubkey,
                        new_timestamp.timestamp_millis(),
                        stored_timestamp.timestamp_millis()
                    );
                    return Ok(false);
                }
                None => {
                    tracing::debug!(
                        target: "whitenoise::users::sync_relay_urls",
                        "No stored {:?} relay timestamps for user {}, accepting new event",
                        relay_type,
                        self.pubkey
                    );
                }
                Some(_) => {
                    tracing::debug!(
                        target: "whitenoise::users::sync_relay_urls",
                        "New {:?} relay event is newer for user {}, proceeding with sync",
                        relay_type,
                        self.pubkey
                    );
                }
            }
        }

        let stored_relays = self.relays(relay_type, &whitenoise.database).await?;
        let stored_urls: HashSet<&RelayUrl> = stored_relays.iter().map(|r| &r.url).collect();
        let new_urls_set: HashSet<&RelayUrl> = new_relay_urls.iter().collect();

        if stored_urls == new_urls_set {
            tracing::debug!(
                target: "whitenoise::users::sync_relay_urls",
                "No changes needed for {:?} relays for user {}",
                relay_type,
                self.pubkey
            );
            return Ok(false);
        }

        // Apply changes
        tracing::info!(
            target: "whitenoise::users::sync_relay_urls",
            "Updating {:?} relays for user {}: {} existing -> {} new",
            relay_type,
            self.pubkey,
            stored_urls.len(),
            new_urls_set.len()
        );

        // Remove relays that are no longer needed
        for existing_relay in &stored_relays {
            if !new_urls_set.contains(&existing_relay.url)
                && let Err(e) = self
                    .remove_relay(existing_relay, relay_type, &whitenoise.database)
                    .await
            {
                tracing::warn!(
                    target: "whitenoise::users::sync_relay_urls",
                    "Failed to remove {:?} relay {} for user {}: {}",
                    relay_type,
                    existing_relay.url,
                    self.pubkey,
                    e
                );
            }
        }

        // Add new relays
        for new_relay_url in new_relay_urls {
            if !stored_urls.contains(new_relay_url) {
                let new_relay = whitenoise
                    .find_or_create_relay_by_url(new_relay_url)
                    .await?;
                if let Err(e) = self
                    .add_relay(&new_relay, relay_type, &whitenoise.database)
                    .await
                {
                    tracing::warn!(
                        target: "whitenoise::users::sync_relay_urls",
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

    /// Synchronizes relays for a specific type with the network state
    ///
    /// Returns `true` if changes were made, `false` if no changes needed
    async fn sync_relays_for_type(
        &self,
        whitenoise: &Whitenoise,
        relay_type: RelayType,
        query_relays: &[Relay],
    ) -> Result<bool> {
        let relays_urls: Vec<_> = Relay::urls(query_relays);
        let relay_event = whitenoise
            .nostr
            .fetch_user_relays(self.pubkey, relay_type, &relays_urls)
            .await
            .map_err(|e| {
                tracing::warn!(
                    target: "whitenoise::users::sync_relays_for_type",
                    "Failed to fetch {:?} relays for user {}: {}",
                    relay_type, self.pubkey, e
                );
                e
            })?;

        match relay_event {
            Some(event) => {
                let relay_hashset: HashSet<_> = NostrManager::relay_urls_from_event(&event);
                let changed = self
                    .sync_relay_urls(
                        whitenoise,
                        relay_type,
                        &relay_hashset,
                        Some(timestamp_to_datetime(event.created_at)?),
                    )
                    .await?;

                if changed {
                    whitenoise
                        .nostr
                        .event_tracker
                        .track_processed_global_event(&event)
                        .await?;

                    tracing::debug!(
                        target: "whitenoise::users::sync_relays_for_type",
                        "Updated {:?} relays for user {} via background sync with event {}",
                        relay_type, self.pubkey, event.id.to_hex()
                    );
                }

                Ok(changed)
            }
            None => {
                tracing::debug!(
                    target: "whitenoise::users::sync_relays_for_type",
                    "No {:?} relay events found for user {}",
                    relay_type, self.pubkey
                );
                Ok(false)
            }
        }
    }

    pub(crate) async fn all_users_with_relay_urls(
        whitenoise: &Whitenoise,
    ) -> Result<Vec<(PublicKey, Vec<RelayUrl>)>> {
        let users = User::all(&whitenoise.database).await?;
        let mut users_with_relays = Vec::new();

        for user in users {
            let relays = user.relays(RelayType::Nip65, &whitenoise.database).await?;
            let relay_urls: Vec<RelayUrl> = Relay::urls(&relays);
            users_with_relays.push((user.pubkey, relay_urls));
        }

        Ok(users_with_relays)
    }

    /// Determines whether metadata should be updated based on comprehensive event processing logic.
    ///
    /// This method implements the complete logic for deciding whether to process a metadata event:
    /// 1. Check if we've already processed this specific event (avoid double processing)
    /// 2. If user is newly created, always accept the event
    /// 3. If user has default metadata, always accept the event
    /// 4. Otherwise, check if the event timestamp is newer than or equal to stored timestamp
    ///
    /// # Arguments
    /// * `event_id` - The ID of the metadata event being considered
    /// * `event_datetime` - The datetime of the metadata event being considered
    /// * `newly_created` - Whether this user was just created
    /// * `database` - Database connection for checking processed events
    ///
    /// # Returns
    /// * `Ok(true)` if metadata should be updated
    /// * `Ok(false)` if the event should be ignored
    pub(crate) async fn should_update_metadata(
        &self,
        event: &Event,
        newly_created: bool,
        database: &crate::whitenoise::database::Database,
    ) -> Result<bool> {
        // Check if we've already processed this specific event from this author
        let already_processed = ProcessedEvent::exists(&event.id, None, database)
            .await
            .map_err(WhitenoiseError::Database)?;

        if already_processed {
            tracing::debug!(
                target: "whitenoise::users::should_update_metadata",
                "Skipping already processed metadata event {} from author {}",
                event.id.to_hex(),
                self.pubkey.to_hex()
            );
            return Ok(false);
        }

        // If user is newly created, always accept the metadata
        if newly_created {
            tracing::debug!(
                target: "whitenoise::users::should_update_metadata",
                "Accepting metadata event for newly created user {}",
                self.pubkey
            );
            return Ok(true);
        }

        // If the current metadata is the same as the new metadata, don't update
        if self.metadata == Metadata::from_json(&event.content)? {
            tracing::debug!(
                target: "whitenoise::users::should_update_metadata",
                "Skipping metadata event for user {} because it's the same as the current metadata",
                self.pubkey
            );
            return Ok(false);
        }

        // Check timestamp against most recent processed metadata event for this specific user
        let newest_processed_timestamp =
            ProcessedEvent::newest_event_timestamp_for_kind(None, 0, Some(&self.pubkey), database)
                .await
                .map_err(WhitenoiseError::Database)?;

        let should_update = match newest_processed_timestamp {
            None => {
                tracing::debug!(
                    target: "whitenoise::users::should_update_metadata",
                    "No processed metadata events for user {}, accepting new event",
                    self.pubkey
                );
                true
            }
            Some(stored_timestamp) => {
                let event_datetime = timestamp_to_datetime(event.created_at)?;
                let is_newer_or_equal =
                    event_datetime.timestamp_millis() >= stored_timestamp.timestamp_millis();
                if !is_newer_or_equal {
                    tracing::debug!(
                        target: "whitenoise::users::should_update_metadata",
                        "Ignoring stale metadata event for user {} (event: {}, stored: {})",
                        self.pubkey,
                        event_datetime,
                        stored_timestamp
                    );
                }
                is_newer_or_equal
            }
        };

        Ok(should_update)
    }
}

impl Whitenoise {
    /// Retrieves a user by their public key.
    ///
    /// This method looks up a user in the database using their Nostr public key.
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

    /// Finds a user by their public key or creates a new one if not found.
    ///
    /// This method looks up a user in the database using their Nostr public key.
    /// If the user doesn't exist, it creates a new user record and optionally syncs
    /// their metadata and relay lists.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The Nostr public key of the user to find or create
    /// * `force_sync` - If true, forces synchronous metadata/relay syncing (blocking).
    ///   If false, uses background syncing with TTL-based refresh logic.
    ///
    /// # Returns
    ///
    /// Returns a `Result<User>` containing:
    /// - `Ok(User)` - The found or created user
    /// - `Err(WhitenoiseError)` - If there's a database error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nostr_sdk::PublicKey;
    /// use whitenoise::Whitenoise;
    ///
    /// # async fn example(whitenoise: &Whitenoise) -> Result<(), Box<dyn std::error::Error>> {
    /// let pubkey = PublicKey::parse("npub1...")?;
    ///
    /// // Fast, non-blocking call with background sync
    /// let user = whitenoise
    ///     .find_or_create_user_by_pubkey(&pubkey, false)
    ///     .await?;
    ///
    /// // Slower, blocking call with immediate metadata
    /// let user_with_metadata = whitenoise
    ///     .find_or_create_user_by_pubkey(&pubkey, true)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - There's a database connection or query error
    /// - The public key format is invalid (though this is typically caught at the type level)
    /// - Network errors occur during forced synchronous syncing
    pub async fn find_or_create_user_by_pubkey(
        &self,
        pubkey: &PublicKey,
        force_sync: bool,
    ) -> Result<User> {
        let (mut user, created) = User::find_or_create_by_pubkey(pubkey, &self.database).await?;

        if force_sync {
            // Force synchronous syncing - blocking network calls
            tracing::debug!(
                target: "whitenoise::users::find_or_create_user_by_pubkey",
                "Force sync requested for user {}, performing blocking metadata and relay sync",
                user.pubkey
            );

            if created {
                // For new users, sync relay lists first, then metadata
                if let Err(e) = user.update_relay_lists(self).await {
                    tracing::warn!(
                        target: "whitenoise::users::find_or_create_user_by_pubkey",
                        "Failed to sync relay lists for new user {}: {}",
                        user.pubkey,
                        e
                    );
                }
                if let Err(e) = self.refresh_global_subscription_for_user(&user).await {
                    tracing::warn!(
                        target: "whitenoise::users::find_or_create_user_by_pubkey",
                        "Failed to refresh global subscription for new user {}: {}",
                        user.pubkey,
                        e
                    );
                }
            }

            // Always sync metadata when force_sync is true
            if let Err(e) = user.sync_metadata(self).await {
                tracing::warn!(
                    target: "whitenoise::users::find_or_create_user_by_pubkey",
                    "Failed to sync metadata for user {}: {}",
                    user.pubkey,
                    e
                );
            }
        } else {
            // Use TTL-based background syncing (non-blocking)
            if created {
                if let Err(e) = self.background_fetch_user_data(&user).await {
                    tracing::warn!(
                        target: "whitenoise::users::find_or_create_user_by_pubkey",
                        "Failed to start background fetch for new user {}: {}",
                        user.pubkey,
                        e
                    );
                }
            } else if user.needs_metadata_refresh() {
                // For existing users, only sync if metadata is stale
                tracing::debug!(
                    target: "whitenoise::users::find_or_create_user_by_pubkey",
                    "User {} metadata is stale (updated_at: {}), starting background refresh",
                    user.pubkey,
                    user.updated_at
                );
                if let Err(e) = self.background_fetch_user_data(&user).await {
                    tracing::warn!(
                        target: "whitenoise::users::find_or_create_user_by_pubkey",
                        "Failed to start background fetch for stale user {}: {}",
                        user.pubkey,
                        e
                    );
                }
            } else {
                tracing::debug!(
                    target: "whitenoise::users::find_or_create_user_by_pubkey",
                    "User {} metadata is fresh (updated_at: {}), skipping sync",
                    user.pubkey,
                    user.updated_at
                );
            }
        }

        Ok(user)
    }

    pub(crate) async fn background_fetch_user_data(&self, user: &User) -> Result<()> {
        let user_clone = user.clone();
        let mut mut_user_clone = user.clone();

        tokio::spawn(async move {
            let whitenoise = Whitenoise::get_instance()?;
            // Do these in series so that we fetch the user's relays before trying to fetch metadata
            // (more likely we find metadata looking on the right relays)
            let relay_result = user_clone.update_relay_lists(whitenoise).await;
            let metadata_result = mut_user_clone.sync_metadata(whitenoise).await;

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

            if let Err(e) = whitenoise
                .refresh_global_subscription_for_user(&user_clone)
                .await
            {
                tracing::warn!(
                    target: "whitenoise::users::background_fetch_user_data",
                    "Failed to refresh global subscription for {}: {}",
                    user_clone.pubkey,
                    e
                );
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
        assert!(
            saved_user
                .relays(RelayType::Nip65, &whitenoise.database)
                .await
                .unwrap()
                .is_empty()
        );
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
        let relay = whitenoise
            .find_or_create_relay_by_url(&relay_url)
            .await
            .unwrap();
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
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let saved_user = user.save(&whitenoise.database).await.unwrap();
        let query_relays = saved_user.get_query_relays(&whitenoise).await.unwrap();

        assert_eq!(Relay::urls(&query_relays), Relay::urls(&Relay::defaults()));
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
        let result = saved_user.sync_metadata(&whitenoise).await;

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
        let result = saved_user.sync_metadata(&whitenoise).await;

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
        let relay = whitenoise
            .find_or_create_relay_by_url(&relay_url)
            .await
            .unwrap();
        saved_user
            .add_relay(&relay, RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();

        let original_id = saved_user.id;
        let result = saved_user.sync_metadata(&whitenoise).await;

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

    #[tokio::test]
    async fn test_find_or_create_user_by_pubkey_existing_user() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let original_user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new()
                .name("Original User")
                .about("Original description"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let saved_user = original_user.save(&whitenoise.database).await.unwrap();
        let original_id = saved_user.id.unwrap();

        let found_user = whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, false)
            .await
            .unwrap();

        assert_eq!(found_user.id, Some(original_id));
        assert_eq!(found_user.pubkey, test_pubkey);
        assert_eq!(found_user.metadata.name, Some("Original User".to_string()));
        assert_eq!(
            found_user.metadata.about,
            Some("Original description".to_string())
        );
    }

    #[tokio::test]
    async fn test_find_or_create_user_by_pubkey_new_user() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();

        let user_exists = whitenoise.find_user_by_pubkey(&test_pubkey).await.is_ok();
        assert!(!user_exists);

        let created_user = whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, false)
            .await
            .unwrap();

        assert!(created_user.id.is_some());
        assert_eq!(created_user.pubkey, test_pubkey);
        assert_eq!(created_user.metadata.name, None);
        assert_eq!(created_user.metadata.about, None);

        let found_user = whitenoise.find_user_by_pubkey(&test_pubkey).await.unwrap();
        assert_eq!(found_user.id, created_user.id);
        assert_eq!(found_user.pubkey, created_user.pubkey);
    }

    #[tokio::test]
    async fn test_all_users_with_relay_urls() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let users_with_relays = User::all_users_with_relay_urls(&whitenoise).await.unwrap();
        assert!(users_with_relays.is_empty());

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let user = User {
            id: None,
            pubkey: test_pubkey,
            metadata: Metadata::new().name("Test User"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let saved_user = user.save(&whitenoise.database).await.unwrap();
        let relay_url = RelayUrl::parse("wss://test.example.com").unwrap();
        let relay = whitenoise
            .find_or_create_relay_by_url(&relay_url)
            .await
            .unwrap();
        saved_user
            .add_relay(&relay, RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();

        let users_with_relays = User::all_users_with_relay_urls(&whitenoise).await.unwrap();
        assert_eq!(users_with_relays.len(), 1);
        assert_eq!(users_with_relays[0].0, test_pubkey);
        assert_eq!(users_with_relays[0].1, vec![relay_url]);
    }

    #[tokio::test]
    async fn test_key_package_event_gradual_relay_addition() {
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

        // Test 1: No relays - should return None
        let kp_relays = saved_user
            .relays(RelayType::KeyPackage, &whitenoise.database)
            .await
            .unwrap();
        assert!(kp_relays.is_empty());

        let nip65_relays = saved_user
            .relays(RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();
        assert!(nip65_relays.is_empty());

        let result = saved_user.key_package_event(&whitenoise).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Test 2: Add only NIP-65 relays - expect Ok(None); actual usage not asserted here
        let nip65_relay_url = RelayUrl::parse("ws://localhost:7777").unwrap();
        let nip65_relay = whitenoise
            .find_or_create_relay_by_url(&nip65_relay_url)
            .await
            .unwrap();
        saved_user
            .add_relay(&nip65_relay, RelayType::Nip65, &whitenoise.database)
            .await
            .unwrap();

        let result = saved_user.key_package_event(&whitenoise).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Test 3: Add a key package relay - expect Ok(None); priority over NIP-65 not asserted here
        let kp_relay_url = RelayUrl::parse("ws://localhost:8080").unwrap();
        let kp_relay = whitenoise
            .find_or_create_relay_by_url(&kp_relay_url)
            .await
            .unwrap();
        saved_user
            .add_relay(&kp_relay, RelayType::KeyPackage, &whitenoise.database)
            .await
            .unwrap();

        let result = saved_user.key_package_event(&whitenoise).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    mod should_update_metadata_tests {
        use super::*;
        use crate::whitenoise::database::processed_events::ProcessedEvent;

        async fn create_test_user(whitenoise: &Whitenoise) -> User {
            let keys = Keys::generate();
            User::find_or_create_by_pubkey(&keys.public_key(), &whitenoise.database)
                .await
                .unwrap()
                .0
        }

        async fn create_test_metadata_event(name: Option<String>) -> Event {
            let keys = Keys::generate();
            let name = name.unwrap_or_else(|| "Test User".to_string());
            let event_builder = EventBuilder::metadata(&Metadata::new().name(name));
            event_builder.sign(&keys).await.unwrap()
        }

        #[tokio::test]
        async fn test_should_update_metadata_already_processed() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let user = create_test_user(&whitenoise).await;
            let event = create_test_metadata_event(None).await;

            // First, create a processed event entry
            ProcessedEvent::create(
                &event.id,
                None, // Global events
                Some(timestamp_to_datetime(event.created_at).unwrap()),
                Some(Kind::Metadata), // Metadata kind
                Some(&user.pubkey),
                &whitenoise.database,
            )
            .await
            .unwrap();

            // Test that already processed event returns false
            let result = user
                .should_update_metadata(&event, false, &whitenoise.database)
                .await
                .unwrap();

            assert!(!result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_newly_created_user() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let user = create_test_user(&whitenoise).await;
            let event = create_test_metadata_event(None).await;

            // Test that newly created user always returns true
            let result = user
                .should_update_metadata(&event, true, &whitenoise.database)
                .await
                .unwrap();

            assert!(result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_default_metadata() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let event = create_test_metadata_event(None).await;

            // Ensure user has default metadata
            user.metadata = Metadata::default();
            user.save(&whitenoise.database).await.unwrap();

            // Test that user with default metadata returns true
            let result = user
                .should_update_metadata(&event, false, &whitenoise.database)
                .await
                .unwrap();

            assert!(result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_no_processed_events() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let event = create_test_metadata_event(Some("Andy Waterman".to_string())).await;

            // Give user some non-default metadata
            user.metadata = Metadata::new().name("Test User");
            user.save(&whitenoise.database).await.unwrap();

            // Test that with no processed events, returns true
            let result = user
                .should_update_metadata(&event, false, &whitenoise.database)
                .await
                .unwrap();

            assert!(result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_newer_event() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let old_event = create_test_metadata_event(None).await;

            // Give user some non-default metadata
            user.metadata = Metadata::new().name("Test User");
            user.save(&whitenoise.database).await.unwrap();

            // Create an older processed event
            ProcessedEvent::create(
                &old_event.id,
                None, // Global events
                Some(timestamp_to_datetime(old_event.created_at).unwrap()),
                Some(Kind::Metadata), // Metadata kind
                Some(&user.pubkey),
                &whitenoise.database,
            )
            .await
            .unwrap();

            // Create a newer event (just create a fresh one, it should be newer due to timing)
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            let new_event = create_test_metadata_event(Some("Bobby Tables".to_string())).await;

            // Test that newer event returns true
            let result = user
                .should_update_metadata(&new_event, false, &whitenoise.database)
                .await
                .unwrap();

            assert!(result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_equal_timestamp() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let old_event = create_test_metadata_event(None).await;

            // Give user some non-default metadata
            user.metadata = Metadata::new().name("Test User");
            user.save(&whitenoise.database).await.unwrap();

            // Create a processed event
            ProcessedEvent::create(
                &old_event.id,
                None, // Global events
                Some(timestamp_to_datetime(old_event.created_at).unwrap()),
                Some(Kind::Metadata), // Metadata kind
                Some(&user.pubkey),
                &whitenoise.database,
            )
            .await
            .unwrap();

            // Create an event with the same timestamp but different content/ID
            let keys = Keys::generate();
            let event_builder = EventBuilder::metadata(&Metadata::new().name("Different Name"));
            let mut new_event = event_builder.sign(&keys).await.unwrap();
            // Force the same timestamp for testing
            new_event.created_at = old_event.created_at;

            // Test that equal timestamp returns true (newer or equal)
            let result = user
                .should_update_metadata(&new_event, false, &whitenoise.database)
                .await
                .unwrap();

            assert!(result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_older_event() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let newer_event = create_test_metadata_event(None).await;

            // Give user some non-default metadata
            user.metadata = Metadata::new().name("Test User");
            user.save(&whitenoise.database).await.unwrap();

            // Create a newer processed event
            ProcessedEvent::create(
                &newer_event.id,
                None, // Global events
                Some(timestamp_to_datetime(newer_event.created_at).unwrap()),
                Some(Kind::Metadata), // Metadata kind
                Some(&user.pubkey),
                &whitenoise.database,
            )
            .await
            .unwrap();

            // Create an older event
            let keys = Keys::generate();
            let event_builder = EventBuilder::metadata(&Metadata::new().name("Old Name"));
            let mut old_event = event_builder.sign(&keys).await.unwrap();
            // Force an older timestamp for testing
            old_event.created_at = newer_event.created_at - 3600; // 1 hour earlier

            // Test that older event returns false
            let result = user
                .should_update_metadata(&old_event, false, &whitenoise.database)
                .await
                .unwrap();

            assert!(!result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_priority_order() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let event = create_test_metadata_event(Some("Andy Waterman".to_string())).await;

            // Give user some non-default metadata
            user.metadata = Metadata::new().name("Test User");
            user.save(&whitenoise.database).await.unwrap();

            // Create a processed event entry for this exact event
            ProcessedEvent::create(
                &event.id,
                None, // Global events
                Some(timestamp_to_datetime(event.created_at).unwrap()),
                Some(Kind::Metadata), // Metadata kind
                Some(&user.pubkey),
                &whitenoise.database,
            )
            .await
            .unwrap();

            // Test that already processed takes priority over newly_created
            // Even though newly_created=true, it should return false because event is already processed
            let result = user
                .should_update_metadata(&event, true, &whitenoise.database)
                .await
                .unwrap();

            assert!(!result);
        }

        #[tokio::test]
        async fn test_should_update_metadata_newly_created_with_default_metadata() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let mut user = create_test_user(&whitenoise).await;
            let event = create_test_metadata_event(None).await;

            // Ensure user has default metadata (redundant but explicit)
            user.metadata = Metadata::default();
            user.save(&whitenoise.database).await.unwrap();

            // Test that newly created user with default metadata returns true
            // (both conditions would return true, but newly_created takes priority)
            let result = user
                .should_update_metadata(&event, true, &whitenoise.database)
                .await
                .unwrap();

            assert!(result);
        }

        #[tokio::test]
        async fn test_needs_metadata_refresh_default_metadata() {
            let test_pubkey = nostr_sdk::Keys::generate().public_key();
            let user = User {
                id: Some(1),
                pubkey: test_pubkey,
                metadata: Metadata::new(), // Default empty metadata
                created_at: Utc::now(),
                updated_at: Utc::now(), // Even if recently updated
            };

            // Should always refresh if metadata is default/empty
            assert!(user.needs_metadata_refresh());
        }

        #[tokio::test]
        async fn test_needs_metadata_refresh_fresh_metadata() {
            let test_pubkey = nostr_sdk::Keys::generate().public_key();
            let user = User {
                id: Some(1),
                pubkey: test_pubkey,
                metadata: Metadata::new().name("Test User"), // Non-default metadata
                created_at: Utc::now(),
                updated_at: Utc::now(), // Recently updated
            };

            // Should not refresh if metadata is fresh and non-default
            assert!(!user.needs_metadata_refresh());
        }

        #[tokio::test]
        async fn test_needs_metadata_refresh_stale_metadata() {
            let test_pubkey = nostr_sdk::Keys::generate().public_key();
            let stale_time = Utc::now() - Duration::hours(METADATA_TTL_HOURS + 1); // Older than TTL
            let user = User {
                id: Some(1),
                pubkey: test_pubkey,
                metadata: Metadata::new().name("Test User"), // Non-default metadata
                created_at: stale_time,
                updated_at: stale_time, // Old update time
            };

            // Should refresh if metadata is older than TTL
            assert!(user.needs_metadata_refresh());
        }

        #[tokio::test]
        async fn test_needs_metadata_refresh_boundary_case() {
            let test_pubkey = nostr_sdk::Keys::generate().public_key();
            let boundary_time =
                Utc::now() - Duration::hours(METADATA_TTL_HOURS) + Duration::minutes(1); // Just within TTL
            let user = User {
                id: Some(1),
                pubkey: test_pubkey,
                metadata: Metadata::new().name("Test User"), // Non-default metadata
                created_at: boundary_time,
                updated_at: boundary_time,
            };

            // Should not refresh if just within TTL boundary
            assert!(!user.needs_metadata_refresh());
        }
    }
}
