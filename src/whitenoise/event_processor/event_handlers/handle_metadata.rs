use nostr_sdk::prelude::*;

use crate::whitenoise::{
    database::processed_events::ProcessedEvent,
    error::{Result, WhitenoiseError},
    users::User,
    utils::timestamp_to_datetime,
    Whitenoise,
};

impl Whitenoise {
    pub async fn handle_metadata(&self, event: Event) -> Result<()> {
        let (mut user, newly_created) =
            User::find_or_create_by_pubkey(&event.pubkey, &self.database).await?;
        match Metadata::from_json(&event.content) {
            Ok(metadata) => {
                let event_timestamp = timestamp_to_datetime(event.created_at)?;
                let should_update = user
                    .should_update_metadata(
                        &event.id,
                        &event_timestamp,
                        newly_created,
                        &self.database,
                    )
                    .await?;

                if should_update {
                    user.metadata = metadata;
                    user.save(&self.database).await?;

                    ProcessedEvent::create(
                        &event.id,
                        None,
                        Some(event_timestamp),
                        Some(0),
                        Some(&event.pubkey),
                        &self.database,
                    )
                    .await?;

                    tracing::debug!(
                        target: "whitenoise::event_processor::handle_metadata",
                        "Updated metadata for user {} with event timestamp {}",
                        event.pubkey,
                        event_timestamp.timestamp_millis()
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
