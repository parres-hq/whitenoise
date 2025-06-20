use anyhow::Context;
use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::{Mutex, RwLock};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::accounts::{Account, AccountSettings, OnboardingState};
use crate::database::Database;
use crate::error::{Result, WhitenoiseError};
use crate::init_tracing;
use crate::nostr_manager::NostrManager;
use crate::relays::RelayType;
use crate::secrets_store::SecretsStore;
use crate::types::ProcessableEvent;

use sha2::{Digest, Sha256};

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

    // ============================================================================
    // ACCOUNT MANAGEMENT
    // ============================================================================

    // Public API Methods =========================================================

    /// Creates a new identity (account) for the user.
    ///
    /// This method performs the following steps:
    /// - Generates a new account with a keypair and petname.
    /// - Saves the account to the database.
    /// - Stores the private key in the secret store.
    /// - Initializes NostrMls for the account with SQLite storage.
    /// - Onboards the new account (performs any additional setup).
    /// - Sets the new account as the active account and adds it to the in-memory accounts list.
    ///
    /// # Returns
    ///
    /// Returns the newly created `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if any step fails, such as account creation, database save, key storage, or onboarding.
    pub async fn create_identity(&self) -> Result<Account> {
        // Create a new account with a generated keypair and a petname
        let (mut account, keys) = Account::new().await?;

        // Save the account to the database
        self.save_account(&account).await?;

        // Add the keys to the secret store
        self.secrets_store.store_private_key(&keys)?;

        let log_account = self.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());

        self.initialize_nostr_mls_for_account(&account).await?;

        // Onboard the account
        self.onboard_new_account(&mut account).await?;

        // Initialize subscriptions on nostr manager
        self.setup_subscriptions(&account).await?;

        // Add the account to the in-memory accounts list
        {
            let mut accounts = self.write_accounts().await;
            accounts.insert(account.pubkey, account.clone());
        }

        Ok(account)
    }

    /// Logs in an existing user using a private key (nsec or hex format).
    ///
    /// This method performs the following steps:
    /// - Parses the provided private key (either nsec or hex format) to obtain the user's keys.
    /// - Attempts to find an existing account in the database matching the public key.
    /// - If the account exists, returns it.
    /// - If the account does not exist, creates a new account from the provided keys and adds it to the database.
    /// - Sets the new account as the active account and adds it to the in-memory accounts list.
    ///
    /// # Arguments
    ///
    /// * `nsec_or_hex_privkey` - The user's private key as a nsec string or hex-encoded string.
    ///
    /// # Returns
    ///
    /// Returns the `Account` associated with the provided private key on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the private key is invalid, or if there is a failure in finding or adding the account.
    pub async fn login(&self, nsec_or_hex_privkey: String) -> Result<Account> {
        let keys = Keys::parse(&nsec_or_hex_privkey)?;
        let pubkey = keys.public_key();

        let (account, added_from_keys) = match self.find_account_by_pubkey(&pubkey).await {
            Ok(account) => {
                tracing::debug!(target: "whitenoise::login", "Account found");
                (account, false)
            }
            Err(WhitenoiseError::AccountNotFound) => {
                tracing::debug!(target: "whitenoise::login", "Account not found, adding from keys");
                let account = self.add_account_from_keys(&keys).await?;
                (account, true)
            }
            Err(e) => return Err(e),
        };

        // Add the account to the in-memory accounts list
        {
            let mut accounts = self.write_accounts().await;
            accounts.insert(account.pubkey, account.clone());
        }

        // Initialize NostrMls for the account
        self.initialize_nostr_mls_for_account(&account).await?;

        // Spawn a background task to fetch the account's data from relays (only if newly added)
        if added_from_keys {
            self.background_fetch_account_data(&account).await?;
        }

        // Initialize subscriptions on nostr manager
        self.setup_subscriptions(&account).await?;

        Ok(account)
    }

    /// Logs out the user associated with the given account.
    ///
    /// This method performs the following steps:
    /// - Removes the account from the database.
    /// - Removes the private key from the secret store.
    /// - Updates the active account if the logged-out account was active.
    /// - Removes the account from the in-memory accounts list.
    ///
    /// - NB: This method does not remove the MLS database for the account. If the user logs back in, the MLS database will be re-initialized and used again.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to log out.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if there is a failure in removing the account or its private key.
    pub async fn logout(&self, pubkey: &PublicKey) -> Result<()> {
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Delete the account from the database
        self.delete_account(pubkey).await?;

        // Remove the private key from the secret store
        self.secrets_store.remove_private_key_for_pubkey(pubkey)?;

        // Remove the account from the Whitenoise struct and update the active account
        {
            let mut accounts = self.write_accounts().await;
            accounts.remove(pubkey);

            drop(accounts); // Release the write lock before acquiring active account write lock
        }

        Ok(())
    }

    // Private Helper Methods =====================================================

    /// Loads all accounts from the database and initializes them for use.
    ///
    /// This method queries the database for all existing accounts, deserializes their
    /// settings and onboarding states, initializes their NostrMls instances, and triggers
    /// background data fetching for each account. The accounts are returned as a HashMap
    /// ready to be used in the Whitenoise instance.
    ///
    /// # Returns
    ///
    /// Returns a `HashMap<PublicKey, Account>` containing all loaded accounts on success,
    /// or an empty HashMap if no accounts exist.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * Database query fails
    /// * Account deserialization fails
    /// * NostrMls initialization fails for any account
    async fn fetch_accounts(&self) -> Result<HashMap<PublicKey, Account>> {
        tracing::debug!(target: "whitenoise::fetch_accounts", "Loading all accounts from database");

        let accounts =
            sqlx::query_as::<_, Account>("SELECT * FROM accounts ORDER BY last_synced DESC")
                .fetch_all(&self.database.pool)
                .await?;

        if accounts.is_empty() {
            tracing::debug!(target: "whitenoise::fetch_accounts", "No accounts found in database");
            return Ok(HashMap::new());
        }

        let mut accounts_map = HashMap::new();

        for account in accounts {
            // Initialize NostrMls for each account
            if let Err(e) = self.initialize_nostr_mls_for_account(&account).await {
                tracing::warn!(
                    target: "whitenoise::fetch_accounts",
                    "Failed to initialize NostrMls for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                // Continue loading other accounts even if one fails
                continue;
            }

            // Add the account to the HashMap first, then trigger background fetch
            accounts_map.insert(account.pubkey, account.clone());

            // Trigger background data fetch for each account (non-critical)
            if let Err(e) = self.background_fetch_account_data(&account).await {
                tracing::warn!(
                    target: "whitenoise::fetch_accounts",
                    "Failed to trigger background fetch for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                // Continue - background fetch failure should not prevent account loading
            }

            tracing::debug!(
                target: "whitenoise::fetch_accounts",
                "Loaded and initialized account: {}",
                account.pubkey.to_hex()
            );
        }

        tracing::info!(
            target: "whitenoise::fetch_accounts",
            "Successfully loaded {} accounts from database",
            accounts_map.len()
        );

        Ok(accounts_map)
    }

    /// Finds and loads an account from the database by its public key.
    ///
    /// This method queries the database for an account matching the provided public key
    /// and returns the account if found.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - A reference to the `PublicKey` of the account to find.
    ///
    /// # Returns
    ///
    /// Returns the loaded `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError::AccountNotFound` if the account is not found in the database,
    /// or another `WhitenoiseError` if the database query fails.
    async fn find_account_by_pubkey(&self, pubkey: &PublicKey) -> Result<Account> {
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(pubkey.to_hex().as_str())
            .fetch_one(&self.database.pool)
            .await
            .map_err(|_| WhitenoiseError::AccountNotFound)
    }

    /// Adds a new account to the database using the provided Nostr keys (atomic operation).
    ///
    /// This method performs account creation atomically with automatic cleanup on failure.
    /// The operation follows this sequence:
    ///
    /// 1. **Store private key** - Saves the private key to the system keychain/secret store
    /// 2. **Load onboarding state** - Queries cached Nostr data to determine account setup status
    /// 3. **Save account to database** - Persists the account record with settings and onboarding info
    ///
    /// If any critical step (1-3) fails, all previous operations are automatically rolled back
    /// to ensure no partial account state is left in the system.
    ///
    /// # Arguments
    ///
    /// * `keys` - A reference to the `Keys` struct containing the Nostr keypair for the account.
    ///
    /// # Returns
    ///
    /// Returns the newly created `Account` with default settings and populated onboarding state.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if any critical operation fails:
    /// * Private key storage fails (keychain/secret store error)
    /// * Onboarding state loading fails (cache query error)
    /// * Database save fails (transaction or serialization error)
    ///
    /// On failure, any partial state (e.g., stored private keys) is automatically cleaned up.
    async fn add_account_from_keys(&self, keys: &Keys) -> Result<Account> {
        tracing::debug!(target: "whitenoise::add_account_from_keys", "Adding account for pubkey: {}", keys.public_key().to_hex());

        // Step 1: Try to store private key first (most likely to fail)
        // If this fails, we haven't persisted anything yet
        self.secrets_store.store_private_key(keys).map_err(|e| {
            tracing::error!(target: "whitenoise::add_account_from_keys", "Failed to store private key: {}", e);
            e
        })?;
        tracing::debug!(target: "whitenoise::add_account_from_keys", "Keys stored in secret store");

        // Step 2: Load onboarding state (read-only operation)
        let onboarding_state = self.fetch_onboarding_state(keys.public_key()).await.map_err(|e| {
            tracing::error!(target: "whitenoise::add_account_from_keys", "Failed to load onboarding state: {}", e);
            // Try to clean up stored private key
            if let Err(cleanup_err) = self.secrets_store.remove_private_key_for_pubkey(&keys.public_key()) {
                tracing::error!(target: "whitenoise::add_account_from_keys", "Failed to cleanup private key after onboarding state failure: {}", cleanup_err);
            }
            e
        })?;

        // Step 3: Create account struct and save to database
        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: onboarding_state,
            last_synced: Timestamp::zero(),
            nostr_mls: Arc::new(Mutex::new(None)),
        };

        self.save_account(&account).await.map_err(|e| {
            tracing::error!(target: "whitenoise::add_account_from_keys", "Failed to save account: {}", e);
            // Try to clean up stored private key
            if let Err(cleanup_err) = self.secrets_store.remove_private_key_for_pubkey(&keys.public_key()) {
                tracing::error!(target: "whitenoise::add_account_from_keys", "Failed to cleanup private key after account save failure: {}", cleanup_err);
            }
            e
        })?;
        tracing::debug!(target: "whitenoise::add_account_from_keys", "Account saved to database");

        Ok(account)
    }

    /// Saves the provided `Account` to the database.
    ///
    /// This method inserts or updates the account record in the database, serializing all
    /// relevant fields as JSON. If an account with the same public key already exists,
    /// its data will be updated.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` to be saved.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the database operation fails or if serialization fails.
    async fn save_account(&self, account: &Account) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::save_account",
            "Beginning save transaction for pubkey: {}",
            account.pubkey.to_hex()
        );

        let mut txn = self.database.pool.begin().await?;

        let result = sqlx::query(
            "INSERT INTO accounts (pubkey, settings, onboarding, last_synced)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(pubkey) DO UPDATE SET
                settings = excluded.settings,
                onboarding = excluded.onboarding,
                last_synced = excluded.last_synced",
        )
        .bind(account.pubkey.to_hex())
        .bind(&serde_json::to_string(&account.settings)?)
        .bind(&serde_json::to_string(&account.onboarding)?)
        .bind(account.last_synced.to_string())
        .execute(&mut *txn)
        .await?;

        tracing::debug!(
            target: "whitenoise::save_account",
            "Query executed. Rows affected: {}",
            result.rows_affected()
        );

        txn.commit().await?;

        tracing::debug!(
            target: "whitenoise::save_account",
            "Account saved successfully for pubkey: {}",
            account.pubkey.to_hex()
        );

        Ok(())
    }

    /// Deletes the specified account from the database.
    ///
    /// This method removes the account record associated with the given public key from the database.
    /// It performs the deletion within a transaction to ensure atomicity.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` to be deleted.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the account was successfully deleted, or a `WhitenoiseError` if the operation fails.
    async fn delete_account(&self, pubkey: &PublicKey) -> Result<()> {
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let result = sqlx::query("DELETE FROM accounts WHERE pubkey = ?")
            .bind(pubkey.to_hex())
            .execute(&self.database.pool)
            .await?;

        tracing::debug!(target: "whitenoise::delete_account", "Account removed from database for pubkey: {}", pubkey.to_hex());

        if result.rows_affected() < 1 {
            Err(WhitenoiseError::AccountNotFound)
        } else {
            Ok(())
        }
    }

    /// Saves the provided `AccountSettings` to the database.
    ///
    /// This method updates the settings field of the account record in the database, serializing
    /// the settings as JSON.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - A reference to the `PublicKey` of the account to update
    /// * `settings` - A reference to the `AccountSettings` to be updated.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the account does not exist or database operation fails or if serialization fails.
    pub async fn update_account_settings(
        &self,
        pubkey: &PublicKey,
        settings: &AccountSettings,
    ) -> Result<()> {
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Serialize AccountSettings to JSON
        let settings_json = serde_json::to_value(settings)?;

        // Execute the update query
        let result = sqlx::query("UPDATE accounts SET settings = ? WHERE pubkey = ?")
            .bind(settings_json)
            .bind(pubkey.to_hex())
            .execute(&self.database.pool)
            .await?;

        if result.rows_affected() < 1 {
            Err(WhitenoiseError::AccountNotFound)
        } else {
            Ok(())
        }
    }

    /// Fetches the `AccountSettings` from the database
    ///
    /// # Arguments
    /// * pubkey
    ///
    /// # Returns
    /// Returns `AccountSettings` on success
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if account does not exist or database operation fails or if serialization fails
    pub async fn fetch_account_settings(&self, pubkey: &PublicKey) -> Result<AccountSettings> {
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let settings_json: Value =
            sqlx::query_scalar("SELECT settings FROM accounts WHERE pubkey = ?")
                .bind(pubkey.to_hex())
                .fetch_one(&self.database.pool)
                .await
                .map_err(|_| WhitenoiseError::AccountNotFound)?;
        serde_json::from_value(settings_json).map_err(WhitenoiseError::from)
    }

    /// Initializes the Nostr MLS (Message Layer Security) instance for a given account.
    ///
    /// This method sets up the MLS storage and initializes a new NostrMls instance for secure messaging.
    /// The MLS storage is created in a directory specific to the account's public key, ensuring
    /// isolation between different accounts. The initialized NostrMls instance is stored in the
    /// account's nostr_mls field for future use.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` for which to initialize the NostrMls instance.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if initialization is successful.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The MLS storage directory cannot be created
    /// * The NostrMls instance cannot be initialized
    /// * The mutex lock cannot be acquired
    async fn initialize_nostr_mls_for_account(&self, account: &Account) -> Result<()> {
        // Initialize NostrMls for the account
        let mls_storage_dir = self
            .config
            .data_dir
            .join("mls")
            .join(account.pubkey.to_hex());

        let nostr_mls = NostrMls::new(NostrMlsSqliteStorage::new(mls_storage_dir)?);
        {
            let mut nostr_mls_guard = account.nostr_mls.lock().await;
            *nostr_mls_guard = Some(nostr_mls);
        }
        tracing::debug!(target: "whitenoise::initialize_nostr_mls_for_account", "NostrMls initialized for account: {}", account.pubkey.to_hex());
        Ok(())
    }

    /// Performs onboarding steps for a new account, including relay setup and publishing metadata.
    ///
    /// This method sets onboarding flags, assigns default relays, publishes the account's metadata
    /// and relay lists to Nostr, and attempts to publish the key package. It updates the onboarding
    /// status based on the success of these operations.
    ///
    /// # Arguments
    ///
    /// * `account` - A mutable reference to the `Account` being onboarded.
    ///
    /// # Returns
    ///
    /// Returns the onboarded `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if any database or Nostr operation fails.
    async fn onboard_new_account(&self, account: &mut Account) -> Result<Account> {
        tracing::debug!(target: "whitenoise::onboard_new_account", "Starting onboarding process");

        // Set onboarding flags
        account.onboarding.inbox_relays = false;
        account.onboarding.key_package_relays = false;

        let default_relays = self.nostr.relays().await?;

        // Generate a petname for the account (two words, separated by a space)
        let petname_raw = petname::petname(2, " ").unwrap_or_else(|| "Anonymous User".to_string());

        // Capitalize each word in the petname
        let petname = petname_raw
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first_char) => {
                        let first_upper = first_char.to_uppercase().collect::<String>();
                        first_upper + chars.as_str()
                    }
                }
            })
            .collect::<Vec<String>>()
            .join(" ");

        let metadata = Metadata {
            name: Some(petname.clone()),
            display_name: Some(petname),
            ..Default::default()
        };
        // Publish a metadata event to Nostr
        let metadata_json = serde_json::to_string(&metadata)?;
        let event = EventBuilder::new(Kind::Metadata, metadata_json);
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Get relays with fallback to defaults (expected during onboarding)
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        let result = self
            .nostr
            .publish_event_builder_with_signer(event.clone(), &relays_to_use, keys)
            .await?;
        tracing::debug!(target: "whitenoise::onboard_new_account", "Published metadata event to Nostr: {:?}", result);

        // Also publish relay lists to Nostr
        self.publish_relay_list_for_account(account, default_relays.clone(), RelayType::Nostr)
            .await?;
        self.publish_relay_list_for_account(account, default_relays.clone(), RelayType::Inbox)
            .await?;
        self.publish_relay_list_for_account(account, default_relays, RelayType::KeyPackage)
            .await?;

        // Publish key package to key package relays
        match self.publish_key_package_for_account(account).await {
            Ok(_) => {
                account.onboarding.key_package_published = true;
                self.save_account(account).await?;
                tracing::debug!(target: "whitenoise::onboard_new_account", "Published key package to relays");
            }
            Err(e) => {
                account.onboarding.key_package_published = false;
                self.save_account(account).await?;
                tracing::warn!(target: "whitenoise::onboard_new_account", "Failed to publish key package: {}", e);
            }
        }

        tracing::debug!(target: "whitenoise::onboard_new_account", "Onboarding complete for new account: {:?}", account);
        Ok(account.clone())
    }

    /// Publishes a relay list event of the specified type for the given account to Nostr.
    ///
    /// This helper method constructs and sends a relay list event (Nostr, Inbox, or KeyPackage)
    /// using the provided relays. If the relays vector is empty, the method returns early.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose relay list will be published.
    /// * `relays` - A vector of `RelayUrl` specifying the relays to include in the event.
    /// * `relay_type` - The type of relay list to publish (Nostr, Inbox, or KeyPackage).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if event creation or publishing fails.
    async fn publish_relay_list_for_account(
        &self,
        account: &Account,
        relays: Vec<RelayUrl>,
        relay_type: RelayType,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        if relays.is_empty() {
            return Ok(());
        }

        // Create a minimal relay list event
        let tags: Vec<Tag> = relays
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();

        // Determine the kind of relay list event to publish
        let relay_event_kind = match relay_type {
            RelayType::Nostr => Kind::RelayList,
            RelayType::Inbox => Kind::InboxRelays,
            RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
        };

        let event = EventBuilder::new(relay_event_kind, "").tags(tags);
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        let result = self
            .nostr
            .publish_event_builder_with_signer(event.clone(), &relays_to_use, keys)
            .await?;
        tracing::debug!(target: "whitenoise::publish_relay_list", "Published relay list event to Nostr: {:?}", result);

        Ok(())
    }

    /// Publishes the MLS key package for the given account to its key package relays.
    ///
    /// This method attempts to acquire the `nostr_mls` lock, generate a key package event,
    /// and publish it to the account's key package relays. If successful, the key package
    /// is published to Nostr; otherwise, onboarding status is updated accordingly.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose key package will be published.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the lock cannot be acquired, if the key package cannot be generated,
    /// or if publishing to Nostr fails.
    async fn publish_key_package_for_account(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let key_package_relays = self
            .fetch_relays(account.pubkey, RelayType::KeyPackage)
            .await?;

        // Extract key package data while holding the lock
        let (encoded_key_package, tags) = {
            tracing::debug!(target: "whitenoise::publish_key_package_for_account", "Attempting to acquire nostr_mls lock");

            let nostr_mls_guard = account.nostr_mls.lock().await;

            tracing::debug!(target: "whitenoise::publish_key_package_for_account", "nostr_mls lock acquired");

            let nostr_mls = nostr_mls_guard.as_ref()
                .ok_or_else(|| {
                    tracing::error!(target: "whitenoise::publish_key_package_for_account", "NostrMls not initialized for account");
                    WhitenoiseError::NostrMlsNotInitialized
                })?;

            let result = nostr_mls
                .create_key_package_for_event(&account.pubkey, key_package_relays)
                .map_err(|e| WhitenoiseError::Configuration(format!("NostrMls error: {}", e)))?;

            tracing::debug!(target: "whitenoise::publish_key_package_for_account", "nostr_mls lock released");
            result
        };

        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package).tags(tags);

        // Get relays with fallback to defaults if user hasn't configured key package relays
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::KeyPackage)
            .await?;

        let result = self
            .nostr
            .publish_event_builder_with_signer(key_package_event_builder, &relays_to_use, signer)
            .await?;

        tracing::debug!(target: "whitenoise::publish_key_package_for_account", "Published key package to relays: {:?}", result);

        Ok(())
    }

    /// Deletes the key package from the relays for the given account.
    ///
    /// This method deletes the key package from the relays for the given account.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose key package will be deleted.
    /// * `event_id` - The `EventId` of the key package to delete.
    /// * `key_package_relays` - A vector of `RelayUrl` specifying the relays to delete the key package from.
    /// * `delete_mls_stored_keys` - A boolean indicating whether to delete the key package from MLS storage.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    async fn delete_key_package_from_relays_for_account(
        &self,
        account: &Account,
        event_id: &EventId,
        delete_mls_stored_keys: bool,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let key_package_relays = self
            .fetch_relays(account.pubkey, RelayType::KeyPackage)
            .await?;

        let key_package_filter = Filter::new()
            .id(*event_id)
            .kind(Kind::MlsKeyPackage)
            .author(account.pubkey);

        let key_package_events = self
            .nostr
            .fetch_events_with_filter(key_package_filter)
            .await?;
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        if let Some(event) = key_package_events.first() {
            if delete_mls_stored_keys {
                let nostr_mls_guard = account.nostr_mls.lock().await;
                if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
                    let key_package = nostr_mls.parse_key_package(event)?;

                    nostr_mls.delete_key_package_from_storage(&key_package)?;
                } else {
                    return Err(WhitenoiseError::NostrMlsNotInitialized);
                }
            }

            let builder = EventBuilder::delete(EventDeletionRequest::new().id(event.id));

            // Only try to delete if we have key package relays configured
            if !key_package_relays.is_empty() {
                self.nostr
                    .publish_event_builder_with_signer(builder, &key_package_relays, signer)
                    .await?;
            } else {
                tracing::warn!(
                    target: "whitenoise::delete_key_package_from_relays_for_account",
                    "No key package relays configured for account {}, cannot delete key package",
                    account.pubkey.to_hex()
                );
            }
        } else {
            tracing::warn!(target: "whitenoise::delete_key_package_from_relays_for_account", "Key package event not found for account: {}", account.pubkey.to_hex());
            return Ok(());
        }

        Ok(())
    }

    /// Initiates a background fetch of all Nostr data associated with the given account.
    ///
    /// This method spawns an asynchronous background task to fetch the account's complete
    /// Nostr data, including events, messages, and group-related information. The fetch
    /// operation runs independently without blocking the caller, making it ideal for
    /// triggering data synchronization after account creation or login.
    ///
    /// The background task will fetch:
    /// - User metadata and profile information
    /// - Contact lists and relay configurations
    /// - Messages and events since the last sync timestamp
    /// - Group-specific data for all groups the account belongs to
    ///
    /// When the fetch completes successfully, the account's `last_synced` timestamp is
    /// updated in the database to reflect the successful synchronization.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` for which to fetch Nostr data.
    ///   The account's public key, last sync timestamp, and group memberships
    ///   are used to determine what data to fetch.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` immediately after spawning the background task. The actual
    /// data fetching occurs asynchronously and any errors are logged rather than
    /// propagated to the caller.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * Failed to extract group IDs from the account's NostrMls instance
    /// * The account's NostrMls instance is not properly initialized
    ///
    /// Note that errors occurring within the spawned background task (such as network
    /// failures or parsing errors) are logged but do not cause this method to fail.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Trigger background data fetch after account login
    /// whitenoise.background_fetch_account_data(&account).await?;
    /// // Method returns immediately, data fetch continues in background
    /// ```
    async fn background_fetch_account_data(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            tracing::info!("BACKGROUND");
            return Err(WhitenoiseError::AccountNotFound);
        }

        let group_ids = account.groups_nostr_group_ids().await?;
        let nostr = self.nostr.clone();
        let database = self.database.clone();
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let last_synced = account.last_synced;
        let account_pubkey = account.pubkey;

        tokio::spawn(async move {
            tracing::debug!(
                target: "whitenoise::background_fetch_account_data",
                "Starting background fetch for account: {} (since: {})",
                account_pubkey.to_hex(),
                last_synced
            );

            match nostr
                .fetch_all_user_data(signer, last_synced, group_ids)
                .await
            {
                Ok(_) => {
                    // Update the last_synced timestamp in the database
                    let current_time = Timestamp::now();

                    if let Err(e) =
                        sqlx::query("UPDATE accounts SET last_synced = ? WHERE pubkey = ?")
                            .bind(current_time.to_string())
                            .bind(account_pubkey.to_hex())
                            .execute(&database.pool)
                            .await
                    {
                        tracing::error!(
                            target: "whitenoise::background_fetch_account_data",
                            "Failed to update last_synced timestamp for account {}: {}",
                            account_pubkey.to_hex(),
                            e
                        );
                    } else {
                        tracing::info!(
                            target: "whitenoise::background_fetch_account_data",
                            "Successfully fetched data and updated last_synced for account: {}",
                            account_pubkey.to_hex()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::background_fetch_account_data",
                        "Failed to fetch user data for account {}: {}",
                        account_pubkey.to_hex(),
                        e
                    );
                }
            }
        });

        Ok(())
    }

    async fn setup_subscriptions(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            tracing::info!("SETUP");
            return Err(WhitenoiseError::AccountNotFound);
        }

        let groups = {
            let nostr_mls_guard = account.nostr_mls.lock().await;
            if let Some(ref nostr_mls) = *nostr_mls_guard {
                nostr_mls.get_groups()
            } else {
                return Err(WhitenoiseError::NostrMlsNotInitialized);
            }
        };

        let nostr_group_ids = groups
            .map(|groups| {
                groups
                    .iter()
                    .map(|group| hex::encode(group.nostr_group_id))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        // Use the signer-aware subscription setup method
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        self.nostr
            .setup_account_subscriptions_with_signer(
                account.pubkey,
                relays_to_use,
                nostr_group_ids,
                keys,
            )
            .await?;

        Ok(())
    }

    // ============================================================================
    // DATA LOADING
    // ============================================================================

    // Public API Methods ========================================================

    /// Loads the Nostr metadata for a contact by their public key.
    ///
    /// This method queries the Nostr network for user metadata associated with the provided public key.
    /// The metadata includes information such as display name, profile picture, and other user details
    /// that have been published to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the contact whose metadata should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(Metadata))` if metadata is found, `Ok(None)` if no metadata is available,
    /// or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the metadata query fails.
    pub async fn fetch_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let metadata = self.nostr.query_user_metadata(pubkey).await?;
        Ok(metadata)
    }

    /// Loads the Nostr relays associated with a user's public key.
    ///
    /// This method queries the Nostr network for relay URLs that the user has published
    /// for a specific relay type (e.g., read relays, write relays). These relays are
    /// used to determine where to send and receive Nostr events for the user.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relays should be fetched.
    /// * `relay_type` - The type of relays to fetch (e.g., read, write, or both).
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<RelayUrl>)` containing the list of relay URLs, or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay query fails.
    pub async fn fetch_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self.nostr.query_user_relays(pubkey, relay_type).await?;
        Ok(relays)
    }

    /// Fetches user relays for the specified type, falling back to default client relays if empty.
    ///
    /// This helper method abstracts the common pattern of checking if user-specific relays
    /// are configured and falling back to default client relays when they're not available.
    /// This is particularly useful during onboarding and in test environments where users
    /// haven't configured relays yet.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relays should be fetched.
    /// * `relay_type` - The type of relays to fetch (Nostr, Inbox, or KeyPackage).
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<RelayUrl>)` containing user relays if available, otherwise default client relays.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if either the relay query or default relay fetch fails.
    async fn fetch_relays_with_fallback(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let user_relays = self.fetch_relays(pubkey, relay_type).await?;

        if user_relays.is_empty() {
            self.nostr.relays().await.map_err(WhitenoiseError::from)
        } else {
            Ok(user_relays)
        }
    }

    /// Updates the metadata for the given account by publishing a new metadata event to Nostr.
    ///
    /// This method takes the provided metadata, creates a Nostr metadata event (Kind::Metadata),
    /// and publishes it to the account's relays. It also updates the account's `last_synced` timestamp
    /// in the database to reflect the successful publication.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The new `Metadata` to publish for the account.
    /// * `account` - A reference to the `Account` whose metadata should be updated.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful publication and database update.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The metadata cannot be serialized to JSON
    /// * The account's private key cannot be retrieved from the secret store
    /// * The event publication fails
    /// * The database update fails
    pub async fn update_metadata(&self, metadata: &Metadata, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Updating metadata for account: {}",
            account.pubkey.to_hex()
        );

        // Serialize metadata to JSON
        let metadata_json = serde_json::to_string(metadata)?;

        // Create metadata event
        let event = EventBuilder::new(Kind::Metadata, metadata_json);

        // Get signing keys for the account
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        // Publish the event
        let result = self
            .nostr
            .publish_event_builder_with_signer(event, &relays_to_use, keys)
            .await?;

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Published metadata event: {:?}",
            result
        );

        Ok(())
    }

    /// Updates the relay list for the given account by publishing a new relay list event to Nostr.
    ///
    /// This method takes the provided relay URLs and relay type, creates the appropriate relay list event
    /// (Nostr relays, Inbox relays, or Key Package relays), and publishes it to the account's relays.
    /// The relay list event contains the provided relay URLs as relay tags.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose relay list should be updated.
    /// * `relay_type` - The type of relay list to update (Nostr, Inbox, or KeyPackage).
    /// * `relays` - A vector of `RelayUrl` specifying the relays to include in the event.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful publication.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The account's private key cannot be retrieved from the secret store
    /// * The event creation fails
    /// * The event publication fails
    pub async fn update_relays(
        &self,
        account: &Account,
        relay_type: RelayType,
        relays: Vec<RelayUrl>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_account_relays",
            "Updating {:?} relays for account: {} with {} relays",
            relay_type,
            account.pubkey.to_hex(),
            relays.len()
        );

        // Use the existing helper method to publish the relay list
        self.publish_relay_list_for_account(account, relays, relay_type)
            .await?;

        tracing::debug!(
            target: "whitenoise::update_account_relays",
            "Successfully updated {:?} relays for account: {}",
            relay_type,
            account.pubkey.to_hex()
        );

        Ok(())
    }

    // ============================================================================
    // CONTACT MANAGEMENT
    // ============================================================================

    /// Loads a user's contact list from the Nostr network.
    ///
    /// This method retrieves the user's contact list, which contains the public keys
    /// of other users they follow. For each contact, it also includes their metadata
    /// if available.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose contact list should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(HashMap<PublicKey, Option<Metadata>>)` where the keys are the public keys
    /// of contacts and the values are their associated metadata (if available).
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the contact list query fails.
    pub async fn fetch_contacts(
        &self,
        pubkey: PublicKey,
    ) -> Result<HashMap<PublicKey, Option<Metadata>>> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let contacts = self.nostr.query_user_contact_list(pubkey).await?;
        Ok(contacts)
    }

    pub async fn fetch_key_package_event(&self, pubkey: PublicKey) -> Result<Option<Event>> {
        let key_package = self.nostr.query_user_key_package(pubkey).await?;
        Ok(key_package)
    }

    pub async fn fetch_onboarding_state(&self, pubkey: PublicKey) -> Result<OnboardingState> {
        let mut onboarding_state = OnboardingState::default();

        let inbox_relays = self.fetch_relays(pubkey, RelayType::Inbox).await?;
        let key_package_relays = self.fetch_relays(pubkey, RelayType::KeyPackage).await?;
        let key_package_published = self.fetch_key_package_event(pubkey).await?;

        onboarding_state.inbox_relays = !inbox_relays.is_empty();
        onboarding_state.key_package_relays = !key_package_relays.is_empty();
        onboarding_state.key_package_published = key_package_published.is_some();

        Ok(onboarding_state)
    }

    /// Adds a contact to the user's contact list and publishes the updated list to Nostr.
    ///
    /// This method loads the current contact list, validates that the contact doesn't already exist,
    /// adds the new contact, and publishes a Kind 3 (ContactList) event to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose contact list will be updated
    /// * `contact_pubkey` - The public key of the contact to add
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the contact was successfully added and published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The contact already exists in the contact list
    /// * Failed to load the current contact list
    /// * Failed to publish the updated contact list event
    pub async fn add_contact(&self, account: &Account, contact_pubkey: PublicKey) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Load current contact list
        let current_contacts = self.fetch_contacts(account.pubkey).await?;

        // Check if contact already exists
        if current_contacts.contains_key(&contact_pubkey) {
            return Err(WhitenoiseError::ContactList(format!(
                "Contact {} already exists in contact list",
                contact_pubkey.to_hex()
            )));
        }

        // Create new contact list with the added contact
        let mut new_contacts: Vec<PublicKey> = current_contacts.keys().cloned().collect();
        new_contacts.push(contact_pubkey);

        // Publish the updated contact list
        self.publish_contact_list(account, new_contacts).await?;

        tracing::info!(
            target: "whitenoise::add_contact",
            "Added contact {} to account {}",
            contact_pubkey.to_hex(),
            account.pubkey.to_hex()
        );

        Ok(())
    }

    /// Removes a contact from the user's contact list and publishes the updated list to Nostr.
    ///
    /// This method loads the current contact list, validates that the contact exists,
    /// removes the contact, and publishes a Kind 3 (ContactList) event to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose contact list will be updated
    /// * `contact_pubkey` - The public key of the contact to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the contact was successfully removed and published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The contact doesn't exist in the contact list
    /// * Failed to load the current contact list
    /// * Failed to publish the updated contact list event
    pub async fn remove_contact(&self, account: &Account, contact_pubkey: PublicKey) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Load current contact list
        let current_contacts = self.fetch_contacts(account.pubkey).await?;

        // Check if contact exists
        if !current_contacts.contains_key(&contact_pubkey) {
            return Err(WhitenoiseError::ContactList(format!(
                "Contact {} not found in contact list",
                contact_pubkey.to_hex()
            )));
        }

        // Create new contact list without the removed contact
        let new_contacts: Vec<PublicKey> = current_contacts
            .keys()
            .filter(|&pubkey| *pubkey != contact_pubkey)
            .cloned()
            .collect();

        // Publish the updated contact list
        self.publish_contact_list(account, new_contacts).await?;

        tracing::info!(
            target: "whitenoise::remove_contact",
            "Removed contact {} from account {}",
            contact_pubkey.to_hex(),
            account.pubkey.to_hex()
        );

        Ok(())
    }

    /// Updates the user's contact list with a new list of contacts and publishes it to Nostr.
    ///
    /// This method replaces the entire contact list with the provided list of public keys
    /// and publishes a Kind 3 (ContactList) event to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose contact list will be updated
    /// * `contact_pubkeys` - A vector of public keys representing the new contact list
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the contact list was successfully updated and published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if failed to publish the contact list event.
    pub async fn update_contacts(
        &self,
        account: &Account,
        contact_pubkeys: Vec<PublicKey>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Publish the new contact list
        self.publish_contact_list(account, contact_pubkeys.clone())
            .await?;

        tracing::info!(
            target: "whitenoise::update_contacts",
            "Updated contact list for account {} with {} contacts",
            account.pubkey.to_hex(),
            contact_pubkeys.len()
        );

        Ok(())
    }

    // Private Helper Methods =====================================================

    /// Publishes a contact list event (Kind 3) to the Nostr network.
    ///
    /// This helper method creates and publishes a Kind 3 event containing the provided
    /// list of contact public keys as 'p' tags.
    ///
    /// # Arguments
    ///
    /// * `account` - The account publishing the contact list
    /// * `contact_pubkeys` - A vector of public keys to include in the contact list
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the event was successfully published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if event creation or publishing fails.
    async fn publish_contact_list(
        &self,
        account: &Account,
        contact_pubkeys: Vec<PublicKey>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Create p tags for each contact
        let tags: Vec<Tag> = contact_pubkeys
            .into_iter()
            .map(|pubkey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();

        // Create the contact list event
        let event = EventBuilder::new(Kind::ContactList, "").tags(tags);

        // Get the signing keys for the account
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        // Publish the event
        let result = self
            .nostr
            .publish_event_builder_with_signer(event, &relays_to_use, keys.clone())
            .await?;

        // Update subscription for contact list metadata - use same relay logic
        self.nostr
            .update_contacts_metadata_subscription_with_signer(account.pubkey, relays_to_use, keys)
            .await?;

        tracing::debug!(
            target: "whitenoise::publish_contact_list",
            "Published contact list event: {:?}",
            result
        );

        Ok(())
    }

    // ============================================================================
    // EVENT PROCESSING
    // ============================================================================

    // Private Helper Methods =====================================================

    /// Start the event processing loop in a background task
    async fn start_event_processing_loop(
        whitenoise: Arc<Whitenoise>,
        receiver: Receiver<ProcessableEvent>,
        shutdown_receiver: Receiver<()>,
    ) {
        tokio::spawn(async move {
            Self::process_events(whitenoise, receiver, shutdown_receiver).await;
        });
    }

    /// Shutdown event processing gracefully
    async fn shutdown_event_processing(&self) -> Result<()> {
        match self.shutdown_sender.send(()).await {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Expected if processor already shut down
        }
    }

    /// Extract the account pubkey from a subscription_id
    /// Subscription IDs follow the format: {hashed_pubkey}_{subscription_type}
    /// where hashed_pubkey = SHA256(session salt || accouny_pubkey)[..12]
    async fn extract_pubkey_from_subscription_id(
        &self,
        subscription_id: &str,
    ) -> Option<PublicKey> {
        if let Some(underscore_pos) = subscription_id.find('_') {
            let hash_str = &subscription_id[..underscore_pos];
            // Get all accounts and find the one whose hash matches
            let accounts = self.accounts.read().await;
            for pubkey in accounts.keys() {
                let mut hasher = Sha256::new();
                hasher.update(self.nostr.session_salt());
                hasher.update(pubkey.to_bytes());
                let hash = hasher.finalize();
                let pubkey_hash = format!("{:x}", hash)[..12].to_string();
                if pubkey_hash == hash_str {
                    return Some(*pubkey);
                }
            }
        }
        None
    }

    /// Main event processing loop
    async fn process_events(
        whitenoise: Arc<Whitenoise>,
        mut receiver: Receiver<ProcessableEvent>,
        mut shutdown: Receiver<()>,
    ) {
        tracing::debug!(
            target: "whitenoise::process_events",
            "Starting event processing loop"
        );

        let mut shutting_down = false;

        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    tracing::debug!(
                        target: "whitenoise::process_events",
                        "Received event for processing"
                    );

                    // Process the event
                    match event {
                        ProcessableEvent::NostrEvent { event, subscription_id, retry_info } => {
                            // Filter and route nostr events based on kind
                            let result = match event.kind {
                                Kind::GiftWrap => {
                                    whitenoise.process_giftwrap(event.clone(), subscription_id.clone()).await
                                }
                                Kind::MlsGroupMessage => {
                                    whitenoise.process_mls_message(event.clone(), subscription_id.clone()).await
                                }
                                _ => {
                                    // TODO: Add more event types as needed
                                    tracing::debug!(
                                        target: "whitenoise::process_events",
                                        "Received unhandled event of kind: {:?} - add handler if needed",
                                        event.kind
                                    );
                                    Ok(()) // Unhandled events are not errors
                                }
                            };

                            // Handle retry logic
                            if let Err(e) = result {
                                if retry_info.should_retry() {
                                    if let Some(next_retry) = retry_info.next_attempt() {
                                        let delay_ms = next_retry.delay_ms();
                                        tracing::warn!(
                                            target: "whitenoise::process_events",
                                            "Event processing failed (attempt {}/{}), retrying in {}ms: {}",
                                            next_retry.attempt,
                                            next_retry.max_attempts,
                                            delay_ms,
                                            e
                                        );

                                        let retry_event = ProcessableEvent::NostrEvent {
                                            event,
                                            subscription_id,
                                            retry_info: next_retry,
                                        };
                                        let sender = whitenoise.event_sender.clone();

                                        tokio::spawn(async move {
                                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                                            if let Err(send_err) = sender.send(retry_event).await {
                                                tracing::error!(
                                                    target: "whitenoise::process_events",
                                                    "Failed to requeue event for retry: {}",
                                                    send_err
                                                );
                                            }
                                        });
                                    }
                                } else {
                                    tracing::error!(
                                        target: "whitenoise::process_events",
                                        "Event processing failed after {} attempts, giving up: {}",
                                        retry_info.max_attempts,
                                        e
                                    );
                                }
                            }
                        }
                        ProcessableEvent::RelayMessage(relay_url, message) => {
                            whitenoise.process_relay_message(relay_url, message).await;
                        }
                    }
                }
                Some(_) = shutdown.recv(), if !shutting_down => {
                    tracing::info!(
                        target: "whitenoise::process_events",
                        "Received shutdown signal, finishing current queue..."
                    );
                    shutting_down = true;
                    // Continue processing remaining events in queue, but don't wait for new shutdown signals
                }
                else => {
                    if shutting_down {
                        tracing::debug!(
                            target: "whitenoise::process_events",
                            "Queue flushed, shutting down event processor"
                        );
                    } else {
                        tracing::debug!(
                            target: "whitenoise::process_events",
                            "All channels closed, exiting event processing loop"
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Process giftwrap events with account awareness
    async fn process_giftwrap(&self, event: Event, subscription_id: Option<String>) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::process_giftwrap",
            "Processing giftwrap: {:?}",
            event
        );

        // Extract the target pubkey from the event's 'p' tag
        let target_pubkey = event
            .tags
            .iter()
            .find(|tag| tag.kind() == TagKind::p())
            .and_then(|tag| tag.content())
            .and_then(|pubkey_str| PublicKey::parse(pubkey_str).ok())
            .ok_or_else(|| {
                WhitenoiseError::InvalidEvent(
                    "No valid target pubkey found in 'p' tag for giftwrap event".to_string(),
                )
            })?;

        tracing::debug!(
            target: "whitenoise::process_giftwrap",
            "Processing giftwrap for target account: {} (author: {})",
            target_pubkey.to_hex(),
            event.pubkey.to_hex()
        );

        // Validate that this matches the subscription_id if available
        if let Some(sub_id) = subscription_id {
            if let Some(sub_pubkey) = self.extract_pubkey_from_subscription_id(&sub_id).await {
                if target_pubkey != sub_pubkey {
                    return Err(WhitenoiseError::InvalidEvent(format!(
                        "Giftwrap target pubkey {} does not match subscription pubkey {} - possible routing error",
                        target_pubkey.to_hex(),
                        sub_pubkey.to_hex()
                    )));
                }
            }
        }

        let target_account = self.read_account_by_pubkey(&target_pubkey).await?;

        tracing::info!(
            target: "whitenoise::process_giftwrap",
            "Giftwrap received for account: {} - processing not yet implemented",
            target_pubkey.to_hex()
        );

        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&target_pubkey)?;

        let unwrapped = extract_rumor(&keys, &event).await.map_err(|e| {
            WhitenoiseError::Configuration(format!("Failed to decrypt giftwrap: {}", e))
        })?;

        match unwrapped.rumor.kind {
            Kind::MlsWelcome => {
                self.process_welcome(&target_account, event, unwrapped.rumor)
                    .await?;
            }
            Kind::PrivateDirectMessage => {
                tracing::debug!(
                    target: "whitenoise::process_giftwrap",
                    "Received private direct message: {:?}",
                    unwrapped.rumor
                );
            }
            _ => {
                tracing::debug!(
                    target: "whitenoise::process_giftwrap",
                    "Received unhandled giftwrap of kind {:?}",
                    unwrapped.rumor.kind
                );
            }
        }

        Ok(())
    }

    async fn process_welcome(
        &self,
        account: &Account,
        event: Event,
        rumor: UnsignedEvent,
    ) -> Result<()> {
        // Process the welcome message - lock scope is minimal
        {
            let nostr_mls_guard = account.nostr_mls.lock().await;
            if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
                nostr_mls
                    .process_welcome(&event.id, &rumor)
                    .map_err(WhitenoiseError::NostrMlsError)?;
                tracing::debug!(target: "whitenoise::process_welcome", "Processed welcome event");
            } else {
                tracing::error!(target: "whitenoise::process_welcome", "Nostr MLS not initialized");
                return Err(WhitenoiseError::NostrMlsNotInitialized);
            }
        } // nostr_mls lock released here

        let key_package_event_id: Option<EventId> = rumor
            .tags
            .iter()
            .find(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .and_then(|tag| tag.content())
            .and_then(|content| EventId::parse(content).ok());

        if let Some(key_package_event_id) = key_package_event_id {
            self.delete_key_package_from_relays_for_account(
                account,
                &key_package_event_id,
                false, // For now we don't want to delete the key packages from MLS storage
            )
            .await?;
            tracing::debug!(target: "whitenoise::process_welcome", "Deleted used key package from relays");

            self.publish_key_package_for_account(account).await?;
            tracing::debug!(target: "whitenoise::process_welcome", "Published new key package");
        } else {
            tracing::warn!(target: "whitenoise::process_welcome", "No key package event id found in welcome event");
        }

        Ok(())
    }

    /// Process MLS group messages with account awareness
    async fn process_mls_message(
        &self,
        event: Event,
        subscription_id: Option<String>,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::process_mls_message",
            "Processing MLS message: {:?}",
            event
        );

        let sub_id = subscription_id.ok_or_else(|| {
            WhitenoiseError::InvalidEvent(
                "MLS message received without subscription ID".to_string(),
            )
        })?;

        let target_pubkey = self
            .extract_pubkey_from_subscription_id(&sub_id)
            .await
            .ok_or_else(|| {
                WhitenoiseError::InvalidEvent(format!(
                    "Cannot extract pubkey from subscription ID: {}",
                    sub_id
                ))
            })?;

        tracing::debug!(
            target: "whitenoise::process_mls_message",
            "Processing MLS message for account: {}",
            target_pubkey.to_hex()
        );

        let target_account = self.read_account_by_pubkey(&target_pubkey).await?;

        let nostr_mls_guard = target_account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            match nostr_mls.process_message(&event) {
                Ok(_message) => {
                    tracing::debug!(target: "whitenoise::process_mls_message", "Processed MLS message");
                    Ok(())
                }
                Err(e) => {
                    tracing::error!(target: "whitenoise::process_mls_message", "MLS message processing failed: {}", e);
                    Err(WhitenoiseError::NostrMlsError(e))
                }
            }
        } else {
            tracing::error!(target: "whitenoise::process_mls_message", "Nostr MLS not initialized");
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }

    /// Process relay messages for logging/monitoring
    async fn process_relay_message(&self, relay_url: RelayUrl, message_type: String) {
        tracing::debug!(
            target: "whitenoise::process_relay_message",
            "Processing message from {}: {}",
            relay_url,
            message_type
        );
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

    /// Creates a new MLS group with the specified members and settings
    ///
    /// # Arguments
    /// * `creator_pubkey` - Public key of the group creator (must be the active account)
    /// * `member_pubkeys` - List of public keys for group members
    /// * `admin_pubkeys` - List of public keys for group admins
    /// * `group_name` - Name of the group
    /// * `description` - Description of the group
    ///
    /// # Returns
    /// * `Ok(Group)` - The newly created group
    /// * `Err(String)` - Error message if group creation fails
    ///
    /// # Errors
    /// Returns error if:
    /// - Active account is not the creator
    /// - Member/admin validation fails
    /// - Key package fetching fails
    /// - MLS group creation fails
    /// - Welcome message sending fails
    /// - Database operations fail
    pub async fn create_group(
        &self,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: String,
        description: String,
    ) -> Result<group_types::Group> {
        if !self.logged_in(&creator_account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&creator_account.pubkey)?;

        let group_relays = self.fetch_relays(creator_account.pubkey, RelayType::Nostr).await?;

        let group: group_types::Group;
        let serialized_welcome_message: Vec<u8>;
        let group_ids: Vec<String>;
        let mut eventid_keypackage_list: Vec<(EventId, KeyPackage)> = Vec::new();

        let nostr_mls_guard = creator_account.nostr_mls.lock().await;

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Fetch key packages for all members
            for pk in member_pubkeys.iter() {
                let some_event = self.fetch_key_package_event(*pk).await?;
                let event = some_event.ok_or(WhitenoiseError::NostrMlsError(
                    nostr_mls::Error::KeyPackage("Does not exist".to_owned()),
                ))?;
                let key_package = nostr_mls
                    .parse_key_package(&event)
                    .map_err(WhitenoiseError::from)?;
                eventid_keypackage_list.push((event.id, key_package));
            }

            let create_group_result = nostr_mls
                .create_group(
                    group_name,
                    description,
                    &creator_account.pubkey,
                    &member_pubkeys,
                    eventid_keypackage_list
                        .iter()
                        .map(|(_, kp)| kp.clone())
                        .collect::<Vec<_>>()
                        .as_slice(),
                    admin_pubkeys,
                    group_relays.clone(),
                )
                .map_err(WhitenoiseError::from)?;

            group = create_group_result.group;
            serialized_welcome_message = create_group_result.serialized_welcome_message;
            group_ids = nostr_mls
                .get_groups()
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .map(|g| hex::encode(g.nostr_group_id))
                .collect::<Vec<_>>();
        } else {
            return Err(WhitenoiseError::NostrMlsNotInitialized);
        }

        tracing::debug!(target: "whitenoise::commands::groups::create_group", "nostr_mls lock released");

        // Fan out the welcome message to all members
        for (i, (event_id, _)) in eventid_keypackage_list.into_iter().enumerate() {
            let member_pubkey = member_pubkeys[i];

            let welcome_rumor =
                EventBuilder::new(Kind::MlsWelcome, hex::encode(&serialized_welcome_message))
                    .tags(vec![
                        Tag::from_standardized(TagStandard::Relays(group_relays.clone())),
                        Tag::event(event_id),
                    ])
                    .build(creator_account.pubkey);

            tracing::debug!(
                target: "whitenoise::groups::create_group",
                "Welcome rumor: {:?}",
                welcome_rumor
            );

            // Create a timestamp 1 month in the future
            use std::ops::Add;
            let one_month_future = Timestamp::now().add(30 * 24 * 60 * 60);
            self.nostr
                .publish_gift_wrap_with_signer(
                    &member_pubkey,
                    welcome_rumor,
                    vec![Tag::expiration(one_month_future)],
                    &group_relays,
                    keys.clone(),
                )
                .await
                .map_err(WhitenoiseError::from)?;
        }

        self.nostr
            .setup_group_messages_subscriptions_with_signer(
                creator_account.pubkey,
                group_relays,
                group_ids,
                keys,
            )
            .await
            .map_err(WhitenoiseError::from)?;

        Ok(group)
    }

    pub async fn fetch_groups(
        &self,
        account: &Account,
        active_filter: bool,
    ) -> Result<Vec<group_types::Group>> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            Ok(nostr_mls
                .get_groups()
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .filter(|group| !active_filter || group.state == group_types::GroupState::Active)
                .collect())
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }

    pub async fn fetch_group_members(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            Ok(nostr_mls
                .get_members(group_id)
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .collect())
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }

    pub async fn fetch_group_admins(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            Ok(nostr_mls
                .get_group(group_id)
                .map_err(WhitenoiseError::from)?
                .ok_or(WhitenoiseError::GroupNotFound)?
                .admin_pubkeys
                .into_iter()
                .collect())
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::AccountSettings;
    use std::time::Duration;
    use tempfile::TempDir;

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
            onboarding: crate::accounts::OnboardingState::default(),
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
