use crate::nostr_manager::NostrManager;
use crate::nostr_manager::NostrManagerError;
use crate::types::EnrichedContact;

use nostr_sdk::prelude::*;
use std::collections::HashMap;

impl NostrManager {
    pub async fn search_users(&self, query: String) -> Result<HashMap<String, EnrichedContact>> {
        let filter = Filter::new().kind(Kind::Metadata).search(query);

        let metadata_events = self
            .client
            .fetch_events(filter.clone(), self.timeout().await.unwrap())
            .await
            .map_err(NostrManagerError::from)?;

        let pubkeys: Vec<PublicKey> = metadata_events
            .iter()
            .map(|user| user.pubkey)
            .collect::<Vec<PublicKey>>();

        let enriching_events = self
            .client
            .fetch_events(
                Filter::new().authors(pubkeys).kinds(vec![
                    Kind::MlsKeyPackageRelays,
                    Kind::InboxRelays,
                    Kind::MlsKeyPackage,
                ]),
                self.timeout().await.unwrap(),
            )
            .await
            .map_err(NostrManagerError::from)?;

        let mut enriched_contacts = HashMap::new();

        for user in metadata_events {
            let enriched_contact = EnrichedContact {
                metadata: Metadata::from_json(&user.content).unwrap_or_default(),
                nip17: enriching_events
                    .iter()
                    .any(|event| event.kind == Kind::InboxRelays && event.pubkey == user.pubkey),
                nip104: enriching_events
                    .iter()
                    .any(|event| event.kind == Kind::MlsKeyPackage && event.pubkey == user.pubkey),
                nostr_relays: Vec::new(), // For now, we don't care about these since we're only searching in the context of finding a person to start a conversation with. We'll fetch all their data later.
                inbox_relays: Vec::new(), // For now, we don't care about these
                key_package_relays: Vec::new(), // For now, we don't care about these
            };
            enriched_contacts.insert(user.pubkey.to_hex(), enriched_contact);
        }

        Ok(enriched_contacts)
    }
}
