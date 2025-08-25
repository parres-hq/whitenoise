use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::{
    nostr_manager::NostrManager,
    whitenoise::{accounts::Account, error::Result, Whitenoise},
};

impl Whitenoise {
    /// Contact list handler that performs the actual processing
    /// Note: Event tracking (published/processed checks) is handled at the processor level
    pub(crate) async fn handle_contact_list(&self, account: &Account, event: Event) -> Result<()> {
        // Acquire per-account semaphore permit to serialize contact list processing for this account
        let semaphore = self
            .contact_list_guards
            .entry(account.pubkey)
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone();

        let _permit = semaphore.acquire_owned().await.map_err(|_| {
            crate::whitenoise::error::WhitenoiseError::ContactList(
                "Failed to acquire semaphore permit for contact list processing".to_string(),
            )
        })?;

        tracing::debug!(
            target: "whitenoise::handle_contact_list",
            "Acquired concurrency guard for account {}",
            account.pubkey.to_hex()
        );

        let contacts_from_event = NostrManager::pubkeys_from_event(&event);
        let contacts_set: std::collections::HashSet<nostr_sdk::PublicKey> =
            contacts_from_event.iter().cloned().collect();

        // Get current follows from database
        let current_follows = account.follows(&self.database).await?;
        let current_follows_set: std::collections::HashSet<nostr_sdk::PublicKey> =
            current_follows.iter().map(|f| f.pubkey).collect();

        // Find users to follow (in event but not in current follows)
        let users_to_follow: Vec<nostr_sdk::PublicKey> = contacts_set
            .difference(&current_follows_set)
            .cloned()
            .collect();

        // Find users to unfollow (in current follows but not in event)
        let users_to_unfollow: Vec<nostr_sdk::PublicKey> = current_follows_set
            .difference(&contacts_set)
            .cloned()
            .collect();

        tracing::debug!(
            target: "whitenoise::handle_contact_list_internal",
            "Processing contact list for account {}: {} to follow, {} to unfollow",
            account.pubkey.to_hex(),
            users_to_follow.len(),
            users_to_unfollow.len()
        );

        // Check if we have changes to make before processing
        let has_changes = !users_to_follow.is_empty() || !users_to_unfollow.is_empty();

        if !has_changes {
            tracing::debug!(
                target: "whitenoise::handle_contact_list_internal",
                "No changes to make to contact list for account {}",
                account.pubkey.to_hex()
            );
            return Ok(());
        }

        // Process new follows (but don't publish follow list after each individual follow)
        for pubkey in &users_to_follow {
            let (user, newly_created) =
                crate::whitenoise::users::User::find_or_create_by_pubkey(pubkey, &self.database)
                    .await?;

            if newly_created {
                self.background_fetch_user_data(&user).await?;
            }

            account.follow_user(&user, &self.database).await?;
        }

        // Process unfollows (but don't publish follow list after each individual unfollow)
        for pubkey in &users_to_unfollow {
            let (user, _) =
                crate::whitenoise::users::User::find_or_create_by_pubkey(pubkey, &self.database)
                    .await?;
            account.unfollow_user(&user, &self.database).await?;
        }

        // Only publish the follow list once after all changes are made
        self.background_publish_account_follow_list(account).await?;

        tracing::debug!(
            target: "whitenoise::handle_contact_list",
            "Releasing concurrency guard for account {}",
            account.pubkey.to_hex()
        );

        // The _permit will be automatically dropped here, releasing the semaphore
        Ok(())
    }
}
