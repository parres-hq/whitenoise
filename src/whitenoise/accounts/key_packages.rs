use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

impl Whitenoise {
    pub(crate) async fn encoded_key_package(
        &self,
        account: &Account,
    ) -> Result<(String, [Tag; 4])> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
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
    ///
    /// This method attempts to acquire the `nostr_mls` lock, generate a key package event,
    /// and publish it to the account's key package relays. If successful, the key package
    /// is published to Nostr; otherwise, onboarding status is updated accordingly.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose key package will be published.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the lock cannot be acquired, if the key package cannot be generated,
    /// or if publishing to Nostr fails.
    pub(crate) async fn publish_key_package_for_account(&self, account: &Account) -> Result<()> {
        // Extract key package data while holding the lock
        let (encoded_key_package, tags) = self.encoded_key_package(account).await?;

        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package).tags(tags);

        let result = self
            .nostr
            .publish_event_builder_with_signer(
                key_package_event_builder,
                &account.key_package_relays(self).await?,
                signer,
            )
            .await?;

        tracing::debug!(target: "whitenoise::publish_key_package_for_account", "Published key package to relays: {:?}", result);

        Ok(())
    }

    /// Deletes the key package from the relays for the given account.
    ///
    /// This method deletes the key package from the relays for the given account.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose key package will be deleted.
    /// * `event_id` - The `EventId` of the key package to delete.
    /// * `key_package_relays` - A vector of `RelayUrl` specifying the relays to delete the key package from.
    /// * `delete_mls_stored_keys` - A boolean indicating whether to delete the key package from MLS storage.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
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

        let key_package_events = self
            .nostr
            .fetch_events_with_filter(key_package_filter)
            .await?;
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        if let Some(event) = key_package_events.first() {
            if delete_mls_stored_keys {
                let nostr_mls =
                    Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
                let key_package = nostr_mls.parse_key_package(event)?;
                nostr_mls.delete_key_package_from_storage(&key_package)?;
            }

            let builder = EventBuilder::delete(EventDeletionRequest::new().id(event.id));

            self.nostr
                .publish_event_builder_with_signer(
                    builder,
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
