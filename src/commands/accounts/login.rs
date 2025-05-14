use crate::accounts::Account;

use nostr_sdk::prelude::*;

/// Logs in with the given public key. Will set the active account if successful.
///
/// # Arguments
///
/// * `wn` - A reference to the Whitenoise state.
/// * `hex_pubkey` - The public key in hexadecimal format.
///
/// # Returns
///
/// * `Ok(Account)` - The account if login was successful.
/// * `Err(String)` - An error message if there was an issue logging in.
pub async fn login(
    nsec_or_hex_privkey: String,
) -> Result<Account, String> {
    let keys = Keys::parse(&nsec_or_hex_privkey).map_err(|e| e.to_string())?;

    match Account::find_by_pubkey(&keys.public_key).await {
        Ok(account) => {
            tracing::debug!("Account found, setting active");
            account
                .set_active()
                .await
                .map_err(|e| format!("Error logging in: {}", e))
        }
        _ => {
            tracing::debug!(target: "whitenoise::commands::accounts","Account not found, adding from keys");
            Account::add_from_keys(&keys, true)
                .await
                .map_err(|e| format!("Error logging in: {}", e))
        }
    }
}
