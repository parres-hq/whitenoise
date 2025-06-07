use crate::accounts::Account;

use nostr_sdk::prelude::*;

/// Removes the Nostr Wallet Connect URI for the active account.
///
/// # Arguments
///
/// * `wn` - A reference to the Whitenoise state
///
/// # Returns
///
/// * `Ok(())` - If the URI was removed successfully
/// * `Err(String)` - An error message if there was an issue removing the URI
pub async fn remove_nostr_wallet_connect_uri(
) -> Result<(), String> {
    let active_account = Account::get_active()
        .await
        .map_err(|e| format!("Error getting active account: {}", e))?;

    active_account
        .remove_nostr_wallet_connect_uri()
        .map_err(|e| format!("Error removing NWC URI: {}", e))
}
