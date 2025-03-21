use crate::types::EnrichedContact;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

#[tauri::command]
pub async fn query_enriched_contacts(
    wn: tauri::State<'_, Whitenoise>,
) -> Result<HashMap<String, EnrichedContact>, String> {
    // Query contact list public keys from local database
    let contact_list_pubkeys = wn
        .nostr
        .query_contact_list_pubkeys()
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!(
        "query_enriched_contacts contact_list_pubkeys length: {:?}",
        contact_list_pubkeys.len()
    );

    let mut contacts_map: HashMap<String, EnrichedContact> = HashMap::new();

    // Bail early if there are no contact list pubkeys
    if contact_list_pubkeys.is_empty() {
        return Ok(contacts_map);
    }

    // Initialize the map with default EnrichedContact for all pubkeys
    for pubkey in &contact_list_pubkeys {
        contacts_map.insert(
            pubkey.to_string(),
            EnrichedContact {
                metadata: Metadata::default(),
                nip17: false,
                nip104: false,
                nostr_relays: Vec::new(),
                inbox_relays: Vec::new(),
                key_package_relays: Vec::new(),
            },
        );
    }

    let filter = Filter::new()
        .kinds(vec![
            Kind::Metadata,
            Kind::RelayList,
            Kind::InboxRelays,
            Kind::MlsKeyPackage,
            Kind::MlsKeyPackageRelays,
        ])
        .authors(contact_list_pubkeys.clone());

    let stored_events = wn
        .nostr
        .client
        .database()
        .query(filter.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Process all events
    for event in stored_events {
        let author = event.pubkey.to_string();
        if let Some(contact) = contacts_map.get_mut(&author) {
            match event.kind {
                Kind::Metadata => {
                    if let Ok(metadata) = Metadata::from_json(&event.content) {
                        contact.metadata = metadata;
                    }
                }
                Kind::RelayList => {
                    contact.nostr_relays.extend(
                        event
                            .tags
                            .iter()
                            .filter(|tag| tag.kind() == TagKind::Relay)
                            .filter_map(|tag| tag.content())
                            .map(|s| s.to_string()),
                    );
                }
                Kind::InboxRelays => {
                    contact.nip17 = true;
                    contact.inbox_relays.extend(
                        event
                            .tags
                            .iter()
                            .filter(|tag| tag.kind() == TagKind::Relay)
                            .filter_map(|tag| tag.content())
                            .map(|s| s.to_string()),
                    );
                }
                Kind::MlsKeyPackageRelays => {
                    contact.key_package_relays.extend(
                        event
                            .tags
                            .iter()
                            .filter(|tag| tag.kind() == TagKind::Relay)
                            .filter_map(|tag| tag.content())
                            .map(|s| s.to_string()),
                    );
                }
                Kind::MlsKeyPackage => {
                    if event.tags.find(TagKind::MlsProtocolVersion).is_some() {
                        contact.nip104 = true;
                    }
                }
                _ => {}
            }
        }
    }

    Ok(contacts_map)
}
