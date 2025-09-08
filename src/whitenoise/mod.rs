use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use dashmap::DashMap;
use nostr_mls::prelude::*;
use tokio::sync::{
    mpsc::{self, Sender},
    OnceCell, Semaphore,
};

pub mod accounts;
pub mod app_settings;
pub mod database;
pub mod error;
mod event_processor;
pub mod event_tracker;
pub mod follows;
pub mod group_information;
pub mod groups;
pub mod key_packages;
pub mod message_aggregator;
pub mod messages;
pub mod relays;
pub mod secrets_store;
pub mod users;
pub mod utils;
pub mod welcomes;

use crate::init_tracing;
use crate::nostr_manager::NostrManager;

use crate::types::ProcessableEvent;
use accounts::*;
use app_settings::*;
use database::*;
use error::{Result, WhitenoiseError};
use event_tracker::WhitenoiseEventTracker;
use relays::*;
use secrets_store::SecretsStore;
use users::User;

#[derive(Clone, Debug)]
pub struct WhitenoiseConfig {
    /// Directory for application data
    pub data_dir: PathBuf,

    /// Directory for application logs
    pub logs_dir: PathBuf,

    /// Configuration for the message aggregator
    pub message_aggregator_config: Option<message_aggregator::AggregatorConfig>,
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
            message_aggregator_config: None, // Use default MessageAggregator configuration
        }
    }

    /// Create a new configuration with custom message aggregator settings
    pub fn new_with_aggregator_config(
        data_dir: &Path,
        logs_dir: &Path,
        aggregator_config: message_aggregator::AggregatorConfig,
    ) -> Self {
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
            message_aggregator_config: Some(aggregator_config),
        }
    }
}

pub struct Whitenoise {
    pub config: WhitenoiseConfig,
    database: Arc<Database>,
    nostr: NostrManager,
    secrets_store: SecretsStore,
    message_aggregator: message_aggregator::MessageAggregator,
    event_sender: Sender<ProcessableEvent>,
    shutdown_sender: Sender<()>,
    /// Per-account concurrency guards to prevent race conditions in contact list processing
    contact_list_guards: DashMap<PublicKey, Arc<Semaphore>>,
}

static GLOBAL_WHITENOISE: OnceCell<Whitenoise> = OnceCell::const_new();

impl std::fmt::Debug for Whitenoise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Whitenoise")
            .field("config", &self.config)
            .field("nostr_mls_map", &"<REDACTED>")
            .field("database", &"<REDACTED>")
            .field("nostr", &"<REDACTED>")
            .field("secrets_store", &"<REDACTED>")
            .finish()
    }
}

impl Whitenoise {
    /// Initializes the Whitenoise application with the provided configuration.
    ///
    /// This method sets up the necessary data and log directories, configures logging,
    /// initializes the database, creates event processing channels, sets up the Nostr client,
    /// loads existing accounts, and starts the event processing loop.
    ///
    /// # Arguments
    ///
    /// * `config` - A [`WhitenoiseConfig`] struct specifying the data and log directories.
    pub async fn initialize_whitenoise(config: WhitenoiseConfig) -> Result<()> {
        // Create event processing channels
        let (event_sender, event_receiver) = mpsc::channel(500);
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        let whitenoise_res: Result<&'static Whitenoise> = GLOBAL_WHITENOISE.get_or_try_init(|| async {
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

        tracing::debug!(target: "whitenoise::initialize_whitenoise", "Logging initialized in directory: {:?}", logs_dir);

        let database = Arc::new(Database::new(data_dir.join("whitenoise.sqlite")).await?);

        // Create NostrManager with event_sender for direct event queuing
        let nostr =
            NostrManager::new(event_sender.clone(), Arc::new(WhitenoiseEventTracker::new()), NostrManager::default_timeout())
                .await?;

        // Create SecretsStore
        let secrets_store = SecretsStore::new(data_dir);

        // Create message aggregator - always initialize, use custom config if provided
        let message_aggregator = if let Some(aggregator_config) = config.message_aggregator_config.clone() {
            message_aggregator::MessageAggregator::with_config(aggregator_config)
        } else {
            message_aggregator::MessageAggregator::new()
        };

        let whitenoise = Self {
            config,
            database,
            nostr,
            secrets_store,
            message_aggregator,
            event_sender,
            shutdown_sender,
            contact_list_guards: DashMap::new(),
        };

        // Create default relays in the database if they don't exist
        // TODO: Make this batch fetch and insert all relays at once
        for relay in Relay::defaults() {
            let _ = whitenoise.find_or_create_relay_by_url(&relay.url).await?;
        }

        // Create default app settings in the database if they don't exist
        AppSettings::find_or_create_default(&whitenoise.database).await?;

        // Add default relays to the Nostr client if they aren't already added
        if whitenoise.nostr.client.relays().await.is_empty() {
            // First time starting the app
            for relay in Relay::defaults() {
                whitenoise.nostr.client.add_relay(relay.url).await?;
            }
        }

        // No need to wait for all the relays to be up
        tokio::spawn({
            let client = whitenoise.nostr.client.clone();
            async move {
                client.connect().await;
            }
        });
        Ok(whitenoise)
        }).await;

        let whitenoise_ref = whitenoise_res?;
        tracing::debug!(
            target: "whitenoise::initialize_whitenoise",
            "Starting event processing loop for loaded accounts"
        );

        Self::start_event_processing_loop(whitenoise_ref, event_receiver, shutdown_receiver).await;

        // Fetch events and setup subscriptions after event processing has started
        Self::setup_global_users_subscriptions(whitenoise_ref).await?;
        Self::setup_accounts_sync_and_subscriptions(whitenoise_ref).await?;

        tracing::debug!(
            target: "whitenoise::initialize_whitenoise",
            "Completed initialization for all loaded accounts"
        );

        Ok(())
    }

    async fn setup_global_users_subscriptions(whitenoise_ref: &'static Whitenoise) -> Result<()> {
        let users_with_relays = User::all_users_with_relay_urls(whitenoise_ref).await?;
        let default_relays: Vec<RelayUrl> =
            Relay::defaults().iter().map(|r| r.url.clone()).collect();

        whitenoise_ref
            .nostr
            .setup_batched_relay_subscriptions(users_with_relays, &default_relays)
            .await?;
        Ok(())
    }

    async fn setup_accounts_sync_and_subscriptions(
        whitenoise_ref: &'static Whitenoise,
    ) -> Result<()> {
        let accounts = Account::all(&whitenoise_ref.database).await?;
        for account in accounts {
            // Trigger background data fetch for each account (non-critical)
            if let Err(e) = whitenoise_ref.background_sync_account_data(&account).await {
                tracing::warn!(
                    target: "whitenoise::load_accounts",
                    "Failed to trigger background fetch for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                // Continue - background fetch failure should not prevent account loading
            }
            // Setup subscriptions for this account
            match whitenoise_ref.setup_subscriptions(&account).await {
                Ok(()) => {
                    tracing::debug!(
                        target: "whitenoise::initialize_whitenoise",
                        "Successfully set up subscriptions for account: {}",
                        account.pubkey.to_hex()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "whitenoise::initialize_whitenoise",
                        "Failed to set up subscriptions for account {}: {}",
                        account.pubkey.to_hex(),
                        e
                    );
                    // Continue with other accounts instead of failing completely
                }
            }
        }
        Ok(())
    }

    /// Returns a reference to the global Whitenoise singleton instance.
    ///
    /// This method provides access to the globally initialized Whitenoise instance that was
    /// created by [`Whitenoise::initialize_whitenoise`]. The instance is stored as a static singleton
    /// using [`tokio::sync::OnceCell`] to ensure async-safe thread-safe access and single initialization.
    ///
    /// This method is particularly useful for accessing the Whitenoise instance from different
    /// parts of the application without passing references around, such as in event handlers,
    /// background tasks, or API endpoints.
    pub fn get_instance() -> Result<&'static Self> {
        GLOBAL_WHITENOISE
            .get()
            .ok_or(WhitenoiseError::Initialization)
    }

    /// Deletes all application data, including the database, MLS data, and log files.
    ///
    /// This asynchronous method removes all persistent data associated with the Whitenoise instance.
    /// It deletes the nostr cache, database, MLS-related directories, and all log files. If the MLS directory exists,
    /// it is removed and then recreated as an empty directory. This is useful for resetting the application
    /// to a clean state.
    pub async fn delete_all_data(&self) -> Result<()> {
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

        Ok(())
    }

    pub async fn export_account_nsec(&self, account: &Account) -> Result<String> {
        Ok(self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?
            .secret_key()
            .to_bech32()
            .unwrap())
    }

    pub async fn export_account_npub(&self, account: &Account) -> Result<String> {
        Ok(account.pubkey.to_bech32().unwrap())
    }

    /// Get a reference to the message aggregator for advanced usage
    /// This allows consumers to access the message aggregator directly for custom processing
    pub fn message_aggregator(&self) -> &message_aggregator::MessageAggregator {
        &self.message_aggregator
    }

    #[cfg(feature = "integration-tests")]
    pub async fn wipe_database(&self) -> Result<()> {
        self.database.delete_all_data().await?;
        Ok(())
    }

    #[cfg(feature = "integration-tests")]
    pub async fn reset_nostr_client(&self) -> Result<()> {
        self.nostr.client.reset().await;
        Ok(())
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use tempfile::TempDir;
    // Test configuration and setup helpers
    pub(crate) fn create_test_config() -> (WhitenoiseConfig, TempDir, TempDir) {
        let data_temp_dir = TempDir::new().expect("Failed to create temp data dir");
        let logs_temp_dir = TempDir::new().expect("Failed to create temp logs dir");
        let config = WhitenoiseConfig::new(data_temp_dir.path(), logs_temp_dir.path());
        (config, data_temp_dir, logs_temp_dir)
    }

    pub(crate) fn create_test_keys() -> Keys {
        Keys::generate()
    }

    pub(crate) async fn create_test_account(whitenoise: &Whitenoise) -> (Account, Keys) {
        let (account, keys) = Account::new(whitenoise, None).await.unwrap();
        (account, keys)
    }

    /// Creates a mock Whitenoise instance for testing.
    ///
    /// This function creates a Whitenoise instance with a minimal configuration and database.
    /// It also creates a NostrManager instance that connects to the local test relays.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - `(Whitenoise, TempDir, TempDir)`
    ///   - `Whitenoise`: The mock Whitenoise instance
    ///   - `TempDir`: The temporary directory for data storage
    ///   - `TempDir`: The temporary directory for log storage
    pub(crate) async fn create_mock_whitenoise() -> (Whitenoise, TempDir, TempDir) {
        // Wait for local relays to be ready in test environment
        wait_for_test_relays().await;

        let (config, data_temp, logs_temp) = create_test_config();

        // Create directories manually to avoid issues
        std::fs::create_dir_all(&config.data_dir).unwrap();
        std::fs::create_dir_all(&config.logs_dir).unwrap();

        // Initialize minimal tracing for tests
        init_tracing(&config.logs_dir);

        let database = Arc::new(
            Database::new(config.data_dir.join("test.sqlite"))
                .await
                .unwrap(),
        );
        let secrets_store = SecretsStore::new(&config.data_dir);

        // Create channels but don't start processing loop to avoid network calls
        let (event_sender, _event_receiver) = mpsc::channel(10);
        let (shutdown_sender, _shutdown_receiver) = mpsc::channel(1);

        // Create NostrManager for testing - now with actual relay connections
        // to use the local development relays running in docker
        let nostr = NostrManager::new(
            event_sender.clone(),
            Arc::new(event_tracker::TestEventTracker::new(database.clone())),
            NostrManager::default_timeout(),
        )
        .await
        .expect("Failed to create NostrManager");

        // connect to default relays
        let default_relays_urls: Vec<RelayUrl> =
            Relay::defaults().iter().map(|r| r.url.clone()).collect();

        for relay in default_relays_urls {
            nostr.client.add_relay(relay).await.unwrap();
        }

        nostr.client.connect().await;

        // Create message aggregator for testing
        let message_aggregator = message_aggregator::MessageAggregator::new();

        let whitenoise = Whitenoise {
            config,
            database,
            nostr,
            secrets_store,
            message_aggregator,
            event_sender,
            shutdown_sender,
            contact_list_guards: DashMap::new(),
        };

        (whitenoise, data_temp, logs_temp)
    }

    /// Wait for local test relays to be ready
    async fn wait_for_test_relays() {
        use std::time::Duration;
        use tokio::time::{sleep, timeout};

        // Only wait for relays in debug builds (where we use localhost relays)
        if !cfg!(debug_assertions) {
            return;
        }

        tracing::debug!(target: "whitenoise::test_utils", "Waiting for local test relays to be ready...");

        let relay_urls = vec!["ws://localhost:8080", "ws://localhost:7777"];

        for relay_url in relay_urls {
            let mut attempts = 0;
            const MAX_ATTEMPTS: u32 = 10;
            const WAIT_INTERVAL: Duration = Duration::from_millis(500);

            while attempts < MAX_ATTEMPTS {
                // Try to establish a WebSocket connection to test readiness
                match timeout(Duration::from_secs(2), test_relay_connection(relay_url)).await {
                    Ok(Ok(())) => {
                        tracing::debug!(target: "whitenoise::test_utils", "Relay {} is ready", relay_url);
                        break;
                    }
                    Ok(Err(e)) => {
                        tracing::debug!(target: "whitenoise::test_utils",
                            "Relay {} not ready yet (attempt {}/{}): {:?}",
                            relay_url, attempts + 1, MAX_ATTEMPTS, e);
                    }
                    Err(_) => {
                        tracing::debug!(target: "whitenoise::test_utils",
                            "Relay {} connection timeout (attempt {}/{})",
                            relay_url, attempts + 1, MAX_ATTEMPTS);
                    }
                }

                attempts += 1;
                if attempts < MAX_ATTEMPTS {
                    sleep(WAIT_INTERVAL).await;
                }
            }

            if attempts >= MAX_ATTEMPTS {
                tracing::warn!(target: "whitenoise::test_utils",
                    "Relay {} may not be fully ready after {} attempts", relay_url, MAX_ATTEMPTS);
            }
        }

        // Give relays a bit more time to stabilize
        sleep(Duration::from_millis(100)).await;
        tracing::debug!(target: "whitenoise::test_utils", "Relay readiness check completed");
    }

    /// Test if a relay is ready by attempting a simple connection
    async fn test_relay_connection(
        relay_url: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use nostr_sdk::prelude::*;

        // Create a minimal client for testing connection
        let client = Client::default();
        client.add_relay(relay_url).await?;

        // Try to connect - this will fail if relay isn't ready
        client.connect().await;

        // Give it a moment to establish connection
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Check if we're connected
        let relay_url_parsed = RelayUrl::parse(relay_url)?;
        match client.relay(&relay_url_parsed).await {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) async fn test_get_whitenoise() -> &'static Whitenoise {
        // Initialize whitenoise for this specific test
        let (config, _data_temp, _logs_temp) = create_test_config();
        Whitenoise::initialize_whitenoise(config).await.unwrap();
        Whitenoise::get_instance().unwrap()
    }

    pub(crate) async fn setup_login_account(whitenoise: &Whitenoise) -> (Account, Keys) {
        let keys = create_test_keys();
        let account = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();
        (account, keys)
    }

    pub(crate) fn create_nostr_group_config_data(admins: Vec<PublicKey>) -> NostrGroupConfigData {
        NostrGroupConfigData {
            name: "Test group".to_owned(),
            description: "test description".to_owned(),
            image_hash: Some([0u8; 32]),  // 32-byte hash for fake image
            image_key: Some([1u8; 32]),   // 32-byte encryption key
            image_nonce: Some([2u8; 12]), // 12-byte nonce
            relays: vec![RelayUrl::parse("ws://localhost:8080/").unwrap()],
            admins,
        }
    }

    pub(crate) async fn setup_multiple_test_accounts(
        whitenoise: &Whitenoise,
        count: usize,
    ) -> Vec<(Account, Keys)> {
        let mut accounts = Vec::new();
        for _ in 0..count {
            // Generate keys first
            let keys = create_test_keys();
            // Use login to create and register the account properly
            let account = whitenoise
                .login(keys.secret_key().to_secret_hex())
                .await
                .unwrap();
            accounts.push((account.clone(), keys.clone()));
            // publish keypackage to relays
            let (ekp, tags) = whitenoise.encoded_key_package(&account).await.unwrap();

            let _ = whitenoise
                .nostr
                .publish_key_package_with_signer(
                    &ekp,
                    &account.key_package_relays(whitenoise).await.unwrap(),
                    &tags,
                    keys,
                )
                .await
                .unwrap();
        }
        accounts
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;

    // Configuration Tests
    mod config_tests {
        use super::*;

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
        fn test_whitenoise_config_debug_and_clone() {
            let (config, _data_temp, _logs_temp) = create_test_config();
            let cloned_config = config.clone();

            assert_eq!(config.data_dir, cloned_config.data_dir);
            assert_eq!(config.logs_dir, cloned_config.logs_dir);
            assert_eq!(
                config.message_aggregator_config,
                cloned_config.message_aggregator_config
            );

            let debug_str = format!("{:?}", config);
            assert!(debug_str.contains("data_dir"));
            assert!(debug_str.contains("logs_dir"));
            assert!(debug_str.contains("message_aggregator_config"));
        }

        #[test]
        fn test_whitenoise_config_with_custom_aggregator() {
            let data_dir = std::path::Path::new("/test/data");
            let logs_dir = std::path::Path::new("/test/logs");

            // Test with custom aggregator config
            let custom_config = message_aggregator::AggregatorConfig {
                max_retry_attempts: 5,
                normalize_emoji: false,
                enable_debug_logging: true,
            };

            let config = WhitenoiseConfig::new_with_aggregator_config(
                data_dir,
                logs_dir,
                custom_config.clone(),
            );

            assert!(config.message_aggregator_config.is_some());
            let aggregator_config = config.message_aggregator_config.unwrap();
            assert_eq!(aggregator_config.max_retry_attempts, 5);
            assert!(!aggregator_config.normalize_emoji);
            assert!(aggregator_config.enable_debug_logging);
        }
    }

    // Initialization Tests
    mod initialization_tests {
        use super::*;

        #[tokio::test]
        async fn test_whitenoise_initialization() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            assert!(Account::all(&whitenoise.database).await.unwrap().is_empty());

            // Verify directories were created
            assert!(whitenoise.config.data_dir.exists());
            assert!(whitenoise.config.logs_dir.exists());
        }

        #[tokio::test]
        async fn test_whitenoise_debug_format() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            let debug_str = format!("{:?}", whitenoise);
            assert!(debug_str.contains("Whitenoise"));
            assert!(debug_str.contains("config"));
            assert!(debug_str.contains("<REDACTED>"));
        }

        #[tokio::test]
        async fn test_multiple_initializations_with_same_config() {
            // Test that we can create multiple mock instances
            let (whitenoise1, _data_temp1, _logs_temp1) = create_mock_whitenoise().await;
            let (whitenoise2, _data_temp2, _logs_temp2) = create_mock_whitenoise().await;

            // Both should have valid configurations (they'll be different temp dirs, which is fine)
            assert!(whitenoise1.config.data_dir.exists());
            assert!(whitenoise2.config.data_dir.exists());
            assert!(Account::all(&whitenoise1.database)
                .await
                .unwrap()
                .is_empty());
            assert!(Account::all(&whitenoise2.database)
                .await
                .unwrap()
                .is_empty());
        }
    }

    // Data Management Tests
    mod data_management_tests {
        use super::*;

        #[tokio::test]
        async fn test_delete_all_data() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create test files in the whitenoise directories
            let test_data_file = whitenoise.config.data_dir.join("test_data.txt");
            let test_log_file = whitenoise.config.logs_dir.join("test_log.txt");
            tokio::fs::write(&test_data_file, "test data")
                .await
                .unwrap();
            tokio::fs::write(&test_log_file, "test log").await.unwrap();
            assert!(test_data_file.exists());
            assert!(test_log_file.exists());

            // Delete all data
            let result = whitenoise.delete_all_data().await;
            assert!(result.is_ok());

            // Verify cleanup
            assert!(Account::all(&whitenoise.database).await.unwrap().is_empty());
            assert!(!test_log_file.exists());

            // MLS directory should be recreated as empty
            let mls_dir = whitenoise.config.data_dir.join("mls");
            assert!(mls_dir.exists());
            assert!(mls_dir.is_dir());
        }
    }

    // API Tests (using mock to minimize network calls)
    // NOTE: These tests still make some network calls through NostrManager
    // For complete isolation, implement the trait-based mocking described above
    mod api_tests {
        use super::*;

        #[tokio::test]
        async fn test_message_aggregator_access() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Test that we can access the message aggregator
            let aggregator = whitenoise.message_aggregator();

            // Check that it has expected default configuration
            let config = aggregator.config();
            assert_eq!(config.max_retry_attempts, 3);
            assert!(config.normalize_emoji);
            assert!(!config.enable_debug_logging);
        }

        #[tokio::test]
        async fn test_fetch_aggregated_messages_basic_error() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let account = whitenoise.create_identity().await.unwrap();

            // Mock group ID for testing
            let group_id = GroupId::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

            // Since create_identity initializes nostr_mls, we should get a different error
            // The error should be about the group not existing, not nostr_mls not being initialized
            let result = whitenoise
                .fetch_aggregated_messages_for_group(&account.pubkey, &group_id)
                .await;

            // Should return an error (group not found or similar), but not NostrMlsNotInitialized
            assert!(result.is_err());
            // The specific error will be about the group not being found since we're using a fake group ID
        }
    }
}
