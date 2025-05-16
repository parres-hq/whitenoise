use crate::accounts::Account;
use crate::media::blossom::BlossomClient;
use crate::nostr_manager::event_processor::EventProcessor;
use crate::relays::RelayType;
use crate::types::NostrEncryptionMethod;
use crate::Whitenoise;
use nostr_sdk::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use thiserror::Error;
use tokio::{spawn, sync::Mutex};

#[cfg(any(target_os = "ios", target_os = "macos"))]
use std::path::Path;

pub mod event_processor;
pub mod fetch;
pub mod parser;
pub mod query;
pub mod search;
pub mod subscriptions;
pub mod sync;

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
    #[error("Tauri error: {0}")]
    TauriError(#[from] tauri::Error),
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
    pub blossom_server: String,
}

#[derive(Debug, Clone)]
pub struct NostrManager {
    pub client: Client,
    pub blossom: BlossomClient,
    pub settings: Arc<Mutex<NostrManagerSettings>>,
    event_processor: Arc<Mutex<EventProcessor>>,
}

impl Default for NostrManagerSettings {
    fn default() -> Self {
        let mut relays = vec![];
        if cfg!(dev) {
            relays.push("ws://localhost:8080".to_string());
            relays.push("ws://localhost:7777".to_string());
            relays.push("wss://purplepag.es".to_string());
            // relays.push("wss://nos.lol".to_string());
        } else {
            relays.push("wss://relay.damus.io".to_string());
            relays.push("wss://relay.primal.net".to_string());
        }

        Self {
            timeout: Duration::from_secs(3),
            relays,
            blossom_server: if cfg!(dev) {
                "http://localhost:3000".to_string()
            } else {
                "https://blossom.primal.net".to_string()
            },
        }
    }
}
pub type Result<T> = std::result::Result<T, NostrManagerError>;

impl NostrManager {
    pub async fn new(db_path: PathBuf, app_handle: AppHandle) -> Result<Self> {
        let opts = Options::default();

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

        let blossom = BlossomClient::new(&settings.blossom_server);

        // Add the default relays
        for relay in &settings.relays {
            client.add_relay(relay).await?;
        }

        // Connect to the default relays
        client.connect().await;

        let event_processor = Arc::new(Mutex::new(EventProcessor::new(app_handle)));

        Ok(Self {
            client,
            blossom,
            settings: Arc::new(Mutex::new(settings)),
            event_processor,
        })
    }

    pub async fn timeout(&self) -> Result<Duration> {
        let guard = self.settings.lock().await;
        Ok(guard.timeout)
    }

    pub async fn relays(&self) -> Result<Vec<String>> {
        let guard = self.settings.lock().await;
        Ok(guard.relays.clone())
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

    pub async fn set_nostr_identity(
        &self,
        account: &Account,
        wn: tauri::State<'_, Whitenoise>,
        app_handle: &tauri::AppHandle,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Starting Nostr identity update for {}",
            account.pubkey
        );

        let keys = account
            .keys(wn.clone())
            .map_err(|e| NostrManagerError::SecretsStoreError(e.to_string()))?;

        // Shutdown existing event processor
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Shutting down existing event processor"
        );
        self.event_processor
            .lock()
            .await
            .clear_queue()
            .await
            .map_err(|e| NostrManagerError::FailedToShutdownEventProcessor(e.to_string()))?;

        // Reset the client
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Resetting client"
        );

        self.client.reset().await;

        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Client reset complete"
        );

        // Set the new signer
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Setting new signer"
        );
        self.client.set_signer(keys.clone()).await;

        // Add the default relays
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Adding default relays"
        );
        for relay in self.relays().await? {
            self.client.add_relay(relay).await?;
        }

        // Connect to the default relays
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Connecting to default relays"
        );
        self.client.connect().await;

        // We only want to connect to user relays in release mode
        if !cfg!(dev) {
            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Setting up user-specific relays"
            );

            // Get currently connected relays to avoid duplicate connections
            let connected_relays = self
                .client
                .relays()
                .await
                .keys()
                .map(|url| url.to_string())
                .collect::<std::collections::HashSet<String>>();

            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Already connected to relays: {:?}",
                connected_relays
            );

            // 1. Try to get relays from account object (cached in database)
            // 2. If none found, try to query from local database
            // 3. If still none found, fetch from network

            // Handle standard Nostr relays
            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Getting user's standard relays"
            );
            let mut relays = account
                .relays(RelayType::Nostr, wn.clone())
                .await
                .map_err(|e| NostrManagerError::AccountError(e.to_string()))?;
            if relays.is_empty() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "No cached relays found, trying query_user_relays"
                );
                relays = self.query_user_relays(keys.public_key()).await?;
            }
            if relays.is_empty() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "No relays found via query, trying fetch_user_relays"
                );
                relays = self.fetch_user_relays(keys.public_key()).await?;
            }

            for relay in relays.iter() {
                if !connected_relays.contains(relay) {
                    self.client.add_relay(relay).await?;
                    self.client.connect_relay(relay).await?;
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Connected to user relay: {}",
                        relay
                    );
                }
            }

            // Handle inbox relays
            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Getting user's inbox relays"
            );
            let mut inbox_relays = account
                .relays(RelayType::Inbox, wn.clone())
                .await
                .map_err(|e| NostrManagerError::AccountError(e.to_string()))?;
            if inbox_relays.is_empty() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "No cached inbox relays found, trying query_user_inbox_relays"
                );
                inbox_relays = self.query_user_inbox_relays(keys.public_key()).await?;
            }
            if inbox_relays.is_empty() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "No inbox relays found via query, trying fetch_user_inbox_relays"
                );
                inbox_relays = self.fetch_user_inbox_relays(keys.public_key()).await?;
            }

            for relay in inbox_relays.iter() {
                if !connected_relays.contains(relay) {
                    self.client.add_read_relay(relay).await?;
                    self.client.connect_relay(relay).await?;
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Connected to user inbox relay: {}",
                        relay
                    );
                }
            }

            // Handle key package relays
            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Getting user's key package relays"
            );
            let mut key_package_relays = account
                .relays(RelayType::KeyPackage, wn.clone())
                .await
                .map_err(|e| NostrManagerError::AccountError(e.to_string()))?;
            if key_package_relays.is_empty() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "No cached key package relays found, trying query_user_key_package_relays"
                );
                key_package_relays = self
                    .query_user_key_package_relays(keys.public_key())
                    .await?;
            }
            if key_package_relays.is_empty() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "No key package relays found via query, trying fetch_user_key_package_relays"
                );
                key_package_relays = self
                    .fetch_user_key_package_relays(keys.public_key())
                    .await?;
            }

            for relay in key_package_relays.iter() {
                if !connected_relays.contains(relay) {
                    self.client.add_relay(relay).await?;
                    self.client.connect_relay(relay).await?;
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Connected to user key package relay: {}",
                        relay
                    );
                }
            }
        }

        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Connected to relays: {:?}",
            self.client
                .relays()
                .await
                .keys()
                .map(|url| url.to_string())
                .collect::<Vec<_>>()
        );

        // Create and store new processor
        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Creating new event processor"
        );
        let new_processor = EventProcessor::new(app_handle.clone());
        *self.event_processor.lock().await = new_processor;

        // Spawn two tasks in parallel:
        // 1. Setup subscriptions to catch future events
        // 2. Fetch past events
        let app_handle_clone_subs = app_handle.clone();
        let account_clone_subs = account.clone();
        spawn(async move {
            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Starting subscriptions"
            );
            let wn_state = app_handle_clone_subs.state::<Whitenoise>();

            let group_ids = account_clone_subs
                .nostr_group_ids(wn_state.clone())
                .await
                .expect("Couldn't get nostr group ids");

            match wn_state
                .nostr
                .setup_subscriptions(account_clone_subs.pubkey, group_ids)
                .await
            {
                Ok(_) => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Subscriptions setup completed"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Error subscribing to events: {}",
                        e
                    );
                }
            }
        });

        let app_handle_clone_fetch = app_handle.clone();
        let pubkey = account.pubkey;
        let last_synced = account.last_synced;
        spawn(async move {
            tracing::debug!(
                target: "whitenoise::nostr_manager::set_nostr_identity",
                "Starting fetch for {}",
                pubkey
            );
            let wn_state = app_handle_clone_fetch.state::<Whitenoise>();

            let group_ids = Account::find_by_pubkey(&pubkey, wn_state.clone())
                .await
                .expect("Couldn't get account")
                .nostr_group_ids(wn_state.clone())
                .await
                .expect("Couldn't get nostr group ids");

            match &wn_state
                .nostr
                .fetch_for_user(pubkey, last_synced, group_ids)
                .await
            {
                Ok(_) => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Fetch completed for {}",
                        pubkey
                    );
                    // Update last_synced through a new database query
                    if let Ok(mut account) =
                        Account::find_by_pubkey(&pubkey, wn_state.clone()).await
                    {
                        account.last_synced = Timestamp::now();
                        if let Err(e) = account.save(wn_state.clone()).await {
                            tracing::error!(
                                target: "whitenoise::nostr_manager::set_nostr_identity",
                                "Error updating last_synced: {}",
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::set_nostr_identity",
                        "Error in fetch: {}",
                        e
                    );
                }
            }
        });

        Ok(())
    }

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

    fn relay_urls_from_events(events: Events) -> Vec<String> {
        events
            .into_iter()
            .flat_map(|e| e.tags)
            .filter(|tag| tag.kind() == TagKind::Relay)
            .map_while(|tag| tag.content().map(|c| c.to_string()))
            .collect()
    }

    pub async fn delete_all_data(
        &self,
        #[cfg(any(target_os = "ios", target_os = "macos"))] data_dir: &Path,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::delete_all_data",
            "Deleting Nostr data"
        );
        self.client.reset().await;

        // Handle database wiping differently based on platform
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            // On macOS/iOS, we need to delete the database files directly
            // since NdbDatabase doesn't support the wipe method
            let db_path = data_dir.join("nostr_ndb");

            // Remove the database directory
            if db_path.exists() {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::delete_all_data",
                    "Removing NDB database directory: {:?}",
                    db_path
                );

                // Use tokio's async filesystem operations
                if let Err(e) = tokio::fs::remove_dir_all(&db_path).await {
                    tracing::error!(
                        target: "whitenoise::nostr_manager::delete_all_data",
                        "Failed to remove NDB database directory: {:?}",
                        e
                    );
                    return Err(NostrManagerError::IoError(e.to_string()));
                }

                // Recreate the empty directory
                if let Err(e) = tokio::fs::create_dir_all(&db_path).await {
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
}
