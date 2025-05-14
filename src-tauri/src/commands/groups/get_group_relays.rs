use std::collections::BTreeSet;
use std::time::Duration;
use tokio::time::timeout;

use nostr_mls::prelude::*;



/// Gets the list of admin members in an MLS group
pub async fn get_group_relays(group_id: &str) -> Result<BTreeSet<RelayUrl>, String> {
    let mls_group_id = GroupId::from_slice(
        &hex::decode(group_id).map_err(|e| format!("Error decoding group id: {}", e))?,
    );

    tracing::debug!(target: "whitenoise::commands::groups::get_group_relays", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::get_group_relays", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::groups::get_group_relays", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        tracing::debug!(target: "whitenoise::commands::groups::get_group_relays", "nostr_mls lock released");
        nostr_mls
            .get_relays(&mls_group_id)
            .map_err(|e| format!("Error fetching group relays: {}", e))
    } else {
        Err("Nostr MLS not initialized".to_string())
    }
}
