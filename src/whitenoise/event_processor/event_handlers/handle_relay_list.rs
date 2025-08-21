use nostr_sdk::prelude::*;

use crate::{
    nostr_manager::NostrManager,
    whitenoise::{error::Result, users::User, Whitenoise},
};

impl Whitenoise {
    pub async fn handle_relay_list(&self, event: Event) -> Result<()> {
        let (user, _newly_created) =
            User::find_or_create_by_pubkey(&event.pubkey, &self.database).await?;
        let relay_urls = NostrManager::relay_urls_from_event(event.clone());
        for url in relay_urls {
            let relay = self.find_or_create_relay_by_url(&url).await?;
            user.add_relay(&relay, event.kind.into(), &self.database)
                .await?;
        }

        Ok(())
    }
}
