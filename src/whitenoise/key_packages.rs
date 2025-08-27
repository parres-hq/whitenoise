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
}
