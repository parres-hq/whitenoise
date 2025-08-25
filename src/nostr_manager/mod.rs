use std::{collections::HashSet, time::Duration};

use ::rand::RngCore;
use nostr_sdk::prelude::*;
use thiserror::Error;
use tokio::sync::mpsc::Sender;

// use crate::media::blossom::BlossomClient;
use crate::{
    types::ProcessableEvent,
    whitenoise::{accounts::Account, database::DatabaseError, relays::Relay, Whitenoise},
};

pub mod parser;
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
}

#[derive(Debug, Clone)]
pub struct NostrManager {
    pub(crate) client: Client,
    session_salt: [u8; 16],
    timeout: Duration,
    // blossom: BlossomClient,
}

pub type Result<T> = std::result::Result<T, NostrManagerError>;

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
    pub async fn new(
        event_sender: Sender<crate::types::ProcessableEvent>,
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
        })
    }

    /// Publishes a Nostr event (which is already signed) to the specified relays.
    ///
    /// This method allows publishing an event to a list of relay URLs. It ensures that the client
    /// is connected to all specified relays before attempting to publish the event.
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
        relays: &[Relay],
    ) -> Result<Output<EventId>> {
        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();
        Ok(self.client.send_event_to(urls, &event).await?)
    }

    /// Publishes a Nostr event using a temporary signer.
    ///
    /// This method allows publishing an event with a signer that is only used for this specific operation.
    /// The signer is set before publishing and unset immediately after. This method also ensures that
    /// the client is connected to all specified relays before attempting to publish.
    ///
    /// Automatically tracks published events in the database by looking up the account from the signer's public key.
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
        event_kind: Kind,
        relays: &[Relay],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        // Get the public key from the signer for account lookup
        let pubkey = signer.get_public_key().await?;

        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();
        self.client.set_signer(signer).await;
        let result = self
            .client
            .send_event_builder_to(urls, event_builder.clone())
            .await?;
        self.client.unset_signer().await;

        // Track the published event if we have a successful result
        if !result.success.is_empty() {
            let event_id = result.id();
            let whitenoise = Whitenoise::get_instance()
                .map_err(|e| NostrManagerError::WhitenoiseInstance(e.to_string()))?;

            // Look up the account by public key
            let account = Account::find_by_pubkey(&pubkey, &whitenoise.database)
                .await
                .map_err(|e| NostrManagerError::WhitenoiseInstance(e.to_string()))?;
            whitenoise
                .record_published_event(event_id, &account, event_kind)
                .await
                .map_err(|e| NostrManagerError::WhitenoiseInstance(e.to_string()))?;
        }

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
        extra_tags: &[Tag],
        relays: &[Relay],
        signer: impl NostrSigner + 'static,
    ) -> Result<Output<EventId>> {
        // Ensure we're connected to all target relays before publishing
        self.ensure_relays_connected(relays).await?;
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();
        let wrapped_event =
            EventBuilder::gift_wrap(&signer, receiver, rumor, extra_tags.to_vec()).await?;
        self.client.set_signer(signer).await;
        let result = self.client.send_event_to(urls, &wrapped_event).await?;
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
        user_relays: &[Relay],
        inbox_relays: &[Relay],
        group_relays: &[Relay],
        nostr_group_ids: &[String],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::setup_account_subscriptions_with_signer",
            "Setting up account subscriptions with signer"
        );
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
        user_relays: &[Relay],
        nostr_group_ids: &[String],
        signer: impl NostrSigner + 'static,
    ) -> Result<()> {
        self.client.set_signer(signer).await;
        let result = self
            .setup_group_messages_subscription(pubkey, nostr_group_ids, user_relays)
            .await;
        self.client.unset_signer().await;
        result
    }

    pub(crate) fn relay_urls_from_event(event: Event) -> HashSet<RelayUrl> {
        event
            .tags
            .into_iter()
            .filter(|tag| Self::is_relay_list_tag_for_event_kind(tag, event.kind))
            .filter_map(|tag| {
                tag.content()
                    .and_then(|content| RelayUrl::parse(content).ok())
            })
            .collect()
    }

    /// Determines if a tag is relevant for the given relay list event kind.
    /// Different relay list kinds use different tag types:
    /// - Kind::RelayList (10002) uses "r" tags (TagKind::SingleLetter)
    /// - Kind::InboxRelays (10050) and Kind::MlsKeyPackageRelays (10051) use "relay" tags (TagKind::Relay)
    fn is_relay_list_tag_for_event_kind(tag: &Tag, kind: Kind) -> bool {
        match kind {
            Kind::RelayList => Self::is_r_tag(tag),
            Kind::InboxRelays | Kind::MlsKeyPackageRelays => Self::is_relay_tag(tag),
            _ => Self::is_relay_tag(tag) || Self::is_r_tag(tag), // backward compatibility
        }
    }

    fn is_r_tag(tag: &Tag) -> bool {
        tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R))
    }

    fn is_relay_tag(tag: &Tag) -> bool {
        tag.kind() == TagKind::Relay
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
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use nostr_sdk::RelayUrl;
    ///
    /// let relay_url = RelayUrl::parse("wss://relay.damus.io")?;
    /// let status = nostr_manager.get_relay_status(&relay_url).await?;
    ///
    /// match status {
    ///     RelayStatus::Connected => println!("Relay is connected"),
    ///     RelayStatus::Disconnected => println!("Relay is disconnected"),
    ///     RelayStatus::Connecting => println!("Relay is connecting"),
    ///     _ => println!("Relay status: {:?}", status),
    /// }
    /// ```
    ///
    /// # Notes
    ///
    /// - This method only checks the status of relays that have been previously
    ///   added to the client's relay pool
    /// - The status reflects the real-time connection state and may change
    ///   frequently as network conditions vary
    /// - Use this method for monitoring relay health and connection diagnostics
    pub(crate) async fn get_relay_status(&self, relay_url: &RelayUrl) -> Result<RelayStatus> {
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
    pub(crate) async fn ensure_relays_connected(&self, relays: &[Relay]) -> Result<()> {
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

        for relay in relays.iter() {
            // Check if we're already connected to this relay by attempting to get its status
            match self.client.relay(relay.url.clone()).await {
                Ok(_) => {
                    // Relay already exists in the client, skip
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::ensure_relays_connected",
                        "Relay {} already connected",
                        relay.url
                    );
                }
                Err(_) => {
                    // Relay not found in client, add it
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::ensure_relays_connected",
                        "Adding new relay: {}",
                        relay.url
                    );

                    match self.client.add_relay(relay.url.clone()).await {
                        Ok(_) => {
                            newly_added_relays.push(relay.url.clone());
                            tracing::debug!(
                                target: "whitenoise::nostr_manager::ensure_relays_connected",
                                "Successfully added relay: {}",
                                relay.url
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "whitenoise::nostr_manager::ensure_relays_connected",
                                "Failed to add relay {}: {}",
                                relay.url,
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
        }

        self.client.connect().await;

        tracing::debug!(
            target: "whitenoise::nostr_manager::ensure_relays_connected",
            "Relay connection ensuring completed"
        );

        Ok(())
    }

    /// Syncs all user data from the Nostr network for an account and their contacts.
    ///
    /// This method performs a comprehensive data synchronization by fetching and processing
    /// various types of Nostr events for the specified account and all users in their contact list.
    /// It streams events in parallel and processes them through the appropriate handlers.
    ///
    /// # Data Types Synchronized
    ///
    /// - **Metadata events** (kind 0): User profile information for the account and contacts
    /// - **Relay list events** (kinds 10002, 10050, 10051): NIP-65 relay lists, inbox relays, and MLS key package relays
    /// - **Gift wrap events** (kind 1059): Private messages directed to the account
    /// - **Group messages** (kind 444): MLS group messages for specified groups since last sync
    ///
    /// # Arguments
    ///
    /// * `signer` - A Nostr signer implementation for authenticating with relays
    /// * `account` - The account to sync data for (includes contact list lookup)
    /// * `group_ids` - Vector of hex-encoded group IDs to fetch group messages for
    ///
    /// # Process Flow
    ///
    /// 1. **Authentication**: Sets the signer on the Nostr client
    /// 2. **Contact Discovery**: Fetches the account's contact list from the network
    /// 3. **Filter Creation**: Creates targeted filters for each event type
    /// 4. **Parallel Streaming**: Initiates concurrent event streams with 10-second timeout
    /// 5. **Event Processing**: Processes each event type through appropriate handlers:
    ///    - Metadata → `handle_metadata()`
    ///    - Relay lists → `handle_relay_list()`
    ///    - Gift wraps → `handle_giftwrap()`
    ///    - Group messages → `handle_mls_message()`
    /// 6. **Cleanup**: Unsets the signer when complete
    ///
    /// # Time-based Filtering
    ///
    /// Group messages are filtered using the account's `last_synced_at` timestamp to only
    /// fetch new messages since the last synchronization, improving efficiency.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if synchronization completes successfully, even if individual
    /// events fail to process (errors are logged but don't halt the overall sync).
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - Failed to get contact list public keys from the network
    /// - Failed to create event streams
    /// - Critical event processing errors occur
    /// - Whitenoise instance is not available
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
    /// nostr_manager.sync_all_user_data(keys, &account, group_ids).await?;
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// - Uses streaming to handle large result sets efficiently
    /// - Parallel event fetching improves overall sync time
    /// - 10-second timeout per stream prevents hanging on slow relays
    /// - Incremental sync for group messages reduces bandwidth usage
    ///
    /// # Security Notes
    ///
    /// - The signer is automatically unset after completion to prevent key leakage
    /// - Gift wrap events are filtered specifically for the account's public key
    /// - Contact list access requires proper authentication via the signer
    pub(crate) async fn sync_all_user_data(
        &self,
        signer: impl NostrSigner + 'static,
        account: &Account,
        group_ids: Vec<String>,
    ) -> Result<()> {
        self.client.set_signer(signer).await;

        let result = async {
            let mut contacts_and_self =
                match self.client.get_contact_list_public_keys(self.timeout).await {
                    Ok(contacts) => contacts,
                    Err(e) => {
                        tracing::error!(
                            target: "whitenoise::nostr_manager::fetch_all_user_data_to_nostr_cache",
                            "Failed to get contact list public keys: {}",
                            e
                        );
                        return Err(NostrManagerError::Client(e));
                    }
                };
            contacts_and_self.push(account.pubkey);

            let metadata_filter = Filter::new()
                .authors(contacts_and_self.clone())
                .kinds(vec![Kind::Metadata]);
            let relay_filter = Filter::new().authors(contacts_and_self).kinds(vec![
                Kind::RelayList,
                Kind::InboxRelays,
                Kind::MlsKeyPackageRelays,
            ]);
            let giftwrap_filter = Filter::new().kind(Kind::GiftWrap).pubkey(account.pubkey);
            let group_messages_filter = Filter::new()
                .kind(Kind::MlsGroupMessage)
                .custom_tags(SingleLetterTag::lowercase(Alphabet::H), group_ids)
                .since(Timestamp::from(
                    account.last_synced_at.unwrap_or_default().timestamp() as u64,
                ));

            let timeout_duration = Duration::from_secs(10);

            let mut metadata_events = self
                .client
                .stream_events(metadata_filter, timeout_duration)
                .await?;
            let mut relay_events = self
                .client
                .stream_events(relay_filter, timeout_duration)
                .await?;
            let mut giftwrap_events = self
                .client
                .stream_events(giftwrap_filter, timeout_duration)
                .await?;
            let mut group_messages = self
                .client
                .stream_events(group_messages_filter, timeout_duration)
                .await?;

            let whitenoise = Whitenoise::get_instance()
                .map_err(|e| NostrManagerError::EventProcessingError(e.to_string()))?;

            while let Some(event) = metadata_events.next().await {
                whitenoise
                    .handle_metadata(event)
                    .await
                    .map_err(|e| NostrManagerError::EventProcessingError(e.to_string()))?;
            }

            while let Some(event) = relay_events.next().await {
                whitenoise
                    .handle_relay_list(event)
                    .await
                    .map_err(|e| NostrManagerError::EventProcessingError(e.to_string()))?;
            }

            while let Some(event) = giftwrap_events.next().await {
                whitenoise
                    .handle_giftwrap(account, event)
                    .await
                    .map_err(|e| NostrManagerError::EventProcessingError(e.to_string()))?;
            }

            while let Some(event) = group_messages.next().await {
                whitenoise
                    .handle_mls_message(account, event)
                    .await
                    .map_err(|e| NostrManagerError::EventProcessingError(e.to_string()))?;
            }

            Ok(())
        }
        .await;

        self.client.unset_signer().await;

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Test data for problematic contact list
    fn get_test_contact_list_event() -> Event {
        let json = r#"{
            "kind": 3,
            "id": "ebdd64bb88ad560aaf949f9c2fc7a5a7bba82100f5767dd4a6422a4cef646951",
            "pubkey": "991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702",
            "created_at": 1752141958,
            "tags": [
              ["e", "25e5c82273a271cb1a840d0060391a0bf4965cafeb029d5ab55350b418953fbb"],
              ["e", "42224859763652914db53052103f0b744df79dfc4efef7e950fc0802fc3df3c5"],
              ["alt", "Follow List"],
              ["p", "e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad"],
              ["p", "c2827524936dedad5f623bcf8a04d201f3fd3ed7d4912a190dbeef685f45b2f7"],
              ["p", "eba7c2b111a28fa8e7cb07f1ae0feef490d49d897bd7b1fb5ce5d3f0d6739e8f"],
              ["p", "ef151c7a380f40a75d7d1493ac347b6777a9d9b5fa0aa3cddb47fc78fab69a8b"],
              ["p", "234c45ff85a31c19bf7108a747fa7be9cd4af95c7d621e07080ca2d663bb47d2"],
              ["p", "8664ff363efcd36a154efdcbc629a4d1e4c511f9114e1d35de73fff31cb783b3"],
              ["p", "6e468422dfb74a5738702a8823b9b28168abab8655faacb6853cd0ee15deee93"],
              ["p", "aac07d95089ce6adf08b9156d43c1a4ab594c6130b7dcb12ec199008c5819a2f"]
            ],
            "content": "{\"wss://nostr.bitcoiner.social/\":{\"read\":true,\"write\":true},\"wss://relay.nostr.bg/\":{\"read\":true,\"write\":true},\"wss://nostr.oxtr.dev/\":{\"read\":true,\"write\":true},\"wss://nostr.fmt.wiz.biz/\":{\"read\":true,\"write\":false},\"wss://relay.damus.io/\":{\"read\":true,\"write\":true},\"wss://nostr.mom/\":{\"read\":true,\"write\":true},\"wss://nos.lol/\":{\"read\":true,\"write\":true},\"wss://nostr.wine/\":{\"read\":true,\"write\":false},\"wss://relay.nostr.band/\":{\"read\":true,\"write\":false},\"wss://relay.noswhere.com/\":{\"read\":true,\"write\":false}}",
            "sig": "8c174dbb1d88065c3d34a4f40d15eda1160a3f041f29e87f881afb44058d8e5405fe02db63655903925f439f64445409b2acad62e059ac9c152e7442972f6ede"
        }"#;

        serde_json::from_str(json).unwrap()
    }

    // Helper function to simulate contact list extraction from NostrManager implementation
    fn extract_contacts_from_event(event: &Event) -> Vec<PublicKey> {
        event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
            .collect()
    }

    #[test]
    fn test_contact_list_with_mixed_tags() {
        let event = get_test_contact_list_event();

        // Count tags by type
        let e_tags = event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::Custom("e".into()))
            .count();
        let p_tags = event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::p())
            .count();
        let alt_tags = event
            .tags
            .iter()
            .filter(|tag| tag.kind() == TagKind::Custom("alt".into()))
            .count();

        // Verify tag counts
        assert_eq!(e_tags, 2);
        assert_eq!(p_tags, 8);
        assert_eq!(alt_tags, 1);

        // Now extract contacts
        let contacts = extract_contacts_from_event(&event);

        // Verify we only get the p tags as contacts
        assert_eq!(contacts.len(), 8);
    }

    #[test]
    fn test_contact_list_with_relay_preferences() {
        let event = get_test_contact_list_event();

        // Verify content contains relay preferences
        assert!(event.content.contains("wss://"));
        assert!(event.content.contains("read"));
        assert!(event.content.contains("write"));

        // Extract contacts - should work despite complex content
        let contacts = extract_contacts_from_event(&event);
        assert_eq!(contacts.len(), 8);

        // Check specific contacts
        let expected_pubkey =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        assert!(contacts.contains(&expected_pubkey));
    }

    #[test]
    fn test_contact_list_with_future_timestamp() {
        let event = get_test_contact_list_event();
        let timestamp = Timestamp::from(1752141958);

        // The event timestamp was from the future when this test was written,
        // but it might not be in the future anymore as time passes
        // Uncomment to check if it's still in future:
        // let current_timestamp = Timestamp::now();
        // println!("Event timestamp: {}, Current time: {}", event.created_at, current_timestamp);

        // Check that we can parse and process events with timestamps from the far future
        // Regardless of whether that time has now passed
        let contacts = extract_contacts_from_event(&event);
        assert_eq!(contacts.len(), 8);

        // Verify we extracted the correct timestamp from the event
        assert_eq!(event.created_at, timestamp);
    }

    #[tokio::test]
    async fn test_create_contact_list_hashmap() {
        let event = get_test_contact_list_event();
        let contacts_pubkeys = extract_contacts_from_event(&event);
        assert_eq!(contacts_pubkeys.len(), 8);

        // Create the HashMap as done in fetch_user_contact_list
        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        for contact in contacts_pubkeys {
            contacts_metadata.insert(contact, None);
        }

        // Verify HashMap was created correctly
        assert_eq!(contacts_metadata.len(), 8);

        // Check specific contacts
        let test_pubkey =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        assert!(contacts_metadata.contains_key(&test_pubkey));
        assert!(contacts_metadata.get(&test_pubkey).unwrap().is_none());
    }

    #[tokio::test]
    async fn test_mock_query_user_contact_list() {
        // We don't need the temp dir and channels for this test, so we'll skip them

        // Mock the database query to return our test event
        let event = get_test_contact_list_event();

        // Simulate the logic of query_user_contact_list
        let contacts_pubkeys = if let Some(event) = Some(&event) {
            event
                .tags
                .iter()
                .filter(|tag| tag.kind() == TagKind::p())
                .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
                .collect::<Vec<PublicKey>>()
        } else {
            vec![]
        };

        // Create the contact metadata HashMap
        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        for contact in contacts_pubkeys {
            contacts_metadata.insert(contact, None);
        }

        // Verify results
        assert_eq!(contacts_metadata.len(), 8);

        // Check for specific contact
        let test_pubkey =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        assert!(contacts_metadata.contains_key(&test_pubkey));
    }

    #[tokio::test]
    async fn test_handle_duplicate_contacts() {
        // Create a contact list with duplicate p tags
        let contact1 =
            PublicKey::from_hex("e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad")
                .unwrap();
        let contact2 =
            PublicKey::from_hex("c2827524936dedad5f623bcf8a04d201f3fd3ed7d4912a190dbeef685f45b2f7")
                .unwrap();

        // Create a mock event with duplicate contacts
        let event_json = format!(
            r#"{{
            "kind": 3,
            "id": "ebdd64bb88ad560aaf949f9c2fc7a5a7bba82100f5767dd4a6422a4cef646951",
            "pubkey": "991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702",
            "created_at": 1752141958,
            "tags": [
              ["p", "{}"],
              ["p", "{}"],
              ["p", "{}"],
              ["e", "25e5c82273a271cb1a840d0060391a0bf4965cafeb029d5ab55350b418953fbb"],
              ["alt", "Follow List"]
            ],
            "content": "{{}}",
            "sig": "8c174dbb1d88065c3d34a4f40d15eda1160a3f041f29e87f881afb44058d8e5405fe02db63655903925f439f64445409b2acad62e059ac9c152e7442972f6ede"
        }}"#,
            contact1.to_hex(),
            contact2.to_hex(),
            contact1.to_hex()
        );

        let event: Event = serde_json::from_str(&event_json).unwrap();

        // Extract contacts
        let contacts = extract_contacts_from_event(&event);

        // Check for duplicate contacts
        let unique_contacts: std::collections::HashSet<_> = contacts.iter().cloned().collect();

        // We should have duplicates in the original list
        assert_eq!(contacts.len(), 3);
        assert_eq!(unique_contacts.len(), 2);

        // Count occurrences of each contact
        let contact1_count = contacts.iter().filter(|&c| *c == contact1).count();
        let contact2_count = contacts.iter().filter(|&c| *c == contact2).count();

        assert_eq!(contact1_count, 2); // Duplicate should be counted twice in the original list
        assert_eq!(contact2_count, 1);

        // Now create HashMap to check how duplicates are handled there
        let mut contacts_metadata: HashMap<PublicKey, Option<Metadata>> = HashMap::new();
        for contact in contacts {
            contacts_metadata.insert(contact, None);
        }

        // Verify HashMap has the right count (deduplicated)
        assert_eq!(contacts_metadata.len(), 2);
        assert!(contacts_metadata.contains_key(&contact1));
        assert!(contacts_metadata.contains_key(&contact2));
    }

    #[test]
    fn test_contact_list_is_parseable() {
        // Test that we can correctly parse the event JSON
        let event_json = r#"{
            "kind": 3,
            "id": "ebdd64bb88ad560aaf949f9c2fc7a5a7bba82100f5767dd4a6422a4cef646951",
            "pubkey": "991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702",
            "created_at": 1752141958,
            "tags": [
              ["e", "25e5c82273a271cb1a840d0060391a0bf4965cafeb029d5ab55350b418953fbb"],
              ["e", "42224859763652914db53052103f0b744df79dfc4efef7e950fc0802fc3df3c5"],
              ["alt", "Follow List"],
              ["p", "e5e4557e6eb9c63bdf8ce7d2082ed543fa433c468d1d25374a97320be6d3b1ad"],
              ["p", "c2827524936dedad5f623bcf8a04d201f3fd3ed7d4912a190dbeef685f45b2f7"]
            ],
            "content": "{\"wss://relay.example.com\":{\"read\":true,\"write\":true}}",
            "sig": "8c174dbb1d88065c3d34a4f40d15eda1160a3f041f29e87f881afb44058d8e5405fe02db63655903925f439f64445409b2acad62e059ac9c152e7442972f6ede"
        }"#;

        let event: Event = serde_json::from_str(event_json).unwrap();

        // Check that event fields are correctly parsed
        assert_eq!(event.kind, Kind::ContactList);
        assert_eq!(
            event.pubkey,
            PublicKey::from_hex("991896cee597dd975c3b87266981387498bffa408fad05dc1ad578269805b702")
                .unwrap()
        );
        assert_eq!(event.created_at.as_u64(), 1752141958);

        // Check that tags are correctly parsed
        assert_eq!(event.tags.len(), 5);

        // Extract contacts
        let contacts = extract_contacts_from_event(&event);
        assert_eq!(contacts.len(), 2);
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_relay_list() {
        use nostr_sdk::prelude::*;

        // Test Kind::RelayList (10002) with "r" tags
        let keys = Keys::generate();

        let r_tags = vec![
            Tag::reference("wss://relay1.example.com"),
            Tag::reference("wss://relay2.example.com"),
            // Add a relay tag that should be ignored for RelayList
            Tag::custom(TagKind::Relay, ["wss://should-be-ignored.com"]),
        ];

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(r_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://relay1.example.com").unwrap()));
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://relay2.example.com").unwrap()));
        assert!(!parsed_relays.contains(&RelayUrl::parse("wss://should-be-ignored.com").unwrap()));
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_inbox_relays() {
        use nostr_sdk::prelude::*;

        // Test Kind::InboxRelays (10050) with "relay" tags
        let keys = Keys::generate();

        let relay_tags = vec![
            Tag::custom(TagKind::Relay, ["wss://inbox1.example.com"]),
            Tag::custom(TagKind::Relay, ["wss://inbox2.example.com"]),
            // Add an "r" tag that should be ignored for InboxRelays
            Tag::reference("wss://should-be-ignored.com"),
        ];

        let event = EventBuilder::new(Kind::InboxRelays, "")
            .tags(relay_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://inbox1.example.com").unwrap()));
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://inbox2.example.com").unwrap()));
        assert!(!parsed_relays.contains(&RelayUrl::parse("wss://should-be-ignored.com").unwrap()));
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_key_package_relays() {
        use nostr_sdk::prelude::*;

        // Test Kind::MlsKeyPackageRelays (10051) with "relay" tags
        let keys = Keys::generate();

        let relay_tags = vec![
            Tag::custom(TagKind::Relay, ["wss://keypackage1.example.com"]),
            Tag::custom(TagKind::Relay, ["wss://keypackage2.example.com"]),
            // Add an "r" tag that should be ignored for MlsKeyPackageRelays
            Tag::reference("wss://should-be-ignored.com"),
        ];

        let event = EventBuilder::new(Kind::MlsKeyPackageRelays, "")
            .tags(relay_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://keypackage1.example.com").unwrap()));
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://keypackage2.example.com").unwrap()));
        assert!(!parsed_relays.contains(&RelayUrl::parse("wss://should-be-ignored.com").unwrap()));
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_unknown_kind_backward_compatibility() {
        use nostr_sdk::prelude::*;

        // Test unknown kind with both "r" and "relay" tags (backward compatibility)
        let keys = Keys::generate();

        let mixed_tags = vec![
            Tag::reference("wss://r-tag-relay.example.com"),
            Tag::custom(TagKind::Relay, ["wss://relay-tag-relay.example.com"]),
        ];

        let event = EventBuilder::new(Kind::Custom(9999), "")
            .tags(mixed_tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://r-tag-relay.example.com").unwrap()));
        assert!(
            parsed_relays.contains(&RelayUrl::parse("wss://relay-tag-relay.example.com").unwrap())
        );
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_invalid_urls_filtered() {
        use nostr_sdk::prelude::*;

        // Test that invalid URLs are filtered out
        let keys = Keys::generate();

        let tags = vec![
            Tag::reference("wss://valid-relay.example.com"),
            Tag::reference("not a valid url"),
            Tag::reference("wss://another-valid.example.com"),
        ];

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(event);

        assert_eq!(parsed_relays.len(), 2);
        assert!(parsed_relays.contains(&RelayUrl::parse("wss://valid-relay.example.com").unwrap()));
        assert!(
            parsed_relays.contains(&RelayUrl::parse("wss://another-valid.example.com").unwrap())
        );
    }

    #[tokio::test]
    async fn test_relay_urls_from_event_empty_tags() {
        use nostr_sdk::prelude::*;

        // Test event with no relay tags
        let keys = Keys::generate();

        let tags = vec![
            Tag::custom(TagKind::Custom("alt".into()), ["Some description"]),
            Tag::custom(TagKind::Custom("d".into()), ["identifier"]),
        ];

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .sign(&keys)
            .await
            .unwrap();

        let parsed_relays = NostrManager::relay_urls_from_event(event);
        assert!(parsed_relays.is_empty());
    }
}
