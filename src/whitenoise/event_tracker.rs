use async_trait::async_trait;
use nostr_sdk::{EventId, PublicKey};
use std::sync::Arc;

use crate::whitenoise::{
    accounts::Account,
    database::published_events::{ProcessedEvent, PublishedEvent},
    Whitenoise,
};

/// Trait for handling event tracking operations
#[async_trait]
pub trait EventTracker: Send + Sync {
    /// Track that we published a specific event
    async fn track_published_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Check if the account was the publisher of a specific event
    async fn account_published_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// Track that we processed a specific event
    async fn track_processed_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Check if we already processed a specific event
    async fn already_processed_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;
}

/// No-op implementation that doesn't track events
pub struct NoEventTracker;

#[async_trait]
impl EventTracker for NoEventTracker {
    async fn track_published_event(
        &self,
        _event_id: &EventId,
        _pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(()) // Do nothing
    }

    async fn account_published_event(
        &self,
        _event_id: &EventId,
        _pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false) // Do nothing
    }

    async fn track_processed_event(
        &self,
        _event_id: &EventId,
        _pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(()) // Do nothing
    }

    async fn already_processed_event(
        &self,
        _event_id: &EventId,
        _pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false) // Do nothing
    }
}

/// Whitenoise implementation of event tracking
#[derive(Default)]
pub struct WhitenoiseEventTracker;

impl WhitenoiseEventTracker {
    pub fn new() -> Self {
        Self {} // Default is no-op
    }
}

#[async_trait]
impl EventTracker for WhitenoiseEventTracker {
    async fn track_published_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        let account = Account::find_by_pubkey(pubkey, &whitenoise.database).await?;
        PublishedEvent::create(event_id, account.id.unwrap(), &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    async fn account_published_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        let account = Account::find_by_pubkey(pubkey, &whitenoise.database).await?;
        PublishedEvent::exists(event_id, account.id.unwrap(), &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn track_processed_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        let account = Account::find_by_pubkey(pubkey, &whitenoise.database).await?;
        ProcessedEvent::create(event_id, account.id.unwrap(), &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn already_processed_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        let account = Account::find_by_pubkey(pubkey, &whitenoise.database).await?;
        ProcessedEvent::exists(event_id, account.id.unwrap(), &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

/// Test-specific event tracker that uses a provided database instance
pub struct TestEventTracker {
    database: Arc<crate::whitenoise::database::Database>,
}

impl TestEventTracker {
    pub fn new(database: Arc<crate::whitenoise::database::Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl EventTracker for TestEventTracker {
    async fn track_published_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        PublishedEvent::create(event_id, account.id.unwrap(), &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    async fn account_published_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        PublishedEvent::exists(event_id, account.id.unwrap(), &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn track_processed_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        ProcessedEvent::create(event_id, account.id.unwrap(), &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn already_processed_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        ProcessedEvent::exists(event_id, account.id.unwrap(), &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}
