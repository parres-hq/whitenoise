use crate::nostr_manager::NostrManager;
use crate::whitenoise::error::Result;
use crate::whitenoise::users::User;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

impl Whitenoise {
    pub async fn handle_relay_list(&self, event: Event) -> Result<()> {
        let (user, _newly_created) =
            User::find_or_create_by_pubkey(&event.pubkey, &self.database).await?;
        let relay_urls = NostrManager::relay_urls_from_event(event.clone());
        for url in relay_urls {
            let relay = self.find_or_create_relay(&url).await?;
            user.add_relay(&relay, event.kind.into(), &self.database)
                .await?;
        }

        Ok(())
    }
}
