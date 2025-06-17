//! Subscription functions for NostrManager
//! This mostly handles subscribing and processing events as they come in while the user is active.

use crate::nostr_manager::{NostrManager, Result};
use nostr_sdk::prelude::*;

impl NostrManager {
    pub async fn setup_account_subscriptions(
        &self,
        pubkey: PublicKey,
        user_relays: Vec<RelayUrl>,
        nostr_group_ids: Vec<String>,
    ) -> Result<()> {
        // Set up core subscriptions in parallel
        let (user_events_result, giftwrap_result, contacts_result, groups_result) = tokio::join!(
            self.setup_user_events_subscription(pubkey, user_relays.clone()),
            self.setup_giftwrap_subscription(pubkey, user_relays.clone()),
            self.setup_contacts_metadata_subscription(pubkey, user_relays.clone()),
            self.setup_group_messages_subscription(pubkey, nostr_group_ids, user_relays.clone())
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
        let subscription_id = SubscriptionId::new(format!("{}_user_events", pubkey.to_hex()));

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
        user_relays: Vec<RelayUrl>,
    ) -> Result<()> {
        let subscription_id = SubscriptionId::new(format!("{}_giftwrap", pubkey.to_hex()));

        let giftwrap_filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(pubkey)
            .since(Timestamp::now());

        self.client
            .subscribe_with_id_to(user_relays, subscription_id, giftwrap_filter, None)
            .await?;

        Ok(())
    }

    /// Set up subscription for contacts' metadata - can be updated when contacts change
    async fn setup_contacts_metadata_subscription(
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

        let subscription_id = SubscriptionId::new(format!("{}_contacts_metadata", pubkey.to_hex()));

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
    async fn setup_group_messages_subscription(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: Vec<String>,
        user_relays: Vec<RelayUrl>,
    ) -> Result<()> {
        if nostr_group_ids.is_empty() {
            // No groups yet, skip subscription
            return Ok(());
        }

        let subscription_id = SubscriptionId::new(format!("{}_mls_messages", pubkey.to_hex()));

        let mls_message_filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), nostr_group_ids)
            .since(Timestamp::now());

        self.client
            .subscribe_with_id_to(user_relays, subscription_id, mls_message_filter, None)
            .await?;

        Ok(())
    }
}
