use std::pin::Pin;
use std::time::Duration;

use ::rand::RngCore;
use futures::Stream;
use nostr_sdk::prelude::*;
use thiserror::Error;
use tokio::sync::mpsc::Sender;

// use crate::media::blossom::BlossomClient;
use crate::{
    types::ProcessableEvent,
    whitenoise::{database::DatabaseError, event_tracker::EventTracker},
};

pub mod parser;
pub mod publisher;
pub mod query;
pub mod subscriptions;
pub mod utils;

#[derive(Error, Debug)]
pub enum NostrManagerError {
    #[error("Whitenoise Instance Error: {0}")]
    WhitenoiseInstance(String),
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
    #[error("Account error: {0}")]
    AccountError(String),
    #[error("Failed to connect to any relays")]
    NoRelayConnections,
    #[error("Nostr Event error: {0}")]
    NostrEventBuilderError(#[from] nostr_sdk::event::builder::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Event processing error: {0}")]
    EventProcessingError(String),
    #[error("Failed to track published event: {0}")]
    FailedToTrackPublishedEvent(String),
}

#[derive(Clone)]
pub struct NostrManager {
    pub(crate) client: Client,
    session_salt: [u8; 16],
    timeout: Duration,
    pub(crate) event_tracker: std::sync::Arc<dyn EventTracker>,
    signer_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    // blossom: BlossomClient,
}

pub type Result<T> = std::result::Result<T, NostrManagerError>;

/// Container for different types of event streams fetched during user data synchronization.
/// Each stream contains events of a specific type that need to be processed separately.
pub struct UserEventStreams {
    /// Stream of metadata events (kind 0) for the account and contacts
    pub metadata_events: Pin<Box<dyn Stream<Item = Event> + Send>>,
    /// Stream of relay list events (kinds 10002, 10050, 10051)
    pub relay_events: Pin<Box<dyn Stream<Item = Event> + Send>>,
    /// Stream of gift wrap events (kind 1059) directed to the account
    pub giftwrap_events: Pin<Box<dyn Stream<Item = Event> + Send>>,
    /// Stream of group message events (kind 444) for specified groups
    pub group_messages: Pin<Box<dyn Stream<Item = Event> + Send>>,
}

impl NostrManager {
    /// Default timeout for client requests
    pub(crate) fn default_timeout() -> Duration {
        Duration::from_secs(5)
    }
    /// Create a new Nostr manager
    ///
    /// # Arguments
    ///
    /// * `event_sender` - Channel sender for forwarding events to Whitenoise for processing
    /// * `timeout` - Timeout for client requests
    pub(crate) async fn new(
        event_sender: Sender<crate::types::ProcessableEvent>,
        event_tracker: std::sync::Arc<dyn EventTracker>,
        timeout: Duration,
    ) -> Result<Self> {
        let opts = ClientOptions::default();

        let client = { Client::builder().opts(opts).build() };

        // Generate a random session salt
        let mut session_salt = [0u8; 16];
        ::rand::rng().fill_bytes(&mut session_salt);

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
            session_salt,
            timeout,
            event_tracker,
            signer_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// Reusable helper to execute operations with a temporary signer.
    ///
    /// This helper ensures that the signer is always unset after the operation completes,
    /// even if the operation returns early or encounters an error.
    async fn with_signer<F, Fut, T>(&self, signer: impl NostrSigner + 'static, f: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>> + Send,
    {
        let _guard = self.signer_lock.lock().await;
        self.client.set_signer(signer).await;
        let result = f().await;
        self.client.unset_signer().await;
        result
    }

    /// Sets up account subscriptions using a temporary signer.
    ///
    /// This method allows setting up subscriptions with a signer that is only used for this specific operation.
    /// The signer is set before subscription setup and unset immediately after.
    pub(crate) async fn setup_account_subscriptions_with_signer(
        &self,
        pubkey: PublicKey,
        user_relays: &[RelayUrl],
        inbox_relays: &[RelayUrl],
        group_relays: &[RelayUrl],
        nostr_group_ids: &[String],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_account_subscriptions_with_signer",
            "Setting up account subscriptions with signer"
        );
        self.with_signer(signer, || async {
            self.setup_account_subscriptions(
                pubkey,
                user_relays,
                inbox_relays,
                group_relays,
                nostr_group_ids,
                None,
            )
            .await
        })
        .await
    }

    pub(crate) async fn setup_group_messages_subscriptions_with_signer(
        &self,
        pubkey: PublicKey,
        group_relays: &[RelayUrl],
        nostr_group_ids: &[String],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_group_messages_subscriptions_with_signer",
            "Setting up group messages subscriptions with signer"
        );
        self.with_signer(signer, || async {
            self.ensure_relays_connected(group_relays).await?;
            self.setup_group_messages_subscription(pubkey, nostr_group_ids, group_relays, None)
                .await
        })
        .await
    }

    /// Updates account subscriptions by clearing from all relays first, then setting up new ones.
    ///
    /// This is necessary when relay sets change, as NIP-01 automatic replacement only works
    /// within the same relay. Without explicit cleanup, old relays would keep orphaned subscriptions.
    ///
    /// Uses a time buffer to prevent missing events during the update window.
    pub(crate) async fn update_account_subscriptions_with_signer(
        &self,
        pubkey: PublicKey,
        user_relays: &[RelayUrl],
        inbox_relays: &[RelayUrl],
        group_relays: &[RelayUrl],
        nostr_group_ids: &[String],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::update_account_subscriptions_with_signer",
            "Updating account subscriptions with cleanup for relay changes"
        );
        let buffer_time = Timestamp::now() - Duration::from_secs(10);
        self.unsubscribe_account_subscriptions(&pubkey).await?;
        self.with_signer(signer, || async {
            self.setup_account_subscriptions(
                pubkey,
                user_relays,
                inbox_relays,
                group_relays,
                nostr_group_ids,
                Some(buffer_time),
            )
            .await
        })
        .await
    }

    /// Ensures that the signer is unset and all subscriptions are cleared.
    pub(crate) async fn delete_all_data(&self) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::delete_all_data",
            "Deleting Nostr data"
        );
        self.client.unset_signer().await;
        self.client.unsubscribe_all().await;
        Ok(())
    }

    /// Expose session_salt for use in subscriptions
    pub(crate) fn session_salt(&self) -> &[u8; 16] {
        &self.session_salt
    }

    /// Retrieves the current connection status of a specific relay.
    ///
    /// This method queries the Nostr client's relay pool to get the current status
    /// of a relay connection. The status indicates whether the relay is connected,
    /// disconnected, connecting, or in an error state.
    ///
    /// # Arguments
    ///
    /// * `relay_url` - The `RelayUrl` of the relay to check the status for
    ///
    /// # Returns
    ///
    /// Returns `Ok(RelayStatus)` with the current status of the relay connection.
    /// The `RelayStatus` enum includes variants such as:
    /// - `Connected` - The relay is successfully connected and operational
    /// - `Disconnected` - The relay is not connected
    /// - `Connecting` - A connection attempt is in progress
    /// - Other status variants depending on the relay's state
    ///
    /// # Errors
    ///
    /// Returns a `NostrManagerError` if:
    /// - The relay URL is not found in the client's relay pool
    /// - There's an error retrieving the relay instance from the client
    /// - The client is in an invalid state
    pub(crate) async fn get_relay_status(&self, relay_url: &RelayUrl) -> Result<RelayStatus> {
        let relay = self.client.relay(relay_url).await?;
        Ok(relay.status())
    }

    /// Ensures that the client is connected to all the specified relay URLs.
    ///
    /// This method checks each relay URL in the provided list and adds it to the client's
    /// relay pool if it's not already connected. It then attempts to establish connections
    /// to any newly added relays.
    ///
    /// This is essential for subscription setup and event publishing to work correctly,
    /// as the nostr-sdk client needs to be connected to relays before it can subscribe
    /// to them or publish events to them.
    pub(crate) async fn ensure_relays_connected(&self, relay_urls: &[RelayUrl]) -> Result<()> {
        if relay_urls.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            target: "whitenoise::nostr_manager::ensure_relays_connected",
            "Ensuring connection to {} relay URLs",
            relay_urls.len()
        );

        let relay_futures = relay_urls
            .iter()
            .map(|relay_url| self.ensure_relay_in_client(relay_url));
        futures::future::join_all(relay_futures).await;

        self.client.connect().await;

        tracing::debug!(
            target: "whitenoise::nostr_manager::ensure_relays_connected",
            "Relay connections ensuring completed"
        );

        Ok(())
    }

    /// Ensures that the client is connected to the specified relay URL.
    async fn ensure_relay_in_client(&self, relay_url: &RelayUrl) -> Result<()> {
        match self.client.relay(relay_url).await {
            Ok(_) => {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::ensure_relays_connected",
                    "Relay {} already connected",
                    relay_url
                );
                Ok(())
            }
            Err(_) => {
                // Relay not found in client, add it
                tracing::debug!(
                    target: "whitenoise::nostr_manager::ensure_relays_connected",
                    "Adding new relay: {}",
                    relay_url
                );

                match self.client.add_relay(relay_url.clone()).await {
                    Ok(_) => {
                        tracing::debug!(
                            target: "whitenoise::nostr_manager::ensure_relays_connected",
                            "Successfully added relay: {}",
                            relay_url
                        );
                        Ok(())
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "whitenoise::nostr_manager::ensure_relays_connected",
                            "Failed to add relay {}: {}",
                            relay_url,
                            e
                        );
                        Err(NostrManagerError::Client(e))
                    }
                }
            }
        }
    }

    /// Fetches event streams for all user data types from the Nostr network.
    ///
    /// This method creates and returns streams for different types of events related to
    /// an account and their contacts. The streams can then be processed separately,
    /// allowing for better separation of concerns between data fetching and processing.
    ///
    /// # Data Types Fetched
    ///
    /// - **Metadata events** (kind 0): User profile information for the account and contacts
    /// - **Relay list events** (kinds 10002, 10050, 10051): NIP-65 relay lists, inbox relays, and MLS key package relays
    /// - **Gift wrap events** (kind 1059): Private messages directed to the account
    /// - **Group messages** (kind 444): MLS group messages for specified groups since last sync
    ///
    /// # Arguments
    ///
    /// * `signer` - A Nostr signer implementation for authenticating with relays
    /// * `account` - The account to fetch data for (includes contact list lookup)
    /// * `group_ids` - Vector of hex-encoded group IDs to fetch group messages for
    ///
    /// # Returns
    ///
    /// Returns `UserEventStreams` containing separate streams for each event type.
    /// These streams can be processed independently.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - Failed to get contact list public keys from the network
    /// - Failed to create event streams
    /// - Authentication with the signer fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use nostr_sdk::Keys;
    /// use whitenoise::accounts::Account;
    ///
    /// let keys = Keys::generate();
    /// let group_ids = vec!["abc123".to_string(), "def456".to_string()];
    ///
    /// let streams = nostr_manager.fetch_user_event_streams(keys, &account, group_ids).await?;
    /// // Process the streams separately...
    /// ```
    pub(crate) async fn fetch_user_event_streams(
        &self,
        signer: impl NostrSigner + 'static,
        pubkey: PublicKey,
        contact_list_pubkeys: Vec<PublicKey>,
        since: Timestamp,
        group_ids: Vec<String>,
    ) -> Result<UserEventStreams> {
        let (metadata_events, relay_events, giftwrap_events, group_messages) =
            self
            .with_signer(signer, || async {

                let mut contacts_and_self = contact_list_pubkeys;
                contacts_and_self.push(pubkey);

                let metadata_filter = Filter::new()
                    .authors(contacts_and_self.clone())
                    .kinds(vec![Kind::Metadata])
                    .since(since);
                let relay_filter = Filter::new()
                    .authors(contacts_and_self.clone())
                    .kinds(vec![
                        Kind::RelayList,
                        Kind::InboxRelays,
                        Kind::MlsKeyPackageRelays,
                    ])
                    .since(since);
                let giftwrap_filter = Filter::new()
                    .kind(Kind::GiftWrap)
                    .pubkey(pubkey)
                    .since(since);
                let group_messages_filter = Filter::new()
                    .kind(Kind::MlsGroupMessage)
                    .custom_tags(SingleLetterTag::lowercase(Alphabet::H), group_ids)
                    .since(since);

                let timeout_duration = Duration::from_secs(10);

                let (
                    metadata_events,
                    relay_events,
                    giftwrap_events,
                    group_messages,
                ) = tokio::try_join!(
                    self.client.stream_events(metadata_filter, timeout_duration),
                    self.client.stream_events(relay_filter, timeout_duration),
                    self.client.stream_events(giftwrap_filter, timeout_duration),
                    self.client
                        .stream_events(group_messages_filter, timeout_duration)
                )?;
                Ok((
                    metadata_events,
                    relay_events,
                    giftwrap_events,
                    group_messages,
                ))
            })
            .await?;

        Ok(UserEventStreams {
            metadata_events: Box::pin(metadata_events),
            relay_events: Box::pin(relay_events),
            giftwrap_events: Box::pin(giftwrap_events),
            group_messages: Box::pin(group_messages),
        })
    }
}
