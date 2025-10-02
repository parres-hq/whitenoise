use std::sync::Arc;

use async_trait::async_trait;
use nostr_sdk::prelude::*;

use crate::whitenoise::{
    Whitenoise,
    accounts::Account,
    database::{processed_events::ProcessedEvent, published_events::PublishedEvent},
    utils::timestamp_to_datetime,
};

/// Trait for handling event tracking operations
#[async_trait]
pub trait EventTracker: Send + Sync {
    /// Track that an account published a specific event
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

    /// Check if we published a given event, regardless of account
    async fn global_published_event(
        &self,
        event_id: &EventId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// Track that we processed a specific event for an account
    async fn track_processed_account_event(
        &self,
        event: &Event,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Check if we already processed a specific event for an account
    async fn already_processed_account_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// Track that we processed a specific global event
    async fn track_processed_global_event(
        &self,
        event: &Event,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Check if we already processed a specific global event
    async fn already_processed_global_event(
        &self,
        event_id: &EventId,
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

    async fn global_published_event(
        &self,
        _event_id: &EventId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false) // Do nothing
    }

    async fn track_processed_account_event(
        &self,
        _event: &Event,
        _pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(()) // Do nothing
    }

    async fn already_processed_account_event(
        &self,
        _event_id: &EventId,
        _pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false) // Do nothing
    }

    async fn track_processed_global_event(
        &self,
        _event: &Event,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(()) // Do nothing
    }

    async fn already_processed_global_event(
        &self,
        _event_id: &EventId,
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
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        PublishedEvent::create(event_id, account_id, &whitenoise.database)
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
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        PublishedEvent::exists(event_id, Some(account_id), &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn global_published_event(
        &self,
        event_id: &EventId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        PublishedEvent::exists(event_id, None, &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn track_processed_account_event(
        &self,
        event: &Event,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        let account = Account::find_by_pubkey(pubkey, &whitenoise.database).await?;
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        ProcessedEvent::create(
            &event.id,
            Some(account_id),
            Some(timestamp_to_datetime(event.created_at)?),
            Some(event.kind),
            Some(&event.pubkey),
            &whitenoise.database,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn already_processed_account_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        let account = Account::find_by_pubkey(pubkey, &whitenoise.database).await?;
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        ProcessedEvent::exists(event_id, Some(account_id), &whitenoise.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn track_processed_global_event(
        &self,
        event: &Event,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        ProcessedEvent::create(
            &event.id,
            None,
            Some(timestamp_to_datetime(event.created_at)?),
            Some(event.kind),
            Some(&event.pubkey),
            &whitenoise.database,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn already_processed_global_event(
        &self,
        event_id: &EventId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let whitenoise = Whitenoise::get_instance()?;
        ProcessedEvent::exists(event_id, None, &whitenoise.database)
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
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        PublishedEvent::create(event_id, account_id, &self.database)
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
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        PublishedEvent::exists(event_id, Some(account_id), &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn global_published_event(
        &self,
        event_id: &EventId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        PublishedEvent::exists(event_id, None, &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn track_processed_account_event(
        &self,
        event: &Event,
        pubkey: &PublicKey,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        ProcessedEvent::create(
            &event.id,
            Some(account_id),
            Some(timestamp_to_datetime(event.created_at)?),
            Some(event.kind),
            Some(&event.pubkey),
            &self.database,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn already_processed_account_event(
        &self,
        event_id: &EventId,
        pubkey: &PublicKey,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        let account_id = account
            .id
            .ok_or_else(|| std::io::Error::other("Account missing id"))?;
        ProcessedEvent::exists(event_id, Some(account_id), &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn track_processed_global_event(
        &self,
        event: &Event,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        ProcessedEvent::create(
            &event.id,
            None,
            Some(timestamp_to_datetime(event.created_at)?),
            Some(event.kind),
            Some(&event.pubkey),
            &self.database,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn already_processed_global_event(
        &self,
        event_id: &EventId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        ProcessedEvent::exists(event_id, None, &self.database)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}
