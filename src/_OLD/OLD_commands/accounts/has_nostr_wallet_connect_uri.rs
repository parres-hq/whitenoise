use crate::accounts::Account;
use nostr_sdk::prelude::*;

/// Checks if a Nostr Wallet Connect URI is configured for the active account.
///
/// # Arguments
///
/// * `wn` - A reference to the Whitenoise state
///
/// # Returns
///
/// * `Ok(bool)` - true if a NWC URI is configured, false otherwise
/// * `Err(String)` - An error message if there was an issue checking the NWC URI
pub async fn has_nostr_wallet_connect_uri() -> Result<bool, String> {
    let active_account = Account::get_active()
        .await
        .map_err(|e| format!("Error getting active account: {}", e))?;

    active_account
        .get_nostr_wallet_connect_uri()
        .map(|opt| opt.is_some())
        .map_err(|e| format!("Error checking NWC URI: {}", e))
}
