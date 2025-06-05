//! Fetch functions for NostrManager
//! This handles on-the-spot fetching of events from relays.
//! In almost all cases, we query for events already stored in our databsae
//! and combine the results from our database with those from relays in the response.

use crate::nostr_manager::event_processor::ProcessableEvent;
use crate::nostr_manager::{NostrManager, NostrManagerError, Result};
use nostr_sdk::prelude::*;

impl NostrManager {
    pub async fn fetch_for_user(
        &self,
        pubkey: PublicKey,
        last_synced: Timestamp,
        group_ids: Vec<String>, // Nostr group ids
    ) -> Result<()> {
        self.fetch_user_metadata(pubkey).await?;
        self.fetch_contacts().await?;
        self.fetch_user_relays(pubkey).await?;
        self.fetch_user_inbox_relays(pubkey).await?;
        self.fetch_user_key_package_relays(pubkey).await?;
        self.fetch_user_key_packages(pubkey).await?;
        self.fetch_user_giftwrapped_events(pubkey).await?;
        self.fetch_group_messages(last_synced, group_ids).await?;
        Ok(())
    }

    pub async fn fetch_user_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
        match self
            .client
            .fetch_metadata(pubkey, self.timeout().await?)
            .await
        {
            Ok(metadata) => Ok(metadata),
            Err(e) => Err(NostrManagerError::from(e)),
        }
    }

    pub async fn fetch_user_relays(&self, pubkey: PublicKey) -> Result<Vec<String>> {
        let filter = Filter::new().author(pubkey).kind(Kind::RelayList).limit(1);

        let events = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await
            .map_err(NostrManagerError::from)?;

        Ok(Self::relay_url_strings_from_events(events))
    }

    pub async fn fetch_user_inbox_relays(&self, pubkey: PublicKey) -> Result<Vec<String>> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::InboxRelays)
            .limit(1);
        let events = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await
            .map_err(NostrManagerError::from)?;

        Ok(Self::relay_url_strings_from_events(events))
    }

    pub async fn fetch_user_key_package_relays(&self, pubkey: PublicKey) -> Result<Vec<String>> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::MlsKeyPackageRelays)
            .limit(1);
        let events = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await
            .map_err(NostrManagerError::from)?;

        Ok(Self::relay_url_strings_from_events(events))
    }

    pub async fn fetch_user_key_packages(&self, pubkey: PublicKey) -> Result<Events> {
        let filter = Filter::new().author(pubkey).kind(Kind::MlsKeyPackage);
        let events = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await
            .map_err(NostrManagerError::from)?;
        Ok(events)
    }

    pub async fn fetch_contacts(&self) -> Result<Vec<Event>> {
        tracing::debug!(
            target: "whitenoise::nostr_client::fetch_contacts",
            "Fetching contacts for: {:?}",
            self.client.signer().await?.get_public_key().await.unwrap().to_hex()
        );
        let contacts_pubkeys = self
            .client
            .get_contact_list_public_keys(self.timeout().await?)
            .await?;

        let filter = Filter::new().kind(Kind::Metadata).authors(contacts_pubkeys);
        let database_contacts = self.client.database().query(filter.clone()).await?;
        let fetched_contacts = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await?;

        let contacts = database_contacts.merge(fetched_contacts);
        Ok(contacts.into_iter().collect())
    }

    async fn fetch_user_giftwrapped_events(&self, pubkey: PublicKey) -> Result<Vec<Event>> {
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(pubkey);
        let stored_events = self.client.database().query(filter.clone()).await?;
        let fetched_events = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await?;

        let events = stored_events.merge(fetched_events);
        for event in events.iter() {
            let processor = self.event_processor.lock().await;
            processor
                .queue_event(ProcessableEvent::GiftWrap(event.clone()))
                .await
                .map_err(|e| NostrManagerError::FailedToQueueEvent(e.to_string()))?;
        }
        Ok(events.into_iter().collect())
    }

    pub async fn fetch_group_messages(
        &self,
        last_synced: Timestamp,
        group_ids: Vec<String>,
    ) -> Result<Vec<Event>> {
        let filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), group_ids)
            .since(last_synced)
            .until(Timestamp::now());

        let stored_events = self.client.database().query(filter.clone()).await?;
        let fetched_events = self
            .client
            .fetch_events(filter, self.timeout().await?)
            .await?;

        let events = stored_events.merge(fetched_events);

        for event in events.iter() {
            let processor = self.event_processor.lock().await;
            processor
                .queue_event(ProcessableEvent::MlsMessage(event.clone()))
                .await
                .map_err(|e| NostrManagerError::FailedToQueueEvent(e.to_string()))?;
        }

        Ok(events.into_iter().collect())
    }
}
