use crate::secrets_store;

use nostr_sdk::prelude::*;


pub async fn export_nsec(pubkey: String) -> Result<String, String> {
    let keys = secrets_store::get_nostr_keys_for_pubkey(&pubkey, &wn.data_dir)
        .map_err(|e| e.to_string())?;

    keys.secret_key().to_bech32().map_err(|e| e.to_string())
}
