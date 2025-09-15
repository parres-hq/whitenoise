use chrono::DateTime;
use nostr_sdk::prelude::*;

use crate::{
    nostr_manager::NostrManager,
    whitenoise::{accounts::Account, error::Result, users::User, Whitenoise},
};

impl Whitenoise {
    pub async fn handle_relay_list(&self, event: Event) -> Result<()> {
        let (user, _newly_created) =
            User::find_or_create_by_pubkey(&event.pubkey, &self.database).await?;

        let relay_type = event.kind.into();
        let relay_urls = NostrManager::relay_urls_from_event(event.clone());
        let event_created_at = Some(
            DateTime::from_timestamp_millis((event.created_at.as_u64() * 1000) as i64)
                .unwrap_or(DateTime::UNIX_EPOCH),
        );
        let relays_changed = user
            .sync_relay_urls(self, relay_type, &relay_urls, event_created_at)
            .await?;

        if relays_changed {
            self.handle_subscriptions_refresh(&user, &event).await;
        }

        Ok(())
    }

    async fn handle_subscriptions_refresh(&self, user: &User, event: &Event) {
        // Refresh global subscriptions for this user (metadata, relay lists, key packages)
        if let Err(e) = self.refresh_global_subscription_for_user(user).await {
            tracing::warn!(
                target: "whitenoise::handle_relay_list",
                "Failed to refresh global subscriptions after relay list change for {}: {}",
                event.pubkey, e
            );
        }

        // If there's an account for this user, also refresh their account subscriptions
        if let Ok(account) = Account::find_by_pubkey(&user.pubkey, &self.database).await {
            if let Err(e) = self.refresh_account_subscriptions(&account).await {
                tracing::warn!(
                    target: "whitenoise::handle_relay_list",
                    "Failed to refresh account subscriptions after relay list change for {}: {}",
                    event.pubkey, e
                );
            }
        }
    }
}
