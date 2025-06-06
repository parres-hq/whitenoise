use crate::accounts::Account;
use nostr_sdk::prelude::*;

/// Publishes a metadata event to the Nostr network and updates the local account metadata.
///
/// This function performs two main operations:
/// 1. Updates the local account's metadata and saves it
/// 2. Publishes the metadata event to all connected Nostr relays
///
/// # Arguments
/// * `new_metadata` - The new metadata to publish and save
/// * `wn` - The Whitenoise application state containing the Nostr client
///
/// # Returns
/// * `Result<(), String>` - Returns Ok(()) on success, or an error message on failure
pub async fn publish_metadata_event(
    new_metadata: Metadata,
) -> Result<(), String> {
    let mut account = Account::get_active()
        .await
        .map_err(|e| e.to_string())?;

    account.metadata = new_metadata.clone();
    account.save().await.map_err(|e| e.to_string())?;
    tracing::debug!("Saved updated metadata");

    let metadata_json = serde_json::to_string(&new_metadata).map_err(|e| e.to_string())?;
    let event = EventBuilder::new(Kind::Metadata, metadata_json);

    wn.nostr
        .client
        .send_event_builder(event.clone())
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!("Published metadata event to relays: {:?}", event);

    Ok(())
}
