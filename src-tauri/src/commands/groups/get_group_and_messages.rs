use nostr_mls::prelude::*;
use serde::Serialize;

use crate::whitenoise::Whitenoise;

#[derive(Debug, Clone, Serialize)]
pub struct GroupAndMessages {
    group: group_types::Group,
    messages: Vec<message_types::Message>,
}

/// Gets a single MLS group and its messages by group ID
///
/// # Arguments
/// * `group_id` - Hex encoded MLS group ID
/// * `wn` - Whitenoise state
///
/// # Returns
/// * `Ok(GroupAndMessages)` - Struct containing:
///   - The requested group if found
///   - Vector of messages for the group
/// * `Err(String)` - Error message if operation fails
///
/// # Errors
/// Returns error if:
/// - Group ID is not valid hex
/// - Group not found in database
/// - Error fetching messages
#[tauri::command]
pub async fn get_group_and_messages(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<GroupAndMessages, String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    tracing::debug!(
        target: "whitenoise::commands::groups::get_group_and_messages",
        "Getting group and messages for group ID: {:?}",
        mls_group_id
    );

    let nostr_mls_guard = wn.nostr_mls.lock().await;

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let group = nostr_mls
            .get_group(&mls_group_id)
            .map_err(|e| format!("Error fetching group: {}", e))?;

        if let Some(group) = group {
            tracing::debug!(
                target: "whitenoise::commands::groups::get_group_and_messages",
                "Group: {:?}",
                group
            );

            let messages = nostr_mls
                .get_messages(&mls_group_id)
                .map_err(|e| format!("Error fetching messages: {}", e))?;

            tracing::debug!(
                target: "whitenoise::commands::groups::get_group_and_messages",
                "Messages: {:?}",
                messages
            );

            Ok(GroupAndMessages { group, messages })
        } else {
            return Err("Group not found".to_string());
        }
    } else {
        return Err("Nostr MLS not initialized".to_string());
    }
}
