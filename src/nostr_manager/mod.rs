// use crate::media::blossom::BlossomClient;
use crate::types::{NostrEncryptionMethod, ProcessableEvent};

use nostr_sdk::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc::Sender, Mutex};

pub mod fetch;
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
            relays.push("wss://purplepag.es".to_string());
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

        // Connect to the default relays
        client.connect().await;

        // Set up notification handler - forward events directly to Whitenoise
        if let Err(e) = client
            .handle_notifications(move |notification| {
                let sender = event_sender.clone();
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
                            Ok(false)
                        }
                        RelayPoolNotification::Shutdown => {
                            tracing::debug!(
                                target: "whitenoise::nostr_client::handle_notifications",
                                "Relay pool shutdown"
                            );
                            Ok(true)
                        }
                        _ => {
                            // Ignore other notification types
                            Ok(false)
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
}
