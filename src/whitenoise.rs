use anyhow::Context;
use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::accounts::{Account, AccountRow, AccountSettings, OnboardingState};
use crate::database::Database;
use crate::error::{Result, WhitenoiseError};
use crate::init_tracing;
use crate::nostr_manager::NostrManager;
use crate::relays::RelayType;
use crate::secrets_store::SecretsStore;
use crate::types::ProcessableEvent;

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
            .field("active_account", &self.active_account)
            .field("database", &"<REDACTED>")
            .field("nostr", &"<REDACTED>")
            .field("secrets_store", &"<REDACTED>")
            .finish()
    }
}

impl Whitenoise {
    // ============================================================================
    // INITIALIZATION & LIFECYCLE
    // ============================================================================

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
    /// Returns a `Result` containing a fully initialized [`Whitenoise`] instance on success,
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

        // Create SecretsStore
        let secrets_store = SecretsStore::new(data_dir);

        // TODO: Load accounts from database

        // Create Whitenoise instance
        let mut whitenoise = Self {
            config,
            database,
            nostr,
            secrets_store,
            accounts: HashMap::new(),
            active_account: None,
            event_sender,
            shutdown_sender,
        };

        // Start the event processing loop
        whitenoise
            .start_event_processing_loop(event_receiver, shutdown_receiver)
            .await;

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
    /// Returns a `Result` which is `Ok(())` if all data is successfully deleted, or an error boxed as
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
    pub async fn create_identity(&mut self) -> Result<Account> {
        // Create a new account with a generated keypair and a petname
        let (initial_account, keys) = Account::new().await?;

        // Save the account to the database
        let mut account = self.save_account(&initial_account).await?;

        // Add the keys to the secret store
        self.secrets_store.store_private_key(&keys)?;

        // TODO: initialize subs on nostr manager

        self.initialize_nostr_mls_for_account(&account).await?;

        // Onboard the account
        self.onboard_new_account(&mut account).await?;

        // initialize subs on nostr manager

        // Add the account to the in-memory accounts list
        self.accounts.insert(account.pubkey, account.clone());

        // Set the account to active
        self.active_account = Some(account.pubkey);

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
    pub async fn login(&mut self, nsec_or_hex_privkey: String) -> Result<Account> {
        let keys = Keys::parse(&nsec_or_hex_privkey)?;
        let pubkey = keys.public_key();

        let account = match self.find_account_by_pubkey(&pubkey).await {
            Ok(account) => {
                tracing::debug!(target: "whitenoise::api::accounts::login", "Account found");
                Ok(account)
            }
            Err(WhitenoiseError::AccountNotFound) => {
                tracing::debug!(target: "whitenoise::api::accounts::login", "Account not found, adding from keys");
                let account = self.add_account_from_keys(&keys).await?;
                Ok(account)
            }
            Err(e) => Err(e),
        }?;

        // TODO: initialize subs on nostr manager

        // Initialize NostrMls for the account
        self.initialize_nostr_mls_for_account(&account).await?;

        // Spawn a background task to fetch the account's data from relays
        self.background_fetch_account_data(&account).await?;

        // Set the account to active
        self.active_account = Some(account.pubkey);

        // Add the account to the in-memory accounts list
        self.accounts.insert(account.pubkey, account.clone());

        Ok(account)
    }

    /// Logs out the user associated with the given public key.
    ///
    /// This method performs the following steps:
    /// - Finds the account associated with the provided public key.
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
    /// Returns a [`WhitenoiseError`] if the account cannot be found, or if there is a failure in removing the account or its private key.
    pub async fn logout(&mut self, account: &Account) -> Result<()> {
        // Delete the account from the database
        self.delete_account(account).await?;

        // Remove the private key from the secret store
        self.secrets_store
            .remove_private_key_for_pubkey(&account.pubkey)?;

        // Remove the account from the Whitenoise struct and update the active account
        self.accounts.remove(&account.pubkey);
        self.active_account = self.accounts.keys().next().copied();

        Ok(())
    }

    /// Sets the provided account as the active account in the Whitenoise instance.
    ///
    /// This method updates the `active_account` field to the public key of the given account.
    /// It does not perform any validation or additional logic beyond updating the active account reference.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` to be set as active.
    ///
    /// # Returns
    ///
    /// Returns the `Account` that was set as active.
    pub fn update_active_account(&mut self, account: &Account) -> Result<Account> {
        self.active_account = Some(account.pubkey);
        Ok(account.clone())
    }

    // Private Helper Methods =====================================================

    /// Finds and loads an account from the database by its public key.
    ///
    /// This method queries the database for an account matching the provided public key,
    /// deserializes its settings, onboarding, and initializes the account's
    /// NostrManager and NostrMls instances. The account is returned fully initialized and ready
    /// for use in the application.
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
    /// Returns a `WhitenoiseError` if the account is not found, if deserialization fails,
    /// or if initialization of the NostrManager or NostrMls fails.
    async fn find_account_by_pubkey(&self, pubkey: &PublicKey) -> Result<Account> {
        let row = sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(pubkey.to_hex().as_str())
            .fetch_optional(&self.database.pool)
            .await?;

        match row {
            Some(row) => {
                let account = Account {
                    pubkey: PublicKey::parse(row.pubkey.as_str()).map_err(|e| {
                        WhitenoiseError::Configuration(format!(
                            "Invalid public key in database: {}",
                            e
                        ))
                    })?,
                    settings: serde_json::from_str(&row.settings)?,
                    onboarding: serde_json::from_str(&row.onboarding)?,
                    last_synced: Timestamp::from(row.last_synced),
                    nostr_mls: Arc::new(Mutex::new(None)),
                };
                Ok(account)
            }
            None => Err(WhitenoiseError::AccountNotFound),
        }
    }

    /// Adds a new account to the database using the provided Nostr keys (atomic operation).
    ///
    /// This method performs account creation atomically with automatic cleanup on failure.
    /// The operation follows this sequence:
    ///
    /// 1. **Store private key** - Saves the private key to the system keychain/secret store
    /// 2. **Load onboarding state** - Queries cached Nostr data to determine account setup status
    /// 3. **Save account to database** - Persists the account record with settings and onboarding info
    /// 4. **Trigger background sync** - Initiates async fetch of account data (non-critical)
    ///
    /// If any critical step (1-3) fails, all previous operations are automatically rolled back
    /// to ensure no partial account state is left in the system. The background sync step (4)
    /// is non-critical and will not cause the operation to fail.
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
        tracing::debug!(target: "whitenoise::accounts", "Adding account for pubkey: {}", keys.public_key().to_hex());

        // Step 1: Try to store private key first (most likely to fail)
        // If this fails, we haven't persisted anything yet
        self.secrets_store.store_private_key(keys).map_err(|e| {
            tracing::error!(target: "whitenoise::accounts::add_account_from_keys", "Failed to store private key: {}", e);
            e
        })?;
        tracing::debug!(target: "whitenoise::accounts::add_account_from_keys", "Keys stored in secret store");

        // Step 2: Load onboarding state (read-only operation)
        let onboarding_state = self.load_onboarding_state(keys.public_key()).await.map_err(|e| {
            tracing::error!(target: "whitenoise::accounts::add_account_from_keys", "Failed to load onboarding state: {}", e);
            // Try to clean up stored private key
            if let Err(cleanup_err) = self.secrets_store.remove_private_key_for_pubkey(&keys.public_key()) {
                tracing::error!(target: "whitenoise::accounts::add_account_from_keys", "Failed to cleanup private key after onboarding state failure: {}", cleanup_err);
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
            tracing::error!(target: "whitenoise::accounts::add_account_from_keys", "Failed to save account: {}", e);
            // Try to clean up stored private key
            if let Err(cleanup_err) = self.secrets_store.remove_private_key_for_pubkey(&keys.public_key()) {
                tracing::error!(target: "whitenoise::accounts::add_account_from_keys", "Failed to cleanup private key after account save failure: {}", cleanup_err);
            }
            e
        })?;
        tracing::debug!(target: "whitenoise::accounts::add_account_from_keys", "Account saved to database");

        // Step 4: Trigger fetch of nostr events on another thread (least critical)
        // Don't fail the whole operation if this fails
        if let Err(e) = self.background_fetch_account_data(&account).await {
            tracing::warn!(target: "whitenoise::accounts::add_account_from_keys", "Failed to trigger background fetch (non-critical): {}", e);
        }

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
    /// Returns the saved `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the database operation fails or if serialization fails.
    async fn save_account(&self, account: &Account) -> Result<Account> {
        tracing::debug!(
            target: "whitenoise::accounts::save_account",
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
            target: "whitenoise::accounts::save",
            "Query executed. Rows affected: {}",
            result.rows_affected()
        );

        txn.commit().await?;

        tracing::debug!(
            target: "whitenoise::accounts::save",
            "Account saved successfully for pubkey: {}",
            account.pubkey.to_hex()
        );

        Ok(account.clone())
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
    async fn delete_account(&self, account: &Account) -> Result<()> {
        let mut txn = self.database.pool.begin().await?;
        sqlx::query("DELETE FROM accounts WHERE pubkey = ?")
            .bind(account.pubkey.to_hex())
            .execute(&mut *txn)
            .await?;

        txn.commit().await?;

        tracing::debug!(target: "whitenoise::accounts::remove_account", "Account removed from database for pubkey: {}", account.pubkey.to_hex());

        Ok(())
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
            let mut nostr_mls_guard = account
                .nostr_mls
                .lock()
                .map_err(|_| WhitenoiseError::NostrMlsNotInitialized)?;
            *nostr_mls_guard = Some(nostr_mls);
        }
        tracing::debug!(target: "whitenoise::api::accounts::login", "NostrMls initialized for account: {}", account.pubkey.to_hex());
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
        tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Starting onboarding process");

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
        let result = self
            .nostr
            .publish_event_builder_with_signer(event.clone(), keys)
            .await?;
        tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Published metadata event to Nostr: {:?}", result);

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
                tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Published key package to relays");
            }
            Err(e) => {
                account.onboarding.key_package_published = false;
                self.save_account(account).await?;
                tracing::warn!(target: "whitenoise::accounts::onboard_new_account", "Failed to publish key package: {}", e);
            }
        }

        tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Onboarding complete for new account: {:?}", account);
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
        let result = self
            .nostr
            .publish_event_builder_with_signer(event.clone(), keys)
            .await?;
        tracing::debug!(target: "whitenoise::accounts::publish_relay_list", "Published relay list event to Nostr: {:?}", result);

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
        let key_package_relays = self
            .load_relays(account.pubkey, RelayType::KeyPackage)
            .await?;

        // Extract key package data while holding the lock
        let (encoded_key_package, tags) = {
            tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "Attempting to acquire nostr_mls lock");

            let nostr_mls_guard = account.nostr_mls.lock()
                .map_err(|_| {
                    tracing::error!(target: "whitenoise::accounts::publish_key_package_for_account", "Timeout waiting for nostr_mls lock");
                    WhitenoiseError::NostrMlsNotInitialized
                })?;

            tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "nostr_mls lock acquired");

            let nostr_mls = nostr_mls_guard.as_ref()
                .ok_or_else(|| {
                    tracing::error!(target: "whitenoise::accounts::publish_key_package_for_account", "NostrMls not initialized for account");
                    WhitenoiseError::NostrMlsNotInitialized
                })?;

            let result = nostr_mls
                .create_key_package_for_event(&account.pubkey, key_package_relays)
                .map_err(|e| WhitenoiseError::Configuration(format!("NostrMls error: {}", e)))?;

            tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "nostr_mls lock released");
            result
        };

        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package).tags(tags);

        let result = self
            .nostr
            .publish_event_builder_with_signer(key_package_event_builder, signer)
            .await?;

        tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "Published key package to relays: {:?}", result);

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
        let group_ids = account.groups_nostr_group_ids()?;
        let nostr = self.nostr.clone();
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let last_synced = account.last_synced;

        tokio::spawn(async move {
            if let Err(e) = nostr
                .fetch_all_user_data(signer, last_synced, group_ids)
                .await
            {
                tracing::error!("Failed to fetch user data: {}", e);
            }
        });

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
    pub async fn load_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
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
    pub async fn load_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self.nostr.query_user_relays(pubkey, relay_type).await?;
        Ok(relays)
    }

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
    pub async fn load_contact_list(
        &self,
        pubkey: PublicKey,
    ) -> Result<HashMap<PublicKey, Option<Metadata>>> {
        let contacts = self.nostr.query_user_contact_list(pubkey).await?;
        Ok(contacts)
    }

    pub async fn load_key_package(&self, pubkey: PublicKey) -> Result<Option<Event>> {
        let key_package = self.nostr.query_user_key_package(pubkey).await?;
        Ok(key_package)
    }

    pub async fn load_onboarding_state(&self, pubkey: PublicKey) -> Result<OnboardingState> {
        let mut onboarding_state = OnboardingState::default();

        let inbox_relays = self.load_relays(pubkey, RelayType::Inbox).await?;
        let key_package_relays = self.load_relays(pubkey, RelayType::KeyPackage).await?;
        let key_package_published = self.load_key_package(pubkey).await?;

        onboarding_state.inbox_relays = !inbox_relays.is_empty();
        onboarding_state.key_package_relays = !key_package_relays.is_empty();
        onboarding_state.key_package_published = key_package_published.is_some();

        Ok(onboarding_state)
    }

    // ============================================================================
    // EVENT PROCESSING
    // ============================================================================

    // Private Helper Methods =====================================================

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
    async fn shutdown_event_processing(&self) -> Result<()> {
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
            nostr_mls: std::sync::Arc::new(std::sync::Mutex::new(None)),
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

        // Create NostrManager - this will still try to connect to localhost relays
        // but will fail gracefully since localhost:8080 and localhost:7777 likely aren't running
        let nostr = NostrManager::new(config.data_dir.join("test_nostr"), event_sender.clone())
            .await
            .expect("Failed to create NostrManager");

        let whitenoise = Whitenoise {
            config,
            database,
            nostr,
            secrets_store,
            accounts: HashMap::new(),
            active_account: None,
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
        async fn test_multiple_initializations_with_same_config() {
            let (config, _data_temp, _logs_temp) = create_test_config();

            let result1 = Whitenoise::initialize_whitenoise(config.clone()).await;
            assert!(result1.is_ok());

            let result2 = Whitenoise::initialize_whitenoise(config).await;
            assert!(result2.is_ok());
        }
    }

    // Event Processing Tests
    mod event_processing_tests {
        use super::*;

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

            // Test edge cases
            let invalid_cases = [
                test_pubkey.to_hex(),                  // no underscore
                "invalid_pubkey_messages".to_string(), // invalid pubkey
                "".to_string(),                        // empty string
                "_messages".to_string(),               // empty pubkey part
            ];

            for invalid_case in &invalid_cases {
                let extracted = Whitenoise::extract_pubkey_from_subscription_id(invalid_case);
                assert!(extracted.is_none(), "Should be None for: {}", invalid_case);
            }

            // Test multiple underscores (should take first part)
            let multi_underscore_id = format!("{}_messages_extra_data", test_pubkey.to_hex());
            let result = Whitenoise::extract_pubkey_from_subscription_id(&multi_underscore_id);
            assert!(result.is_some());
            assert_eq!(result.unwrap(), test_pubkey);
        }

        #[tokio::test]
        async fn test_queue_operations_after_shutdown() {
            let (config, _data_temp, _logs_temp) = create_test_config();
            let whitenoise = Whitenoise::initialize_whitenoise(config).await.unwrap();

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
            let (config, _data_temp, _logs_temp) = create_test_config();
            let mut whitenoise = Whitenoise::initialize_whitenoise(config.clone())
                .await
                .unwrap();

            // Create test files
            let test_data_file = config.data_dir.join("test_data.txt");
            let test_log_file = config.logs_dir.join("test_log.txt");
            tokio::fs::write(&test_data_file, "test data")
                .await
                .unwrap();
            tokio::fs::write(&test_log_file, "test log").await.unwrap();
            assert!(test_data_file.exists());
            assert!(test_log_file.exists());

            // Add test account
            let (test_account, test_keys) = create_test_account();
            let pubkey = test_keys.public_key();
            whitenoise.accounts.insert(pubkey, test_account);
            whitenoise.active_account = Some(pubkey);
            assert!(!whitenoise.accounts.is_empty());
            assert!(whitenoise.active_account.is_some());

            // Delete all data
            let result = whitenoise.delete_all_data().await;
            assert!(result.is_ok());

            // Verify cleanup
            assert!(whitenoise.accounts.is_empty());
            assert!(whitenoise.active_account.is_none());
            assert!(!test_log_file.exists());

            // MLS directory should be recreated as empty
            let mls_dir = config.data_dir.join("mls");
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
        async fn test_load_methods_return_types() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();

            // Test all load methods return expected types (though they may be empty in test env)
            let metadata = whitenoise.load_metadata(pubkey).await;
            assert!(metadata.is_ok());

            let relays = whitenoise.load_relays(pubkey, RelayType::Inbox).await;
            assert!(relays.is_ok());

            let contacts = whitenoise.load_contact_list(pubkey).await;
            assert!(contacts.is_ok());

            let key_package = whitenoise.load_key_package(pubkey).await;
            assert!(key_package.is_ok());

            let onboarding = whitenoise.load_onboarding_state(pubkey).await;
            assert!(onboarding.is_ok());
        }

        #[tokio::test]
        async fn test_load_all_relay_types() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();

            let relay_types = [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage];
            for relay_type in relay_types {
                let result = whitenoise.load_relays(pubkey, relay_type).await;
                assert!(result.is_ok());
                let relays = result.unwrap();
                assert!(relays.is_empty()); // Empty in test environment
            }
        }

        #[tokio::test]
        async fn test_load_onboarding_state_structure() {
            let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
            let test_keys = create_test_keys();
            let pubkey = test_keys.public_key();

            let result = whitenoise.load_onboarding_state(pubkey).await;
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

            // Test concurrent API calls
            let results = tokio::join!(
                whitenoise.load_metadata(pubkey),
                whitenoise.load_relays(pubkey, RelayType::Inbox),
                whitenoise.load_contact_list(pubkey),
                whitenoise.load_key_package(pubkey),
                whitenoise.load_onboarding_state(pubkey)
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

                // Test that all methods work with different pubkeys
                assert!(whitenoise.load_metadata(pubkey).await.is_ok());
                assert!(whitenoise
                    .load_relays(pubkey, RelayType::Inbox)
                    .await
                    .is_ok());
                assert!(whitenoise.load_contact_list(pubkey).await.is_ok());
                assert!(whitenoise.load_key_package(pubkey).await.is_ok());
                assert!(whitenoise.load_onboarding_state(pubkey).await.is_ok());
            }
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
}
