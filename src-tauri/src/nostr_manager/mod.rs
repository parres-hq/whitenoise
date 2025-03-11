use crate::accounts::Account;
use crate::nostr_manager::event_processor::EventProcessor;
use crate::types::NostrEncryptionMethod;
use crate::Whitenoise;
use nostr_sdk::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use thiserror::Error;
use tokio::{spawn, sync::Mutex};

pub mod event_processor;
pub mod fetch;
pub mod query;
pub mod search;
pub mod subscriptions;
pub mod sync;

#[derive(Error, Debug)]
pub enum NostrManagerError {
    #[error("Client Error: {0}")]
    Client(#[from] nostr_sdk::client::Error),
    #[error("Metadata Error: {0}")]
    Metadata(#[from] nostr_sdk::types::metadata::Error),
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
}

#[derive(Debug, Clone)]
pub struct NostrManagerSettings {
    pub timeout: Duration,
    pub relays: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NostrManager {
    pub client: Client,
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
            relays.push("wss://purplepag.es".to_string());
            relays.push("wss://relay.primal.net".to_string());
            relays.push("wss://nostr.oxtr.dev".to_string());
        }

        Self {
            timeout: Duration::from_secs(5),
            relays,
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

        // Add the default relays
        for relay in &settings.relays {
            client.add_relay(relay).await?;
        }

        // Connect to the default relays
        client.connect().await;

        let event_processor = Arc::new(Mutex::new(EventProcessor::new(app_handle)));

        Ok(Self {
            client,
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
        self.event_processor
            .lock()
            .await
            .clear_queue()
            .await
            .map_err(|e| NostrManagerError::FailedToShutdownEventProcessor(e.to_string()))?;

        // Reset the client
        self.client.reset().await.map_err(NostrManagerError::from)?;

        // Set the new signer
        self.client.set_signer(keys.clone()).await;

        // Add the default relays
        for relay in self.relays().await? {
            self.client.add_relay(relay).await?;
        }

        // Connect to the default relays
        self.client.connect().await;

        // We only want to connect to user relays in release mode
        if !cfg!(dev) {
            // Add the new user's relays
            // TODO: We should query first and only fetch if we don't have them
            let relays = self.fetch_user_relays(keys.public_key()).await?;
            for relay in relays.iter() {
                self.client.add_relay(relay).await?;
                self.client.connect_relay(relay).await?;
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "Connected to user relay: {}",
                    relay
                );
            }

            // Add the new user's inbox relays
            // TODO: We should query first and only fetch if we don't have them
            let inbox_relays = self.fetch_user_inbox_relays(keys.public_key()).await?;
            for relay in inbox_relays.iter() {
                self.client.add_read_relay(relay).await?;
                self.client.connect_relay(relay).await?;
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "Connected to user inbox relay: {}",
                    relay
                );
            }

            // Add the new user's key package relays
            // TODO: We should query first and only fetch if we don't have them
            let key_package_relays = self
                .fetch_user_key_package_relays(keys.public_key())
                .await?;
            for relay in key_package_relays.iter() {
                self.client.add_relay(relay).await?;
                self.client.connect_relay(relay).await?;
                tracing::debug!(
                    target: "whitenoise::nostr_manager::set_nostr_identity",
                    "Connected to user key package relay: {}",
                    relay
                );
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

        tracing::debug!(
            target: "whitenoise::nostr_manager::set_nostr_identity",
            "Nostr identity updated and connected to relays"
        );

        // Create and store new processor
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
                        "Subscriptions shutdown triggered"
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

    pub async fn delete_all_data(&self) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::delete_all_data",
            "Deleting Nostr data"
        );
        self.client.reset().await.map_err(NostrManagerError::from)?;
        self.client
            .database()
            .wipe()
            .await
            .map_err(NostrManagerError::from)?;
        Ok(())
    }
}
