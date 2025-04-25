use nostr_mls::prelude::*;

use crate::whitenoise::Whitenoise;

/// Fetches welcomes from the database for the active user
#[tauri::command]
pub async fn get_welcomes(
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Vec<welcome_types::Welcome>, String> {
    let nostr_mls_guard = wn.nostr_mls.lock().await;
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let pending_welcomes = nostr_mls
            .get_pending_welcomes()
            .map_err(|e| e.to_string())?;
        tracing::debug!(target: "whitenoise::commands::welcomes::get_welcomes", "Pending welcomes: {:?}", pending_welcomes);
        Ok(pending_welcomes)
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
