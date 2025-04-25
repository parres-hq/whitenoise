use std::collections::BTreeSet;

use nostr_mls::prelude::*;

use crate::whitenoise::Whitenoise;

/// Gets the list of admin members in an MLS group
#[tauri::command]
pub async fn get_group_admins(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<BTreeSet<PublicKey>, String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    let nostr_mls_guard = wn.nostr_mls.lock().await;

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let group = nostr_mls
            .get_group(&mls_group_id)
            .map_err(|e| format!("Error fetching group: {}", e))?;
        match group {
            Some(group) => Ok(group.admin_pubkeys),
            None => Err("Group not found".to_string()),
        }
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
