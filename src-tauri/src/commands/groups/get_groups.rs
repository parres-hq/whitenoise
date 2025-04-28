use nostr_mls::prelude::*;
use std::time::Duration;
use tokio::time::timeout;

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
    tracing::debug!(target: "whitenoise::commands::groups::get_groups", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::get_groups", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(
                target: "whitenoise::commands::groups::get_groups",
                "Timeout waiting for nostr_mls lock"
            );
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        tracing::debug!(target: "whitenoise::commands::groups::get_groups", "Fetching groups");
        let groups = nostr_mls
            .get_groups()
            .map_err(|e| format!("Error fetching groups for account: {}", e))?;
        tracing::debug!(target: "whitenoise::commands::groups::get_groups", "nostr_mls lock released");
        Ok(groups)
    } else {
        tracing::debug!(target: "whitenoise::commands::groups::get_groups", "nostr_mls lock released");
        Err("Nostr MLS not initialized".to_string())
    }
}
