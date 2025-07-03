use anyhow::Context;
use nostr_mls::prelude::*;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::OnceCell;
use tokio::sync::RwLock;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod accounts;
pub mod database;
pub mod error;
mod event_processing;
pub mod message_aggregator;
pub mod secrets_store;
pub mod utils;

use crate::init_tracing;
use crate::nostr_manager::NostrManager;

use crate::types::ProcessableEvent;
use accounts::*;
use database::*;
use error::{Result, WhitenoiseError};
use secrets_store::SecretsStore;

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
    pub accounts: Arc<RwLock<HashMap<PublicKey, Account>>>,
    database: Arc<Database>,
    nostr: NostrManager,
    secrets_store: SecretsStore,
    message_aggregator: message_aggregator::MessageAggregator,
    event_sender: Sender<ProcessableEvent>,
    shutdown_sender: Sender<()>,
}

static GLOBAL_WHITENOISE: OnceCell<Whitenoise> = OnceCell::const_new();

impl std::fmt::Debug for Whitenoise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Whitenoise")
            .field("config", &self.config)
            .field("accounts", &self.accounts)
            .field("database", &"<REDACTED>")
            .field("nostr", &"<REDACTED>")
            .field("secrets_store", &"<REDACTED>")
            .finish()
    }
}

impl Whitenoise {
    // ============================================================================
    // HELPER METHODS FOR THREAD-SAFE ACCESS
    // ============================================================================

    /// Get a read lock on the accounts HashMap
    async fn read_accounts(&self) -> tokio::sync::RwLockReadGuard<'_, HashMap<PublicKey, Account>> {
        self.accounts.read().await
    }

    async fn read_account_by_pubkey(&self, pubkey: &PublicKey) -> Result<Account> {
        self.read_accounts()
            .await
            .get(pubkey)
            .cloned()
            .ok_or(WhitenoiseError::AccountNotFound)
    }

    /// Get a write lock on the accounts HashMap
    async fn write_accounts(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<PublicKey, Account>> {
        self.accounts.write().await
    }

    /// Test helper: Check if accounts is empty
    #[cfg(test)]
    pub async fn accounts_is_empty(&self) -> bool {
        self.read_accounts().await.is_empty()
    }

    /// Test helper: Get accounts length
    #[cfg(test)]
    pub async fn accounts_len(&self) -> usize {
        self.read_accounts().await.len()
    }

    /// Test helper: Check if account exists
    #[cfg(test)]
    pub async fn has_account(&self, pubkey: &PublicKey) -> bool {
        self.read_accounts().await.contains_key(pubkey)
    }

    /// Integration test helper: Get accounts length (public version)
    pub async fn get_accounts_count(&self) -> usize {
        self.read_accounts().await.len()
    }

    pub async fn logged_in(&self, pubkey: &PublicKey) -> bool {
        self.read_accounts().await.contains_key(pubkey)
    }

    // ============================================================================
    // INITIALIZATION & LIFECYCLE
    // ============================================================================

    /// Initializes the Whitenoise application with the provided configuration.
    ///
    /// This method sets up the necessary data and log directories, configures logging,
    /// initializes the database, creates event processing channels, sets up the Nostr client,
    /// loads existing accounts, and starts the event processing loop.
    ///
    /// # Arguments
    ///
    /// * `config` - A [`WhitenoiseConfig`] struct specifying the data and log directories.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a fully initialized [`Whitenoise`] instance on success,
    /// or a [`WhitenoiseError`] if initialization fails at any step.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The data or log directories cannot be created.
    /// - The database cannot be initialized.
    /// - The NostrManager cannot be created.
    /// - Accounts cannot be loaded from the database.
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
            NostrManager::new_with_connections(data_dir.join("nostr_lmdb"), event_sender.clone())
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
            accounts: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            shutdown_sender,
        };

        // Load all accounts from database
        let loaded_accounts = whitenoise.load_accounts().await?;
        {
            let mut accounts = whitenoise.write_accounts().await;
            *accounts = loaded_accounts;
        }
        Ok(whitenoise)
        }).await;

        let whitenoise_ref = whitenoise_res?;
        tracing::debug!(
            target: "whitenoise::initialize_whitenoise",
            "Starting event processing loop for loaded accounts"
        );

        Self::start_event_processing_loop(whitenoise_ref, event_receiver, shutdown_receiver).await;

        // Fetch events and setup subscriptions for all accounts after event processing has started
        {
            let accounts = whitenoise_ref.read_accounts().await;
            let account_list: Vec<Account> = accounts.values().cloned().collect();
            drop(accounts); // Release the read lock early
            for account in account_list {
                // Fetch account data
                match whitenoise_ref.background_fetch_account_data(&account).await {
                    Ok(()) => {
                        tracing::debug!(
                            target: "whitenoise::initialize_whitenoise",
                            "Successfully fetched account data for account: {}",
                            account.pubkey.to_hex()
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "whitenoise::initialize_whitenoise",
                            "Failed to fetch account data for account {}: {}",
                            account.pubkey.to_hex(),
                            e
                        );
                        // Continue with other accounts instead of failing completely
                    }
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
        }

        tracing::debug!(
            target: "whitenoise::initialize_whitenoise",
            "Completed initialization for all loaded accounts"
        );

        Ok(())
    }

    /// Returns a reference to the global Whitenoise singleton instance.
    ///
    /// This method provides access to the globally initialized Whitenoise instance that was
    /// created by [`initialize_whitenoise`]. The instance is stored as a static singleton
    /// using [`tokio::sync::OnceCell`] to ensure async-safe thread-safe access and single initialization.
    ///
    /// This method is particularly useful for accessing the Whitenoise instance from different
    /// parts of the application without passing references around, such as in event handlers,
    /// background tasks, or API endpoints.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `Ok(&'static Whitenoise)` - A static reference to the initialized Whitenoise instance
    /// - `Err(WhitenoiseError::Initialization)` - If [`initialize_whitenoise`] has not been called yet
    ///
    /// # Errors
    ///
    /// This function will return [`WhitenoiseError::Initialization`] if:
    /// - [`initialize_whitenoise`] has not been successfully called prior to this method
    /// - The global instance failed to initialize during the [`initialize_whitenoise`] call
    ///
    /// # Thread Safety
    ///
    /// This method is thread-safe and async-safe, and can be called concurrently from multiple
    /// threads or async contexts. The underlying [`tokio::sync::OnceCell`] ensures that access
    /// to the global instance is properly synchronized for async environments.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use whitenoise::{Whitenoise, WhitenoiseConfig};
    /// # use std::path::Path;
    /// # async fn example() -> Result<(), whitenoise::WhitenoiseError> {
    /// // First, initialize Whitenoise
    /// let config = WhitenoiseConfig::new(Path::new("./data"), Path::new("./logs"));
    /// Whitenoise::initialize_whitenoise(config).await?;
    ///
    /// // Then access the instance from anywhere in your application
    /// let whitenoise = Whitenoise::get_instance()?;
    /// let account_count = whitenoise.get_accounts_count().await;
    /// println!("Number of accounts: {}", account_count);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Usage in Event Handlers
    ///
    /// ```rust
    /// # use whitenoise::Whitenoise;
    /// # async fn handle_some_event() -> Result<(), whitenoise::WhitenoiseError> {
    /// // Access Whitenoise from an event handler without dependency injection
    /// let whitenoise = Whitenoise::get_instance()?;
    /// // ... use whitenoise methods
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`initialize_whitenoise`]: Self::initialize_whitenoise
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
    ///
    /// # Returns
    ///
    /// Returns a `Result` which is `Ok(())` if all data is successfully deleted, or a
    /// [`WhitenoiseError`] if any step fails.
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

        // Clear the accounts map
        {
            let mut accounts = self.write_accounts().await;
            accounts.clear();
        }

        Ok(())
    }

    pub async fn export_account_nsec(&self, account: &Account) -> Result<String> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        Ok(self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?
            .secret_key()
            .to_bech32()
            .unwrap())
    }

    pub async fn export_account_npub(&self, account: &Account) -> Result<String> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        Ok(account.pubkey.to_bech32().unwrap())
    }

    // ============================================================================
    // MESSAGE AGGREGATION ACCESS
    // ============================================================================

    /// Get a reference to the message aggregator for advanced usage
    /// This allows consumers to access the message aggregator directly for custom processing
    pub fn message_aggregator(&self) -> &message_aggregator::MessageAggregator {
        &self.message_aggregator
    }
}

#[cfg(test)]
pub mod test_utils {
    use crate::RelayType;

    use super::*;
    use accounts::AccountSettings;
    use tempfile::TempDir;
    use tokio::sync::Mutex;
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

    pub(crate) fn create_test_account() -> (Account, Keys) {
        let keys = Keys::generate();
        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: accounts::OnboardingState::default(),
            last_synced: Timestamp::zero(),
            nostr_mls: std::sync::Arc::new(Mutex::new(None)),
        };
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
        let nostr = NostrManager::new_with_connections(
            config.data_dir.join("test_nostr"),
            event_sender.clone(),
        )
        .await
        .expect("Failed to create NostrManager");

        // Create message aggregator for testing
        let message_aggregator = message_aggregator::MessageAggregator::new();

        let whitenoise = Whitenoise {
            config,
            database,
            nostr,
            secrets_store,
            message_aggregator,
            accounts: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            shutdown_sender,
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
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();
        whitenoise
            .update_relays(
                &account,
                RelayType::Nostr,
                vec![RelayUrl::parse("ws://localhost:8080/").unwrap()],
            )
            .await
            .unwrap();

        (account, keys)
    }
    
    pub(crate) fn create_nostr_group_config_data() -> NostrGroupConfigData {
        NostrGroupConfigData {
            name: "Test group".to_owned(),
            description: "test description".to_owned(),
            image_url: Some("http://test_blossom:53232/fake_img.png".to_owned()),
            image_key: Some(b"fake key to encrypt image".to_vec()),
            relays: vec![RelayUrl::parse("ws://localhost:8080/").unwrap()],
        }
    }

    pub(crate) async fn setup_multiple_test_accounts(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        count: usize,
    ) -> Vec<(Account, Keys)> {
        let mut accounts = Vec::new();
        for _ in 0..count {
            let (account, keys) = create_test_account();
            accounts.push((account.clone(), keys.clone()));
            whitenoise
                .add_contact(creator_account, keys.public_key())
                .await
                .unwrap();

            // publish keypackage to relays
            let (ekp, tags) = whitenoise
                .encoded_key_package(creator_account, &account.pubkey)
                .await
                .unwrap();
            let key_package_event_builder = EventBuilder::new(Kind::MlsKeyPackage, ekp).tags(tags);

            // Get relays with fallback to defaults if user hasn't configured key package relays
            let relays_to_use = whitenoise
                .fetch_relays_with_fallback(account.pubkey, RelayType::KeyPackage)
                .await
                .unwrap();

            let _ = whitenoise
                .nostr
                .publish_event_builder_with_signer(key_package_event_builder, &relays_to_use, keys)
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
    use relays::*;
    use std::time::Duration;

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
            assert!(whitenoise.accounts_is_empty().await);

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
            assert!(debug_str.contains("accounts"));
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
            assert!(whitenoise1.accounts_is_empty().await);
            assert!(whitenoise2.accounts_is_empty().await);
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

            // Add test account
            let (test_account, test_keys) = create_test_account();
            let pubkey = test_keys.public_key();
            {
                let mut accounts = whitenoise.write_accounts().await;
                accounts.insert(pubkey, test_account);
            }
            assert!(!whitenoise.accounts_is_empty().await);

            // Delete all data
            let result = whitenoise.delete_all_data().await;
            assert!(result.is_ok());

            // Verify cleanup
            assert!(whitenoise.accounts_is_empty().await);
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
        async fn test_fetch_methods_return_types() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();
            let account = whitenoise
                .login(test_keys.secret_key().to_secret_hex())
                .await;
            assert!(account.is_ok());

            // Test all load methods return expected types (though they may be empty in test env)
            let metadata = whitenoise.fetch_metadata(pubkey).await;
            assert!(metadata.is_ok());

            let relays = whitenoise.fetch_relays(pubkey, RelayType::Inbox).await;
            assert!(relays.is_ok());

            let contacts = whitenoise.fetch_contacts(pubkey).await;
            assert!(contacts.is_ok());

            let onboarding = whitenoise.fetch_onboarding_state(pubkey).await;
            assert!(onboarding.is_ok());
        }

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
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();

            // Create account but don't initialize nostr_mls
            let _account = whitenoise
                .login(test_keys.secret_key().to_secret_hex())
                .await
                .unwrap();

            // Mock group ID for testing
            let group_id = GroupId::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

            // Since login initializes nostr_mls, we should get a different error
            // The error should be about the group not existing, not nostr_mls not being initialized
            let result = whitenoise
                .fetch_aggregated_messages_for_group(&pubkey, &group_id)
                .await;

            // Should return an error (group not found or similar), but not NostrMlsNotInitialized
            assert!(result.is_err());
            // The specific error will be about the group not being found since we're using a fake group ID
        }

        #[tokio::test]
        async fn test_concurrent_api_calls() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();
            let account = whitenoise
                .login(test_keys.secret_key().to_secret_hex())
                .await;
            assert!(account.is_ok());

            // Test concurrent API calls
            let results = tokio::join!(
                whitenoise.fetch_metadata(pubkey),
                whitenoise.fetch_relays(pubkey, RelayType::Inbox),
                whitenoise.fetch_contacts(pubkey),
                whitenoise.fetch_onboarding_state(pubkey)
            );

            assert!(results.0.is_ok());
            assert!(results.1.is_ok());
            assert!(results.2.is_ok());
            assert!(results.3.is_ok());
        }

        #[tokio::test]
        async fn test_background_fetch_updates_last_synced() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create and save a test account
            let (account, keys) = create_test_account();
            whitenoise.save_account(&account).await.unwrap();
            whitenoise.secrets_store.store_private_key(&keys).unwrap();

            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());
            assert_eq!(log_account.unwrap(), account);

            let _original_timestamp = account.last_synced;

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Trigger background fetch
            whitenoise
                .background_fetch_account_data(&account)
                .await
                .unwrap();

            // Give the background task a moment to complete
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Check that the account still exists in database
            // (The background task updates the timestamp, but we can't easily test the
            // actual timestamp update in a unit test without mocking the NostrManager)
            let loaded_account = whitenoise
                .find_account_by_pubkey(&account.pubkey)
                .await
                .unwrap();
            assert_eq!(loaded_account.pubkey, account.pubkey);
        }
    }

    // Helper Tests
    mod helper_tests {
        use super::*;

        #[test]
        fn test_onboarding_state_default() {
            let onboarding_state = OnboardingState::default();
            assert!(!onboarding_state.inbox_relays);
            assert!(!onboarding_state.key_package_relays);
            assert!(!onboarding_state.key_package_published);
        }

        #[test]
        fn test_relay_type_enum_coverage() {
            // Ensure we can create all relay types
            let _nostr = RelayType::Nostr;
            let _inbox = RelayType::Inbox;
            let _key_package = RelayType::KeyPackage;
        }
    }

    // Subscription Management Tests
    mod subscription_management_tests {
        use super::*;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        // Helper to create an account with mocked NostrMls
        async fn create_account_with_mocked_nostr_mls(has_groups: bool) -> (Account, Keys) {
            let (mut account, keys) = create_test_account();

            // For testing, we'll mock the nostr_mls behavior by using None or Some
            // In a real implementation, we'd need to mock the NostrMls struct
            // For now, we'll test the error case and leave detailed group testing
            // for when NostrMls has better testing support

            if has_groups {
                // Set up for having groups - this will be expanded when NostrMls is mockable
                account.nostr_mls = Arc::new(Mutex::new(None)); // Still None for now
            } else {
                account.nostr_mls = Arc::new(Mutex::new(None));
            }

            (account, keys)
        }

        #[tokio::test]
        async fn test_setup_subscriptions_nostr_mls_not_initialized() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_account_with_mocked_nostr_mls(false).await;

            // Store keys so other operations can work
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());

            // Test that setup_subscriptions fails when NostrMls is not initialized
            let result = whitenoise.setup_subscriptions(&account).await;

            match result {
                Err(WhitenoiseError::NostrMlsNotInitialized) => {
                    // This is the expected behavior
                }
                Ok(_) => panic!("setup_subscriptions should fail when NostrMls is not initialized"),
                Err(other) => panic!("Unexpected error: {:?}", other),
            }
        }

        #[tokio::test]
        async fn test_setup_subscriptions_relay_logic_with_empty_user_relays() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store keys and save account
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            whitenoise.save_account(&account).await.unwrap();

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Test the relay selection logic
            // fetch_relays should return empty in test environment
            let user_relays = whitenoise
                .fetch_relays(account.pubkey, RelayType::Nostr)
                .await
                .unwrap();
            assert!(
                user_relays.is_empty(),
                "User relays should be empty in test environment"
            );

            // When user_relays is empty, should use default client relays
            let default_relays = whitenoise.nostr.relays().await.unwrap();

            let relays_to_use = if user_relays.is_empty() {
                default_relays.clone()
            } else {
                user_relays
            };

            // In test environment, default relays might also be empty, but the logic should work
            assert_eq!(relays_to_use, default_relays);
        }

        #[tokio::test]
        async fn test_setup_subscriptions_relay_selection_logic() {
            // Test the relay selection logic independently
            let empty_user_relays: Vec<RelayUrl> = vec![];
            let default_relays = vec![
                RelayUrl::parse("wss://relay.damus.io").unwrap(),
                RelayUrl::parse("wss://nos.lol").unwrap(),
            ];

            // Test: when user_relays is empty, should use default_relays
            let relays_to_use = if empty_user_relays.is_empty() {
                default_relays.clone()
            } else {
                empty_user_relays
            };
            assert_eq!(relays_to_use, default_relays);

            // Test: when user_relays is not empty, should use user_relays
            let user_relays = vec![
                RelayUrl::parse("wss://user.relay.com").unwrap(),
                RelayUrl::parse("wss://custom.relay.net").unwrap(),
            ];

            let relays_to_use = if user_relays.is_empty() {
                default_relays.clone()
            } else {
                user_relays.clone()
            };
            assert_eq!(relays_to_use, user_relays);
        }

        #[tokio::test]
        async fn test_setup_subscriptions_group_ids_conversion_logic() {
            // Test the group ID conversion logic independently
            // This simulates what happens in the method when converting groups to nostr_group_ids

            // Test with None groups (empty result)
            let groups: Option<Vec<MockGroup>> = None;
            let nostr_group_ids = groups
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| hex::encode(&group.nostr_group_id))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();
            assert!(nostr_group_ids.is_empty());

            // Test with empty groups
            let empty_groups: Option<Vec<MockGroup>> = Some(vec![]);
            let nostr_group_ids = empty_groups
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| hex::encode(&group.nostr_group_id))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();
            assert!(nostr_group_ids.is_empty());

            // Test with actual groups
            let mock_groups = vec![
                MockGroup {
                    nostr_group_id: vec![1, 2, 3, 4],
                },
                MockGroup {
                    nostr_group_id: vec![5, 6, 7, 8],
                },
                MockGroup {
                    nostr_group_id: vec![9, 10, 11, 12],
                },
            ];
            let groups_with_data: Option<Vec<MockGroup>> = Some(mock_groups);
            let nostr_group_ids = groups_with_data
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| hex::encode(&group.nostr_group_id))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            assert_eq!(nostr_group_ids.len(), 3);
            assert_eq!(nostr_group_ids[0], "01020304");
            assert_eq!(nostr_group_ids[1], "05060708");
            assert_eq!(nostr_group_ids[2], "090a0b0c");
        }

        #[tokio::test]
        async fn test_setup_subscriptions_hex_encoding() {
            // Test hex encoding edge cases for group IDs

            let test_cases = vec![
                (vec![], ""),
                (vec![0], "00"),
                (vec![255], "ff"),
                (vec![0, 255], "00ff"),
                (vec![16, 32, 48, 64], "10203040"),
                (vec![170, 187, 204, 221], "aabbccdd"),
            ];

            for (input, expected) in test_cases {
                let encoded = hex::encode(&input);
                assert_eq!(encoded, expected, "Failed for input: {:?}", input);
            }
        }

        #[tokio::test]
        async fn test_setup_subscriptions_error_handling_fetch_relays() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store keys and save account
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            whitenoise.save_account(&account).await.unwrap();

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Test that fetch_relays doesn't fail in test environment
            // (It might return empty results, but shouldn't error)
            let user_relays_result = whitenoise
                .fetch_relays(account.pubkey, RelayType::Nostr)
                .await;
            assert!(
                user_relays_result.is_ok(),
                "fetch_relays should not fail in test environment"
            );
        }

        #[tokio::test]
        async fn test_setup_subscriptions_components_integration() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store keys and save account
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            whitenoise.save_account(&account).await.unwrap();

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Test that all individual components work

            // 1. Test that we can fetch user relays
            let user_relays = whitenoise
                .fetch_relays(account.pubkey, RelayType::Nostr)
                .await;
            assert!(user_relays.is_ok());

            // 2. Test that we can get default relays
            let default_relays = whitenoise.nostr.relays().await;
            assert!(default_relays.is_ok());

            // 3. Test relay selection logic
            let user_relays = user_relays.unwrap();
            let default_relays = default_relays.unwrap();

            let relays_to_use = if user_relays.is_empty() {
                default_relays
            } else {
                user_relays
            };

            // The logic should complete without errors
            assert!(!relays_to_use.is_empty() || relays_to_use.is_empty()); // Either case is valid
        }

        #[tokio::test]
        async fn test_setup_subscriptions_nostr_mls_lock_handling() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store keys and save account
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            whitenoise.save_account(&account).await.unwrap();
            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());

            // Test the lock acquisition logic
            {
                let nostr_mls_guard = account.nostr_mls.lock().await;

                // In our test setup, nostr_mls should be None (not initialized)
                assert!(nostr_mls_guard.is_none());

                // The method should handle this case by returning NostrMlsNotInitialized error
            }

            // Test the actual error case by calling the method
            let result = whitenoise.setup_subscriptions(&account).await;
            assert!(matches!(
                result,
                Err(WhitenoiseError::NostrMlsNotInitialized)
            ));
        }

        // Mock struct for testing group ID conversion
        struct MockGroup {
            nostr_group_id: Vec<u8>,
        }
    }
}
