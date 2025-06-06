pub use crate::accounts::{Account, AccountSettings, OnboardingState};
use crate::database::Database;
pub use crate::error::{Result, WhitenoiseError};
use crate::nostr_manager::NostrManager;
use crate::types::ProcessableEvent;

use anyhow::Context;
use nostr_sdk::prelude::*;
use once_cell::sync::OnceCell;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::EnvFilter, fmt::Layer, prelude::*, registry::Registry};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod accounts;
mod api;
mod database;
mod error;
// mod key_packages;
mod nostr_manager;
mod relays;
// mod media;
mod secrets_store;
mod types;

static TRACING_GUARDS: OnceCell<Mutex<Option<(WorkerGuard, WorkerGuard)>>> = OnceCell::new();
static TRACING_INIT: OnceCell<()> = OnceCell::new();

fn init_tracing(logs_dir: &std::path::Path) {
    TRACING_INIT.get_or_init(|| {
        let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("whitenoise")
            .filename_suffix("log")
            .build(logs_dir)
            .expect("Failed to create file appender");

        let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);
        let (non_blocking_stdout, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());

        TRACING_GUARDS
            .set(Mutex::new(Some((file_guard, stdout_guard))))
            .ok();

        let stdout_layer = Layer::new()
            .with_writer(non_blocking_stdout)
            .with_ansi(true)
            .with_target(true);

        let file_layer = Layer::new()
            .with_writer(non_blocking_file)
            .with_ansi(false)
            .with_target(true);

        Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(stdout_layer)
            .with(file_layer)
            .init();
    });
}

#[derive(Clone, Debug)]
pub struct WhitenoiseConfig {
    /// Directory for application data
    pub data_dir: PathBuf,

    /// Directory for application logs
    pub logs_dir: PathBuf,
}

impl WhitenoiseConfig {
    pub fn new(data_dir: &Path, logs_dir: &Path) -> Self {
        let env_suffix = if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        };
        let formatted_data_dir = data_dir.join(env_suffix);
        let formatted_logs_dir = logs_dir.join(env_suffix);

        Self {
            data_dir: formatted_data_dir,
            logs_dir: formatted_logs_dir,
        }
    }
}

pub struct Whitenoise {
    pub config: WhitenoiseConfig,
    pub accounts: HashMap<PublicKey, Account>,
    pub active_account: Option<PublicKey>,
    pub(crate) database: Arc<Database>,
    pub(crate) nostr: NostrManager,
    event_sender: Sender<ProcessableEvent>,
    shutdown_sender: Sender<()>,
}

impl Whitenoise {
    /// Initializes the Whitenoise application with the provided configuration.
    ///
    /// This method sets up the necessary data and log directories, configures logging,
    /// initializes the database, and sets up the Nostr client with appropriate relays
    /// based on the build environment (development or release).
    ///
    /// # Arguments
    ///
    /// * `config` - A [`WhitenoiseConfig`] struct specifying the data and log directories.
    ///
    /// # Returns
    ///
    /// Returns a [`Result`] containing a fully initialized [`Whitenoise`] instance on success,
    /// or a [`WhitenoiseError`] if initialization fails at any step.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The data or log directories cannot be created.
    /// - Logging cannot be set up.
    /// - The database cannot be initialized.
    /// - The Nostr client cannot be configured or fails to connect to relays.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use whitenoise::{Whitenoise, WhitenoiseConfig};
    /// # use std::path::Path;
    /// # async fn example() -> Result<(), whitenoise::WhitenoiseError> {
    /// let config = WhitenoiseConfig::new(Path::new("./data"), Path::new("./logs"));
    /// let whitenoise = Whitenoise::initialize_whitenoise(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn initialize_whitenoise(config: WhitenoiseConfig) -> Result<Self> {
        let data_dir = &config.data_dir;
        let logs_dir = &config.logs_dir;

        // Setup directories
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {:?}", data_dir))
            .map_err(WhitenoiseError::from)?;
        std::fs::create_dir_all(logs_dir)
            .with_context(|| format!("Failed to create logs directory: {:?}", logs_dir))
            .map_err(WhitenoiseError::from)?;

        // Only initialize tracing once
        init_tracing(logs_dir);

        tracing::debug!("Logging initialized in directory: {:?}", logs_dir);

        let database = Arc::new(Database::new(data_dir.join("whitenoise.sqlite")).await?);

        // Create event processing channels
        let (event_sender, event_receiver) = mpsc::channel(500);
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        // Create NostrManager with event_sender for direct event queuing
        let nostr = NostrManager::new(data_dir.join("nostr_lmdb"), event_sender.clone()).await?;

        // TODO: Load accounts from database

        // Create Whitenoise instance
        let mut whitenoise = Self {
            config,
            database,
            nostr,
            accounts: HashMap::new(),
            active_account: None,
            event_sender,
            shutdown_sender,
        };

        // Start the event processing loop only when not running tests
        if !cfg!(test) {
            whitenoise
                .start_event_processing_loop(event_receiver, shutdown_receiver)
                .await;
        }

        // Return fully configured, ready-to-go instance
        Ok(whitenoise)
    }

    /// Deletes all application data, including the database, MLS data, and log files.
    ///
    /// This asynchronous method removes all persistent data associated with the Whitenoise instance.
    /// It deletes the nostr cache, database, MLS-related directories, and all log files. If the MLS directory exists,
    /// it is removed and then recreated as an empty directory. This is useful for resetting the application
    /// to a clean state.
    ///
    /// # Returns
    ///
    /// Returns a [`Result`] which is `Ok(())` if all data is successfully deleted, or an error boxed as
    /// [`Box<dyn std::error::Error>`] if any step fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The Nostr cache cannot be deleted.
    /// - The database data cannot be deleted.
    /// - The MLS directory cannot be removed or recreated.
    /// - Log files or directories cannot be deleted.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use whitenoise::{Whitenoise, WhitenoiseConfig};
    /// # use std::path::Path;
    /// # async fn example(whitenoise: Whitenoise) -> Result<(), whitenoise::WhitenoiseError> {
    /// whitenoise.delete_all_data().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_all_data(&self) -> Result<()> {
        tracing::debug!(target: "whitenoise::delete_all_data", "Deleting all data");

        // Remove nostr cache first
        self.nostr.delete_all_data().await?;

        // Remove database (accounts and media) data
        self.database.delete_all_data().await?;

        // Remove MLS related data
        // TODO: MOVE TO ACCOUNTS
        // {
        //     let mut nostr_mls = self.nostr_mls.lock().unwrap_or_else(|e| {
        //         tracing::error!("Failed to lock nostr_mls: {:?}", e);
        //         panic!("Mutex poisoned: {}", e);
        //     });

        //     if let Some(_mls) = nostr_mls.as_mut() {
        //         // Close the current MLS instance
        //         *nostr_mls = None;
        //     }
        // }

        // Remove MLS related data
        let mls_dir = self.config.data_dir.join("mls");
        if mls_dir.exists() {
            tracing::debug!(
                target: "whitenoise::delete_all_data",
                "Removing MLS directory: {:?}",
                mls_dir
            );
            tokio::fs::remove_dir_all(&mls_dir).await?;
            // Recreate the empty directory
            tokio::fs::create_dir_all(&mls_dir).await?;
        }

        // Remove logs
        if self.config.logs_dir.exists() {
            for entry in std::fs::read_dir(&self.config.logs_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    std::fs::remove_file(path)?;
                } else if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                }
            }
        }

        Ok(())
    }

    /// Start the event processing loop in a background task
    async fn start_event_processing_loop(
        &mut self,
        receiver: Receiver<ProcessableEvent>,
        shutdown_receiver: Receiver<()>,
    ) {
        tokio::spawn(async move {
            Self::process_events(receiver, shutdown_receiver).await;
        });
    }

    /// Queue an event for processing
    pub async fn queue_event(&self, event: Event, subscription_id: Option<String>) -> Result<()> {
        match self
            .event_sender
            .send(ProcessableEvent::NostrEvent(event, subscription_id))
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(WhitenoiseError::Other(anyhow::anyhow!(
                "Failed to queue event: {}",
                e
            ))),
        }
    }

    /// Queue a relay message for processing
    pub async fn queue_message(&self, relay_url: RelayUrl, message_str: String) -> Result<()> {
        match self
            .event_sender
            .send(ProcessableEvent::RelayMessage(relay_url, message_str))
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(WhitenoiseError::Other(anyhow::anyhow!(
                "Failed to queue message: {}",
                e
            ))),
        }
    }

    /// Shutdown event processing gracefully
    pub async fn shutdown_event_processing(&self) -> Result<()> {
        match self.shutdown_sender.send(()).await {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Expected if processor already shut down
        }
    }

    /// Extract the account pubkey from a subscription_id
    /// Subscription IDs follow the format: {pubkey}_{subscription_type}
    fn extract_pubkey_from_subscription_id(subscription_id: &str) -> Option<PublicKey> {
        if let Some(underscore_pos) = subscription_id.find('_') {
            let pubkey_str = &subscription_id[..underscore_pos];
            PublicKey::parse(pubkey_str).ok()
        } else {
            None
        }
    }

    /// Main event processing loop
    async fn process_events(mut receiver: Receiver<ProcessableEvent>, mut shutdown: Receiver<()>) {
        tracing::debug!(
            target: "whitenoise::event_processing",
            "Starting event processing loop"
        );

        let mut shutting_down = false;

        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    tracing::debug!(
                        target: "whitenoise::event_processing",
                        "Received event for processing"
                    );

                    // Process the event
                    match event {
                        ProcessableEvent::NostrEvent(event, subscription_id) => {
                            // Filter and route nostr events based on kind
                            match event.kind {
                                Kind::GiftWrap => {
                                    if let Err(e) = Self::process_giftwrap(event, subscription_id).await {
                                        tracing::error!(
                                            target: "whitenoise::event_processing",
                                            "Error processing giftwrap: {}",
                                            e
                                        );
                                    }
                                }
                                Kind::MlsGroupMessage => {
                                    if let Err(e) = Self::process_mls_message(event, subscription_id).await {
                                        tracing::error!(
                                            target: "whitenoise::event_processing",
                                            "Error processing MLS message: {}",
                                            e
                                        );
                                    }
                                }
                                _ => {
                                    // For now, just log other event types
                                    tracing::debug!(
                                        target: "whitenoise::event_processing",
                                        "Received unhandled event of kind: {:?}",
                                        event.kind
                                    );
                                }
                            }
                        }
                        ProcessableEvent::RelayMessage(relay_url, message) => {
                            Self::process_relay_message(relay_url, message);
                        }
                    }
                }
                Some(_) = shutdown.recv(), if !shutting_down => {
                    tracing::info!(
                        target: "whitenoise::event_processing",
                        "Received shutdown signal, finishing current queue..."
                    );
                    shutting_down = true;
                    // Continue processing remaining events in queue, but don't wait for new shutdown signals
                }
                else => {
                    if shutting_down {
                        tracing::debug!(
                            target: "whitenoise::event_processing",
                            "Queue flushed, shutting down event processor"
                        );
                    } else {
                        tracing::debug!(
                            target: "whitenoise::event_processing",
                            "All channels closed, exiting event processing loop"
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Process giftwrap events with account awareness
    async fn process_giftwrap(event: Event, subscription_id: Option<String>) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::event_processing",
            "Processing giftwrap: {:?}",
            event
        );

        // For giftwrap events, the target account (who the giftwrap is encrypted for)
        // is specified in a 'p' tag, not in the event.pubkey field
        let target_pubkey = event
            .tags
            .iter()
            .find(|tag| tag.kind() == TagKind::p())
            .and_then(|tag| tag.content())
            .and_then(|pubkey_str| PublicKey::parse(pubkey_str).ok());

        let target_pubkey = match target_pubkey {
            Some(pk) => pk,
            None => {
                tracing::warn!(
                    target: "whitenoise::event_processing",
                    "No target pubkey found in 'p' tag for giftwrap event"
                );
                return Ok(());
            }
        };

        tracing::debug!(
            target: "whitenoise::event_processing",
            "Processing giftwrap for target account: {} (author: {})",
            target_pubkey.to_hex(),
            event.pubkey.to_hex()
        );

        // Validate that this matches the subscription_id if available
        if let Some(sub_id) = subscription_id {
            if let Some(sub_pubkey) = Self::extract_pubkey_from_subscription_id(&sub_id) {
                if target_pubkey != sub_pubkey {
                    tracing::warn!(
                        target: "whitenoise::event_processing",
                        "Giftwrap target pubkey {} does not match subscription pubkey {} - possible routing error",
                        target_pubkey.to_hex(),
                        sub_pubkey.to_hex()
                    );
                    return Ok(());
                }
            }
        }

        // TODO: Implement account-aware giftwrap processing
        // This requires access to self.accounts and self.get_nostr_keys_for_pubkey()
        // For now, just log that we received it
        tracing::info!(
            target: "whitenoise::event_processing",
            "Giftwrap processing not yet implemented for account: {}",
            target_pubkey.to_hex()
        );

        Ok(())
    }

    /// Process MLS group messages with account awareness
    async fn process_mls_message(event: Event, subscription_id: Option<String>) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::event_processing",
            "Processing MLS message: {:?}",
            event
        );

        // Extract the account pubkey from the subscription_id if available
        if let Some(sub_id) = subscription_id {
            if let Some(target_pubkey) = Self::extract_pubkey_from_subscription_id(&sub_id) {
                tracing::debug!(
                    target: "whitenoise::event_processing",
                    "Processing MLS message for account: {}",
                    target_pubkey.to_hex()
                );
            }
        }

        // TODO: Implement account-aware MLS message processing
        // This requires access to self.accounts and MLS state
        // For now, just log that we received it
        tracing::info!(
            target: "whitenoise::event_processing",
            "MLS message processing not yet implemented"
        );

        Ok(())
    }

    /// Process relay messages for logging/monitoring
    fn process_relay_message(relay_url: RelayUrl, message_type: String) {
        tracing::debug!(
            target: "whitenoise::event_processing::relay_message",
            "Processing message from {}: {}",
            relay_url,
            message_type
        );
    }
}

impl std::fmt::Debug for Whitenoise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Whitenoise")
            .field("config", &self.config)
            .field("accounts", &self.accounts)
            .field("active_account", &self.active_account)
            .field("database", &"<REDACTED>")
            .field("nostr", &"<REDACTED>")
            .finish()
    }
}
