use nostr_mls::prelude::*;

use crate::whitenoise::Whitenoise;

/// Gets all MLS groups that the active account is a member of
/// This is scoped so that we can return only the groups that the user is a member of.
///
/// # Arguments
/// * `wn` - Whitenoise state containing account and group managers
///
/// # Returns
/// * `Ok(Vec<Group>)` - List of groups the active account belongs to
/// * `Err(String)` - Error message if retrieval fails
///
/// # Errors
/// Returns error if:
/// - No active account found
/// - Database error occurs retrieving groups
#[tauri::command]
pub async fn get_groups(
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Vec<group_types::Group>, String> {
    let nostr_mls_guard = wn.nostr_mls.lock().await;
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        nostr_mls
            .get_groups()
            .map_err(|e| format!("Error fetching groups for account: {}", e))
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
