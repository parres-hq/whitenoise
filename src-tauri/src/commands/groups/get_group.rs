use std::collections::BTreeSet;
use std::time::Duration;
use tokio::time::timeout;

use nostr_mls::prelude::*;
use serde::Serialize;

use crate::whitenoise::Whitenoise;

#[derive(Debug, Clone, Serialize)]
pub struct GroupWithRelays {
    group: group_types::Group,
    relays: BTreeSet<RelayUrl>,
}

/// Gets a single MLS group by its group ID
///
/// # Arguments
/// * `group_id` - Hex encoded MLS group ID
/// * `wn` - Whitenoise state
///
/// # Returns
/// * `Ok(Group)` - The requested group if found
/// * `Err(String)` - Error message if group not found or other error occurs
///
/// # Errors
/// Returns error if:
/// - Group ID is not valid hex
/// - Group not found in database
/// - Database error occurs
#[tauri::command]
pub async fn get_group(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<GroupWithRelays, String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    tracing::debug!(target: "whitenoise::commands::groups::get_group", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::get_group", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::groups::get_group", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let group = nostr_mls
            .get_group(&mls_group_id)
            .map_err(|e| format!("Error fetching group: {}", e))?;

        if let Some(group) = group {
            let relays = match nostr_mls.get_relays(&mls_group_id) {
                Ok(relays) => relays,
                Err(e) => {
                    return Err(format!("Error fetching group relays: {}", e));
                }
            };
            tracing::debug!(target: "whitenoise::commands::groups::get_group", "nostr_mls lock released");
            Ok(GroupWithRelays { group, relays })
        } else {
            tracing::debug!(target: "whitenoise::commands::groups::get_group", "Group not found");
            Err("Group not found".to_string())
        }
    } else {
        tracing::error!(target: "whitenoise::commands::groups::get_group", "Nostr MLS not initialized");
        Err("Nostr MLS not initialized".to_string())
    }
}
