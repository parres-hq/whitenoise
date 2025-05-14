use crate::types::NostrEncryptionMethod;

use nostr_sdk::prelude::*;


pub async fn encrypt_content(
    content: String,
    pubkey: String,
    method: NostrEncryptionMethod,
) -> Result<String, String> {
    wn.nostr
        .encrypt_content(content, pubkey, method)
        .await
        .map_err(|e| e.to_string())
}
