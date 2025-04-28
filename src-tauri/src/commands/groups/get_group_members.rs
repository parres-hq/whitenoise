use std::collections::BTreeSet;
use std::time::Duration;
use tokio::time::timeout;

use nostr_mls::prelude::*;

use crate::whitenoise::Whitenoise;

/// Gets the list of members in an MLS group
///
/// # Arguments
/// * `group_id` - Hex-encoded MLS group ID
/// * `wn` - Whitenoise state handle
///
/// # Returns
/// * `Ok(Vec<String>)` - List of member public keys if successful
/// * `Err(String)` - Error message if operation fails
///
/// # Errors
/// * If no active account is found
/// * If group ID cannot be decoded from hex
/// * If group cannot be found
/// * If members cannot be retrieved
#[tauri::command]
pub async fn get_group_members(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<BTreeSet<PublicKey>, String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    tracing::debug!(target: "whitenoise::commands::groups::get_group_members", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::get_group_members", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::groups::get_group_members", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let members = nostr_mls
            .get_members(&mls_group_id)
            .map_err(|e| e.to_string())?;
        tracing::debug!(target: "whitenoise::commands::groups::get_group_members", "nostr_mls lock released");
        Ok(members)
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
