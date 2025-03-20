//! Negentropy syncing functions for NostrManager
//! Negentropy is a fast/efficient way to fetch only the events that we don't have.
//! It's currently only suppoted by strfry relays so this is not used as extensively as it will be in the future.

#![allow(unused)]

use crate::nostr_manager::{NostrManager, NostrManagerError, Result};
use nostr_sdk::prelude::*;

impl NostrManager {
    pub async fn sync_for_user(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
        group_ids: Vec<String>,
    ) -> Result<()> {
        self.sync_user_metadata(pubkey, last_synced).await?;
        self.sync_contacts(last_synced).await?;
        self.sync_contacts_metadata(last_synced).await?;
        self.sync_user_relays(pubkey, last_synced).await?;
        self.sync_user_inbox_relays(pubkey, last_synced).await?;
        self.sync_user_key_package_relays(pubkey, last_synced)
            .await?;
        self.sync_user_key_packages(pubkey, last_synced).await?;
        self.sync_user_giftwrapped_events(pubkey, last_synced)
            .await?;
        self.sync_group_messages(last_synced, group_ids).await?;
        Ok(())
    }

    pub async fn sync_user_metadata(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
    ) -> Result<()> {
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .author(pubkey)
            .since(last_synced)
            .until(Timestamp::now());
        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_metadata",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_metadata",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_metadata",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_user_relays(&self, pubkey: PublicKey, last_synced: Timestamp) -> Result<()> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::RelayList)
            .since(last_synced)
            .until(Timestamp::now());
        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_relays",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_relays",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_relays",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_user_inbox_relays(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
    ) -> Result<()> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::InboxRelays)
            .since(last_synced)
            .until(Timestamp::now());
        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_inbox_relays",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_inbox_relays",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_inbox_relays",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_user_key_package_relays(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
    ) -> Result<()> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::MlsKeyPackageRelays)
            .since(last_synced)
            .until(Timestamp::now());
        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_key_package_relays",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_key_package_relays",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_key_package_relays",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_user_key_packages(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
    ) -> Result<()> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::MlsKeyPackage)
            .since(last_synced)
            .until(Timestamp::now());
        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_key_packages",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_key_packages",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_key_packages",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_contacts(&self, last_synced: Timestamp) -> Result<()> {
        let pubkey = self
            .client
            .signer()
            .await
            .map_err(NostrManagerError::Client)?
            .get_public_key()
            .await
            .map_err(NostrManagerError::Signer)?;

        let filter = Filter::new()
            .kind(Kind::ContactList)
            .author(pubkey)
            .since(last_synced)
            .until(Timestamp::now());

        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_contacts",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_contacts",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_contacts",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_contacts_metadata(&self, last_synced: Timestamp) -> Result<()> {
        let contacts_pubkeys = self.query_contact_list_pubkeys().await?;

        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(contacts_pubkeys)
            .since(last_synced)
            .until(Timestamp::now());

        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_contacts_metadata",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_contacts_metadata",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_contacts_metadata",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_group_messages(
        &self,
        last_synced: Timestamp,
        group_ids: Vec<String>,
    ) -> Result<()> {
        let filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), group_ids)
            .since(last_synced)
            .until(Timestamp::now());

        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_group_messages",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_group_messages",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_group_messages",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }

    async fn sync_user_giftwrapped_events(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
    ) -> Result<()> {
        let filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkeys(vec![pubkey])
            .since(last_synced)
            .until(Timestamp::now());
        let output = self
            .client
            .sync(filter, &nostr_sdk::SyncOptions::default())
            .await?;
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_giftwrapped_events",
            "SUCCESS: {:?}",
            output.success
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_giftwrapped_events",
            "FAILED: {:?}",
            output.failed
        );
        tracing::debug!(
            target: "whitenoise::nostr_client::sync_user_giftwrapped_events",
            "VALUES: local: {:?}, remote: {:?}, sent: {:?}, received: {:?}, send_failed: {:?}",
            output.val.local.len(),
            output.val.remote.len(),
            output.val.sent.len(),
            output.val.received.len(),
            output.val.send_failures.len()
        );
        Ok(())
    }
}
