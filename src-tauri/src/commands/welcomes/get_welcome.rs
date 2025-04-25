use nostr_mls::prelude::*;

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
    let nostr_mls_guard = wn.nostr_mls.lock().await;
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let welcome = nostr_mls
            .get_welcome(&event_id)
            .map_err(|e| e.to_string())?;

        if let Some(welcome) = welcome {
            Ok(welcome)
        } else {
            Err("Welcome not found".to_string())
        }
    } else {
        return Err("Nostr MLS not initialized".to_string());
    }
}
