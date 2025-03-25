use crate::groups::Group;
use crate::messages::Message;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GroupAndMessages {
    group: Group,
    messages: Vec<Message>,
}

/// Gets a single MLS group and its messages by group ID
///
/// # Arguments
/// * `group_id` - Hex encoded MLS group ID
/// * `wn` - Whitenoise state
///
/// # Returns
/// * `Ok((Group, Vec<UnsignedEvent>))` - Tuple containing:
///   - The requested group if found
///   - Vector of unsigned message events for the group
/// * `Err(String)` - Error message if operation fails
///
/// # Errors
/// Returns error if:
/// - No active account found
/// - Group ID is not valid hex
/// - Group not found in database
/// - Error fetching messages
#[tauri::command]
pub async fn get_group_and_messages(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<GroupAndMessages, String> {
    let mls_group_id =
        hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?;
    tracing::debug!(
        target: "whitenoise::commands::groups::get_group_and_messages",
        "Getting group and messages for group ID: {:?}",
        mls_group_id
    );
    let group = Group::find_by_mls_group_id(&mls_group_id, wn.clone())
        .await
        .map_err(|e| format!("Error fetching group: {}", e))?;
    tracing::debug!(
        target: "whitenoise::commands::groups::get_group_and_messages",
        "Group: {:?}",
        group
    );
    let messages = group
        .messages(wn.clone())
        .await
        .map_err(|e| format!("Error fetching messages: {}", e))?;

    tracing::debug!(
        target: "whitenoise::commands::groups::get_group_and_messages",
        "Messages: {:?}",
        messages
    );
    Ok(GroupAndMessages { group, messages })
}
