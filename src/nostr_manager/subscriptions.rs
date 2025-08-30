//! Subscription functions for NostrManager
//! This mostly handles subscribing and processing events as they come in while the user is active.

use nostr_sdk::prelude::*;
use sha2::{Digest, Sha256};

use crate::{
    nostr_manager::{NostrManager, Result},
    whitenoise::relays::Relay,
};

impl NostrManager {
    /// Create a short hash from a pubkey for use in subscription IDs
    /// Uses first 12 characters of SHA256 hash for privacy and collision resistance, salted per session
    fn create_pubkey_hash(&self, pubkey: &PublicKey) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.session_salt());
        hasher.update(pubkey.to_bytes());
        let hash = hasher.finalize();
        format!("{:x}", hash)[..12].to_string()
    }

    pub(crate) async fn setup_global_users_subscriptions(
        &self,
        users_with_relays: Vec<(PublicKey, Vec<RelayUrl>)>,
        default_relays: &[RelayUrl],
    ) -> Result<()> {
        if users_with_relays.is_empty() {
            return Ok(());
        }

        for (user_pubkey, mut relay_urls) in users_with_relays {
            if relay_urls.is_empty() {
                // If we don't know the user relays
                relay_urls = default_relays.to_vec(); // Use default relays
            }

            let pubkey_hex = user_pubkey.to_hex();
            let subscription_id = SubscriptionId::new(format!(
                "{}_global_users",
                &pubkey_hex[..13.min(pubkey_hex.len())]
            ));

            let filter = Filter::new().author(user_pubkey).kinds([
                Kind::Metadata,
                Kind::RelayList,
                Kind::InboxRelays,
                Kind::MlsKeyPackageRelays,
            ]);

            self.ensure_relays_connected(&relay_urls).await?;

            self.client
                .subscribe_with_id_to(relay_urls, subscription_id, filter, None)
                .await?;
        }
        Ok(())
    }

    pub async fn setup_account_subscriptions(
        &self,
        pubkey: PublicKey,
        user_relays: &[Relay],
        inbox_relays: &[Relay],
        group_relays: &[Relay],
        nostr_group_ids: &[String],
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_account_subscriptions",
            "Setting up account subscriptions"
        );
        // Set up core subscriptions in parallel
        let (user_events_result, giftwrap_result, contacts_result, groups_result) = tokio::join!(
            self.setup_user_events_subscription(pubkey, user_relays),
            self.setup_giftwrap_subscription(pubkey, inbox_relays),
            self.setup_contacts_metadata_subscription(pubkey, user_relays),
            self.setup_group_messages_subscription(pubkey, nostr_group_ids, group_relays)
        );

        // Handle results
        user_events_result?;
        giftwrap_result?;
        contacts_result?;
        groups_result?;

        Ok(())
    }

    /// Set up subscription for user's own events (contact list, metadata, relay lists)
    async fn setup_user_events_subscription(
        &self,
        pubkey: PublicKey,
        user_relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_user_events_subscription",
            "Setting up user events subscription"
        );
        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_user_events", pubkey_hash));

        let urls: Vec<RelayUrl> = user_relays.iter().map(|r| r.url.clone()).collect();
        // Ensure we're connected to all user relays before subscribing
        self.ensure_relays_connected(&urls).await?;

        // Combine all user event types into a single subscription
        let user_events_filter = Filter::new()
            .kinds([
                Kind::ContactList,
                Kind::Metadata,
                Kind::RelayList,
                Kind::InboxRelays,
            ])
            .author(pubkey);

        self.client
            .subscribe_with_id_to(urls, subscription_id, user_events_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_user_events_subscription",
            "User events subscription set up"
        );
        Ok(())
    }

    /// Set up subscription for giftwrap messages to the user
    async fn setup_giftwrap_subscription(
        &self,
        pubkey: PublicKey,
        inbox_relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_giftwrap_subscription",
            "Setting up giftwrap subscription"
        );
        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_giftwrap", pubkey_hash));

        let urls: Vec<RelayUrl> = inbox_relays.iter().map(|r| r.url.clone()).collect();
        // Ensure we're connected to all inbox relays before subscribing
        self.ensure_relays_connected(&urls).await?;

        let giftwrap_filter = Filter::new().kind(Kind::GiftWrap).pubkey(pubkey);

        self.client
            .subscribe_with_id_to(urls, subscription_id, giftwrap_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_giftwrap_subscription",
            "Giftwrap subscription set up"
        );
        Ok(())
    }

    /// Set up subscription for contacts' metadata - can be updated when contacts change
    pub(crate) async fn setup_contacts_metadata_subscription(
        &self,
        pubkey: PublicKey,
        user_relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_contacts_metadata_subscription",
            "Setting up contacts metadata subscription using user relays, {:?}",
            user_relays
        );
        let contacts_pubkeys = self
            .client
            .get_contact_list_public_keys(self.timeout)
            .await?;

        if contacts_pubkeys.is_empty() {
            // No contacts yet, skip subscription
            return Ok(());
        }

        let urls: Vec<RelayUrl> = user_relays.iter().map(|r| r.url.clone()).collect();
        // Ensure we're connected to all user relays before subscribing
        self.ensure_relays_connected(&urls).await?;

        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_contacts_metadata", pubkey_hash));

        let contacts_metadata_filter = Filter::new().kind(Kind::Metadata).authors(contacts_pubkeys);
        self.client
            .subscribe_with_id_to(urls, subscription_id, contacts_metadata_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_contacts_metadata_subscription",
            "Contacts metadata subscription set up"
        );
        Ok(())
    }

    /// Set up subscription for group messages - can be updated when groups change
    pub(crate) async fn setup_group_messages_subscription(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: &[String],
        group_relays: &[Relay], // TODO: Refactor this method to use RelayUrls instead of Relays
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_group_messages_subscription",
            "Setting up group messages subscription"
        );
        if nostr_group_ids.is_empty() {
            // No groups yet, skip subscription
            return Ok(());
        }

        let urls: Vec<RelayUrl> = group_relays.iter().map(|r| r.url.clone()).collect();
        // Ensure we're connected to all group relays before subscribing
        self.ensure_relays_connected(&urls).await?;

        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_mls_messages", pubkey_hash));

        let mls_message_filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), nostr_group_ids);

        self.client
            .subscribe_with_id_to(urls, subscription_id, mls_message_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_group_messages_subscription",
            "Group messages subscription set up"
        );
        Ok(())
    }
}
