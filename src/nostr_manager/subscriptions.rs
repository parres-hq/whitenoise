//! Subscription functions for NostrManager
//! This mostly handles subscribing and processing events as they come in while the user is active.

use crate::nostr_manager::{NostrManager, Result};
use nostr_sdk::prelude::*;
use sha2::{Digest, Sha256};

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

    pub async fn setup_account_subscriptions(
        &self,
        pubkey: PublicKey,
        user_relays: Vec<RelayUrl>,
        inbox_relays: Vec<RelayUrl>,
        group_relays: Vec<RelayUrl>,
        nostr_group_ids: Vec<String>,
    ) -> Result<()> {
        // Set up core subscriptions in parallel
        let (user_events_result, giftwrap_result, contacts_result, groups_result) = tokio::join!(
            self.setup_user_events_subscription(pubkey, user_relays.clone()),
            self.setup_giftwrap_subscription(pubkey, inbox_relays.clone()),
            self.setup_contacts_metadata_subscription(pubkey, user_relays.clone()),
            self.setup_group_messages_subscription(pubkey, nostr_group_ids, group_relays.clone())
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
        user_relays: Vec<RelayUrl>,
    ) -> Result<()> {
        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_user_events", pubkey_hash));

        // Combine all user event types into a single subscription
        let user_events_filter = Filter::new()
            .kinds([
                Kind::ContactList,
                Kind::Metadata,
                Kind::RelayList,
                Kind::InboxRelays,
            ])
            .author(pubkey)
            .since(Timestamp::now());

        self.client
            .subscribe_with_id_to(user_relays, subscription_id, user_events_filter, None)
            .await?;

        Ok(())
    }

    /// Set up subscription for giftwrap messages to the user
    async fn setup_giftwrap_subscription(
        &self,
        pubkey: PublicKey,
        inbox_relays: Vec<RelayUrl>,
    ) -> Result<()> {
        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_giftwrap", pubkey_hash));

        let giftwrap_filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(pubkey)
            .since(Timestamp::now());

        self.client
            .subscribe_with_id_to(inbox_relays, subscription_id, giftwrap_filter, None)
            .await?;

        Ok(())
    }

    /// Set up subscription for contacts' metadata - can be updated when contacts change
    pub(crate) async fn setup_contacts_metadata_subscription(
        &self,
        pubkey: PublicKey,
        user_relays: Vec<RelayUrl>,
    ) -> Result<()> {
        let contacts_pubkeys = self
            .client
            .get_contact_list_public_keys(self.timeout().await?)
            .await?;

        if contacts_pubkeys.is_empty() {
            // No contacts yet, skip subscription
            return Ok(());
        }

        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_contacts_metadata", pubkey_hash));

        let contacts_metadata_filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(contacts_pubkeys)
            .since(Timestamp::now());

        self.client
            .subscribe_with_id_to(user_relays, subscription_id, contacts_metadata_filter, None)
            .await?;

        Ok(())
    }

    /// Set up subscription for group messages - can be updated when groups change
    pub(crate) async fn setup_group_messages_subscription(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: Vec<String>,
        group_relays: Vec<RelayUrl>,
    ) -> Result<()> {
        if nostr_group_ids.is_empty() {
            // No groups yet, skip subscription
            return Ok(());
        }

        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_mls_messages", pubkey_hash));

        let mls_message_filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), nostr_group_ids)
            .since(Timestamp::now());

        self.client
            .subscribe_with_id_to(group_relays, subscription_id, mls_message_filter, None)
            .await?;

        Ok(())
    }
}
