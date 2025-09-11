use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use std::time::Duration;

impl Whitenoise {
    /// Helper method to create and encode a key package for the given account.
    pub(crate) async fn encoded_key_package(
        &self,
        account: &Account,
    ) -> Result<(String, [Tag; 4])> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let key_package_relay_urls = account
            .key_package_relays(self)
            .await?
            .iter()
            .map(|r| r.url.clone())
            .collect::<Vec<RelayUrl>>();
        let result = nostr_mls
            .create_key_package_for_event(&account.pubkey, key_package_relay_urls)
            .map_err(|e| WhitenoiseError::Configuration(format!("NostrMls error: {}", e)))?;

        Ok(result)
    }

    /// Publishes the MLS key package for the given account to its key package relays.
    pub(crate) async fn publish_key_package_for_account(&self, account: &Account) -> Result<()> {
        // Extract key package data while holding the lock
        let (encoded_key_package, tags) = self.encoded_key_package(account).await?;
        let relays = account.key_package_relays(self).await?;
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let result = self
            .nostr
            .publish_key_package_with_signer(&encoded_key_package, &relays, &tags, signer)
            .await?;

        tracing::debug!(target: "whitenoise::publish_key_package_for_account", "Published key package to relays: {:?}", result);

        Ok(())
    }

    /// Deletes the key package from the relays for the given account.
    pub(crate) async fn delete_key_package_from_relays_for_account(
        &self,
        account: &Account,
        event_id: &EventId,
        delete_mls_stored_keys: bool,
    ) -> Result<()> {
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
                let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
                let key_package = nostr_mls.parse_key_package(event)?;
                nostr_mls.delete_key_package_from_storage(&key_package)?;
            }

            self.nostr
                .publish_event_deletion_with_signer(
                    &event.id,
                    &account.key_package_relays(self).await?,
                    signer,
                )
                .await?;
        } else {
            tracing::warn!(target: "whitenoise::delete_key_package_from_relays_for_account", "Key package event not found for account: {}", account.pubkey.to_hex());
            return Ok(());
        }

        Ok(())
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
    /// - Failed to retrieve account's key package relays
    /// - Network error while fetching events from relays
    /// - NostrSDK error during event streaming
    pub async fn find_all_key_packages_for_account(&self, account: &Account) -> Result<Vec<Event>> {
        let key_package_relays = account.key_package_relays(self).await?;
        let relay_urls: Vec<RelayUrl> = key_package_relays.iter().map(|r| r.url.clone()).collect();

        if relay_urls.is_empty() {
            tracing::warn!(
                target: "whitenoise::find_all_key_packages_for_account",
                "Account {} has no key package relays configured",
                account.pubkey.to_hex()
            );
            return Ok(Vec::new());
        }

        let key_package_filter = Filter::new()
            .kind(Kind::MlsKeyPackage)
            .author(account.pubkey);

        let mut key_package_stream = self
            .nostr
            .client
            .stream_events(key_package_filter, Duration::from_secs(10))
            .await?;

        let mut key_package_events = Vec::new();
        while let Some(event) = key_package_stream.next().await {
            key_package_events.push(event);
        }

        tracing::debug!(
            target: "whitenoise::find_all_key_packages_for_account",
            "Found {} key package events for account {}",
            key_package_events.len(),
            account.pubkey.to_hex()
        );

        Ok(key_package_events)
    }

    /// Deletes all key package events from relays for the given account.
    ///
    /// This method finds all key package events authored by the account and publishes
    /// deletion events to remove them from the relays. Optionally, it can also delete
    /// the MLS stored keys from local storage.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to delete key packages for
    /// * `delete_mls_stored_keys` - Whether to also delete MLS keys from local storage
    ///
    /// # Returns
    ///
    /// Returns the number of key packages that were deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to retrieve account's key package relays
    /// - Failed to get signing keys for the account
    /// - Network error while fetching or publishing events
    /// - Failed to delete MLS keys from storage (if requested)
    pub async fn delete_all_key_packages_for_account(
        &self,
        account: &Account,
        delete_mls_stored_keys: bool,
    ) -> Result<usize> {
        let key_package_events = self.find_all_key_packages_for_account(account).await?;

        if key_package_events.is_empty() {
            tracing::info!(
                target: "whitenoise::delete_all_key_packages_for_account",
                "No key package events found for account {}",
                account.pubkey.to_hex()
            );
            return Ok(0);
        }

        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let key_package_relays = account.key_package_relays(self).await?;

        let mut deleted_count = 0;

        for event in &key_package_events {
            if delete_mls_stored_keys {
                match Account::create_nostr_mls(account.pubkey, &self.config.data_dir) {
                    Ok(nostr_mls) => match nostr_mls.parse_key_package(event) {
                        Ok(key_package) => {
                            if let Err(e) = nostr_mls.delete_key_package_from_storage(&key_package)
                            {
                                tracing::warn!(
                                    target: "whitenoise::delete_all_key_packages_for_account",
                                    "Failed to delete key package from storage for event {}: {}",
                                    event.id,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "whitenoise::delete_all_key_packages_for_account",
                                "Failed to parse key package for event {}: {}",
                                event.id,
                                e
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            target: "whitenoise::delete_all_key_packages_for_account",
                            "Failed to create NostrMls instance: {}",
                            e
                        );
                    }
                }
            }

            // Publish deletion event
            match self
                .nostr
                .publish_event_deletion_with_signer(&event.id, &key_package_relays, signer.clone())
                .await
            {
                Ok(_) => {
                    deleted_count += 1;
                    tracing::debug!(
                        target: "whitenoise::delete_all_key_packages_for_account",
                        "Published deletion event for key package {}",
                        event.id
                    );
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::delete_all_key_packages_for_account",
                        "Failed to publish deletion event for key package {}: {}",
                        event.id,
                        e
                    );
                }
            }
        }

        tracing::info!(
            target: "whitenoise::delete_all_key_packages_for_account",
            "Deleted {} out of {} key package events for account {}",
            deleted_count,
            key_package_events.len(),
            account.pubkey.to_hex()
        );

        Ok(deleted_count)
    }
}
