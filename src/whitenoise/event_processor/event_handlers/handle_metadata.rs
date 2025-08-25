use nostr_sdk::prelude::*;

use crate::whitenoise::{
    error::{Result, WhitenoiseError},
    users::User,
    Whitenoise,
};

impl Whitenoise {
    pub async fn handle_metadata(&self, event: Event) -> Result<()> {
        let (mut user, newly_created) =
            User::find_or_create_by_pubkey(&event.pubkey, &self.database).await?;
        match Metadata::from_json(&event.content) {
            Ok(metadata) => {
                // Only update metadata if this event is newer than our current data
                // For newly created users, always accept the metadata
                let event_timestamp = event.created_at.as_u64() as i64;
                if newly_created || event_timestamp > user.updated_at.timestamp() {
                    user.metadata = metadata;
                    let _ = user.save(&self.database).await?;
                    tracing::debug!(
                        target: "whitenoise::event_processor::handle_metadata",
                        "Updated metadata for user {} with event timestamp {}",
                        event.pubkey,
                        event_timestamp
                    );
                } else {
                    tracing::debug!(
                        target: "whitenoise::event_processor::handle_metadata",
                        "Ignoring stale metadata event for user {} (event: {}, current: {})",
                        event.pubkey,
                        event_timestamp,
                        user.updated_at.timestamp()
                    );
                }
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
