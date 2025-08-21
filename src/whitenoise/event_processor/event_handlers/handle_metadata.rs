use nostr_sdk::prelude::*;

use crate::whitenoise::{
    error::{Result, WhitenoiseError},
    users::User,
    Whitenoise,
};

impl Whitenoise {
    pub async fn handle_metadata(&self, event: Event) -> Result<()> {
        let (mut user, _newly_created) =
            User::find_or_create_by_pubkey(&event.pubkey, &self.database).await?;
        match Metadata::from_json(&event.content) {
            Ok(metadata) => {
                user.metadata = metadata;
                let _ = user.save(&self.database).await?;
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::nostr_manager::fetch_all_user_data",
                    "Failed to parse metadata for user {}: {}",
                    event.pubkey,
                    e
                );
                Err(WhitenoiseError::EventProcessor(e.to_string()))
            }
        }
    }
}
