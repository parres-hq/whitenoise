use crate::accounts::Account;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

/// Fetches a list of relays associated with a specific user and kind.
///
/// This function retrieves either inbox relays (kind 10050) or key package relays (kind 10051)
/// for a specified public key. If no public key is provided, it uses the currently active account.
///
/// # Arguments
///
/// * `kind` - The type of relays to fetch:
///   - `10050`: Inbox relays
///   - `10051`: Key package relays
/// * `pubkey` - Optional public key of the user. If None, uses the active account's public key.
/// * `wn` - The Whitenoise application state containing the Nostr client.
///
/// # Returns
///
/// Returns a `Result` containing:
/// - `Ok(Vec<String>)`: A vector of relay URLs
/// - `Err(String)`: An error message if:
///   - The provided public key is invalid
///   - Failed to get the active account
///   - Failed to fetch relays from the network
///   - Invalid relay list kind was provided
#[tauri::command]
pub async fn fetch_relays_list(
    kind: u64,
    pubkey: Option<String>,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Vec<String>, String> {
    // Get the target pubkey
    let target_pubkey = if let Some(key) = pubkey {
        match PublicKey::parse(&key) {
            Ok(pk) => pk,
            Err(e) => return Err(format!("Invalid public key: {}", e)),
        }
    } else {
        // Use active account if no pubkey provided
        match Account::get_active_pubkey(wn.clone()).await {
            Ok(pk) => pk,
            Err(e) => return Err(format!("Failed to get active account: {}", e)),
        }
    };

    let relay_urls = match kind {
        10050 => wn
            .nostr
            .fetch_user_inbox_relays(target_pubkey)
            .await
            .map_err(|e| e.to_string())?,
        10051 => wn
            .nostr
            .fetch_user_key_package_relays(target_pubkey)
            .await
            .map_err(|e| e.to_string())?,
        _ => {
            return Err(
                "Invalid relay list kind. Must be 10050 (inbox) or 10051 (key package)".to_string(),
            )
        }
    };

    Ok(relay_urls)
}
