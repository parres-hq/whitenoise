//! Subscription functions for NostrManager
//! This mostly handles subscribing and processing events as they come in while the user is active.

use crate::nostr_manager::event_processor::ProcessableEvent;
use crate::nostr_manager::{NostrManager, NostrManagerError, Result};
use nostr_sdk::prelude::*;

impl NostrManager {
    async fn setup_account_subscriptions(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: Vec<String>,
    ) -> Result<()> {
        // Get user's relays to ensure we're subscribing on the right relays
        let user_relays = self.client.get_relays().await?;

        // Create subscription IDs for each type of subscription
        let contact_list_sub = SubscriptionId::new(format!("{}_contact_list", pubkey.to_hex()));
        let metadata_sub = SubscriptionId::new(format!("{}_metadata", pubkey.to_hex()));
        let contacts_metadata_sub = SubscriptionId::new(format!("{}_contacts_metadata", pubkey.to_hex()));
        let relay_list_sub = SubscriptionId::new(format!("{}_relay_list", pubkey.to_hex()));
        let giftwrap_sub = SubscriptionId::new(format!("{}_giftwrap", pubkey.to_hex()));
        let mls_messages_sub = SubscriptionId::new(format!("{}_mls_messages", pubkey.to_hex()));

        // Create filters for each subscription type
        let contact_list_filter = Filter::new()
            .kind(Kind::ContactList)
            .author(pubkey)
            .since(Timestamp::now());

        let contacts_pubkeys = self
            .client
            .get_contact_list_public_keys(self.timeout().await?)
            .await?;

        // Separate filter for user's own metadata
        let metadata_filter = Filter::new()
            .kind(Kind::Metadata)
            .author(pubkey)
            .since(Timestamp::now());

        // Separate filter for contacts' metadata
        let contacts_metadata_filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(contacts_pubkeys)
            .since(Timestamp::now());

        let relay_list_filter = Filter::new()
            .kind(Kind::RelayList)
            .author(pubkey)
            .since(Timestamp::now());

        let inbox_relay_list_filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(pubkey)
            .since(Timestamp::now());

        let giftwrap_filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(pubkey)
            .since(Timestamp::now());

        // Set up all subscriptions in parallel
        let (contact_list_result, metadata_result, contacts_metadata_result, relay_list_result, inbox_relay_result, giftwrap_result) = tokio::join!(
            self.client.subscribe_with_id_to(contact_list_sub, contact_list_filter, user_relays.clone()),
            self.client.subscribe_with_id_to(metadata_sub, metadata_filter, user_relays.clone()),
            self.client.subscribe_with_id_to(contacts_metadata_sub, contacts_metadata_filter, user_relays.clone()),
            self.client.subscribe_with_id_to(relay_list_sub, relay_list_filter, user_relays.clone()),
            self.client.subscribe_with_id_to(relay_list_sub.clone(), inbox_relay_list_filter, user_relays.clone()),
            self.client.subscribe_with_id_to(giftwrap_sub, giftwrap_filter, user_relays.clone())
        );

        // Handle results
        contact_list_result?;
        metadata_result?;
        contacts_metadata_result?;
        relay_list_result?;
        inbox_relay_result?;
        giftwrap_result?;

        // Set up MLS group messages subscription if needed
        if !nostr_group_ids.is_empty() {
            let mls_message_filter = Filter::new()
                .kind(Kind::MlsGroupMessage)
                .custom_tags(SingleLetterTag::lowercase(Alphabet::H), nostr_group_ids)
                .since(Timestamp::now());

            self.client
                .subscribe_with_id_to(mls_messages_sub, mls_message_filter, user_relays)
                .await?;
        }

        Ok(())
    }

    pub async fn setup_subscriptions(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: Vec<String>,
    ) -> Result<()> {
        // Set up all subscriptions for the account
        self.setup_account_subscriptions(pubkey, nostr_group_ids).await?;

        Ok(())
    }
}

