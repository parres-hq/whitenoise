use nostr_mls::prelude::*;
use std::time::Duration;
use tokio::time::timeout;

use crate::whitenoise::Whitenoise;

/// Fetches welcomes from the database for the active user
#[tauri::command]
pub async fn get_welcomes(
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Vec<welcome_types::Welcome>, String> {
    tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "Fetching welcomes");
    tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::welcomes::get_welcomes", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "Fetching welcomes");
        let pending_welcomes = nostr_mls
            .get_pending_welcomes()
            .map_err(|e| e.to_string())?;
        tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "Pending welcomes: {:?}", pending_welcomes);
        tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "nostr_mls lock released");
        Ok(pending_welcomes)
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
