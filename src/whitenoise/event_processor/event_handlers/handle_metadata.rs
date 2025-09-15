use chrono::DateTime;
use nostr_sdk::prelude::*;

use crate::whitenoise::{
    database::processed_events::ProcessedEvent,
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
                // Note: Nostr event timestamps are in seconds, but we store in milliseconds for consistency
                let event_timestamp =
                    DateTime::from_timestamp_millis((event.created_at.as_u64() * 1000) as i64)
                        .ok_or_else(|| {
                            WhitenoiseError::EventProcessor(format!(
                                "Invalid timestamp in metadata event: {}",
                                event.created_at.as_u64()
                            ))
                        })?;

                let should_update = newly_created
                    || user.metadata == Metadata::default()
                    || event_timestamp.timestamp_millis() > user.updated_at.timestamp_millis();

                if !should_update {
                    tracing::debug!(
                        target: "whitenoise::event_processor::handle_metadata",
                        "Ignoring stale metadata event for user {} (event: {}, user_updated: {})",
                        event.pubkey,
                        event_timestamp.timestamp_millis(),
                        user.updated_at.timestamp_millis()
                    );
                }

                if should_update {
                    user.metadata = metadata;
                    // Save the updated metadata (no longer storing event timestamp in users table)
                    let _ = user.save(&self.database).await?;

                    // Create ProcessedEvent entry to track this metadata event
                    ProcessedEvent::create(
                        &event.id,
                        None, // Global events (user metadata)
                        Some(event_timestamp),
                        Some(0),             // Metadata events are kind 0
                        Some(&event.pubkey), // Track the author
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
