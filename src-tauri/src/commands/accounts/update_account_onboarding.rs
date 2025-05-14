use crate::accounts::Account;

use nostr_sdk::prelude::*;

/// Updates the onboarding status for a specific account.
///
/// # Arguments
///
/// * `pubkey` - The public key of the account to update
/// * `inbox_relays` - Whether inbox relays have been configured
/// * `key_package_relays` - Whether key package relays have been configured
/// * `publish_key_package` - Whether the key package has been published
/// * `wn` - A reference to the Whitenoise state
///
/// # Returns
///
/// * `Ok(Account)` - The updated account if successful
/// * `Err(String)` - An error message if there was an issue updating the account
pub async fn update_account_onboarding(
    pubkey: String,
    inbox_relays: bool,
    key_package_relays: bool,
    publish_key_package: bool,
) -> Result<Account, String> {
    let pubkey =
        PublicKey::parse(&pubkey).map_err(|e| format!("Error parsing public key: {}", e))?;
    let mut account = Account::find_by_pubkey(&pubkey)
        .await
        .map_err(|e| format!("Error fetching account: {}", e))?;
    account.onboarding.inbox_relays = inbox_relays;
    account.onboarding.key_package_relays = key_package_relays;
    account.onboarding.publish_key_package = publish_key_package;
    account
        .save()
        .await
        .map_err(|e| format!("Error saving account: {}", e))?;
    Ok(account)
}
