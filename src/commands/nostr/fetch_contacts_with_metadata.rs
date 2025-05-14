
use nostr_sdk::prelude::*;
use std::collections::HashMap;


pub async fn fetch_contacts_with_metadata() -> Result<HashMap<String, Metadata>, String> {
    let events = wn.nostr.fetch_contacts().await.map_err(|e| e.to_string())?;
    let mut metadata_map = HashMap::new();

    for event in events {
        if let Ok(metadata) = serde_json::from_str::<Metadata>(&event.content) {
            metadata_map.insert(event.pubkey.to_hex(), metadata);
        }
    }

    Ok(metadata_map)
}
