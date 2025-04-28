use nostr_mls::prelude::*;
use std::time::Duration;
use tokio::time::timeout;

use crate::whitenoise::Whitenoise;

/// Gets a specific invite by its ID.
///
/// # Arguments
/// * `invite_id` - The ID of the invite to retrieve
/// * `wn` - The Whitenoise state
///
/// # Returns
/// * `Ok(Invite)` if the invite was found
/// * `Err(String)` if there was an error retrieving the invite or it wasn't found
#[tauri::command]
pub async fn get_welcome(
    event_id: String,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<welcome_types::Welcome, String> {
    let event_id = EventId::parse(&event_id).map_err(|e| e.to_string())?;
    tracing::debug!(target: "whitenoise::commands::welcomes::get_welcome", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::welcomes::get_welcome", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::welcomes::get_welcome", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let welcome = nostr_mls
            .get_welcome(&event_id)
            .map_err(|e| e.to_string())?;

        if let Some(welcome) = welcome {
            tracing::debug!(target: "whitenoise::commands::welcomes::get_welcome", "nostr_mls lock released");
            Ok(welcome)
        } else {
            tracing::debug!(target: "whitenoise::commands::welcomes::get_welcome", "Welcome not found");
            Err("Welcome not found".to_string())
        }
    } else {
        tracing::error!(target: "whitenoise::commands::welcomes::get_welcome", "Nostr MLS not initialized");
        Err("Nostr MLS not initialized".to_string())
    }
}
