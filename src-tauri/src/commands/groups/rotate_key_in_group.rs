use nostr_mls::prelude::*;
use std::time::Duration;
use tokio::time::timeout;



// TODO: THIS ISN'T CORRECT

pub async fn rotate_key_in_group(
    group_id: &str,
) -> Result<(), String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    tracing::debug!(target: "whitenoise::commands::groups::rotate_key_in_group", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::rotate_key_in_group", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::groups::rotate_key_in_group", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        nostr_mls
            .self_update(&mls_group_id)
            .map_err(|e| format!("Error rotating key in group: {}", e))?;
    } else {
        return Err("Nostr MLS not initialized".to_string());
    }

    tracing::debug!(target: "whitenoise::commands::groups::rotate_key_in_group", "nostr_mls lock released");
    Ok(())
}
