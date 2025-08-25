use crate::whitenoise::{
    accounts::Account,
    database::published_events::{ProcessedEvent, PublishedEvent},
    error::Result,
    Whitenoise,
};

impl Whitenoise {
    /// Records that we published a specific event to prevent processing our own events
    pub(crate) async fn record_published_event(
        &self,
        event_id: &nostr_sdk::EventId,
        account: &Account,
        event_kind: nostr_sdk::Kind,
    ) -> Result<()> {
        PublishedEvent::create(&self.database, event_id, account.id.unwrap(), event_kind).await?;
        Ok(())
    }

    /// Checks if we published a specific event
    pub(crate) async fn did_we_publish_event(
        &self,
        event_id: &nostr_sdk::EventId,
        account: &Account,
    ) -> Result<bool> {
        let result = PublishedEvent::exists(&self.database, event_id, account.id.unwrap()).await?;
        Ok(result)
    }

    /// Records that we processed a specific event to ensure idempotency
    pub(crate) async fn record_processed_event(
        &self,
        event_id: &nostr_sdk::EventId,
        event_kind: nostr_sdk::Kind,
    ) -> Result<()> {
        ProcessedEvent::create(&self.database, event_id, event_kind).await?;
        Ok(())
    }

    /// Checks if we already processed a specific event
    pub(crate) async fn already_processed_event(
        &self,
        event_id: &nostr_sdk::EventId,
    ) -> Result<bool> {
        let result = ProcessedEvent::exists(&self.database, event_id).await?;
        Ok(result)
    }
}
