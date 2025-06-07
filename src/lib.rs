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
    /// # async fn example(mut whitenoise: Whitenoise) -> Result<(), whitenoise::WhitenoiseError> {
    /// whitenoise.delete_all_data().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_all_data(&mut self) -> Result<()> {
        tracing::debug!(target: "whitenoise::delete_all_data", "Deleting all data");

        // Remove nostr cache first
        self.nostr.delete_all_data().await?;

        // Remove database (accounts and media) data
        self.database.delete_all_data().await?;

        // Remove MLS related data
        let mls_dir = self.config.data_dir.join("mls");
        if mls_dir.exists() {
            tracing::debug!(
                target: "whitenoise::delete_all_data",
                "Removing MLS directory: {:?}",
                mls_dir
            );
            tokio::fs::remove_dir_all(&mls_dir).await?;
        }
        // Always recreate the empty MLS directory
        tokio::fs::create_dir_all(&mls_dir).await?;

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

        // Shutdown the event processing loop
        self.shutdown_event_processing().await?;

        // Clear the accounts map
        self.accounts.clear();
        self.active_account = None;

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

    /// Shutdown event processing gracefully
    pub(crate) async fn shutdown_event_processing(&self) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_config() -> (WhitenoiseConfig, TempDir, TempDir) {
        let data_temp_dir = TempDir::new().expect("Failed to create temp data dir");
        let logs_temp_dir = TempDir::new().expect("Failed to create temp logs dir");

        let config = WhitenoiseConfig::new(data_temp_dir.path(), logs_temp_dir.path());

        (config, data_temp_dir, logs_temp_dir)
    }

    #[test]
    fn test_whitenoise_config_new() {
        let data_dir = std::path::Path::new("/test/data");
        let logs_dir = std::path::Path::new("/test/logs");

        let config = WhitenoiseConfig::new(data_dir, logs_dir);

        if cfg!(debug_assertions) {
            assert_eq!(config.data_dir, data_dir.join("dev"));
            assert_eq!(config.logs_dir, logs_dir.join("dev"));
        } else {
            assert_eq!(config.data_dir, data_dir.join("release"));
            assert_eq!(config.logs_dir, logs_dir.join("release"));
        }
    }

    #[test]
    fn test_whitenoise_config_debug() {
        let config = WhitenoiseConfig {
            data_dir: PathBuf::from("/test/data"),
            logs_dir: PathBuf::from("/test/logs"),
        };

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("data_dir"));
        assert!(debug_str.contains("logs_dir"));
    }

    #[tokio::test]
    async fn test_whitenoise_initialization() {
        let (config, _data_temp, _logs_temp) = create_test_config();

        let result = Whitenoise::initialize_whitenoise(config.clone()).await;
        assert!(result.is_ok());

        let whitenoise = result.unwrap();
        assert_eq!(whitenoise.config.data_dir, config.data_dir);
        assert_eq!(whitenoise.config.logs_dir, config.logs_dir);
        assert!(whitenoise.accounts.is_empty());
        assert!(whitenoise.active_account.is_none());

        // Verify directories were created
        assert!(config.data_dir.exists());
        assert!(config.logs_dir.exists());
    }

    #[tokio::test]
    async fn test_whitenoise_debug_format() {
        let (config, _data_temp, _logs_temp) = create_test_config();
        let whitenoise = Whitenoise::initialize_whitenoise(config).await.unwrap();

        let debug_str = format!("{:?}", whitenoise);
        assert!(debug_str.contains("Whitenoise"));
        assert!(debug_str.contains("config"));
        assert!(debug_str.contains("accounts"));
        assert!(debug_str.contains("active_account"));
        assert!(debug_str.contains("<REDACTED>"));
    }

    #[tokio::test]
    async fn test_shutdown_event_processing() {
        let (config, _data_temp, _logs_temp) = create_test_config();
        let whitenoise = Whitenoise::initialize_whitenoise(config).await.unwrap();

        let result = whitenoise.shutdown_event_processing().await;
        assert!(result.is_ok());

        // Test that multiple shutdowns don't cause errors
        let result2 = whitenoise.shutdown_event_processing().await;
        assert!(result2.is_ok());
    }

    #[test]
    fn test_extract_pubkey_from_subscription_id() {
        // Test valid subscription ID format
        let test_pubkey = Keys::generate().public_key();
        let subscription_id = format!("{}_messages", test_pubkey.to_hex());

        let extracted = Whitenoise::extract_pubkey_from_subscription_id(&subscription_id);
        assert!(extracted.is_some());
        assert_eq!(extracted.unwrap(), test_pubkey);

        // Test invalid format (no underscore)
        let invalid_id = test_pubkey.to_hex();
        let extracted = Whitenoise::extract_pubkey_from_subscription_id(&invalid_id);
        assert!(extracted.is_none());

        // Test invalid pubkey
        let invalid_subscription = "invalid_pubkey_messages";
        let extracted = Whitenoise::extract_pubkey_from_subscription_id(invalid_subscription);
        assert!(extracted.is_none());
    }

    #[tokio::test]
    async fn test_delete_all_data() {
        let (config, _data_temp, _logs_temp) = create_test_config();
        let mut whitenoise = Whitenoise::initialize_whitenoise(config.clone())
            .await
            .unwrap();

        // Create some test files in the directories
        let test_data_file = config.data_dir.join("test_data.txt");
        let test_log_file = config.logs_dir.join("test_log.txt");

        tokio::fs::write(&test_data_file, "test data")
            .await
            .unwrap();
        tokio::fs::write(&test_log_file, "test log").await.unwrap();

        // Verify files exist
        assert!(test_data_file.exists());
        assert!(test_log_file.exists());

        // Add a test account to verify clearing
        let (test_account, test_keys) = Account::new().await.unwrap();
        let pubkey = test_keys.public_key();
        whitenoise.accounts.insert(pubkey, test_account);
        whitenoise.active_account = Some(pubkey);

        assert!(!whitenoise.accounts.is_empty());
        assert!(whitenoise.active_account.is_some());

        // Delete all data
        let result = whitenoise.delete_all_data().await;
        assert!(result.is_ok());

        // Verify accounts are cleared
        assert!(whitenoise.accounts.is_empty());
        assert!(whitenoise.active_account.is_none());

        // Verify log file is deleted
        assert!(!test_log_file.exists());

        // MLS directory should be recreated as empty
        let mls_dir = config.data_dir.join("mls");
        assert!(mls_dir.exists());
        assert!(mls_dir.is_dir());
    }

    #[tokio::test]
    async fn test_queue_operations_after_shutdown() {
        let (config, _data_temp, _logs_temp) = create_test_config();
        let whitenoise = Whitenoise::initialize_whitenoise(config).await.unwrap();

        // Shutdown event processing
        whitenoise.shutdown_event_processing().await.unwrap();

        // Give a moment for shutdown to complete
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Test that shutdown completed successfully
        // (We can't test queuing operations since those methods were removed)
    }

    #[tokio::test]
    async fn test_multiple_initializations_with_same_config() {
        let (config, _data_temp, _logs_temp) = create_test_config();

        // First initialization
        let result1 = Whitenoise::initialize_whitenoise(config.clone()).await;
        assert!(result1.is_ok());

        // Second initialization with same config should also work
        let result2 = Whitenoise::initialize_whitenoise(config).await;
        assert!(result2.is_ok());
    }

    #[test]
    fn test_whitenoise_config_clone() {
        let (config, _data_temp, _logs_temp) = create_test_config();
        let cloned_config = config.clone();

        assert_eq!(config.data_dir, cloned_config.data_dir);
        assert_eq!(config.logs_dir, cloned_config.logs_dir);
    }

    // Test helper functions for subscription ID parsing edge cases
    #[test]
    fn test_extract_pubkey_edge_cases() {
        // Empty string
        let result = Whitenoise::extract_pubkey_from_subscription_id("");
        assert!(result.is_none());

        // String with underscore but empty pubkey
        let result = Whitenoise::extract_pubkey_from_subscription_id("_messages");
        assert!(result.is_none());

        // String with multiple underscores (should take first part)
        let test_pubkey = Keys::generate().public_key();
        let subscription_id = format!("{}_messages_extra_data", test_pubkey.to_hex());
        let result = Whitenoise::extract_pubkey_from_subscription_id(&subscription_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), test_pubkey);
    }
}
