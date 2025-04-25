use nostr_mls::prelude::*;

use crate::whitenoise::Whitenoise;

// TODO: THIS ISN'T CORRECT
#[tauri::command]
pub async fn rotate_key_in_group(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<(), String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    let nostr_mls_guard = wn.nostr_mls.lock().await;
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        nostr_mls
            .self_update(&mls_group_id)
            .map_err(|e| format!("Error rotating key in group: {}", e))?;
    } else {
        return Err("Nostr MLS not initialized".to_string());
    }

    Ok(())
}
