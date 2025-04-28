use nostr_mls::prelude::*;
use std::time::Duration;
use tauri::Emitter;
use tokio::time::timeout;

use crate::whitenoise::Whitenoise;

/// Accepts a group welcome.
///
/// # Arguments
/// * `welcome_event_id` - The event ID of the welcome to accept
/// * `wn` - The Whitenoise state
/// * `app_handle` - The Tauri app handle
///
/// # Returns
/// * `Ok(())` if the welcome was successfully accepted
/// * `Err(String)` if there was an error accepting the welcome
///
/// # Events Emitted
/// * `welcome_accepted` - Emitted with the updated welcome after it is accepted
#[tauri::command]
pub async fn accept_welcome(
    welcome_event_id: String,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|e| e.to_string())?;

    tracing::debug!(target: "whitenoise::commands::welcomes::accept_welcome", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::welcomes::accept_welcome", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::welcomes::accept_welcome", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let welcome = nostr_mls
            .get_welcome(&welcome_event_id)
            .map_err(|e| e.to_string())?;
        if let Some(welcome) = welcome {
            tracing::debug!(target: "whitenoise::welcomes::accept_welcome", "Accepting welcome {:?}", welcome_event_id);
            nostr_mls
                .accept_welcome(&welcome)
                .map_err(|e| e.to_string())?;
        } else {
            return Err("Welcome not found".to_string());
        }
    } else {
        return Err("Nostr MLS not initialized".to_string());
    }
    tracing::debug!(target: "whitenoise::commands::welcomes::accept_welcome", "nostr_mls lock released");
    app_handle
        .emit("welcome_accepted", welcome_event_id)
        .map_err(|e| e.to_string())?;

    Ok(())
}
