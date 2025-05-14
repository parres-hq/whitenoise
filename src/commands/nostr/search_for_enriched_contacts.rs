use crate::types::EnrichedContact;
use nostr_sdk::prelude::*;
use std::collections::HashMap;


pub async fn search_for_enriched_contacts(
    query: String,
) -> Result<HashMap<String, EnrichedContact>, String> {
    let enriched_users = wn
        .nostr
        .search_users(query)
        .await
        .map_err(|e| e.to_string())?;

    Ok(enriched_users)
}
