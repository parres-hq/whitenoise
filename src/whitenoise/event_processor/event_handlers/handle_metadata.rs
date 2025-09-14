use chrono::DateTime;
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
                // Note: Nostr event timestamps are in seconds, but we store in milliseconds for consistency
                let event_timestamp = (event.created_at.as_u64() * 1000) as i64;
                let should_update = newly_created
                    || match user.event_created_at {
                        None => {
                            // No stored event timestamp (legacy data), accept the new event
                            tracing::debug!(
                                target: "whitenoise::event_processor::handle_metadata",
                                "No stored event timestamp for user {}, accepting new event",
                                event.pubkey
                            );
                            true
                        }
                        Some(stored_timestamp) => {
                            // Compare with the actual stored event timestamp (both in milliseconds)
                            event_timestamp > stored_timestamp.timestamp_millis()
                        }
                    };

                if should_update {
                    user.metadata = metadata;
                    // Update the event timestamp to the new event's timestamp
                    user.event_created_at = Some(
                        DateTime::from_timestamp_millis(event_timestamp)
                            .unwrap_or_else(chrono::Utc::now),
                    );
                    let _ = user.save(&self.database).await?;
                    tracing::debug!(
                        target: "whitenoise::event_processor::handle_metadata",
                        "Updated metadata for user {} with event timestamp {}",
                        event.pubkey,
                        event_timestamp
                    );
                } else {
                    let stored_timestamp = user
                        .event_created_at
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or(0);
                    tracing::debug!(
                        target: "whitenoise::event_processor::handle_metadata",
                        "Ignoring stale metadata event for user {} (event: {}, stored: {})",
                        event.pubkey,
                        event_timestamp,
                        stored_timestamp
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
