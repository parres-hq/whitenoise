//! Subscription functions for NostrManager
//! This mostly handles subscribing and processing events as they come in while the user is active.

use std::collections::HashMap;
use std::time::Duration;

use nostr_sdk::prelude::*;
use sha2::{Digest, Sha256};

const MAX_USERS_PER_GLOBAL_SUBSCRIPTION: usize = 1000;

use crate::nostr_manager::{NostrManager, NostrManagerError, Result};

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

    // Sets up subscriptions in batches for all users and their relays
    pub(crate) async fn setup_batched_relay_subscriptions(
        &self,
        users_with_relays: Vec<(PublicKey, Vec<RelayUrl>)>,
        default_relays: &[RelayUrl],
    ) -> Result<()> {
        // 1. Group users by relay
        let relay_user_map = self.group_users_by_relay(users_with_relays, default_relays);

        // 2. Create deterministic batches per relay in parallel
        let batch_futures = relay_user_map
            .into_iter()
            .map(|(relay_url, users)| async move {
                if let Err(e) = self
                    .create_deterministic_batches_for_relay(relay_url.clone(), users)
                    .await
                {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::setup_batched_relay_subscriptions",
                        error = %e,
                        "Failed to create deterministic batches for relay: {}",
                        relay_url
                    );
                }
            });

        futures::future::join_all(batch_futures).await;

        Ok(())
    }

    async fn create_deterministic_batches_for_relay(
        &self,
        relay_url: RelayUrl,
        users: Vec<PublicKey>,
    ) -> Result<()> {
        let batch_count = self.calculate_batch_count(users.len());

        // Group users into deterministic batches
        let mut batches: Vec<Vec<PublicKey>> = vec![Vec::new(); batch_count];
        for user in users {
            let batch_id = self.user_to_batch_id(&user, batch_count);
            batches[batch_id].push(user);
        }

        let mut non_empty_batches = 0;
        let mut failed_batches = 0;
        // Create subscription for each non-empty batch
        for (batch_id, batch_users) in batches.into_iter().enumerate() {
            if !batch_users.is_empty() {
                non_empty_batches += 1;
                let subscription_id = self.batched_subscription_id(&relay_url, batch_id);
                if let Err(e) = self
                    .subscribe_user_batch(relay_url.clone(), batch_users, subscription_id, None)
                    .await
                {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::create_deterministic_batches_for_relay",
                        error = %e,
                        "Failed to subscribe user batch for relay: {}",
                        relay_url
                    );
                    failed_batches += 1;
                }
            }
        }

        if failed_batches == non_empty_batches {
            return Err(NostrManagerError::NoRelayConnections);
        }

        Ok(())
    }

    /// Calculate batch count based on user count (stateless)
    fn calculate_batch_count(&self, user_count: usize) -> usize {
        if user_count == 0 {
            1
        } else {
            user_count.div_ceil(MAX_USERS_PER_GLOBAL_SUBSCRIPTION)
        }
    }

    /// Deterministic batch assignment: hash(pubkey) % batch_count
    fn user_to_batch_id(&self, pubkey: &PublicKey, batch_count: usize) -> usize {
        let mut hasher = Sha256::new();
        hasher.update(pubkey.to_bytes());
        let hash = hasher.finalize();
        let hash_int = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
        (hash_int as usize) % batch_count
    }

    fn group_users_by_relay(
        &self,
        users_with_relays: Vec<(PublicKey, Vec<RelayUrl>)>,
        default_relays: &[RelayUrl],
    ) -> HashMap<RelayUrl, Vec<PublicKey>> {
        let mut relay_user_map: HashMap<RelayUrl, Vec<PublicKey>> = HashMap::new();

        for (user_pubkey, mut user_relays) in users_with_relays {
            if user_relays.is_empty() {
                user_relays = default_relays.to_vec();
            }

            for relay_url in user_relays {
                relay_user_map
                    .entry(relay_url)
                    .or_default()
                    .push(user_pubkey);
            }
        }

        relay_user_map
    }

    /// Helper methods for batched subscriptions
    fn batched_subscription_id(&self, relay_url: &RelayUrl, batch_id: usize) -> SubscriptionId {
        let relay_hash = self.create_relay_hash(relay_url);
        SubscriptionId::new(format!("global_users_{}_{}", relay_hash, batch_id))
    }

    fn create_relay_hash(&self, relay_url: &RelayUrl) -> String {
        let mut hasher = Sha256::new();
        hasher.update(relay_url.as_str().as_bytes());
        let hash = hasher.finalize();
        format!("{:x}", hash)[..12].to_string()
    }

    async fn subscribe_user_batch(
        &self,
        relay_url: RelayUrl,
        batch_users: Vec<PublicKey>,
        subscription_id: SubscriptionId,
        since: Option<Timestamp>,
    ) -> Result<()> {
        let mut filter = Filter::new().authors(batch_users).kinds([
            Kind::Metadata,
            Kind::RelayList,
            Kind::InboxRelays,
            Kind::MlsKeyPackageRelays,
        ]);
        if let Some(since) = since {
            filter = filter.since(since);
        }

        self.ensure_relays_connected(&[relay_url.clone()]).await?;
        self.client
            .subscribe_with_id_to(vec![relay_url], subscription_id, filter, None)
            .await?;
        Ok(())
    }

    /// Refresh subscriptions for a specific user across all their relays
    pub(crate) async fn refresh_user_global_subscriptions(
        &self,
        user_pubkey: PublicKey,
        users_with_relays: Vec<(PublicKey, Vec<RelayUrl>)>,
        default_relays: &[RelayUrl],
    ) -> Result<()> {
        let relay_user_map = self.group_users_by_relay(users_with_relays, default_relays);

        for (relay_url, users) in relay_user_map {
            // Only refresh batches only for relays where the triggering user is present
            if users.contains(&user_pubkey) {
                self.refresh_batch_for_relay_containing_user(relay_url, users, user_pubkey)
                    .await?;
            }
        }

        Ok(())
    }

    /// This method rebuilds the subscriptions for all of the relays the user has
    async fn refresh_batch_for_relay_containing_user(
        &self,
        relay_url: RelayUrl,
        users: Vec<PublicKey>,
        user_pubkey: PublicKey,
    ) -> Result<()> {
        let batch_count = self.calculate_batch_count(users.len());
        let user_batch_id = self.user_to_batch_id(&user_pubkey, batch_count);

        // Group users into deterministic batches (same logic as setup)
        // we need this because we need to know all the users present in the batch
        let mut batches: Vec<Vec<PublicKey>> = vec![Vec::new(); batch_count];
        for user in users {
            let batch_id = self.user_to_batch_id(&user, batch_count);
            batches[batch_id].push(user);
        }

        // Only refresh the batch containing the triggering user
        if let Some(batch_users) = batches.get(user_batch_id) {
            if !batch_users.is_empty() {
                self.refresh_batch_subscription(relay_url, user_batch_id, batch_users.clone())
                    .await?;
            }
        }

        Ok(())
    }

    async fn refresh_batch_subscription(
        &self,
        relay_url: RelayUrl,
        batch_id: usize,
        batch_users: Vec<PublicKey>,
    ) -> Result<()> {
        let buffer_time = Timestamp::now() - Duration::from_secs(10);

        let subscription_id = self.batched_subscription_id(&relay_url, batch_id);
        self.client.unsubscribe(&subscription_id).await;

        self.subscribe_user_batch(relay_url, batch_users, subscription_id, Some(buffer_time))
            .await
    }

    pub async fn setup_account_subscriptions(
        &self,
        pubkey: PublicKey,
        user_relays: &[RelayUrl],
        inbox_relays: &[RelayUrl],
        group_relays: &[RelayUrl],
        nostr_group_ids: &[String],
        since: Option<Timestamp>,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_account_subscriptions",
            "Setting up account subscriptions"
        );
        // Set up core subscriptions in parallel
        let (user_follow_list_result, giftwrap_result, groups_result) = tokio::join!(
            self.setup_user_follow_list_subscription(pubkey, user_relays, since),
            self.setup_giftwrap_subscription(pubkey, inbox_relays, since),
            self.setup_group_messages_subscription(pubkey, nostr_group_ids, group_relays, since)
        );

        // Handle results
        user_follow_list_result?;
        giftwrap_result?;
        groups_result?;

        Ok(())
    }

    async fn setup_user_follow_list_subscription(
        &self,
        pubkey: PublicKey,
        user_relays: &[RelayUrl],
        since: Option<Timestamp>,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_user_follow_list_subscription",
            "Setting up user follow list subscription"
        );
        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_user_follow_list", pubkey_hash));

        // Ensure we're connected to all user relays before subscribing
        self.ensure_relays_connected(user_relays).await?;

        let mut user_follow_list_filter = Filter::new().kind(Kind::ContactList).author(pubkey);
        if let Some(since) = since {
            user_follow_list_filter = user_follow_list_filter.since(since);
        }

        self.client
            .subscribe_with_id_to(user_relays, subscription_id, user_follow_list_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_user_follow_list_subscription",
            "User follow list subscription set up"
        );
        Ok(())
    }

    async fn setup_giftwrap_subscription(
        &self,
        pubkey: PublicKey,
        inbox_relays: &[RelayUrl],
        since: Option<Timestamp>,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_giftwrap_subscription",
            "Setting up giftwrap subscription"
        );
        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_giftwrap", pubkey_hash));

        // Ensure we're connected to all inbox relays before subscribing
        self.ensure_relays_connected(inbox_relays).await?;

        let mut giftwrap_filter = Filter::new().kind(Kind::GiftWrap).pubkey(pubkey);
        if let Some(since) = since {
            giftwrap_filter = giftwrap_filter.since(since);
        }

        self.client
            .subscribe_with_id_to(inbox_relays, subscription_id, giftwrap_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_giftwrap_subscription",
            "Giftwrap subscription set up"
        );
        Ok(())
    }

    /// Set up subscription for group messages - can be updated when groups change
    pub(crate) async fn setup_group_messages_subscription(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: &[String],
        group_relays: &[RelayUrl],
        since: Option<Timestamp>,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_group_messages_subscription",
            "Setting up group messages subscription"
        );
        if nostr_group_ids.is_empty() {
            // No groups yet, skip subscription
            return Ok(());
        }

        // Ensure we're connected to all group relays before subscribing
        self.ensure_relays_connected(group_relays).await?;

        let pubkey_hash = self.create_pubkey_hash(&pubkey);
        let subscription_id = SubscriptionId::new(format!("{}_mls_messages", pubkey_hash));

        let mut mls_message_filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), nostr_group_ids);

        if let Some(since) = since {
            mls_message_filter = mls_message_filter.since(since);
        }

        self.client
            .subscribe_with_id_to(group_relays, subscription_id, mls_message_filter, None)
            .await?;

        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_group_messages_subscription",
            "Group messages subscription set up"
        );
        Ok(())
    }

    /// Unsubscribe from all account-specific subscriptions for a given pubkey.
    /// This includes user follow list, giftwrap, and MLS group message subscriptions.
    pub(crate) async fn unsubscribe_account_subscriptions(&self, pubkey: &PublicKey) -> Result<()> {
        let pubkey_hash = self.create_pubkey_hash(pubkey);

        let subscription_ids = [
            SubscriptionId::new(format!("{}_user_follow_list", pubkey_hash)),
            SubscriptionId::new(format!("{}_giftwrap", pubkey_hash)),
            SubscriptionId::new(format!("{}_mls_messages", pubkey_hash)),
        ];

        let unsubscribe_futures = subscription_ids
            .iter()
            .map(|id| self.client.unsubscribe(id));

        futures::future::join_all(unsubscribe_futures).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::event_tracker::NoEventTracker;
    use nostr_sdk::Keys;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_create_pubkey_hash() {
        let (event_sender, _) = mpsc::channel(100);
        let event_tracker = Arc::new(NoEventTracker);
        let nostr_manager =
            NostrManager::new(event_sender, event_tracker, NostrManager::default_timeout())
                .await
                .unwrap();

        let pubkey = Keys::generate().public_key();
        let hash1 = nostr_manager.create_pubkey_hash(&pubkey);
        let hash2 = nostr_manager.create_pubkey_hash(&pubkey);

        // Same pubkey should produce same hash with same session salt
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 12); // Should be 12 characters as specified
    }
}
