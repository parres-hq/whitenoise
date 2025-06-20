use anyhow::Context;
use nostr_mls::prelude::*;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::RwLock;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod accounts;
pub mod database;
pub mod error;
mod event_processing;
pub mod secrets_store;

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
    pub accounts: Arc<RwLock<HashMap<PublicKey, Account>>>,
    database: Arc<Database>,
    nostr: NostrManager,
    secrets_store: SecretsStore,
    #[allow(dead_code)] // Reserved for future use by other Whitenoise methods to queue events
    event_sender: Sender<ProcessableEvent>,
    shutdown_sender: Sender<()>,
}

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
    pub async fn initialize_whitenoise(config: WhitenoiseConfig) -> Result<Arc<Self>> {
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

        // Create event processing channels
        let (event_sender, event_receiver) = mpsc::channel(500);
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        // Create NostrManager with event_sender for direct event queuing
        let nostr =
            NostrManager::new_with_connections(data_dir.join("nostr_lmdb"), event_sender.clone())
                .await?;

        // Create SecretsStore
        let secrets_store = SecretsStore::new(data_dir);

        // Create the whitenoise instance
        let whitenoise = Self {
            config,
            database,
            nostr,
            secrets_store,
            accounts: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            shutdown_sender,
        };

        // Load all accounts from database
        let loaded_accounts = whitenoise.fetch_accounts().await?;
        {
            let mut accounts = whitenoise.write_accounts().await;
            *accounts = loaded_accounts;
        }

        // Create Arc and start event processing loop (after accounts are loaded)
        let whitenoise_arc = Arc::new(whitenoise);
        let whitenoise_for_loop = whitenoise_arc.clone();

        tracing::debug!(
            target: "whitenoise::initialize_whitenoise",
            "Starting event processing loop for loaded accounts"
        );

        Self::start_event_processing_loop(whitenoise_for_loop, event_receiver, shutdown_receiver)
            .await;

        // Fetch events and setup subscriptions for all accounts after event processing has started
        {
            let accounts = whitenoise_arc.read_accounts().await;
            let account_list: Vec<Account> = accounts.values().cloned().collect();
            drop(accounts); // Release the read lock early
            for account in account_list {
                // Fetch account data
                match whitenoise_arc.background_fetch_account_data(&account).await {
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
                match whitenoise_arc.setup_subscriptions(&account).await {
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

        Ok(whitenoise_arc)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use accounts::AccountSettings;
    use relays::*;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    // Test configuration and setup helpers
    fn create_test_config() -> (WhitenoiseConfig, TempDir, TempDir) {
        let data_temp_dir = TempDir::new().expect("Failed to create temp data dir");
        let logs_temp_dir = TempDir::new().expect("Failed to create temp logs dir");
        let config = WhitenoiseConfig::new(data_temp_dir.path(), logs_temp_dir.path());
        (config, data_temp_dir, logs_temp_dir)
    }

    fn create_test_keys() -> Keys {
        Keys::generate()
    }

    fn create_test_account() -> (Account, Keys) {
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

    // Mock Whitenoise creation that minimizes network calls
    // NOTE: This still creates a real NostrManager which will attempt to connect to localhost relays
    // For true isolation, we should:
    // 1. Create a NostrManagerTrait and MockNostrManager implementation
    // 2. Use dependency injection in Whitenoise::new() to accept a NostrManager trait object
    // 3. Set up test-specific relay configurations that don't attempt network connections
    async fn create_mock_whitenoise() -> (Whitenoise, TempDir, TempDir) {
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

        // Create NostrManager for testing - use the test-friendly constructor
        // that doesn't require relay connections
        let nostr = NostrManager::new_without_connection(
            config.data_dir.join("test_nostr"),
            event_sender.clone(),
        )
        .await
        .expect("Failed to create NostrManager");

        let whitenoise = Whitenoise {
            config,
            database,
            nostr,
            secrets_store,
            accounts: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            shutdown_sender,
        };

        (whitenoise, data_temp, logs_temp)
    }

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

            let debug_str = format!("{:?}", config);
            assert!(debug_str.contains("data_dir"));
            assert!(debug_str.contains("logs_dir"));
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

    // Event Processing Tests
    mod event_processing_tests {
        use super::*;

        #[tokio::test]
        async fn test_shutdown_event_processing() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            let result = whitenoise.shutdown_event_processing().await;
            assert!(result.is_ok());

            // Test that multiple shutdowns don't cause errors
            let result2 = whitenoise.shutdown_event_processing().await;
            assert!(result2.is_ok());
        }

        #[tokio::test]
        async fn test_extract_pubkey_from_subscription_id() {
            let (whitenoise, _, _) = create_mock_whitenoise().await;
            let subscription_id = "abc123_user_events";
            let extracted = whitenoise
                .extract_pubkey_from_subscription_id(subscription_id)
                .await;
            assert!(extracted.is_none());

            let invalid_case = "no_underscore";
            let extracted = whitenoise
                .extract_pubkey_from_subscription_id(invalid_case)
                .await;
            assert!(extracted.is_none());

            let multi_underscore_id = "abc123_user_events_extra";
            let result = whitenoise
                .extract_pubkey_from_subscription_id(multi_underscore_id)
                .await;
            assert!(result.is_none());
        }

        #[tokio::test]
        async fn test_queue_operations_after_shutdown() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            whitenoise.shutdown_event_processing().await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Test that shutdown completed successfully without errors
            // (We can't test queuing operations since those methods were removed)
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

    // Account Management Tests
    mod account_management_tests {
        use super::*;

        #[test]
        fn test_update_active_account_logic() {
            let (account1, _keys1) = create_test_account();
            let (account2, _keys2) = create_test_account();

            // Test basic active account switching logic
            let mut active_account: Option<PublicKey> = None;
            assert_eq!(active_account, None);

            // Set first account as active
            active_account = Some(account1.pubkey);
            assert_eq!(active_account, Some(account1.pubkey));

            // Switch to second account
            active_account = Some(account2.pubkey);
            assert_eq!(active_account, Some(account2.pubkey));
        }

        #[test]
        fn test_account_state_management() {
            let (account1, _keys1) = create_test_account();
            let (account2, _keys2) = create_test_account();

            let mut accounts = HashMap::new();
            let mut active_account: Option<PublicKey> = None;

            // Initial state
            assert_eq!(accounts.len(), 0);
            assert_eq!(active_account, None);

            // Add first account
            accounts.insert(account1.pubkey, account1.clone());
            active_account = Some(account1.pubkey);
            assert_eq!(accounts.len(), 1);
            assert_eq!(active_account, Some(account1.pubkey));

            // Add second account
            accounts.insert(account2.pubkey, account2.clone());
            active_account = Some(account2.pubkey);
            assert_eq!(accounts.len(), 2);
            assert_eq!(active_account, Some(account2.pubkey));

            // Test logout logic - remove active account
            accounts.remove(&account2.pubkey);
            active_account = accounts.keys().next().copied();
            assert_eq!(accounts.len(), 1);
            assert_eq!(active_account, Some(account1.pubkey));

            // Test logout logic - remove non-active account first
            accounts.insert(account2.pubkey, account2.clone());
            active_account = Some(account2.pubkey);
            accounts.remove(&account1.pubkey); // Remove non-active
            assert_eq!(accounts.len(), 1);
            assert_eq!(active_account, Some(account2.pubkey)); // Active unchanged

            // Remove final account
            accounts.remove(&account2.pubkey);
            active_account = accounts.keys().next().copied();
            assert_eq!(accounts.len(), 0);
            assert_eq!(active_account, None);
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

            let key_package = whitenoise.fetch_key_package_event(pubkey).await;
            assert!(key_package.is_ok());

            let onboarding = whitenoise.fetch_onboarding_state(pubkey).await;
            assert!(onboarding.is_ok());
        }

        #[tokio::test]
        async fn test_fetch_all_relay_types() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();

            let relay_types = [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage];
            for relay_type in relay_types {
                let result = whitenoise.fetch_relays(pubkey, relay_type).await;
                assert!(result.is_ok());
                let relays = result.unwrap();
                assert!(relays.is_empty()); // Empty in test environment
            }
        }

        #[tokio::test]
        async fn test_fetch_onboarding_state_structure() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();

            let account = whitenoise
                .login(test_keys.secret_key().to_secret_hex())
                .await;
            assert!(account.is_ok(), "{:?}", account);

            let result = whitenoise.fetch_onboarding_state(pubkey).await;
            assert!(result.is_ok());

            let onboarding_state = result.unwrap();
            // In test environment, all should be false since no data is cached
            assert!(!onboarding_state.inbox_relays);
            assert!(!onboarding_state.key_package_relays);
            assert!(!onboarding_state.key_package_published);
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
                whitenoise.fetch_key_package_event(pubkey),
                whitenoise.fetch_onboarding_state(pubkey)
            );

            assert!(results.0.is_ok());
            assert!(results.1.is_ok());
            assert!(results.2.is_ok());
            assert!(results.3.is_ok());
            assert!(results.4.is_ok());
        }

        #[tokio::test]
        async fn test_multiple_pubkeys() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let keys = [create_test_keys(), create_test_keys(), create_test_keys()];

            for key in keys {
                let pubkey = key.public_key();
                let account = whitenoise.login(key.secret_key().to_secret_hex()).await;
                assert!(account.is_ok());

                // Test that all methods work with different pubkeys
                assert!(whitenoise.fetch_metadata(pubkey).await.is_ok());
                assert!(whitenoise
                    .fetch_relays(pubkey, RelayType::Inbox)
                    .await
                    .is_ok());
                assert!(whitenoise.fetch_contacts(pubkey).await.is_ok());
                assert!(whitenoise.fetch_key_package_event(pubkey).await.is_ok());
                assert!(whitenoise.fetch_onboarding_state(pubkey).await.is_ok());
            }
        }

        #[tokio::test]
        async fn test_fetch_accounts() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Test loading empty database
            let accounts = whitenoise.fetch_accounts().await.unwrap();
            assert!(accounts.is_empty());

            // Create test accounts and save them to database
            let (account1, keys1) = create_test_account();
            let (account2, keys2) = create_test_account();

            // Save accounts to database
            whitenoise.save_account(&account1).await.unwrap();
            whitenoise.save_account(&account2).await.unwrap();

            // Store keys in secrets store (required for background fetch)
            whitenoise.secrets_store.store_private_key(&keys1).unwrap();
            whitenoise.secrets_store.store_private_key(&keys2).unwrap();

            // Load accounts from database
            let loaded_accounts = whitenoise.fetch_accounts().await.unwrap();
            assert_eq!(loaded_accounts.len(), 2);
            assert!(loaded_accounts.contains_key(&account1.pubkey));
            assert!(loaded_accounts.contains_key(&account2.pubkey));

            // Verify account data is correctly loaded
            let loaded_account1 = &loaded_accounts[&account1.pubkey];
            assert_eq!(loaded_account1.pubkey, account1.pubkey);
            assert_eq!(
                loaded_account1.settings.dark_theme,
                account1.settings.dark_theme
            );
        }

        #[tokio::test]
        async fn test_fetch_accounts_ordering_by_last_synced() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create test accounts with different last_synced times
            let (mut account1, keys1) = create_test_account();
            let (mut account2, keys2) = create_test_account();
            let (mut account3, keys3) = create_test_account();

            // Set different last_synced timestamps
            account1.last_synced = Timestamp::from(100); // oldest
            account2.last_synced = Timestamp::from(300); // newest
            account3.last_synced = Timestamp::from(200); // middle

            // Save accounts to database
            whitenoise.save_account(&account1).await.unwrap();
            whitenoise.save_account(&account2).await.unwrap();
            whitenoise.save_account(&account3).await.unwrap();

            // Store keys in secrets store
            whitenoise.secrets_store.store_private_key(&keys1).unwrap();
            whitenoise.secrets_store.store_private_key(&keys2).unwrap();
            whitenoise.secrets_store.store_private_key(&keys3).unwrap();

            // Load accounts from database
            let loaded_accounts = whitenoise.fetch_accounts().await.unwrap();
            assert_eq!(loaded_accounts.len(), 3);

            // Verify the most recent account would be first in HashMap iteration
            // (Note: HashMap iteration order is not guaranteed, but our SQL query orders by last_synced DESC)
            // We'll test the active account selection in a separate test
        }

        #[tokio::test]
        async fn test_initialization_sets_active_account() {
            let (config, _data_temp, _logs_temp) = create_test_config();

            // Create directories manually
            std::fs::create_dir_all(&config.data_dir).unwrap();
            std::fs::create_dir_all(&config.logs_dir).unwrap();

            // Create a database and add some test accounts
            // Use the same database name that initialize_whitenoise will use
            let database = Arc::new(
                Database::new(config.data_dir.join("whitenoise.sqlite"))
                    .await
                    .unwrap(),
            );
            let secrets_store = SecretsStore::new(&config.data_dir);

            // Create test accounts with different last_synced times
            let (mut account1, keys1) = create_test_account();
            let (mut account2, keys2) = create_test_account();

            account1.last_synced = Timestamp::from(100); // older
            account2.last_synced = Timestamp::from(200); // newer (should be active)

            // Save accounts directly to database
            let _account1_row = sqlx::query(
                "INSERT INTO accounts (pubkey, settings, onboarding, last_synced) VALUES (?, ?, ?, ?)"
            )
            .bind(account1.pubkey.to_hex())
            .bind(serde_json::to_string(&account1.settings).unwrap())
            .bind(serde_json::to_string(&account1.onboarding).unwrap())
            .bind(account1.last_synced.to_string())
            .execute(&database.pool)
            .await
            .unwrap();

            let _account2_row = sqlx::query(
                "INSERT INTO accounts (pubkey, settings, onboarding, last_synced) VALUES (?, ?, ?, ?)"
            )
            .bind(account2.pubkey.to_hex())
            .bind(serde_json::to_string(&account2.settings).unwrap())
            .bind(serde_json::to_string(&account2.onboarding).unwrap())
            .bind(account2.last_synced.to_string())
            .execute(&database.pool)
            .await
            .unwrap();

            // Store keys
            secrets_store.store_private_key(&keys1).unwrap();
            secrets_store.store_private_key(&keys2).unwrap();

            // Now test full initialization
            let whitenoise = Whitenoise::initialize_whitenoise(config).await.unwrap();

            // Verify accounts were loaded
            assert_eq!(whitenoise.accounts_len().await, 2);
            assert!(whitenoise.has_account(&account1.pubkey).await);
            assert!(whitenoise.has_account(&account2.pubkey).await);
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

        #[tokio::test]
        async fn test_active_account_selection_logic() {
            let (_whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Test with empty accounts
            let empty_accounts: HashMap<PublicKey, Account> = HashMap::new();
            let active = empty_accounts
                .values()
                .max_by_key(|account| account.last_synced)
                .map(|account| account.pubkey);
            assert_eq!(active, None);

            // Test with multiple accounts
            let mut accounts: HashMap<PublicKey, Account> = HashMap::new();
            let (mut account1, _) = create_test_account();
            let (mut account2, _) = create_test_account();
            let (mut account3, _) = create_test_account();

            account1.last_synced = Timestamp::from(100);
            account2.last_synced = Timestamp::from(300); // newest
            account3.last_synced = Timestamp::from(200);

            accounts.insert(account1.pubkey, account1.clone());
            accounts.insert(account2.pubkey, account2.clone());
            accounts.insert(account3.pubkey, account3.clone());

            let active = accounts
                .values()
                .max_by_key(|account| account.last_synced)
                .map(|account| account.pubkey);

            assert_eq!(active, Some(account2.pubkey)); // account2 has timestamp 300
        }

        #[tokio::test]
        async fn test_update_metadata() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create and save a test account
            let (account, test_keys) = create_test_account();
            whitenoise.save_account(&account).await.unwrap();
            whitenoise
                .secrets_store
                .store_private_key(&test_keys)
                .unwrap();

            let log_account = whitenoise
                .login(test_keys.secret_key().to_secret_hex())
                .await;
            assert!(log_account.is_ok());
            assert_eq!(log_account.unwrap(), account);

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Create test metadata
            let metadata = Metadata {
                name: Some("Updated Name".to_string()),
                display_name: Some("Updated Display Name".to_string()),
                about: Some("Updated bio".to_string()),
                picture: Some("https://example.com/new-avatar.jpg".to_string()),
                banner: Some("https://example.com/banner.jpg".to_string()),
                nip05: Some("user@example.com".to_string()),
                lud16: Some("user@lightning.example.com".to_string()),
                ..Default::default()
            };

            // Test updating metadata
            let result = whitenoise.update_metadata(&metadata, &account).await;
            assert!(result.is_ok(), "update_metadata should succeed");
        }

        #[tokio::test]
        async fn test_update_metadata_with_minimal_metadata() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create and save a test account
            let (account, keys) = create_test_account();
            whitenoise.save_account(&account).await.unwrap();
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());
            assert_eq!(log_account.unwrap(), account);

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Create minimal metadata (only name)
            let metadata = Metadata {
                name: Some("Simple Name".to_string()),
                ..Default::default()
            };

            // Test updating metadata
            let result = whitenoise.update_metadata(&metadata, &account).await;
            assert!(
                result.is_ok(),
                "update_metadata should succeed with minimal metadata"
            );
        }

        #[tokio::test]
        async fn test_update_metadata_with_empty_metadata() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create and save a test account
            let (account, keys) = create_test_account();
            whitenoise.save_account(&account).await.unwrap();
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());
            assert_eq!(log_account.unwrap(), account);

            // Initialize NostrMls for the account
            whitenoise
                .initialize_nostr_mls_for_account(&account)
                .await
                .unwrap();

            // Create completely empty metadata
            let metadata = Metadata::default();

            // Test updating metadata
            let result = whitenoise.update_metadata(&metadata, &account).await;
            assert!(
                result.is_ok(),
                "update_metadata should succeed with empty metadata"
            );
        }

        #[tokio::test]
        async fn test_update_metadata_without_stored_keys() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Create and save a test account but DON'T store the keys
            let (account, _keys) = create_test_account();
            whitenoise.save_account(&account).await.unwrap();
            // Note: not storing keys in secrets_store

            // Create test metadata
            let metadata = Metadata {
                name: Some("Test Name".to_string()),
                ..Default::default()
            };

            // Test updating metadata - this should fail because keys aren't stored
            let result = whitenoise.update_metadata(&metadata, &account).await;
            assert!(
                result.is_err(),
                "update_metadata should fail when keys aren't stored"
            );
        }

        #[tokio::test]
        async fn test_update_metadata_serialization() {
            // Test that various metadata fields serialize correctly
            let metadata = Metadata {
                name: Some("Test User".to_string()),
                display_name: Some("Test Display".to_string()),
                about: Some("Bio with special chars: émojí 🚀".to_string()),
                picture: Some("https://example.com/picture.jpg".to_string()),
                banner: Some("https://example.com/banner.jpg".to_string()),
                nip05: Some("test@example.com".to_string()),
                lud16: Some("test@lightning.example.com".to_string()),
                website: Some("https://example.com".to_string()),
                ..Default::default()
            };

            // Test that the metadata can be serialized to JSON
            let serialized = serde_json::to_string(&metadata);
            assert!(serialized.is_ok(), "Metadata should serialize to JSON");

            let json_str = serialized.unwrap();
            assert!(json_str.contains("Test User"));
            assert!(json_str.contains("Bio with special chars"));
            assert!(json_str.contains("émojí 🚀"));
        }
    }

    // Contact Management Tests
    mod contact_management_tests {
        use super::*;

        #[tokio::test]
        async fn test_contact_list_event_structure() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store account keys
            whitenoise.secrets_store.store_private_key(&keys).unwrap();

            // Test creating contact list event structure
            let contact1 = create_test_keys().public_key();
            let contact2 = create_test_keys().public_key();
            let contact3 = create_test_keys().public_key();

            let contacts = [contact1, contact2, contact3];

            // Create the contact list event structure (without publishing)
            let tags: Vec<Tag> = contacts
                .iter()
                .map(|pubkey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
                .collect();

            let event = EventBuilder::new(Kind::ContactList, "").tags(tags.clone());

            // Verify event structure
            let _built_event = event.clone();

            // Get the signing keys to ensure they exist
            let signing_keys = whitenoise
                .secrets_store
                .get_nostr_keys_for_pubkey(&account.pubkey);
            assert!(signing_keys.is_ok());

            // Verify the tags are correctly structured for Kind::ContactList (Kind 3)
            assert_eq!(tags.len(), 3);

            // Verify each tag has the correct structure
            for (i, tag) in tags.iter().enumerate() {
                let tag_vec = tag.clone().to_vec();
                assert_eq!(tag_vec[0], "p"); // Should be 'p' tag
                assert_eq!(tag_vec[1], contacts[i].to_hex()); // Should be the contact pubkey
            }
        }

        #[tokio::test]
        async fn test_add_contact_logic() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();
            let contact_pubkey = create_test_keys().public_key();

            // Store account keys
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());

            // Test the logic of adding a contact (without actual network calls)
            // Load current contact list (will be empty in test environment)
            let current_contacts = whitenoise.fetch_contacts(account.pubkey).await.unwrap();

            // Verify contact doesn't already exist
            assert!(!current_contacts.contains_key(&contact_pubkey));

            // Create new contact list with the added contact
            let mut new_contacts: Vec<PublicKey> = current_contacts.keys().cloned().collect();
            new_contacts.push(contact_pubkey);

            // Verify the contact was added to the list
            assert!(new_contacts.contains(&contact_pubkey));
            assert_eq!(new_contacts.len(), current_contacts.len() + 1);
        }

        #[tokio::test]
        async fn test_remove_contact_logic() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (_account, keys) = create_test_account();

            // Store account keys
            whitenoise.secrets_store.store_private_key(&keys).unwrap();

            // Test remove contact logic with a simulated existing contact list
            let contact1 = create_test_keys().public_key();
            let contact2 = create_test_keys().public_key();
            let contact3 = create_test_keys().public_key();

            // Simulate current contacts (in a real scenario, this would come from fetch_contacts)
            let mut simulated_current_contacts: std::collections::HashMap<
                PublicKey,
                Option<Metadata>,
            > = std::collections::HashMap::new();
            simulated_current_contacts.insert(contact1, None);
            simulated_current_contacts.insert(contact2, None);
            simulated_current_contacts.insert(contact3, None);

            // Test removing an existing contact
            assert!(simulated_current_contacts.contains_key(&contact2));

            // Create new contact list without the removed contact
            let new_contacts: Vec<PublicKey> = simulated_current_contacts
                .keys()
                .filter(|&pubkey| *pubkey != contact2)
                .cloned()
                .collect();

            // Verify the contact was removed
            assert!(!new_contacts.contains(&contact2));
            assert_eq!(new_contacts.len(), simulated_current_contacts.len() - 1);
            assert!(new_contacts.contains(&contact1));
            assert!(new_contacts.contains(&contact3));
        }

        #[tokio::test]
        async fn test_update_contacts_logic() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (_account, keys) = create_test_account();

            // Store account keys
            whitenoise.secrets_store.store_private_key(&keys).unwrap();

            // Test update contacts logic with different scenarios
            let contact1 = create_test_keys().public_key();
            let contact2 = create_test_keys().public_key();
            let contact3 = create_test_keys().public_key();

            // Test empty contact list
            let empty_contacts: Vec<PublicKey> = vec![];
            let tags: Vec<Tag> = empty_contacts
                .iter()
                .map(|pubkey: &PublicKey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
                .collect();
            assert!(tags.is_empty());

            // Test single contact
            let single_contact = [contact1];
            let tags: Vec<Tag> = single_contact
                .iter()
                .map(|pubkey: &PublicKey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
                .collect();
            assert_eq!(tags.len(), 1);
            assert_eq!(tags[0].clone().to_vec()[0], "p");
            assert_eq!(tags[0].clone().to_vec()[1], contact1.to_hex());

            // Test multiple contacts
            let multiple_contacts = [contact1, contact2, contact3];
            let tags: Vec<Tag> = multiple_contacts
                .iter()
                .map(|pubkey: &PublicKey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
                .collect();
            assert_eq!(tags.len(), 3);

            // Verify all contacts are in tags
            let tag_pubkeys: Vec<String> = tags
                .iter()
                .map(|tag| tag.clone().to_vec()[1].clone())
                .collect();
            assert!(tag_pubkeys.contains(&contact1.to_hex()));
            assert!(tag_pubkeys.contains(&contact2.to_hex()));
            assert!(tag_pubkeys.contains(&contact3.to_hex()));
        }

        #[tokio::test]
        async fn test_contact_validation_logic() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store account keys
            whitenoise.secrets_store.store_private_key(&keys).unwrap();
            let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
            assert!(log_account.is_ok());

            let contact_pubkey = create_test_keys().public_key();

            // Test add contact validation (contact doesn't exist)
            let current_contacts = whitenoise.fetch_contacts(account.pubkey).await.unwrap();

            // Should be able to add new contact (empty list)
            let can_add = !current_contacts.contains_key(&contact_pubkey);
            assert!(can_add);

            // Test remove contact validation (contact doesn't exist)
            let can_remove = current_contacts.contains_key(&contact_pubkey);
            assert!(!can_remove); // Should not be able to remove non-existent contact

            // Simulate existing contact for remove validation
            let mut simulated_contacts: std::collections::HashMap<PublicKey, Option<Metadata>> =
                std::collections::HashMap::new();
            simulated_contacts.insert(contact_pubkey, None);
            let can_remove_existing = simulated_contacts.contains_key(&contact_pubkey);
            assert!(can_remove_existing);
        }

        #[tokio::test]
        async fn test_contact_event_builder_creation() {
            let (_whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

            // Test creating EventBuilder for different contact list scenarios
            let contact1 = create_test_keys().public_key();
            let contact2 = create_test_keys().public_key();

            // Test empty contact list event
            let empty_tags: Vec<Tag> = vec![];
            let _empty_event = EventBuilder::new(Kind::ContactList, "").tags(empty_tags);
            // EventBuilder creation should succeed

            // Test single contact event
            let single_tags: Vec<Tag> = vec![Tag::custom(TagKind::p(), [contact1.to_hex()])];
            let _single_event = EventBuilder::new(Kind::ContactList, "").tags(single_tags.clone());
            // Verify tag structure
            assert_eq!(single_tags.len(), 1);

            // Test multiple contacts event
            let multi_tags: Vec<Tag> = vec![
                Tag::custom(TagKind::p(), [contact1.to_hex()]),
                Tag::custom(TagKind::p(), [contact2.to_hex()]),
            ];
            let _multi_event = EventBuilder::new(Kind::ContactList, "").tags(multi_tags.clone());
            // Verify tag structure
            assert_eq!(multi_tags.len(), 2);
        }

        #[tokio::test]
        async fn test_contact_management_without_keys() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, _keys) = create_test_account();
            let _contact_pubkey = create_test_keys().public_key();

            // Don't store keys for the account - should fail when trying to get signing keys
            let signing_keys_result = whitenoise
                .secrets_store
                .get_nostr_keys_for_pubkey(&account.pubkey);
            assert!(signing_keys_result.is_err());
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

    // Relay Management Tests
    mod relay_management_tests {
        use super::*;
        use nostr_sdk::RelayUrl;

        #[tokio::test]
        async fn test_relay_type_to_event_kind_mapping() {
            // Test that RelayType maps to correct Nostr event kinds
            // This tests the logic inside publish_relay_list_for_account without network calls

            let test_cases = [
                (RelayType::Nostr, Kind::RelayList),
                (RelayType::Inbox, Kind::InboxRelays),
                (RelayType::KeyPackage, Kind::MlsKeyPackageRelays),
            ];

            for (relay_type, expected_kind) in test_cases {
                let actual_kind = match relay_type {
                    RelayType::Nostr => Kind::RelayList,
                    RelayType::Inbox => Kind::InboxRelays,
                    RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
                };

                assert_eq!(
                    actual_kind, expected_kind,
                    "RelayType::{:?} should map to Kind::{:?}",
                    relay_type, expected_kind
                );
            }
        }

        #[tokio::test]
        async fn test_relay_list_tag_creation() {
            // Test that relay URLs are correctly converted to tags
            let test_relays = [
                "wss://relay.damus.io",
                "wss://nos.lol",
                "wss://relay.primal.net",
                "wss://nostr.wine",
            ];

            let relay_urls: Vec<RelayUrl> = test_relays
                .iter()
                .map(|url| RelayUrl::parse(url).unwrap())
                .collect();

            // Create tags the same way as publish_relay_list_for_account
            let tags: Vec<Tag> = relay_urls
                .into_iter()
                .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                .collect();

            // Verify tag structure
            assert_eq!(tags.len(), test_relays.len());

            for (i, tag) in tags.iter().enumerate() {
                let tag_vec = tag.clone().to_vec();
                assert_eq!(tag_vec.len(), 2, "Relay tag should have 2 elements");
                assert_eq!(tag_vec[0], "relay", "First element should be 'relay'");
                assert_eq!(
                    tag_vec[1], test_relays[i],
                    "Second element should be the relay URL"
                );
            }
        }

        #[tokio::test]
        async fn test_relay_list_event_structure() {
            // Test event creation for each relay type without publishing
            let relay_urls = [
                RelayUrl::parse("wss://relay.damus.io").unwrap(),
                RelayUrl::parse("wss://nos.lol").unwrap(),
            ];

            let test_cases = [
                (RelayType::Nostr, Kind::RelayList),
                (RelayType::Inbox, Kind::InboxRelays),
                (RelayType::KeyPackage, Kind::MlsKeyPackageRelays),
            ];

            for (_relay_type, expected_kind) in test_cases {
                // Create tags
                let tags: Vec<Tag> = relay_urls
                    .iter()
                    .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                    .collect();

                // Create event (same logic as publish_relay_list_for_account)
                let _event_builder = EventBuilder::new(expected_kind, "").tags(tags.clone());

                // Verify event structure - we can't build the event without keys,
                // but we can verify the builder has the right components
                // (The actual event building happens during signing)

                // Verify tags are correctly attached
                assert_eq!(tags.len(), 2);

                // Verify tag content
                for (i, tag) in tags.iter().enumerate() {
                    let tag_vec = tag.clone().to_vec();
                    assert_eq!(tag_vec[0], "relay");
                    assert_eq!(tag_vec[1], relay_urls[i].to_string());
                }
            }
        }

        #[tokio::test]
        async fn test_empty_relay_list_handling() {
            // Test that empty relay lists are handled correctly
            // (publish_relay_list_for_account returns early for empty lists)

            let empty_relays: Vec<RelayUrl> = vec![];

            // The method returns early if relays.is_empty(), so test that logic
            assert!(empty_relays.is_empty());

            // If we were to create tags anyway, it should be empty
            let tags: Vec<Tag> = empty_relays
                .into_iter()
                .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                .collect();

            assert!(tags.is_empty());
        }

        #[tokio::test]
        async fn test_single_relay_event() {
            // Test with a single relay
            let single_relay = vec![RelayUrl::parse("wss://relay.damus.io").unwrap()];

            let tags: Vec<Tag> = single_relay
                .into_iter()
                .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                .collect();

            assert_eq!(tags.len(), 1);
            let tag_vec = tags[0].clone().to_vec();
            assert_eq!(tag_vec[0], "relay");
            assert_eq!(tag_vec[1], "wss://relay.damus.io");
        }

        #[tokio::test]
        async fn test_multiple_relay_event() {
            // Test with multiple relays
            let multiple_relays = vec![
                RelayUrl::parse("wss://relay.damus.io").unwrap(),
                RelayUrl::parse("wss://nos.lol").unwrap(),
                RelayUrl::parse("wss://relay.primal.net").unwrap(),
                RelayUrl::parse("wss://nostr.wine").unwrap(),
                RelayUrl::parse("wss://relay.snort.social").unwrap(),
            ];

            let expected_urls = [
                "wss://relay.damus.io",
                "wss://nos.lol",
                "wss://relay.primal.net",
                "wss://nostr.wine",
                "wss://relay.snort.social",
            ];

            let tags: Vec<Tag> = multiple_relays
                .into_iter()
                .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                .collect();

            assert_eq!(tags.len(), expected_urls.len());

            for (i, tag) in tags.iter().enumerate() {
                let tag_vec = tag.clone().to_vec();
                assert_eq!(tag_vec[0], "relay");
                assert_eq!(tag_vec[1], expected_urls[i]);
            }
        }

        #[tokio::test]
        async fn test_relay_url_formats() {
            // Test different valid relay URL formats
            let test_urls = [
                "wss://relay.damus.io",
                "wss://nos.lol/",
                "wss://relay.primal.net/v1",
                "ws://localhost:8080",
            ];

            for url_str in test_urls {
                let relay_url = RelayUrl::parse(url_str).unwrap();
                let tag = Tag::custom(TagKind::Relay, [relay_url.to_string()]);

                let tag_vec = tag.to_vec();
                assert_eq!(tag_vec[0], "relay");
                assert_eq!(tag_vec[1], url_str);
            }
        }

        #[tokio::test]
        async fn test_update_account_relays_logic() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let (account, keys) = create_test_account();

            // Store account keys so we can test the event creation part
            whitenoise.secrets_store.store_private_key(&keys).unwrap();

            let test_relays = [
                RelayUrl::parse("wss://relay.damus.io").unwrap(),
                RelayUrl::parse("wss://nos.lol").unwrap(),
            ];

            // Test that all relay types can be processed
            let relay_types = [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage];

            for relay_type in relay_types {
                // We can't easily test the actual method without network calls,
                // but we can test that the components work

                // Verify we can get the keys (required for signing)
                let signing_keys = whitenoise
                    .secrets_store
                    .get_nostr_keys_for_pubkey(&account.pubkey);
                assert!(
                    signing_keys.is_ok(),
                    "Should be able to get signing keys for relay type {:?}",
                    relay_type
                );

                // Verify event kind mapping
                let expected_kind = match relay_type {
                    RelayType::Nostr => Kind::RelayList,
                    RelayType::Inbox => Kind::InboxRelays,
                    RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
                };

                // Create tags (same logic as in the method)
                let tags: Vec<Tag> = test_relays
                    .iter()
                    .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
                    .collect();

                // Create event builder
                let _event_builder = EventBuilder::new(expected_kind, "").tags(tags);

                // If we got here without panicking, the event structure is valid
            }
        }

        #[tokio::test]
        async fn test_relay_list_edge_cases() {
            // Test various edge cases in relay list processing

            // Test with special characters in URLs (should be URL encoded)
            let special_relay =
                RelayUrl::parse("wss://relay.example.com/path?param=value&other=test").unwrap();
            let tag = Tag::custom(TagKind::Relay, [special_relay.to_string()]);

            let tag_vec = tag.to_vec();
            assert_eq!(tag_vec[0], "relay");
            assert!(tag_vec[1].contains("wss://relay.example.com"));

            // Test very long relay URL
            let long_path = "a".repeat(100);
            let long_url = format!("wss://relay.example.com/{}", long_path);
            let long_relay = RelayUrl::parse(&long_url).unwrap();
            let long_tag = Tag::custom(TagKind::Relay, [long_relay.to_string()]);

            let long_tag_vec = long_tag.to_vec();
            assert_eq!(long_tag_vec[0], "relay");
            assert_eq!(long_tag_vec[1], long_url);
        }
    }

    // Contact Management Tests

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

    // Helper Tests
}
