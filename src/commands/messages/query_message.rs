
use nostr_mls::prelude::*;
use std::time::Duration;
use tokio::time::timeout;


pub async fn query_message(message_id: &str) -> Result<Option<message_types::Message>, String> {
    let event_id = EventId::parse(message_id).map_err(|e| e.to_string())?;

    tracing::debug!(target: "whitenoise::commands::messages::query_message", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::messages::query_message", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::messages::query_message", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let message = nostr_mls
            .get_message(&event_id)
            .map_err(|e| format!("Error fetching message: {}", e))?;
        tracing::debug!(target: "whitenoise::commands::messages::query_message", "nostr_mls lock released");
        Ok(message)
    } else {
        tracing::debug!(target: "whitenoise::commands::messages::query_message", "nostr_mls lock released");
        Err("NostrMls instance not initialized".to_string())
    }
}
