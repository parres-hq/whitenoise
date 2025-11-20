use crate::whitenoise::Whitenoise;
use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::relays::Relay;
use nostr_sdk::prelude::*;
use std::time::Duration;

impl Whitenoise {
    /// Helper method to create and encode a key package for the given account.
    pub(crate) async fn encoded_key_package(
        &self,
        account: &Account,
    ) -> Result<(String, [Tag; 4])> {
        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        let key_package_relays = account.key_package_relays(self).await?;

        if key_package_relays.is_empty() {
            return Err(WhitenoiseError::AccountMissingKeyPackageRelays);
        }

        let key_package_relay_urls = Relay::urls(&key_package_relays);
        let result = mdk
            .create_key_package_for_event(&account.pubkey, key_package_relay_urls)
            .map_err(|e| WhitenoiseError::Configuration(format!("NostrMls error: {}", e)))?;

        Ok(result)
    }

    /// Publishes the MLS key package for the given account to its key package relays.
    pub async fn publish_key_package_for_account(&self, account: &Account) -> Result<()> {
        // Extract key package data while holding the lock
        let (encoded_key_package, tags) = self.encoded_key_package(account).await?;
        let relays = account.key_package_relays(self).await?;

        if relays.is_empty() {
            return Err(WhitenoiseError::AccountMissingKeyPackageRelays);
        }
        let relays_urls = Relay::urls(&relays);
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let result = self
            .nostr
            .publish_key_package_with_signer(&encoded_key_package, &relays_urls, &tags, signer)
            .await?;

        tracing::debug!(target: "whitenoise::publish_key_package_for_account", "Published key package to relays: {:?}", result);

        Ok(())
    }

    /// Deletes the key package from the relays for the given account.
    ///
    /// Returns `true` if a key package was found and deleted, `false` if no key package was found.
    pub async fn delete_key_package_for_account(
        &self,
        account: &Account,
        event_id: &EventId,
        delete_mls_stored_keys: bool,
    ) -> Result<bool> {
        let key_package_filter = Filter::new()
            .id(*event_id)
            .kind(Kind::MlsKeyPackage)
            .author(account.pubkey);

        let mut key_package_stream = self
            .nostr
            .client
            .stream_events(key_package_filter, Duration::from_secs(5))
            .await?;

        let mut key_package_events = Vec::new();
        while let Some(event) = key_package_stream.next().await {
            key_package_events.push(event);
        }
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        if let Some(event) = key_package_events.first() {
            if delete_mls_stored_keys {
                let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
                let key_package = mdk.parse_key_package(event)?;
                mdk.delete_key_package_from_storage(&key_package)?;
            }

            let key_package_relays = account.key_package_relays(self).await?;
            if key_package_relays.is_empty() {
                return Err(WhitenoiseError::AccountMissingKeyPackageRelays);
            }

            let key_package_relays_urls = Relay::urls(&key_package_relays);

            let result = self
                .nostr
                .publish_event_deletion_with_signer(&event.id, &key_package_relays_urls, signer)
                .await?;
            return Ok(!result.success.is_empty());
        }
        Ok(false)
    }

    /// Finds and returns all key package events for the given account from its key package relays.
    ///
    /// This method fetches all key package events (not just the latest) authored by the account
    /// from the account's key package relays. This is useful for getting a complete view of
    /// all published key packages.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to find key packages for
    ///
    /// # Returns
    ///
    /// Returns a vector of all key package events found for the account.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Account has no key package relays configured
    /// - Failed to retrieve account's key package relays
    /// - Network error while fetching events from relays
    /// - NostrSDK error during event streaming
    pub async fn fetch_all_key_packages_for_account(
        &self,
        account: &Account,
    ) -> Result<Vec<Event>> {
        let key_package_relays = account.key_package_relays(self).await?;
        let relay_urls: Vec<RelayUrl> = Relay::urls(&key_package_relays);

        if relay_urls.is_empty() {
            return Err(WhitenoiseError::AccountMissingKeyPackageRelays);
        }

        let key_package_filter = Filter::new()
            .kind(Kind::MlsKeyPackage)
            .author(account.pubkey);

        let mut key_package_stream = self
            .nostr
            .client
            .stream_events_from(relay_urls, key_package_filter, Duration::from_secs(10))
            .await?;

        let mut key_package_events = Vec::new();
        while let Some(event) = key_package_stream.next().await {
            key_package_events.push(event);
        }

        tracing::debug!(
            target: "whitenoise::fetch_all_key_packages_for_account",
            "Found {} key package events for account {}",
            key_package_events.len(),
            account.pubkey.to_hex()
        );

        Ok(key_package_events)
    }

    /// Deletes all key package events from relays for the given account.
    ///
    /// This method finds all key package events authored by the account and publishes
    /// a batch deletion event to efficiently remove them from the relays. It then verifies
    /// the deletions by refetching and returns the actual count of deleted key packages.
    /// Optionally, it can also delete the MLS stored keys from local storage.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to delete key packages for
    /// * `delete_mls_stored_keys` - Whether to also delete MLS keys from local storage
    ///
    /// # Returns
    ///
    /// Returns the number of key packages that were successfully deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Account has no key package relays configured
    /// - Failed to retrieve account's key package relays
    /// - Failed to get signing keys for the account
    /// - Network error while fetching or publishing events
    /// - Batch deletion event publishing failed
    pub async fn delete_all_key_packages_for_account(
        &self,
        account: &Account,
        delete_mls_stored_keys: bool,
    ) -> Result<usize> {
        // Fetch all key packages once upfront
        let key_package_events = self.fetch_all_key_packages_for_account(account).await?;

        if key_package_events.is_empty() {
            tracing::info!(
                target: "whitenoise::delete_all_key_packages_for_account",
                "No key package events found for account {}",
                account.pubkey.to_hex()
            );
            return Ok(0);
        }

        let initial_count = key_package_events.len();
        tracing::debug!(
            target: "whitenoise::delete_all_key_packages_for_account",
            "Found {} key package events to delete for account {}",
            initial_count,
            account.pubkey.to_hex()
        );

        // Get signer and relays once - reuse for all operations
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let key_package_relays = account.key_package_relays(self).await?;

        if key_package_relays.is_empty() {
            return Err(WhitenoiseError::AccountMissingKeyPackageRelays);
        }

        let key_package_relays_urls = Relay::urls(&key_package_relays);

        // Delete from MLS storage first (if requested) before publishing deletion events
        if delete_mls_stored_keys {
            // Create NostrMls instance once for MLS storage deletion
            let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
            let mut storage_delete_count = 0;

            for event in &key_package_events {
                // Delete from MLS storage
                match mdk.parse_key_package(event) {
                    Ok(key_package) => match mdk.delete_key_package_from_storage(&key_package) {
                        Ok(_) => {
                            storage_delete_count += 1;
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "whitenoise::delete_all_key_packages_for_account",
                                "Failed to delete key package from storage for event {}: {}",
                                event.id,
                                e
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            target: "whitenoise::delete_all_key_packages_for_account",
                            "Failed to parse key package for event {}: {}",
                            event.id,
                            e
                        );
                    }
                }
            }

            tracing::debug!(
                target: "whitenoise::delete_all_key_packages_for_account",
                "Deleted {} out of {} key packages from MLS storage",
                storage_delete_count,
                initial_count
            );
        }

        // Batch delete from relays - much more efficient than individual deletion events
        let event_ids: Vec<EventId> = key_package_events.iter().map(|e| e.id).collect();

        tracing::debug!(
            target: "whitenoise::delete_all_key_packages_for_account",
            "Publishing batch deletion event for {} key packages",
            event_ids.len()
        );

        match self
            .nostr
            .publish_batch_event_deletion_with_signer(
                &event_ids,
                &key_package_relays_urls,
                signer.clone(),
            )
            .await
        {
            Ok(result) => {
                if result.success.is_empty() {
                    tracing::error!(
                        target: "whitenoise::delete_all_key_packages_for_account",
                        "Batch deletion event was not accepted by any relay"
                    );
                } else {
                    tracing::info!(
                        target: "whitenoise::delete_all_key_packages_for_account",
                        "Published batch deletion event to {} relay(s) for {} key packages",
                        result.success.len(),
                        event_ids.len()
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::delete_all_key_packages_for_account",
                    "Failed to publish batch deletion event: {}",
                    e
                );
                return Err(e.into());
            }
        }

        // Wait for relays to process deletion events
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify deletion by refetching key packages
        let remaining_events = self.fetch_all_key_packages_for_account(account).await?;
        let actually_deleted = initial_count.saturating_sub(remaining_events.len());

        if !remaining_events.is_empty() {
            tracing::warn!(
                target: "whitenoise::delete_all_key_packages_for_account",
                "After deletion, {} key package(s) still remain for account {}. Will retry with batch deletion.",
                remaining_events.len(),
                account.pubkey.to_hex()
            );

            // Retry batch deletion for remaining key packages
            let remaining_ids: Vec<EventId> = remaining_events.iter().map(|e| e.id).collect();

            match self
                .nostr
                .publish_batch_event_deletion_with_signer(
                    &remaining_ids,
                    &key_package_relays_urls,
                    signer.clone(),
                )
                .await
            {
                Ok(result) => {
                    if result.success.is_empty() {
                        tracing::error!(
                            target: "whitenoise::delete_all_key_packages_for_account",
                            "Retry: Batch deletion event was not accepted by any relay"
                        );
                    } else {
                        tracing::info!(
                            target: "whitenoise::delete_all_key_packages_for_account",
                            "Retry: Published batch deletion event to {} relay(s) for {} remaining key packages",
                            result.success.len(),
                            remaining_ids.len()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::delete_all_key_packages_for_account",
                        "Retry: Failed to publish batch deletion event: {}",
                        e
                    );
                }
            }

            // Wait again and verify
            tokio::time::sleep(Duration::from_millis(500)).await;
            let final_remaining = self.fetch_all_key_packages_for_account(account).await?;
            let final_deleted = initial_count.saturating_sub(final_remaining.len());

            if !final_remaining.is_empty() {
                tracing::error!(
                    target: "whitenoise::delete_all_key_packages_for_account",
                    "After retry, {} key package(s) still remain for account {}. Event IDs: {:?}",
                    final_remaining.len(),
                    account.pubkey.to_hex(),
                    final_remaining.iter().map(|e| e.id.to_hex()).collect::<Vec<_>>()
                );
            } else {
                tracing::info!(
                    target: "whitenoise::delete_all_key_packages_for_account",
                    "Successfully deleted all {} key packages after retry",
                    final_deleted
                );
            }

            return Ok(final_deleted);
        }

        tracing::info!(
            target: "whitenoise::delete_all_key_packages_for_account",
            "Successfully deleted all {} key package events for account {}",
            actually_deleted,
            account.pubkey.to_hex()
        );

        Ok(actually_deleted)
    }
}
