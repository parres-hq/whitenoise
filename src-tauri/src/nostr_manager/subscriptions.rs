//! Subscription functions for NostrManager
//! This mostly handles subscribing and processing events as they come in while the user is active.

use crate::nostr_manager::event_processor::ProcessableEvent;
use crate::nostr_manager::{NostrManager, NostrManagerError, Result};
use nostr_sdk::prelude::*;

const MLS_MESSAGES_SUB: &str = "mls_messages";

impl NostrManager {
    async fn subscribe_contact_list(&self, pubkey: PublicKey) -> Result<Output<SubscriptionId>> {
        let contacts_filter = Filter::new()
            .kind(Kind::ContactList)
            .author(pubkey)
            .since(Timestamp::now());

        Ok(self.client.subscribe(contacts_filter, None).await?)
    }

    async fn subscribe_contacts_metadata(&self) -> Result<Output<SubscriptionId>> {
        let contact_list_pubkeys = self
            .client
            .get_contact_list_public_keys(self.timeout().await?)
            .await?;

        let contact_metadata_filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(contact_list_pubkeys)
            .since(Timestamp::now());

        Ok(self.client.subscribe(contact_metadata_filter, None).await?)
    }

    async fn subscribe_metadata(&self, pubkey: PublicKey) -> Result<Output<SubscriptionId>> {
        let metadata_filter = Filter::new()
            .kind(Kind::Metadata)
            .author(pubkey)
            .since(Timestamp::now());

        Ok(self.client.subscribe(metadata_filter, None).await?)
    }

    async fn subscribe_relay_list(&self, pubkey: PublicKey) -> Result<Output<SubscriptionId>> {
        let relay_list_filter = Filter::new()
            .kind(Kind::RelayList)
            .author(pubkey)
            .since(Timestamp::now());

        Ok(self.client.subscribe(relay_list_filter, None).await?)
    }

    async fn subscribe_inbox_relay_list(
        &self,
        pubkey: PublicKey,
    ) -> Result<Output<SubscriptionId>> {
        let inbox_relay_list_filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(pubkey)
            .since(Timestamp::now());

        Ok(self.client.subscribe(inbox_relay_list_filter, None).await?)
    }

    async fn subscribe_giftwraps(&self, pubkey: PublicKey) -> Result<Output<SubscriptionId>> {
        let giftwrap_filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(pubkey)
            .since(Timestamp::now());

        Ok(self.client.subscribe(giftwrap_filter, None).await?)
    }

    pub async fn subscribe_mls_group_messages(&self, group_ids: Vec<String>) -> Result<Output<()>> {
        let sub_id = SubscriptionId::new(MLS_MESSAGES_SUB);
        let mls_message_filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), group_ids)
            .since(Timestamp::now());

        Ok(self
            .client
            .subscribe_with_id(sub_id, mls_message_filter, None)
            .await?)
    }

    pub async fn setup_subscriptions(
        &self,
        pubkey: PublicKey,
        nostr_group_ids: Vec<String>,
    ) -> Result<()> {
        self.subscribe_contact_list(pubkey).await?;
        self.subscribe_contacts_metadata().await?;
        self.subscribe_metadata(pubkey).await?;
        self.subscribe_relay_list(pubkey).await?;
        self.subscribe_inbox_relay_list(pubkey).await?;
        self.subscribe_giftwraps(pubkey).await?;

        if !nostr_group_ids.is_empty() {
            self.subscribe_mls_group_messages(nostr_group_ids).await?;
        }

        if let Err(e) = self
            .client
            .handle_notifications(|notification| async {
                match notification {
                    RelayPoolNotification::Event { event, .. } => {
                        self.handle_event(*event).await?;
                        Ok(false)
                    }
                    RelayPoolNotification::Message { relay_url, message } => {
                        self.handle_message(relay_url, message)?;
                        Ok(false)
                    }
                    RelayPoolNotification::Shutdown => {
                        self.handle_shutdown()?;
                        Ok(true)
                    }
                }
            })
            .await
        {
            tracing::error!(
                target: "whitenoise::nostr_client::handle_notifications",
                "Notification handler error: {:?}",
                e
            );
        }

        Ok(())
    }

    // Handle events
    async fn handle_event(&self, event: Event) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_client::subscriptions::handle_event",
            "Received event: {:?}",
            event
        );
        match event.kind {
            Kind::GiftWrap => {
                self.event_processor
                    .lock()
                    .await
                    .queue_event(ProcessableEvent::GiftWrap(event))
                    .await
                    .map_err(|e| NostrManagerError::FailedToQueueEvent(e.to_string()))?;
            }
            Kind::MlsGroupMessage => {
                self.event_processor
                    .lock()
                    .await
                    .queue_event(ProcessableEvent::MlsMessage(event))
                    .await
                    .map_err(|e| NostrManagerError::FailedToQueueEvent(e.to_string()))?;
            }
            _ => {}
        }
        Ok(())
    }

    // Handle other types of notifications
    fn handle_message(&self, relay_url: RelayUrl, message: RelayMessage) -> Result<()> {
        let variant_name = match message {
            RelayMessage::Event { .. } => "Event",
            RelayMessage::Ok { .. } => "Ok",
            RelayMessage::Notice { .. } => "Notice",
            RelayMessage::Closed { .. } => "Closed",
            RelayMessage::EndOfStoredEvents(_) => "EndOfStoredEvents",
            RelayMessage::Auth { .. } => "Auth",
            RelayMessage::Count { .. } => "Count",
            RelayMessage::NegMsg { .. } => "NegMsg",
            RelayMessage::NegErr { .. } => "NegErr",
        };
        tracing::debug!(
            target: "whitenoise::nostr_client::handle_notifications",
            "Received message from {}: {}",
            relay_url,
            variant_name
        );
        Ok(())
    }

    fn handle_shutdown(&self) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_client::handle_notifications",
            "Relay pool shutdown"
        );
        Ok(())
    }
}
