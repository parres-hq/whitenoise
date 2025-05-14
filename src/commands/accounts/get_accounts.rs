use crate::accounts::Account;
use nostr_sdk::prelude::*;

/// Lists all accounts.
///
/// # Arguments
///
/// * `wn` - A reference to the Whitenoise state.
///
/// # Returns
///
/// * `Ok(Vec<Account>)` - A vector of accounts if successful.
/// * `Err(String)` - An error message if there was an issue listing the accounts.
pub async fn get_accounts() -> Result<Vec<Account>, String> {
    Account::all()
        .await
        .map_err(|e| format!("Error fetching accounts: {}", e))
}
