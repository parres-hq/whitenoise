use crate::accounts::Account;

use nostr_sdk::prelude::*;

/// Creates a new identity by generating a new keypair, logging in with it, and onboarding the account by publishing the metadata event and key package to the relays.
///
/// # Arguments
///
/// * `wn` - A reference to the Whitenoise state.
/// * `app_handle` - The app handle.
///
/// # Returns
///
/// * `Ok(Account)` - The newly created account.
/// * `Err(String)` - An error message if there was an issue creating the identity.
pub async fn create_identity() -> Result<Account, String> {
    // Create a new account with a generated keypair
    let initial_account = Account::new()
        .await
        .map_err(|e| format!("Error creating account: {}", e))?;

    // Set the account as active and get the updated account state directly from the database
    // This ensures the active flag is properly set in our account instance
    let account = initial_account
        .set_active()
        .await
        .map_err(|e| format!("Error setting active account: {}", e))?;

    // Now onboard the account with the correct active state
    // Fetch the account from DB to ensure we have the most up-to-date state
    Account::find_by_pubkey(&account.pubkey)
        .await
        .map_err(|e| format!("Error fetching account after activation: {}", e))?
        .onboard_new_account()
        .await
        .map_err(|e| format!("Error onboarding new account: {}", e))
}
