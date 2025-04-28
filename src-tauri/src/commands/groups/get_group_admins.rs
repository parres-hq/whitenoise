use std::collections::BTreeSet;
use std::time::Duration;
use tokio::time::timeout;

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

    tracing::debug!(target: "whitenoise::commands::groups::get_group_admins", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::get_group_admins", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::groups::get_group_admins", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let group = nostr_mls
            .get_group(&mls_group_id)
            .map_err(|e| format!("Error fetching group: {}", e))?;
        match group {
            Some(group) => {
                tracing::debug!(target: "whitenoise::commands::groups::get_group_admins", "nostr_mls lock released");
                Ok(group.admin_pubkeys)
            }
            None => {
                tracing::debug!(target: "whitenoise::commands::groups::get_group_admins", "nostr_mls lock released");
                Err("Group not found".to_string())
            }
        }
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
