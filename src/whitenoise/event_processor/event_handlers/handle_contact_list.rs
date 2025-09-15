use chrono::DateTime;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::{
    nostr_manager::pubkeys_from_event,
    whitenoise::{
        accounts::Account,
        database::processed_events::ProcessedEvent,
        error::{Result, WhitenoiseError},
        users::User,
        Whitenoise,
    },
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

        // Check if this event is newer than what we already have using unified ProcessedEvent approach
        let account_id = account.id.ok_or_else(|| WhitenoiseError::AccountNotFound)?;

        let event_timestamp =
            DateTime::from_timestamp_millis((event.created_at.as_u64() * 1000) as i64)
                .unwrap_or(DateTime::UNIX_EPOCH);

        let current_event_time =
            ProcessedEvent::newest_contact_list_timestamp(account_id, &self.database).await?;

        if let Some(current_time) = current_event_time {
            if event_timestamp.timestamp_millis() <= current_time.timestamp_millis() {
                tracing::debug!(
                    target: "whitenoise::handle_contact_list",
                    "Ignoring older contact list event (event: {}, current: {}) for account {}",
                    event_timestamp.timestamp_millis(),
                    current_time.timestamp_millis(),
                    account.pubkey.to_hex()
                );
                return Ok(());
            }
        }

        tracing::debug!(
            target: "whitenoise::handle_contact_list",
            "Processing contact list event (timestamp: {}) for account {}",
            event_timestamp.timestamp_millis(),
            account.pubkey.to_hex()
        );

        let contacts_from_event = pubkeys_from_event(&event);

        // Use the new bulk update method and get the list of newly created users
        let newly_created_pubkeys = account
            .update_follows_from_event(
                contacts_from_event.clone(),
                event_timestamp.timestamp_millis() as u64,
                &self.database,
            )
            .await?;

        // Store count for logging before consuming the vector
        let newly_created_count = newly_created_pubkeys.len();

        // Background fetch user data for newly created users
        for pubkey in newly_created_pubkeys {
            if let Ok((user, _)) = User::find_or_create_by_pubkey(&pubkey, &self.database).await {
                self.background_fetch_user_data(&user).await?;
            }
        }

        // Create ProcessedEvent entry to track this contact list event
        ProcessedEvent::create(
            &event.id,
            Some(account_id), // Account-specific event
            Some(event_timestamp),
            Some(3), // Contact list events are kind 3
            &self.database,
        )
        .await?;

        tracing::debug!(
            target: "whitenoise::handle_contact_list",
            "Successfully processed contact list with {} contacts ({} newly created) for account {}",
            contacts_from_event.len(),
            newly_created_count,
            account.pubkey.to_hex()
        );

        // The _permit will be automatically dropped here, releasing the semaphore
        Ok(())
    }
}
