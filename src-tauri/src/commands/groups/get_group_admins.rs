use crate::groups::Group;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

/// Gets the list of admin members in an MLS group
///
/// # Arguments
/// * `group_id` - Hex-encoded MLS group ID
/// * `wn` - Whitenoise state handle
///
/// # Returns
/// * `Ok(Vec<String>)` - List of admin public keys if successful
/// * `Err(String)` - Error message if operation fails
///
/// # Errors
/// * If no active account is found
/// * If group ID cannot be decoded from hex
/// * If group cannot be found
/// * If admin list cannot be retrieved
#[tauri::command]
pub async fn get_group_admins(
    group_id: &str,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Vec<PublicKey>, String> {
    let mls_group_id =
        hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?;
    let group = Group::find_by_mls_group_id(&mls_group_id, wn.clone())
        .await
        .map_err(|e| format!("Error fetching group: {}", e))?;
    let admins = group.admins().map_err(|e| e.to_string())?;
    Ok(admins)
}
