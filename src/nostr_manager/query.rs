//! Query functions for NostrManager
//! This handles fetching events from the database cache.

use crate::nostr_manager::{NostrManager, Result};
use crate::whitenoise::accounts::relays::RelayType;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

impl NostrManager {
    pub(crate) async fn query_user_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
        Ok(self.client.database().metadata(pubkey).await?)
    }

    pub(crate) async fn fetch_user_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
        let metadata = self
            .client
            .fetch_metadata(pubkey, self.timeout().await?)
            .await?;
        Ok(metadata)
    }

    pub(crate) async fn query_user_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(relay_type.into())
            .limit(1);
        let relay_events = self
            .client
            .fetch_events(filter.clone(), self.timeout().await?)
            .await?;
        let database_events = self.client.database().query(filter).await?;
        Ok(Self::relay_urls_from_events(
            relay_events.merge(database_events),
        ))
    }

    pub(crate) async fn query_user_contact_list(
        &self,
        pubkey: PublicKey,
    ) -> Result<HashMap<PublicKey, Option<Metadata>>> {
        let filter = Filter::new()
            .kind(Kind::ContactList)
            .author(pubkey)
            .limit(1);
        let events = self.client.database().query(filter).await?;

        let contacts_pubkeys = if let Some(event) = events.first() {
            event
                .tags
                .iter()
                .filter(|tag| tag.kind() == TagKind::p())
                .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
                .collect()
        } else {
            vec![]
        };

        let mut contacts_metadata = HashMap::new();
        for contact in contacts_pubkeys {
            let metadata = self.query_user_metadata(contact).await?;
            contacts_metadata.insert(contact, metadata);
        }

        Ok(contacts_metadata)
    }

    pub(crate) async fn fetch_user_key_package(
        &self,
        pubkey: PublicKey,
        urls: Vec<RelayUrl>,
    ) -> Result<Option<Event>> {
        let filter = Filter::new()
            .kind(Kind::MlsKeyPackage)
            .author(pubkey)
            .limit(1);
        let events = self
            .client
            .fetch_events_from(urls, filter.clone(), self.timeout().await?)
            .await?;

        let stored_events = self.client.database().query(filter).await?;
        Ok(events.merge(stored_events).first_owned())
    }
}
