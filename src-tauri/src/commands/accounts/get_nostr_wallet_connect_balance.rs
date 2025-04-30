use crate::accounts::Account;
use crate::whitenoise::Whitenoise;
use nwc::prelude::*;

/// Gets the balance information from the connected Nostr Wallet Connect wallet.
///
/// # Arguments
///
/// * `wn` - A reference to the Whitenoise state
///
/// # Returns
///
/// * `Ok(u64)` - The balance in sats if successful
/// * `Err(String)` - An error message if there was an issue getting the balance
#[tauri::command]
pub async fn get_nostr_wallet_connect_balance(
    wn: tauri::State<'_, Whitenoise>,
) -> Result<u64, String> {
    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| format!("Error getting active account: {}", e))?;

    let nwc_uri = active_account
        .get_nostr_wallet_connect_uri(wn.clone())
        .map_err(|e| format!("Error getting NWC URI: {}", e))?
        .ok_or_else(|| "No NWC URI configured".to_string())?;

    let uri = NostrWalletConnectURI::parse(&nwc_uri)
        .map_err(|e| format!("Error parsing NWC URI: {}", e))?;
    let nwc = NWC::new(uri);

    nwc.get_balance()
        .await
        .map_err(|e| format!("Error getting NWC info: {}", e))
}
