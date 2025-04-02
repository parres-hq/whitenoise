use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

/// Fetches the current status of all connected Nostr relays.
///
/// Returns a HashMap where:
/// - Keys are the relay URLs as strings
/// - Values are the current status of each relay as strings
///
/// # Arguments
/// * `wn` - A reference to the Whitenoise application state containing the Nostr client
///
/// # Returns
/// * `Result<HashMap<String, String>, String>` - A map of relay URLs to their statuses, or an error string if something goes wrong
#[tauri::command]
pub async fn fetch_relays(
    wn: tauri::State<'_, Whitenoise>,
) -> Result<HashMap<String, String>, String> {
    Ok(wn
        .nostr
        .client
        .relays()
        .await
        .into_iter()
        .map(|(url, relay)| (url.to_string(), relay.status().to_string()))
        .collect::<HashMap<String, String>>())
}
