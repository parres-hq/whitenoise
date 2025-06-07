// use crate::media::blossom::BlossomClient;
use crate::types::{NostrEncryptionMethod, ProcessableEvent};

use nostr_sdk::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc::Sender, Mutex};

pub mod parser;
pub mod query;
// pub mod search;
// pub mod subscriptions;
// pub mod sync;

#[derive(Error, Debug)]
pub enum NostrManagerError {
    #[error("Client Error: {0}")]
    Client(#[from] nostr_sdk::client::Error),
    #[error("Database Error: {0}")]
    Database(#[from] DatabaseError),
    #[error("Signer Error: {0}")]
    Signer(#[from] nostr_sdk::signer::SignerError),
    #[error("Error with secrets store: {0}")]
    SecretsStoreError(String),
    #[error("Failed to queue event: {0}")]
    FailedToQueueEvent(String),
    #[error("Failed to shutdown event processor: {0}")]
    FailedToShutdownEventProcessor(String),
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("Account error: {0}")]
    AccountError(String),
}

#[derive(Debug, Clone)]
pub struct NostrManagerSettings {
    pub timeout: Duration,
    pub relays: Vec<String>,
    // pub blossom_server: String,
}

#[derive(Debug, Clone)]
pub struct NostrManager {
    pub settings: Arc<Mutex<NostrManagerSettings>>,
    client: Client,
    // blossom: BlossomClient,
}

impl Default for NostrManagerSettings {
    fn default() -> Self {
        let mut relays = vec![];
        if cfg!(debug_assertions) {
            relays.push("ws://localhost:8080".to_string());
            relays.push("ws://localhost:7777".to_string());
            // relays.push("wss://purplepag.es".to_string());
            // relays.push("wss://nos.lol".to_string());
        } else {
            relays.push("wss://relay.damus.io".to_string());
            relays.push("wss://purplepag.es".to_string());
            relays.push("wss://relay.primal.net".to_string());
            relays.push("wss://nos.lol".to_string());
        }

        Self {
            timeout: Duration::from_secs(3),
            relays,
            // blossom_server: if cfg!(debug_assertions) {
            //     "http://localhost:3000".to_string()
            // } else {
            //     "https://blossom.primal.net".to_string()
            // },
        }
    }
}
pub type Result<T> = std::result::Result<T, NostrManagerError>;

impl NostrManager {
    /// Create a new Nostr manager
    ///
    /// # Arguments
    ///
    /// * `db_path` - The path to the nostr cache database
    /// * `event_sender` - Channel sender for forwarding events to Whitenoise for processing
    pub async fn new(
        db_path: PathBuf,
        event_sender: Sender<crate::types::ProcessableEvent>,
    ) -> Result<Self> {
        let opts = Options::default();

        // Initialize the client with the appropriate database based on platform
        let client = {
            let full_path = db_path.join("nostr_lmdb");
            let db = NostrLMDB::open(full_path).expect("Failed to open Nostr database");
            Client::builder().database(db).opts(opts).build()
        };

        let settings = NostrManagerSettings::default();

        // let blossom = BlossomClient::new(&settings.blossom_server);

        // Add the default relays
        for relay in &settings.relays {
            client.add_relay(relay).await?;
        }

        // Connect to relays with a timeout to prevent blocking
        let connection_timeout = Duration::from_secs(5);
        tracing::debug!(
            target: "whitenoise::nostr_manager::new",
            "Attempting to connect to relays with {}s timeout...",
            connection_timeout.as_secs()
        );

        // Use timeout for connection to prevent indefinite blocking
        match tokio::time::timeout(connection_timeout, client.connect()).await {
            Ok(_) => {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::new",
                    "Successfully connected to relays"
                );
            }
            Err(_) => {
                tracing::warn!(
                    target: "whitenoise::nostr_manager::new",
                    "Connection timeout after {}s - continuing without relay connections",
                    connection_timeout.as_secs()
                );
            }
        }

        // Set up notification handler with error handling
        tracing::debug!(
            target: "whitenoise::nostr_manager::new",
            "Setting up notification handler..."
        );

        // Spawn notification handler in a background task to prevent blocking
        let client_clone = client.clone();
        let event_sender_clone = event_sender.clone();
        tokio::spawn(async move {
            if let Err(e) = client_clone
                .handle_notifications(move |notification| {
                    let sender = event_sender_clone.clone();
                    async move {
                        match notification {
                            RelayPoolNotification::Message { relay_url, message } => {
                                // Extract events and send to Whitenoise queue
                                match message {
                                    RelayMessage::Event { subscription_id, event } => {
                                        if let Err(e) = sender
                                            .send(ProcessableEvent::NostrEvent(
                                                event.as_ref().clone(),
                                                Some(subscription_id.to_string()),
                                            ))
                                            .await
                                        {
                                            tracing::error!(
                                                target: "whitenoise::nostr_client::handle_notifications",
                                                "Failed to queue event: {}",
                                                e
                                            );
                                        }
                                    }
                                    _ => {
                                        // Handle other relay messages as before
                                        let message_str = match message {
                                            RelayMessage::Ok { .. } => "Ok".to_string(),
                                            RelayMessage::Notice { .. } => "Notice".to_string(),
                                            RelayMessage::Closed { .. } => "Closed".to_string(),
                                            RelayMessage::EndOfStoredEvents(_) => "EndOfStoredEvents".to_string(),
                                            RelayMessage::Auth { .. } => "Auth".to_string(),
                                            RelayMessage::Count { .. } => "Count".to_string(),
                                            RelayMessage::NegMsg { .. } => "NegMsg".to_string(),
                                            RelayMessage::NegErr { .. } => "NegErr".to_string(),
                                            _ => "Unknown".to_string(),
                                        };

                                        if let Err(e) = sender
                                            .send(ProcessableEvent::RelayMessage(relay_url, message_str))
                                            .await
                                        {
                                            tracing::error!(
                                                target: "whitenoise::nostr_client::handle_notifications",
                                                "Failed to queue message: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                                Ok(false) // Continue processing notifications
                            }
                            RelayPoolNotification::Shutdown => {
                                tracing::debug!(
                                    target: "whitenoise::nostr_client::handle_notifications",
                                    "Relay pool shutdown"
                                );
                                Ok(true) // Exit notification loop
                            }
                            _ => {
                                // Ignore other notification types
                                Ok(false) // Continue processing notifications
                            }
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
        });

        tracing::debug!(
            target: "whitenoise::nostr_manager::new",
            "NostrManager initialization completed"
        );

        Ok(Self {
            client,
            // blossom,
            settings: Arc::new(Mutex::new(settings)),
        })
    }

    /// Get the timeout for the Nostr manager
    pub(crate) async fn timeout(&self) -> Result<Duration> {
        let guard = self.settings.lock().await;
        Ok(guard.timeout)
    }

    /// Get the relays for the Nostr manager
    pub(crate) async fn relays(&self) -> Result<Vec<RelayUrl>> {
        let guard = self.settings.lock().await;
        Ok(guard
            .relays
            .clone()
            .into_iter()
            .map(|r| RelayUrl::parse(&r).unwrap())
            .collect())
    }

    /// Publishes a Nostr event using a temporary signer.
    ///
    /// This method allows publishing an event with a signer that is only used for this specific operation.
    /// The signer is set before publishing and unset immediately after.
    ///
    /// # Arguments
    ///
    /// * `event_builder` - The event builder containing the event to publish
    /// * `signer` - A signer that implements `NostrSigner` and has a 'static lifetime
    ///
    /// # Returns
    ///
    /// * `Result<Output<EventId>>` - The published event ID if successful, or an error if publishing fails
    pub(crate) async fn publish_event_builder_with_signer(
        &self,
        event_builder: EventBuilder,
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        self.client.set_signer(signer).await;
        let result = self
            .client
            .send_event_builder(event_builder.clone())
            .await?;
        self.client.unset_signer().await;
        Ok(result)
    }

    /// Extracts welcome events from a list of giftwrapped events.
    ///
    /// This function processes a list of giftwrapped events and extracts the welcome events
    /// (events with Kind::MlsWelcome) from them.
    ///
    /// # Arguments
    ///
    /// * `gw_events` - A vector of giftwrapped Event objects to process.
    ///
    /// # Returns
    ///
    /// A vector of tuples containing the gift-wrap event id and the inner welcome event (the gift wrap rumor event)
    #[allow(dead_code)]
    async fn extract_invite_events(&self, gw_events: Vec<Event>) -> Vec<(EventId, UnsignedEvent)> {
        let mut invite_events: Vec<(EventId, UnsignedEvent)> = Vec::new();

        for event in gw_events {
            if let Ok(unwrapped) = extract_rumor(&self.client.signer().await.unwrap(), &event).await
            {
                if unwrapped.rumor.kind == Kind::MlsWelcome {
                    invite_events.push((event.id, unwrapped.rumor));
                }
            }
        }

        invite_events
    }

    #[allow(dead_code)]
    pub async fn encrypt_content(
        &self,
        content: String,
        pubkey: String,
        method: NostrEncryptionMethod,
    ) -> Result<String> {
        let recipient_pubkey = PublicKey::from_hex(&pubkey).unwrap();
        let signer = self.client.signer().await.unwrap();
        match method {
            NostrEncryptionMethod::Nip04 => {
                let encrypted = signer
                    .nip04_encrypt(&recipient_pubkey, &content)
                    .await
                    .unwrap();
                Ok(encrypted)
            }
            NostrEncryptionMethod::Nip44 => {
                let encrypted = signer
                    .nip44_encrypt(&recipient_pubkey, &content)
                    .await
                    .unwrap();
                Ok(encrypted)
            }
        }
    }

    #[allow(dead_code)]
    pub async fn decrypt_content(
        &self,
        content: String,
        pubkey: String,
        method: NostrEncryptionMethod,
    ) -> Result<String> {
        let author_pubkey = PublicKey::from_hex(&pubkey).unwrap();
        let signer = self.client.signer().await.unwrap();
        match method {
            NostrEncryptionMethod::Nip04 => {
                let decrypted = signer
                    .nip04_decrypt(&author_pubkey, &content)
                    .await
                    .unwrap();
                Ok(decrypted)
            }
            NostrEncryptionMethod::Nip44 => {
                let decrypted = signer
                    .nip44_decrypt(&author_pubkey, &content)
                    .await
                    .unwrap();
                Ok(decrypted)
            }
        }
    }

    /// Extracts and parses relay URLs from a collection of Nostr events.
    ///
    /// This helper method processes a collection of Nostr events and extracts all valid
    /// relay URLs from their tags. It filters for tags of kind `Relay` and attempts to
    /// parse each tag's content as a valid relay URL.
    ///
    /// The method performs the following operations:
    /// 1. Iterates through all events in the collection
    /// 2. Extracts all tags from each event
    /// 3. Filters for tags with kind `TagKind::Relay`
    /// 4. Attempts to parse each tag's content as a `RelayUrl`
    /// 5. Collects all successfully parsed relay URLs into a vector
    ///
    /// # Arguments
    ///
    /// * `events` - A collection of `Event` structs containing relay information in their tags
    ///
    /// # Returns
    ///
    /// Returns a `Vec<RelayUrl>` containing all valid relay URLs found in the events.
    /// Invalid or malformed relay URLs are silently skipped.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let events = fetch_relay_list_events().await?;
    /// let relay_urls = relay_urls_from_events(events);
    /// // relay_urls now contains all valid relay URLs from the events
    /// ```
    ///
    /// # Notes
    ///
    /// * This method silently skips any tags that:
    ///   - Are not of kind `Relay`
    ///   - Have no content
    ///   - Contain invalid relay URL formats
    /// * The order of relay URLs in the returned vector is not guaranteed to match
    ///   the order they appeared in the events
    fn relay_urls_from_events(events: Events) -> Vec<RelayUrl> {
        events
            .into_iter()
            .flat_map(|e| e.tags)
            .filter(|tag| tag.kind() == TagKind::Relay)
            .filter_map(|tag| {
                tag.content()
                    .and_then(|content| RelayUrl::parse(content).ok())
            })
            .collect()
    }

    /// Permanently deletes all Nostr data managed by this NostrManager instance.
    ///
    /// This is a destructive operation that completely removes all stored Nostr data,
    /// including events, messages, relay connections, and cached information. The operation
    /// resets the client to a clean state and wipes the underlying database.
    ///
    /// **⚠️ WARNING: This operation is irreversible and will permanently delete all data.**
    ///
    /// The deletion process includes:
    /// - Resetting the Nostr client and disconnecting from all relays
    /// - Wiping the entire Nostr database, removing all stored events and metadata
    /// - Clearing any cached relay information and connection state
    /// - Removing all locally stored messages and contact data
    ///
    /// This method is typically used during:
    /// - Account deletion workflows
    /// - Application uninstall procedures
    /// - Debug/testing scenarios requiring a clean slate
    /// - Factory reset operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful completion of the deletion process.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The database wipe operation fails due to I/O errors
    /// * File system permissions prevent deletion of database files
    /// * The database is locked by another process
    ///
    /// Note that the client reset operation is infallible and will not cause this method to fail.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // During account deletion
    /// nostr_manager.delete_all_data().await?;
    /// // All Nostr data has been permanently removed
    /// ```
    ///
    /// # Safety
    ///
    /// This method should only be called when you are certain that all Nostr data
    /// should be permanently removed. Consider backing up important data before
    /// calling this method if recovery might be needed.
    pub(crate) async fn delete_all_data(&self) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::delete_all_data",
            "Deleting Nostr data"
        );
        self.client.reset().await;
        self.client.database().wipe().await?;
        Ok(())
    }

    pub async fn fetch_all_user_data(
        &self,
        signer: impl NostrSigner + 'static,
        last_synced: Timestamp,
        group_ids: Vec<String>,
    ) -> Result<()> {
        let pubkey = signer.get_public_key().await?;
        self.client.set_signer(signer).await;

        // Create a filter for all metadata-related events (user metadata and contacts)
        let contacts_pubkeys = self
            .client
            .get_contact_list_public_keys(self.timeout().await?)
            .await?;

        let mut metadata_authors = contacts_pubkeys;
        metadata_authors.push(pubkey);

        let metadata_filter = Filter::new().kind(Kind::Metadata).authors(metadata_authors);

        // Create a filter for all relay-related events
        let relay_filter = Filter::new().author(pubkey).kinds(vec![
            Kind::RelayList,
            Kind::InboxRelays,
            Kind::MlsKeyPackageRelays,
        ]);

        // Create a filter for all MLS-related events
        let mls_filter = Filter::new().author(pubkey).kind(Kind::MlsKeyPackage);

        // Create a filter for gift wrapped events
        let giftwrap_filter = Filter::new().kind(Kind::GiftWrap).pubkey(pubkey);

        // Create a filter for group messages
        let group_messages_filter = Filter::new()
            .kind(Kind::MlsGroupMessage)
            .custom_tags(SingleLetterTag::lowercase(Alphabet::H), group_ids)
            .since(last_synced)
            .until(Timestamp::now());

        // Fetch all events in parallel
        // We don't need to handle the events, they'll be processed in the background by the event processor.
        let (_metadata_events, _relay_events, _mls_events, _giftwrap_events, _group_messages) = tokio::join!(
            self.client
                .fetch_events(metadata_filter, self.timeout().await?),
            self.client
                .fetch_events(relay_filter, self.timeout().await?),
            self.client.fetch_events(mls_filter, self.timeout().await?),
            self.client
                .fetch_events(giftwrap_filter, self.timeout().await?),
            self.client
                .fetch_events(group_messages_filter, self.timeout().await?)
        );

        self.client.unset_signer().await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    #[test]
    fn test_nostr_manager_settings_default() {
        let settings = NostrManagerSettings::default();

        // Test timeout
        assert_eq!(settings.timeout, Duration::from_secs(3));

        // Test that relays are configured
        assert!(!settings.relays.is_empty());

        // Test that debug and release builds have different relay configurations
        if cfg!(debug_assertions) {
            assert!(settings.relays.contains(&"ws://localhost:8080".to_string()));
            assert!(settings.relays.contains(&"ws://localhost:7777".to_string()));
        } else {
            assert!(settings
                .relays
                .contains(&"wss://relay.damus.io".to_string()));
            assert!(settings.relays.contains(&"wss://purplepag.es".to_string()));
            assert!(settings
                .relays
                .contains(&"wss://relay.primal.net".to_string()));
            assert!(settings.relays.contains(&"wss://nos.lol".to_string()));
        }
    }

    #[test]
    fn test_nostr_manager_settings_clone_and_debug() {
        let settings = NostrManagerSettings::default();
        let cloned_settings = settings.clone();

        assert_eq!(settings.timeout, cloned_settings.timeout);
        assert_eq!(settings.relays, cloned_settings.relays);

        // Test Debug implementation
        let debug_str = format!("{:?}", settings);
        assert!(debug_str.contains("NostrManagerSettings"));
        assert!(debug_str.contains("timeout"));
        assert!(debug_str.contains("relays"));
    }

    #[tokio::test]
    async fn test_nostr_manager_new() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let result = NostrManager::new(db_path, tx).await;
        assert!(result.is_ok());

        let manager = result.unwrap();

        // Test that settings are properly initialized
        let settings = manager.settings.lock().await;
        assert_eq!(settings.timeout, Duration::from_secs(3));
        assert!(!settings.relays.is_empty());
    }

    #[tokio::test]
    async fn test_nostr_manager_timeout() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let manager = NostrManager::new(db_path, tx).await.unwrap();
        let timeout = manager.timeout().await.unwrap();

        assert_eq!(timeout, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_nostr_manager_relays() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let manager = NostrManager::new(db_path, tx).await.unwrap();
        let relays = manager.relays().await.unwrap();

        assert!(!relays.is_empty());

        // Verify that all returned relays are valid RelayUrl objects
        for relay in relays {
            assert!(
                relay.to_string().starts_with("ws://") || relay.to_string().starts_with("wss://")
            );
        }
    }

    #[tokio::test]
    async fn test_nostr_manager_clone_and_debug() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let manager = NostrManager::new(db_path, tx).await.unwrap();
        let cloned_manager = manager.clone();

        // Test that cloned manager has the same settings
        let original_timeout = manager.timeout().await.unwrap();
        let cloned_timeout = cloned_manager.timeout().await.unwrap();
        assert_eq!(original_timeout, cloned_timeout);

        // Test Debug implementation
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("NostrManager"));
        assert!(debug_str.contains("settings"));
        assert!(debug_str.contains("client"));
    }

    #[tokio::test]
    async fn test_delete_all_data() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let manager = NostrManager::new(db_path, tx).await.unwrap();

        // Test that delete_all_data succeeds
        let result = manager.delete_all_data().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_nostr_manager_error_display() {
        let secrets_error = NostrManagerError::SecretsStoreError("test error".to_string());
        assert!(secrets_error
            .to_string()
            .contains("Error with secrets store"));

        let queue_error = NostrManagerError::FailedToQueueEvent("test error".to_string());
        assert!(queue_error.to_string().contains("Failed to queue event"));

        let shutdown_error =
            NostrManagerError::FailedToShutdownEventProcessor("test error".to_string());
        assert!(shutdown_error
            .to_string()
            .contains("Failed to shutdown event processor"));

        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            let io_error = NostrManagerError::IoError("test error".to_string());
            assert!(io_error.to_string().contains("I/O error"));
        }

        let account_error = NostrManagerError::AccountError("test error".to_string());
        assert!(account_error.to_string().contains("Account error"));
    }

    #[test]
    fn test_nostr_manager_error_debug() {
        let error = NostrManagerError::SecretsStoreError("test error".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("SecretsStoreError"));
        assert!(debug_str.contains("test error"));
    }

    #[tokio::test]
    async fn test_extract_invite_events_empty() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let manager = NostrManager::new(db_path, tx).await.unwrap();

        // Test with empty vector
        let result = manager.extract_invite_events(vec![]).await;
        assert!(result.is_empty());
    }

    #[test]
    fn test_result_type_alias() {
        // Test that our Result type alias works correctly
        let ok_result: Result<String> = Ok("test".to_string());
        assert!(ok_result.is_ok());
        assert_eq!("test", "test");

        let err_result: Result<String> =
            Err(NostrManagerError::SecretsStoreError("test".to_string()));
        assert!(err_result.is_err());
    }
}
