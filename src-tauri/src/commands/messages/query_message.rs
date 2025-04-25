use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::*;

#[tauri::command]
pub async fn query_message(
    message_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Option<message_types::Message>, String> {
    let event_id = EventId::parse(message_id).map_err(|e| e.to_string())?;

    let nostr_mls_guard = wn.nostr_mls.lock().await;

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let message = nostr_mls
            .get_message(&event_id)
            .map_err(|e| format!("Error fetching message: {}", e))?;
        Ok(message)
    } else {
        Err("NostrMls instance not initialized".to_string())
    }
}
