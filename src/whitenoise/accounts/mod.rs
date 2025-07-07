use derivative::Derivative;
use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::nostr_manager::NostrManagerError;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::relays::RelayType;
use crate::whitenoise::Whitenoise;

pub mod contacts;
pub mod groups;
pub mod messages;
pub mod metadata;
pub mod relays;
pub mod welcomes;

#[derive(Error, Debug)]
pub enum AccountError {
    #[error("Failed to parse public key: {0}")]
    PublicKeyError(#[from] nostr_sdk::key::Error),

    #[error("Failed to initialize Nostr manager: {0}")]
    NostrManagerError(#[from] NostrManagerError),

    #[error("Nostr MLS error: {0}")]
    NostrMlsError(#[from] nostr_mls::Error),

    #[error("Nostr MLS SQLite storage error: {0}")]
    NostrMlsSqliteStorageError(#[from] nostr_mls_sqlite_storage::error::Error),

    #[error("Nostr MLS not initialized")]
    NostrMlsNotInitialized,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AccountSettings {
    pub dark_theme: bool,
    pub dev_mode: bool,
    pub lockdown_mode: bool,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            dark_theme: true,
            dev_mode: false,
            lockdown_mode: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct OnboardingState {
    pub inbox_relays: bool,
    pub key_package_relays: bool,
    pub key_package_published: bool,
}

#[derive(Derivative)]
#[derivative(PartialEq)]
#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub pubkey: PublicKey,
    pub settings: AccountSettings,
    pub onboarding: OnboardingState,
    pub last_synced: Timestamp,
    #[serde(skip)]
    #[doc(hidden)]
    #[derivative(PartialEq = "ignore")]
    pub(crate) nostr_mls: Arc<Mutex<Option<NostrMls<NostrMlsSqliteStorage>>>>,
}

impl<'r, R> sqlx::FromRow<'r, R> for Account
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        // Extract raw values from the database row
        let pubkey_str: String = row.try_get("pubkey")?;
        let settings_json: String = row.try_get("settings")?;
        let onboarding_json: String = row.try_get("onboarding")?;
        let last_synced_i64: i64 = row.try_get("last_synced")?;

        // Parse pubkey from hex string
        let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "pubkey".to_string(),
            source: Box::new(e),
        })?;

        // Parse settings from JSON
        let settings: AccountSettings =
            serde_json::from_str(&settings_json).map_err(|e| sqlx::Error::ColumnDecode {
                index: "settings".to_string(),
                source: Box::new(e),
            })?;

        // Parse onboarding from JSON
        let onboarding: OnboardingState =
            serde_json::from_str(&onboarding_json).map_err(|e| sqlx::Error::ColumnDecode {
                index: "onboarding".to_string(),
                source: Box::new(e),
            })?;

        // Convert last_synced from i64 to Timestamp
        let last_synced = Timestamp::from(last_synced_i64 as u64);

        Ok(Account {
            pubkey,
            settings,
            onboarding,
            last_synced,
            nostr_mls: Arc::new(Mutex::new(None)),
        })
    }
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("pubkey", &self.pubkey)
            .field("settings", &self.settings)
            .field("onboarding", &self.onboarding)
            .field("last_synced", &self.last_synced)
            .field("nostr_mls", &"<REDACTED>")
            .finish()
    }
}

impl Account {
    /// Creates a new `Account` with a freshly generated keypair and default settings.
    ///
    /// This function generates a new cryptographic keypair, initializes an `Account` struct
    /// with default metadata, settings, onboarding flags, relays, and other fields. It also
    /// generates a random petname for the account, which is set as both the `name` and
    /// `display_name` in the account's metadata.
    ///
    /// # Returns
    ///
    /// Returns a tuple containing the new `Account` and its associated `Keys`.
    ///
    /// # Errors
    ///
    /// This function does not currently return any errors, but it is fallible to allow for
    /// future error handling and to match the expected signature for account creation.
    pub(crate) fn new() -> core::result::Result<(Account, Keys), AccountError> {
        tracing::debug!(target: "whitenoise::accounts::new", "Generating new keypair");
        let keys = Keys::generate();

        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: OnboardingState::default(),
            last_synced: Timestamp::zero(),
            nostr_mls: Arc::new(Mutex::new(None)),
        };

        Ok((account, keys))
    }

    pub(crate) async fn groups_nostr_group_ids(
        &self,
    ) -> core::result::Result<Vec<String>, AccountError> {
        let nostr_mls_guard = self.nostr_mls.lock().await;

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            let groups = nostr_mls.get_groups()?;
            Ok(groups
                .iter()
                .map(|g| hex::encode(g.nostr_group_id))
                .collect())
        } else {
            Ok(Vec::new())
        }
    }
}

impl Whitenoise {
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
        let (mut account, keys) = Account::new()?;

        // Save the account to the database
        self.save_account(&account).await?;

        // Add the keys to the secret store
        self.secrets_store.store_private_key(&keys)?;

        let log_account = self.login(keys.secret_key().to_secret_hex()).await;
        if let Err(e) = log_account {
            tracing::error!("Failed to login during create_identity: {}", e);
            return Err(e);
        }

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

    /// Fetches all currently loaded accounts from memory.
    ///
    /// This method returns a snapshot of all accounts that are currently loaded in memory.
    /// The accounts are returned as a HashMap where the key is the account's public key
    /// and the value is the Account struct containing all account data including settings,
    /// onboarding state, and last sync timestamp.
    ///
    /// This method retrieves accounts from the in-memory cache rather than querying the
    /// database directly, making it fast but limited to accounts that have been loaded
    /// during the current session (either through login or startup).
    ///
    /// # Returns
    ///
    /// Returns a `HashMap<PublicKey, Account>` containing all currently loaded accounts.
    /// The HashMap will be empty if no accounts are currently loaded in memory.
    ///
    /// # Errors
    ///
    /// This method does not typically return errors as it only reads from memory,
    /// but it returns a `Result` for consistency with other account-related methods
    /// and to allow for future error handling if needed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let accounts = whitenoise.fetch_accounts().await?;
    ///
    /// for (pubkey, account) in accounts {
    ///     println!("Account: {}, Dark theme: {}",
    ///              pubkey.to_hex(),
    ///              account.settings.dark_theme);
    /// }
    /// ```
    pub async fn fetch_accounts(&self) -> Result<HashMap<PublicKey, Account>> {
        Ok(self.read_accounts().await.clone())
    }

    /// Fetches a specific account by its public key from memory.
    ///
    /// This method retrieves a single account from the in-memory cache using the provided
    /// public key. If the account is found, it returns a clone of the Account struct.
    /// If the account is not found in memory, it returns an error.
    ///
    /// This method only searches accounts that are currently loaded in memory and does
    /// not query the database. For accounts that exist in the database but are not
    /// currently loaded, this method will return an error.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - A reference to the `PublicKey` of the account to fetch.
    ///
    /// # Returns
    ///
    /// Returns the `Account` associated with the provided public key if found in memory,
    /// or an error if the account is not found in memory.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError::AccountNotFound` if the account is not found in memory.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let account = whitenoise.fetch_account(&pubkey).await?;
    ///
    /// println!("Found account: {}", account.pubkey.to_hex());
    /// ```
    ///
    /// # Note
    ///
    /// Consider using methods that return proper error states (like `AccountNotFound`)
    /// if you need to distinguish between existing and non-existing accounts.
    pub async fn fetch_account(&self, pubkey: &PublicKey) -> Result<Account> {
        self.read_accounts()
            .await
            .get(pubkey)
            .cloned()
            .ok_or(WhitenoiseError::AccountNotFound)
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
    pub(crate) async fn load_accounts(&self) -> Result<HashMap<PublicKey, Account>> {
        tracing::debug!(target: "whitenoise::load_accounts", "Loading all accounts from database");

        let accounts =
            sqlx::query_as::<_, Account>("SELECT * FROM accounts ORDER BY last_synced DESC")
                .fetch_all(&self.database.pool)
                .await?;

        if accounts.is_empty() {
            tracing::debug!(target: "whitenoise::load_accounts", "No accounts found in database");
            return Ok(HashMap::new());
        }

        let mut accounts_map = HashMap::new();

        for account in accounts {
            // Initialize NostrMls for each account
            if let Err(e) = self.initialize_nostr_mls_for_account(&account).await {
                tracing::warn!(
                    target: "whitenoise::load_accounts",
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
                    target: "whitenoise::load_accounts",
                    "Failed to trigger background fetch for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                // Continue - background fetch failure should not prevent account loading
            }

            tracing::debug!(
                target: "whitenoise::load_accounts",
                "Loaded and initialized account: {}",
                account.pubkey.to_hex()
            );
        }

        tracing::info!(
            target: "whitenoise::load_accounts",
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
    pub(crate) async fn find_account_by_pubkey(&self, pubkey: &PublicKey) -> Result<Account> {
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
    pub(crate) async fn save_account(&self, account: &Account) -> Result<()> {
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
    pub(crate) async fn initialize_nostr_mls_for_account(&self, account: &Account) -> Result<()> {
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

        self.update_metadata(&metadata, &account.pubkey).await?;

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

    /// Refreshes and updates the onboarding state for an existing account.
    ///
    /// This method re-evaluates the current onboarding status of an account by checking:
    /// - Whether inbox relays are configured and available
    /// - Whether key package relays are configured and available
    /// - Whether a key package has been successfully published to the key package relays
    ///
    /// The method performs the following operations:
    /// 1. Fetches the current inbox relays for the account
    /// 2. Fetches the current key package relays for the account
    /// 3. Updates the `inbox_relays` onboarding flag based on whether inbox relays exist
    /// 4. Updates the `key_package_relays` onboarding flag based on whether key package relays exist
    /// 5. If key package relays are available, checks for an existing key package event and updates the `key_package_published` flag
    /// 6. Updates the account's `last_synced` timestamp to the current time
    /// 7. Persists the updated account state to the database
    ///
    /// This method is typically called:
    /// - After initial account setup to verify onboarding completion
    /// - Periodically to refresh the onboarding state based on current relay configuration
    /// - When the user manually triggers a sync operation
    /// - After relay configuration changes to update the onboarding status
    ///
    /// # Arguments
    ///
    /// * `account` - A mutable reference to the `Account` whose onboarding state should be refreshed.
    ///   The account's onboarding fields and last_synced timestamp will be updated based on the current state.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the onboarding state was successfully refreshed and the account was saved to the database.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The account is not found or not logged in
    /// * Failed to fetch inbox relays from the database or Nostr network
    /// * Failed to fetch key package relays from the database or Nostr network
    /// * Failed to query for existing key package events from the relays
    /// * Failed to save the updated account state to the database
    /// * Network connectivity issues when querying relays
    /// * Database transaction failures during account save
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut account = whitenoise.fetch_account(&pubkey).await?;
    ///
    /// // Refresh the onboarding state to get current status
    /// whitenoise.refresh_account_onboarding_state(&mut account).await?;
    ///
    /// // Check the updated onboarding state
    /// if account.onboarding.key_package_published {
    ///     println!("Account is fully onboarded");
    /// } else {
    ///     println!("Account setup is incomplete");
    /// }
    /// ```
    pub async fn refresh_account_onboarding_state(&self, account: &mut Account) -> Result<()> {
        let inbox_relays = self.fetch_relays(account.pubkey, RelayType::Inbox).await?;
        let key_package_relays = self
            .fetch_relays(account.pubkey, RelayType::KeyPackage)
            .await?;
        account.onboarding.inbox_relays = !inbox_relays.is_empty();
        account.onboarding.key_package_relays = !key_package_relays.is_empty();
        
        // Only check relay state for key package publication if it's not already marked as published
        // This prevents overwriting a recently published key package with false when relay propagation is slow
        if !account.onboarding.key_package_published && !key_package_relays.is_empty() {
            let key_package_event = self
                .fetch_key_package_event(account.pubkey, key_package_relays)
                .await?;
            if key_package_event.is_some() {
                account.onboarding.key_package_published = true;
            }
        }
        account.last_synced = Timestamp::now();
        self.save_account(account).await?;
        Ok(())
    }

    /// Completes any pending onboarding steps for an account after background data fetching.
    ///
    /// This method is designed to be called after `background_fetch_account_data()` completes
    /// successfully to ensure that the account's onboarding process is fully completed. It:
    ///
    /// 1. **Refreshes the onboarding state** - Checks current relay configuration and key package status
    /// 2. **Publishes missing relay lists** - Creates and publishes inbox and key package relay lists if missing
    /// 3. **Publishes key package** - Creates and publishes a new key package if one doesn't exist
    /// 4. **Updates the account** - Saves the final onboarding state to the database
    /// 5. **Updates in-memory cache** - Ensures the cached account reflects the latest onboarding state
    ///
    /// The method implements partial completion recovery - if some steps succeed but others fail,
    /// it will update the onboarding state to reflect what was actually completed, allowing
    /// future calls to only attempt the remaining incomplete steps.
    ///
    /// # Arguments
    ///
    /// * `account_pubkey` - The `PublicKey` of the account to complete onboarding for.
    ///   The account must be currently logged in and available in the in-memory cache.
    ///
    /// # Returns
    ///
    /// Returns `Ok(OnboardingState)` containing the final onboarding state after completion attempts.
    /// Even if some steps fail, the method returns the current state showing which steps succeeded.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The account is not found or not logged in
    /// * Critical failures occur during state refresh or account saving
    /// * NostrMls is not properly initialized for key package operations
    ///
    /// Note: Individual step failures (e.g., network errors during publishing) are logged as warnings
    /// but do not cause the method to fail. The returned `OnboardingState` shows actual completion status.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // After successful background fetch, complete any missing onboarding steps
    /// let final_state = whitenoise.complete_pending_onboarding_steps(&pubkey).await?;
    ///
    /// if final_state.key_package_published {
    ///     println!("Account is fully onboarded and ready to use");
    /// } else {
    ///     println!("Some onboarding steps are still incomplete");
    /// }
    /// ```
    ///
    /// # Usage in Background Tasks
    ///
    /// This method is particularly useful when called from within the completion handler
    /// of `background_fetch_account_data()`:
    ///
    /// ```rust,ignore
    /// // In background task completion
    /// tokio::spawn(async move {
    ///     match nostr.fetch_all_user_data(signer, last_synced, group_ids).await {
    ///         Ok(_) => {
    ///             // Update sync timestamp
    ///             update_last_synced(&database, &account_pubkey).await;
    ///
    ///             // Complete any pending onboarding steps
    ///             if let Err(e) = whitenoise.complete_pending_onboarding_steps(&account_pubkey).await {
    ///                 tracing::warn!("Failed to complete onboarding for {}: {}", account_pubkey.to_hex(), e);
    ///             }
    ///         }
    ///         Err(e) => { /* handle fetch error */ }
    ///     }
    /// });
    /// ```
    pub async fn complete_pending_onboarding_steps(
        &self,
        account_pubkey: &PublicKey,
    ) -> Result<OnboardingState> {
        tracing::debug!(
            target: "whitenoise::complete_pending_onboarding_steps",
            "Starting onboarding completion for account: {}",
            account_pubkey.to_hex()
        );

        // Step 1: Get the current account from memory and refresh its onboarding state
        let mut account = self.fetch_account(account_pubkey).await?;
        self.refresh_account_onboarding_state(&mut account).await?;

        let initial_state = account.onboarding.clone();
        tracing::debug!(
            target: "whitenoise::complete_pending_onboarding_steps",
            "Initial onboarding state for {}: inbox_relays={}, key_package_relays={}, key_package_published={}",
            account_pubkey.to_hex(),
            initial_state.inbox_relays,
            initial_state.key_package_relays,
            initial_state.key_package_published
        );

        // Step 2: Complete missing inbox relay list if needed
        if !account.onboarding.inbox_relays {
            tracing::debug!(
                target: "whitenoise::complete_pending_onboarding_steps",
                "Publishing inbox relay list for account: {}",
                account_pubkey.to_hex()
            );

            match self.ensure_inbox_relay_list_published(&account).await {
                Ok(_) => {
                    account.onboarding.inbox_relays = true;
                    tracing::debug!(
                        target: "whitenoise::complete_pending_onboarding_steps",
                        "Successfully published inbox relay list for account: {}",
                        account_pubkey.to_hex()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "whitenoise::complete_pending_onboarding_steps",
                        "Failed to publish inbox relay list for account {}: {}",
                        account_pubkey.to_hex(),
                        e
                    );
                    // Continue with other steps even if this one fails
                }
            }
        }

        // Step 3: Complete missing key package relay list if needed
        if !account.onboarding.key_package_relays {
            tracing::debug!(
                target: "whitenoise::complete_pending_onboarding_steps",
                "Publishing key package relay list for account: {}",
                account_pubkey.to_hex()
            );

            match self.ensure_key_package_relay_list_published(&account).await {
                Ok(_) => {
                    account.onboarding.key_package_relays = true;
                    tracing::debug!(
                        target: "whitenoise::complete_pending_onboarding_steps",
                        "Successfully published key package relay list for account: {}",
                        account_pubkey.to_hex()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "whitenoise::complete_pending_onboarding_steps",
                        "Failed to publish key package relay list for account {}: {}",
                        account_pubkey.to_hex(),
                        e
                    );
                    // Continue with other steps even if this one fails
                }
            }
        }

        // Step 4: Complete missing key package publication if needed and relays are available
        if !account.onboarding.key_package_published && account.onboarding.key_package_relays {
            tracing::debug!(
                target: "whitenoise::complete_pending_onboarding_steps",
                "Publishing key package for account: {}",
                account_pubkey.to_hex()
            );

            match self.publish_key_package_for_account(&account).await {
                Ok(_) => {
                    account.onboarding.key_package_published = true;
                    tracing::debug!(
                        target: "whitenoise::complete_pending_onboarding_steps",
                        "Successfully published key package for account: {}",
                        account_pubkey.to_hex()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "whitenoise::complete_pending_onboarding_steps",
                        "Failed to publish key package for account {}: {}",
                        account_pubkey.to_hex(),
                        e
                    );
                    // key_package_published remains false
                }
            }
        } else if !account.onboarding.key_package_published {
            tracing::debug!(
                target: "whitenoise::complete_pending_onboarding_steps",
                "Skipping key package publication for account {} - key package relays not available",
                account_pubkey.to_hex()
            );
        }

        // Step 5: Update the account's last_synced timestamp and save the final state
        account.last_synced = Timestamp::now();
        self.save_account(&account).await?;

        // Step 6: Update the in-memory cache with the updated account
        {
            let mut accounts = self.write_accounts().await;
            accounts.insert(*account_pubkey, account.clone());
        }

        let final_state = account.onboarding.clone();
        tracing::info!(
            target: "whitenoise::complete_pending_onboarding_steps",
            "Onboarding completion finished for {}: inbox_relays={}, key_package_relays={}, key_package_published={}",
            account_pubkey.to_hex(),
            final_state.inbox_relays,
            final_state.key_package_relays,
            final_state.key_package_published
        );

        Ok(final_state)
    }

    /// Ensures that an inbox relay list is published for the account.
    ///
    /// This helper method checks if the account has inbox relays configured, and if so,
    /// publishes an inbox relay list event to Nostr. If no inbox relays are configured,
    /// it falls back to using the default Nostr relays.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` for which to ensure the inbox relay list.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the inbox relay list was successfully published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay list cannot be published.
    async fn ensure_inbox_relay_list_published(&self, account: &Account) -> Result<()> {
        let inbox_relays = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Inbox)
            .await?;

        if !inbox_relays.is_empty() {
            self.publish_relay_list_for_account(account, inbox_relays, RelayType::Inbox)
                .await?;
        } else {
            tracing::warn!(
                target: "whitenoise::ensure_inbox_relay_list_published",
                "No inbox relays available for account {}, cannot publish inbox relay list",
                account.pubkey.to_hex()
            );
        }

        Ok(())
    }

    /// Ensures that a key package relay list is published for the account.
    ///
    /// This helper method checks if the account has key package relays configured, and if so,
    /// publishes a key package relay list event to Nostr. If no key package relays are configured,
    /// it falls back to using the default Nostr relays.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` for which to ensure the key package relay list.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the key package relay list was successfully published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay list cannot be published.
    async fn ensure_key_package_relay_list_published(&self, account: &Account) -> Result<()> {
        let key_package_relays = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::KeyPackage)
            .await?;

        if !key_package_relays.is_empty() {
            self.publish_relay_list_for_account(account, key_package_relays, RelayType::KeyPackage)
                .await?;
        } else {
            tracing::warn!(
                target: "whitenoise::ensure_key_package_relay_list_published",
                "No key package relays available for account {}, cannot publish key package relay list",
                account.pubkey.to_hex()
            );
        }

        Ok(())
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
    pub(crate) async fn publish_relay_list_for_account(
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

    pub(crate) async fn encoded_key_package(
        &self,
        account: &Account,
        pubkey: &PublicKey,
    ) -> Result<(String, [Tag; 4])> {
        let key_package_relays = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::KeyPackage)
            .await?;

        let nostr_mls_guard = account.nostr_mls.lock().await;

        let nostr_mls = nostr_mls_guard
            .as_ref()
            .ok_or_else(|| WhitenoiseError::NostrMlsNotInitialized)?;

        let result = nostr_mls
            .create_key_package_for_event(pubkey, key_package_relays)
            .map_err(|e| WhitenoiseError::Configuration(format!("NostrMls error: {}", e)))?;

        Ok(result)
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
    pub(crate) async fn publish_key_package_for_account(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Extract key package data while holding the lock
        let (encoded_key_package, tags) =
            self.encoded_key_package(account, &account.pubkey).await?;

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
    pub(crate) async fn delete_key_package_from_relays_for_account(
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
    /// When the fetch completes successfully:
    /// 1. The account's `last_synced` timestamp is updated in the database
    /// 2. Any pending onboarding steps are completed automatically
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
    /// // Onboarding completion happens automatically after successful fetch
    /// ```
    pub(crate) async fn background_fetch_account_data(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
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

                        // Complete any pending onboarding steps after successful data fetch
                        if let Ok(whitenoise) = Whitenoise::get_instance() {
                            tracing::debug!(
                                target: "whitenoise::background_fetch_account_data",
                                "Attempting to complete pending onboarding steps for account: {}",
                                account_pubkey.to_hex()
                            );

                            match whitenoise
                                .complete_pending_onboarding_steps(&account_pubkey)
                                .await
                            {
                                Ok(final_state) => {
                                    tracing::info!(
                                        target: "whitenoise::background_fetch_account_data",
                                        "Onboarding completion finished for {}: inbox_relays={}, key_package_relays={}, key_package_published={}",
                                        account_pubkey.to_hex(),
                                        final_state.inbox_relays,
                                        final_state.key_package_relays,
                                        final_state.key_package_published
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        target: "whitenoise::background_fetch_account_data",
                                        "Failed to complete pending onboarding steps for account {}: {}",
                                        account_pubkey.to_hex(),
                                        e
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                target: "whitenoise::background_fetch_account_data",
                                "Cannot access Whitenoise instance to complete onboarding for account: {}",
                                account_pubkey.to_hex()
                            );
                        }
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

    pub(crate) async fn setup_subscriptions(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            tracing::info!("SETUP");
            return Err(WhitenoiseError::AccountNotFound);
        }

        let mut group_relays = Vec::new();
        let groups: Vec<group_types::Group>;
        {
            let nostr_mls_guard = account.nostr_mls.lock().await;
            if let Some(ref nostr_mls) = *nostr_mls_guard {
                groups = nostr_mls.get_groups()?;
                // Collect all relays from all groups into a single vector
                for group in &groups {
                    let relays = nostr_mls.get_relays(&group.mls_group_id)?;
                    group_relays.extend(relays);
                }
                // Remove duplicates by sorting and deduplicating
                group_relays.sort();
                group_relays.dedup();
            } else {
                return Err(WhitenoiseError::NostrMlsNotInitialized);
            }
        };

        let nostr_group_ids = groups
            .into_iter()
            .map(|group| hex::encode(group.nostr_group_id))
            .collect::<Vec<String>>();

        // Get relays with fallback to defaults if user hasn't configured any
        let user_relays = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        let inbox_relays = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Inbox)
            .await?;

        // Use the signer-aware subscription setup method
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        self.nostr
            .setup_account_subscriptions_with_signer(
                account.pubkey,
                user_relays,
                inbox_relays,
                group_relays,
                nostr_group_ids,
                keys,
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_login_after_delete_all_data() {
        let whitenoise = test_get_whitenoise().await;

        let account = setup_login_account(whitenoise).await;
        whitenoise.delete_all_data().await.unwrap();
        let _acc = whitenoise
            .login(account.1.secret_key().to_secret_hex())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_account_new_creates_account_and_keys() {
        let (account, keys) = Account::new().unwrap();
        assert_eq!(account.pubkey, keys.public_key());
        // Check defaults
        assert!(account.settings.dark_theme);
        assert!(!account.settings.dev_mode);
        assert!(!account.settings.lockdown_mode);
        assert!(!account.onboarding.inbox_relays);
        assert!(!account.onboarding.key_package_relays);
        assert!(!account.onboarding.key_package_published);
    }

    #[test]
    fn test_account_settings_default() {
        let settings = AccountSettings::default();
        assert!(settings.dark_theme);
        assert!(!settings.dev_mode);
        assert!(!settings.lockdown_mode);
    }

    #[test]
    fn test_onboarding_state_default() {
        let onboarding = OnboardingState::default();
        assert!(!onboarding.inbox_relays);
        assert!(!onboarding.key_package_relays);
        assert!(!onboarding.key_package_published);
    }

    #[test]
    fn test_account_debug_formatting() {
        let keys = Keys::generate();
        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: OnboardingState::default(),
            last_synced: Timestamp::zero(),
            nostr_mls: Arc::new(tokio::sync::Mutex::new(None)),
        };

        let debug_str = format!("{:?}", account);
        assert!(debug_str.contains("Account"));
        assert!(debug_str.contains(&keys.public_key().to_hex()));
        assert!(debug_str.contains("<REDACTED>"));
        assert!(!debug_str.contains("NostrMls"));
    }

    #[tokio::test]
    async fn test_groups_nostr_group_ids_when_nostr_mls_none() {
        let keys = Keys::generate();
        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: OnboardingState::default(),
            last_synced: Timestamp::zero(),
            nostr_mls: Arc::new(tokio::sync::Mutex::new(None)),
        };

        let group_ids = account.groups_nostr_group_ids().await.unwrap();
        assert!(group_ids.is_empty());
    }

    #[test]
    fn test_account_error_display() {
        let key_error = AccountError::PublicKeyError(nostr_sdk::key::Error::InvalidSecretKey);
        assert!(key_error.to_string().contains("Failed to parse public key"));

        let nostr_mls_not_init = AccountError::NostrMlsNotInitialized;
        assert_eq!(nostr_mls_not_init.to_string(), "Nostr MLS not initialized");
    }

    #[test]
    fn test_account_error_from_conversions() {
        let key_error = nostr_sdk::key::Error::InvalidSecretKey;
        let account_error: AccountError = key_error.into();
        match account_error {
            AccountError::PublicKeyError(_) => {} // Expected
            _ => panic!("Expected PublicKeyError variant"),
        }
    }

    #[tokio::test]
    async fn test_multiple_account_creation() {
        // Test that Account::new() creates different accounts each time
        let (account1, keys1) = Account::new().unwrap();
        let (account2, keys2) = Account::new().unwrap();

        assert_ne!(account1.pubkey, account2.pubkey);
        assert_ne!(keys1.public_key(), keys2.public_key());
        assert_ne!(keys1.secret_key(), keys2.secret_key());
    }

    #[tokio::test]
    async fn test_from_row_implementation() {
        use sqlx::SqlitePool;

        // Create an in-memory database for testing
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Apply the accounts table schema
        sqlx::query(
            "CREATE TABLE accounts (
                pubkey TEXT PRIMARY KEY,
                settings TEXT NOT NULL,
                onboarding TEXT NOT NULL,
                last_synced INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a test account
        let test_pubkey = Keys::generate().public_key();
        let test_settings = serde_json::to_string(&AccountSettings::default()).unwrap();
        let test_onboarding = serde_json::to_string(&OnboardingState::default()).unwrap();
        let test_timestamp = 1234567890u64;

        sqlx::query(
            "INSERT INTO accounts (pubkey, settings, onboarding, last_synced) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_settings)
        .bind(&test_onboarding)
        .bind(test_timestamp as i64)
        .execute(&pool)
        .await
        .unwrap();

        // Test FromRow implementation by querying the account
        let account: Account = sqlx::query_as("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        // Verify the account was correctly parsed
        assert_eq!(account.pubkey, test_pubkey);
        assert_eq!(account.settings, AccountSettings::default());
        assert_eq!(account.onboarding, OnboardingState::default());
        assert_eq!(account.last_synced.as_u64(), test_timestamp);
    }

    #[test]
    fn test_account_settings_modifications() {
        let mut settings = AccountSettings::default();
        assert!(settings.dark_theme);
        assert!(!settings.dev_mode);
        assert!(!settings.lockdown_mode);

        settings.dark_theme = false;
        settings.dev_mode = true;
        settings.lockdown_mode = true;

        assert!(!settings.dark_theme);
        assert!(settings.dev_mode);
        assert!(settings.lockdown_mode);
    }

    #[test]
    fn test_onboarding_state_modifications() {
        let mut onboarding = OnboardingState::default();
        assert!(!onboarding.inbox_relays);
        assert!(!onboarding.key_package_relays);
        assert!(!onboarding.key_package_published);

        onboarding.inbox_relays = true;
        onboarding.key_package_relays = true;
        onboarding.key_package_published = true;

        assert!(onboarding.inbox_relays);
        assert!(onboarding.key_package_relays);
        assert!(onboarding.key_package_published);
    }

    // Integration test helpers for future database testing
    mod integration_test_helpers {
        use super::*;

        #[allow(dead_code)]
        pub fn create_test_account_with_settings(
            dark_theme: bool,
            dev_mode: bool,
            lockdown_mode: bool,
        ) -> (Account, Keys) {
            let keys = Keys::generate();
            let account = Account {
                pubkey: keys.public_key(),
                settings: AccountSettings {
                    dark_theme,
                    dev_mode,
                    lockdown_mode,
                },
                onboarding: OnboardingState::default(),
                last_synced: Timestamp::zero(),
                nostr_mls: Arc::new(tokio::sync::Mutex::new(None)),
            };
            (account, keys)
        }
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
            assert!(whitenoise.fetch_onboarding_state(pubkey).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_load_accounts() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Test loading empty database
        let accounts = whitenoise.load_accounts().await.unwrap();
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
        let loaded_accounts = whitenoise.load_accounts().await.unwrap();
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
    async fn test_load_accounts_ordering_by_last_synced() {
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
        let loaded_accounts = whitenoise.load_accounts().await.unwrap();
        assert_eq!(loaded_accounts.len(), 3);

        // Verify the most recent account would be first in HashMap iteration
        // (Note: HashMap iteration order is not guaranteed, but our SQL query orders by last_synced DESC)
        // We'll test the active account selection in a separate test
    }

    #[tokio::test]
    async fn test_complete_pending_onboarding_steps() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account with incomplete onboarding
        let (mut test_account, test_keys) = create_test_account();
        test_account.onboarding = OnboardingState {
            inbox_relays: false,
            key_package_relays: false,
            key_package_published: false,
        };

        // Store keys and save account
        whitenoise
            .secrets_store
            .store_private_key(&test_keys)
            .unwrap();
        whitenoise.save_account(&test_account).await.unwrap();

        // Log in the account to ensure it's in memory
        let _logged_account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Test the complete_pending_onboarding_steps method
        let result = whitenoise
            .complete_pending_onboarding_steps(&test_account.pubkey)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_complete_pending_onboarding_steps_account_not_found() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Test with a non-existent account
        let fake_keys = create_test_keys();
        let result = whitenoise
            .complete_pending_onboarding_steps(&fake_keys.public_key())
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WhitenoiseError::AccountNotFound
        ));
    }

    #[tokio::test]
    async fn test_refresh_account_onboarding_state() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (mut test_account, test_keys) = create_test_account();

        // Store keys and save account
        whitenoise
            .secrets_store
            .store_private_key(&test_keys)
            .unwrap();
        whitenoise.save_account(&test_account).await.unwrap();

        // Log in the account to ensure it's in memory
        let _logged_account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Test refresh_account_onboarding_state
        let result = whitenoise
            .refresh_account_onboarding_state(&mut test_account)
            .await;
        assert!(result.is_ok());

        // Verify that last_synced was updated
        assert!(test_account.last_synced > Timestamp::zero());
    }

    #[tokio::test]
    async fn test_ensure_relay_list_helper_methods() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (test_account, test_keys) = create_test_account();

        // Store keys and save account
        whitenoise
            .secrets_store
            .store_private_key(&test_keys)
            .unwrap();
        whitenoise.save_account(&test_account).await.unwrap();

        // Log in the account to ensure it's in memory and initialize nostr_mls
        let _logged_account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Test inbox relay list helper
        let inbox_result = whitenoise
            .ensure_inbox_relay_list_published(&test_account)
            .await;
        assert!(inbox_result.is_ok());

        // Test key package relay list helper
        let key_package_result = whitenoise
            .ensure_key_package_relay_list_published(&test_account)
            .await;
        assert!(key_package_result.is_ok());
    }

    #[tokio::test]
    async fn test_onboarding_completion_partial_success() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account with some onboarding steps completed
        let (mut test_account, test_keys) = create_test_account();
        test_account.onboarding = OnboardingState {
            inbox_relays: true, // This step is already complete
            key_package_relays: false,
            key_package_published: false,
        };

        // Store keys and save account
        whitenoise
            .secrets_store
            .store_private_key(&test_keys)
            .unwrap();
        whitenoise.save_account(&test_account).await.unwrap();

        // Log in the account
        let _logged_account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Test that the method handles partial completion correctly
        let result = whitenoise
            .complete_pending_onboarding_steps(&test_account.pubkey)
            .await;
        assert!(result.is_ok());

        let final_state = result.unwrap();
        // The already completed step should remain true
        assert!(final_state.inbox_relays);
    }
}
