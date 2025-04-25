use crate::accounts::Account;
use crate::whitenoise::Whitenoise;
use nwc::prelude::*;

/// Sets the Nostr Wallet Connect URI for the active account.
///
/// # Arguments
///
/// * `nostr_wallet_connect_uri` - The NWC URI to store
/// * `wn` - A reference to the Whitenoise state
///
/// # Returns
///
/// * `Ok(())` - If the URI was stored successfully
/// * `Err(String)` - An error message if there was an issue storing the URI
#[tauri::command]
pub async fn set_nostr_wallet_connect_uri(
    nostr_wallet_connect_uri: String,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<(), String> {
    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| format!("Error getting active account: {}", e))?;
    let uri: NostrWalletConnectURI =
        NostrWalletConnectURI::parse(&nostr_wallet_connect_uri).expect("Failed to parse NWC URI");
    let nwc: NWC = NWC::new(uri);
    nwc.get_info()
        .await
        .map_err(|e| format!("Error getting NWC info: {}", e))?;

    active_account
        .store_nostr_wallet_connect_uri(&nostr_wallet_connect_uri, wn.clone())
        .map_err(|e| format!("Error storing NWC URI: {}", e))
}
