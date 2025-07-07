// use crate::media::blossom::BlossomClient;
use crate::types::{NostrEncryptionMethod, ProcessableEvent};

use ::rand::RngCore;
use nostr_sdk::prelude::*;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc::Sender, Mutex};

pub mod parser;
pub mod query;
// pub mod search;
pub mod subscriptions;
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
    #[error("Failed to connect to any relays")]
    NoRelayConnections,
    #[error("Nostr Event error: {0}")]
    NostrEventBuilderError(#[from] nostr::event::builder::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
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
    #[allow(dead_code)] // Allow dead code because this triggers a warning when linting on linux.
    db_path: PathBuf,
    session_salt: [u8; 16],
    // blossom: BlossomClient,
}

impl Default for NostrManagerSettings {
    fn default() -> Self {
        let mut relays = vec![];
        if cfg!(debug_assertions) {
            relays.push("ws://localhost:8080".to_string());
            relays.push("ws://localhost:7777".to_string());
        } else {
            relays.push("wss://relay.damus.io".to_string());
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
    /// * `connect_to_relays` - Whether to attempt connecting to relays (false for testing)
    async fn new(
        db_path: PathBuf,
        event_sender: Sender<crate::types::ProcessableEvent>,
        connect_to_relays: bool,
    ) -> Result<Self> {
        let opts = ClientOptions::default();

        // Initialize the client with the appropriate database based on platform
        let client = {
            #[cfg(any(target_os = "ios", target_os = "macos"))]
            {
                let full_path = db_path.join("nostr_ndb");
                let db = NdbDatabase::open(full_path.to_str().expect("Invalid path"))
                    .expect("Failed to open Nostr database");
                Client::builder().database(db).opts(opts).build()
            }

            #[cfg(not(any(target_os = "ios", target_os = "macos")))]
            {
                let full_path = db_path.join("nostr_lmdb");
                let db = NostrLMDB::open(full_path).expect("Failed to open Nostr database");
                Client::builder().database(db).opts(opts).build()
            }
        };

        let settings = NostrManagerSettings::default();

        // Generate a random session salt
        let mut session_salt = [0u8; 16];
        ::rand::rng().fill_bytes(&mut session_salt);

        // Add the default relays
        for relay in &settings.relays {
            client.add_relay(relay).await?;
        }
        // Add the purplepag.es relay as discovery only
        client
            .add_discovery_relay("wss://purplepag.es".to_string())
            .await?;

        // Connect to relays if requested
        if connect_to_relays {
            tracing::debug!(
                target: "whitenoise::nostr_manager::new",
                "Connecting to relays..."
            );
            client.connect().await;
        } else {
            tracing::debug!(
                target: "whitenoise::nostr_manager::new",
                "Created NostrManager without connecting to relays (connect_to_relays=false)"
            );
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
                                        if let Err(_e) = sender
                                            .send(ProcessableEvent::new_nostr_event(
                                                event.as_ref().clone(),
                                                Some(subscription_id.to_string()),
                                            ))
                                            .await
                                        {
                                            // SendError only occurs when channel is closed, so exit gracefully
                                            tracing::debug!(
                                                target: "whitenoise::nostr_client::handle_notifications",
                                                "Event channel closed, exiting notification handler"
                                            );
                                            return Ok(true); // Exit notification loop
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

                                        if let Err(_e) = sender
                                            .send(ProcessableEvent::RelayMessage(relay_url, message_str))
                                            .await
                                        {
                                            // SendError only occurs when channel is closed, so exit gracefully
                                            tracing::debug!(
                                                target: "whitenoise::nostr_client::handle_notifications",
                                                "Message channel closed, exiting notification handler"
                                            );
                                            return Ok(true); // Exit notification loop
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
            settings: Arc::new(Mutex::new(settings)),
            db_path,
            session_salt,
        })
    }

    /// Create a new Nostr manager with relay connections (for production use)
    pub async fn new_with_connections(
        db_path: PathBuf,
        event_sender: Sender<crate::types::ProcessableEvent>,
    ) -> Result<Self> {
        Self::new(db_path, event_sender, true).await
    }

    /// Create a new Nostr manager without attempting to connect to relays (for testing)
    #[cfg(test)]
    pub async fn new_without_connection(
        db_path: PathBuf,
        event_sender: Sender<crate::types::ProcessableEvent>,
    ) -> Result<Self> {
        Self::new(db_path, event_sender, false).await
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

    /// Fetch an event (first from database, then from relays) with a filter
    pub(crate) async fn fetch_events_with_filter(&self, filter: Filter) -> Result<Events> {
        let events = self.client.database().query(filter.clone()).await?;
        if events.is_empty() {
            let events = self
                .client
                .fetch_events(filter, self.timeout().await.unwrap())
                .await?;
            Ok(events)
        } else {
            Ok(events)
        }
    }

    /// Publishes a Nostr event (which is already signed) to the specified relays.
    ///
    /// This method allows publishing an event to a list of relay URLs. It uses the client's
    /// built-in relay handling to send the event to the specified relays.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to publish
    /// * `relays` - The list of relay URLs to publish the event to
    ///
    /// # Returns
    ///
    /// * `Result<Output<EventId>>` - The published event ID if successful, or an error if publishing fails
    pub(crate) async fn publish_event_to(
        &self,
        event: Event,
        relays: &BTreeSet<RelayUrl>,
    ) -> Result<Output<EventId>> {
        Ok(self.client.send_event_to(relays, &event).await?)
    }

    /// Publishes a Nostr event using a temporary signer.
    ///
    /// This method allows publishing an event with a signer that is only used for this specific operation.
    /// The signer is set before publishing and unset immediately after. This method also ensures that
    /// the client is connected to all specified relays before attempting to publish.
    ///
    /// # Arguments
    ///
    /// * `event_builder` - The event builder containing the event to publish
    /// * `relays` - The list of relay URLs to publish the event to
    /// * `signer` - A signer that implements `NostrSigner` and has a 'static lifetime
    ///
    /// # Returns
    ///
    /// * `Result<Output<EventId>>` - The published event ID if successful, or an error if publishing fails
    pub(crate) async fn publish_event_builder_with_signer(
        &self,
        event_builder: EventBuilder,
        relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;

        self.client.set_signer(signer).await;
        let result = self
            .client
            .send_event_builder_to(relays, event_builder.clone())
            .await?;
        self.client.unset_signer().await;
        Ok(result)
    }

    /// Constructs and publishes a Nostr gift wrap event using a temporary signer.
    ///
    /// This method creates a gift-wrapped Nostr event and publishes it to specified relays using a
    /// temporary signer. Gift wrapping provides privacy by encrypting the inner event (rumor) and
    /// hiding the recipient's identity from relay operators and other observers.
    ///
    /// The signer is set before publishing and automatically unset immediately after the operation
    /// completes, ensuring it doesn't persist in the client state. This method also ensures that
    /// the client is connected to all specified relays before attempting to publish.
    ///
    /// # Arguments
    ///
    /// * `receiver` - The public key of the intended recipient of the gift wrapped message
    /// * `rumor` - The `UnsignedEvent` that will be encrypted and wrapped inside the gift wrap
    /// * `extra_tags` - Additional tags to include in the gift wrap event for metadata or routing
    /// * `relays` - The specific relay URLs where the gift wrap event should be published
    /// * `signer` - A signer that implements `NostrSigner` and has a 'static lifetime
    ///
    /// # Returns
    ///
    /// * `Result<Output<EventId>>` - The published event ID if successful, or an error if publishing fails
    ///
    /// # Privacy Notes
    ///
    /// Gift wrapping provides the following privacy benefits:
    /// - The inner event content is encrypted and only readable by the receiver
    /// - The receiver's identity is hidden from relay operators
    /// - Metadata about the communication is minimized
    pub(crate) async fn publish_gift_wrap_with_signer(
        &self,
        receiver: &PublicKey,
        rumor: UnsignedEvent,
        extra_tags: Vec<Tag>,
        relays: &[RelayUrl],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;

        let wrapped_event = EventBuilder::gift_wrap(&signer, receiver, rumor, extra_tags).await?;

        self.client.set_signer(signer).await;
        let result = self.client.send_event_to(relays, &wrapped_event).await?;
        self.client.unset_signer().await;
        Ok(result)
    }

    /// Sets up account subscriptions using a temporary signer.
    ///
    /// This method allows setting up subscriptions with a signer that is only used for this specific operation.
    /// The signer is set before subscription setup and unset immediately after.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the account to set up subscriptions for
    /// * `user_relays` - The relays to use for subscriptions
    /// * `nostr_group_ids` - Group IDs for MLS message subscriptions
    /// * `signer` - A signer that implements `NostrSigner` and has a 'static lifetime
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success if subscriptions were set up, or an error if setup fails
    pub(crate) async fn setup_account_subscriptions_with_signer(
        &self,
        pubkey: PublicKey,
        user_relays: Vec<RelayUrl>,
        inbox_relays: Vec<RelayUrl>,
        group_relays: Vec<RelayUrl>,
        nostr_group_ids: Vec<String>,
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        self.client.set_signer(signer).await;
        let result = self
            .setup_account_subscriptions(
                pubkey,
                user_relays,
                inbox_relays,
                group_relays,
                nostr_group_ids,
            )
            .await;
        self.client.unset_signer().await;
        result
    }

    pub(crate) async fn setup_group_messages_subscriptions_with_signer(
        &self,
        pubkey: PublicKey,
        user_relays: Vec<RelayUrl>,
        nostr_group_ids: Vec<String>,
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        self.client.set_signer(signer).await;
        let result = self
            .setup_group_messages_subscription(pubkey, nostr_group_ids, user_relays)
            .await;
        self.client.unset_signer().await;
        result
    }

    /// Updates the metadata subscription for a user's contacts using a temporary signer.
    ///
    /// This method allows updating the metadata subscription for a user's contacts with a signer
    /// that is only used for this specific operation. The signer is set before subscription setup
    /// and unset immediately after.
    ///
    /// The method performs the following operations:
    /// 1. Sets the provided signer for the client
    /// 2. Sets up a subscription to receive metadata updates for the user's contacts
    /// 3. Unsets the signer after the operation is complete
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the user whose contacts' metadata should be subscribed to
    /// * `user_relays` - The list of relay URLs to use for the subscription
    /// * `signer` - A signer that implements `NostrSigner` and has a 'static lifetime
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success if the subscription was updated, or an error if the operation fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let pubkey = PublicKey::from_hex("...").unwrap();
    /// let relays = vec![RelayUrl::parse("wss://relay.example.com").unwrap()];
    /// let signer = MySigner::new();
    /// nostr_manager.update_contacts_metadata_subscription_with_signer(pubkey, relays, signer).await?;
    /// ```
    pub(crate) async fn update_contacts_metadata_subscription_with_signer(
        &self,
        pubkey: PublicKey,
        user_relays: Vec<RelayUrl>,
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        self.client.set_signer(signer).await;
        let result = self
            .setup_contacts_metadata_subscription(pubkey, user_relays)
            .await;
        self.client.unset_signer().await;
        result
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
        self.client.unset_signer().await;
        self.client.unsubscribe_all().await;

        // Handle database wiping differently based on platform
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            // On macOS/iOS, we need to delete the database files directly
            // since NdbDatabase doesn't support the wipe method
            let database_path = self.db_path.join("nostr_ndb");

            // Remove the database directory
            if database_path.exists() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::delete_all_data",
                    "Removing NDB database directory: {:?}",
                    database_path
                );

                // Use tokio's async filesystem operations
                if let Err(e) = tokio::fs::remove_dir_all(&database_path).await {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::delete_all_data",
                        "Failed to remove NDB database directory: {:?}",
                        e
                    );
                    return Err(NostrManagerError::IoError(e.to_string()));
                }

                // Recreate the empty directory
                if let Err(e) = tokio::fs::create_dir_all(&database_path).await {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::delete_all_data",
                        "Failed to recreate NDB database directory: {:?}",
                        e
                    );
                    return Err(NostrManagerError::IoError(e.to_string()));
                }
            }
        }

        #[cfg(not(any(target_os = "ios", target_os = "macos")))]
        {
            // On other platforms, use the wipe method
            self.client.database().wipe().await?;
        }
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

    /// Expose session_salt for use in subscriptions
    pub fn session_salt(&self) -> &[u8; 16] {
        &self.session_salt
    }

    /// Get the status of a specific relay
    pub async fn get_relay_status(&self, relay_url: &RelayUrl) -> Result<RelayStatus> {
        let relay = self.client.relay(relay_url).await?;
        Ok(relay.status())
    }

    /// Ensures that the client is connected to all the specified relays.
    ///
    /// This method checks each relay URL in the provided list and adds it to the client's
    /// relay pool if it's not already connected. It then attempts to establish connections
    /// to any newly added relays.
    ///
    /// This is essential for subscription setup and event publishing to work correctly,
    /// as the nostr-sdk client needs to be connected to relays before it can subscribe
    /// to them or publish events to them.
    ///
    /// # Arguments
    ///
    /// * `relays` - A slice of `RelayUrl` objects representing the relays to ensure connections to.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all relays are successfully added and connection attempts are made.
    /// Note that individual relay connection failures are logged but don't cause this method to fail,
    /// as partial connectivity is often acceptable.
    ///
    /// # Errors
    ///
    /// Returns a `NostrManagerError` if:
    /// * Adding a relay to the client fails due to invalid URL format
    /// * Client configuration errors prevent relay addition
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user_relays = vec![
    ///     RelayUrl::parse("wss://relay.damus.io").unwrap(),
    ///     RelayUrl::parse("wss://nos.lol").unwrap(),
    /// ];
    /// nostr_manager.ensure_relays_connected(&user_relays).await?;
    /// // Now safe to call client.subscribe_with_id_to(user_relays, ...)
    /// ```
    pub(crate) async fn ensure_relays_connected(&self, relays: &[RelayUrl]) -> Result<()> {
        if relays.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            target: "whitenoise::nostr_manager::ensure_relays_connected",
            "Ensuring connection to {} relays",
            relays.len()
        );

        // Track newly added relays for connection
        let mut newly_added_relays = Vec::new();

        for relay_url in relays {
            // Check if we're already connected to this relay by attempting to get its status
            match self.client.relay(relay_url).await {
                Ok(_) => {
                    // Relay already exists in the client, skip
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::ensure_relays_connected",
                        "Relay {} already connected",
                        relay_url
                    );
                }
                Err(_) => {
                    // Relay not found in client, add it
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::ensure_relays_connected",
                        "Adding new relay: {}",
                        relay_url
                    );

                    match self.client.add_relay(relay_url).await {
                        Ok(_) => {
                            newly_added_relays.push(relay_url.clone());
                            tracing::debug!(
                                target: "whitenoise::nostr_manager::ensure_relays_connected",
                                "Successfully added relay: {}",
                                relay_url
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "whitenoise::nostr_manager::ensure_relays_connected",
                                "Failed to add relay {}: {}",
                                relay_url,
                                e
                            );
                            // Continue with other relays rather than failing completely
                        }
                    }
                }
            }
        }

        // If we added any new relays, trigger connection to establish them
        if !newly_added_relays.is_empty() {
            tracing::debug!(
                target: "whitenoise::nostr_manager::ensure_relays_connected",
                "Connecting to {} newly added relays",
                newly_added_relays.len()
            );

            // The connect() method is async but we don't wait for full connection
            // as subscription setup should work even with partially connected relays
            tokio::spawn({
                let client = self.client.clone();
                async move {
                    client.connect().await;
                }
            });
        }

        tracing::debug!(
            target: "whitenoise::nostr_manager::ensure_relays_connected",
            "Relay connection ensuring completed"
        );

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

        let result = NostrManager::new_without_connection(db_path, tx).await;
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

        let manager = NostrManager::new_without_connection(db_path, tx)
            .await
            .unwrap();
        let timeout = manager.timeout().await.unwrap();

        assert_eq!(timeout, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_nostr_manager_relays() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_path_buf();
        let (tx, _rx) = mpsc::channel(10);

        let manager = NostrManager::new_without_connection(db_path, tx)
            .await
            .unwrap();
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

        let manager = NostrManager::new_without_connection(db_path, tx)
            .await
            .unwrap();
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

        let manager = NostrManager::new_without_connection(db_path, tx)
            .await
            .unwrap();

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

        let manager = NostrManager::new_without_connection(db_path, tx)
            .await
            .unwrap();

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
