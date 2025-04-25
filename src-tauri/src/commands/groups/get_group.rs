use std::collections::BTreeSet;

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

    let nostr_mls_guard = wn.nostr_mls.lock().await;

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
            Ok(GroupWithRelays { group, relays })
        } else {
            Err("Group not found".to_string())
        }
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
