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
    let group_ids: Vec<String>;
    {
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

                group_ids = nostr_mls
                    .get_groups()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|g| hex::encode(g.nostr_group_id))
                    .collect::<Vec<_>>();
            } else {
                return Err("Welcome not found".to_string());
            }
        } else {
            return Err("Nostr MLS not initialized".to_string());
        }
    }
    tracing::debug!(target: "whitenoise::commands::welcomes::accept_welcome", "nostr_mls lock released");

    tracing::debug!(target: "whitenoise::commands::welcomes::accept_welcome", "Fetching group messages");
    let _ = wn
        .nostr
        .fetch_group_messages(Timestamp::zero(), group_ids.clone())
        .await
        .map_err(|e| format!("Failed to fetch group messages: {}", e))?;

    tracing::debug!(target: "whitenoise::commands::welcomes::accept_welcome", "Updating MLS group subscription");
    wn.nostr
        .subscribe_mls_group_messages(group_ids)
        .await
        .map_err(|e| format!("Failed to update MLS group subscription: {}", e))?;
    app_handle
        .emit("welcome_accepted", welcome_event_id)
        .map_err(|e| e.to_string())?;

    Ok(())
}
