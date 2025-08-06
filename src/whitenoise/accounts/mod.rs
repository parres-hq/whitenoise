use dashmap::DashSet;
use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use crate::nostr_manager::NostrManagerError;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use crate::RelayType;

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

#[derive(Clone)]
pub struct Account {
    pub pubkey: PublicKey,
    pub settings: AccountSettings,
    pub nip65_relays: DashSet<RelayUrl>,
    pub inbox_relays: DashSet<RelayUrl>,
    pub key_package_relays: DashSet<RelayUrl>,
    pub last_synced: Timestamp,
    #[doc(hidden)]
    pub(crate) nostr_mls: Arc<Mutex<NostrMls<NostrMlsSqliteStorage>>>,
}

impl PartialEq for Account {
    fn eq(&self, other: &Self) -> bool {
        self.pubkey == other.pubkey
            && self.settings == other.settings
            && self.last_synced == other.last_synced
            && Whitenoise::relayurl_dashset_eq(
                self.nip65_relays.clone(),
                other.nip65_relays.clone(),
            )
            && Whitenoise::relayurl_dashset_eq(
                self.inbox_relays.clone(),
                other.inbox_relays.clone(),
            )
            && Whitenoise::relayurl_dashset_eq(
                self.key_package_relays.clone(),
                other.key_package_relays.clone(),
            )
        // -- note: `nostr_mls` is deliberately omitted
    }
}

impl Eq for Account {}

struct AccountRow {
    pubkey: PublicKey,
    settings: AccountSettings,
    last_synced: Timestamp,
    nip65_relays: Vec<RelayUrl>,
    inbox_relays: Vec<RelayUrl>,
    key_package_relays: Vec<RelayUrl>,
}

impl<'r, R> sqlx::FromRow<'r, R> for AccountRow
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
        let last_synced_i64: i64 = row.try_get("last_synced")?;
        let nip65_relays: String = row.try_get("nip65_relays")?;
        let inbox_relays: String = row.try_get("inbox_relays")?;
        let key_package_relays: String = row.try_get("key_package_relays")?;

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

        let nip65_relays = Whitenoise::parse_relays_from_sql(nip65_relays)?;
        let inbox_relays = Whitenoise::parse_relays_from_sql(inbox_relays)?;
        let key_package_relays = Whitenoise::parse_relays_from_sql(key_package_relays)?;

        // Convert last_synced from i64 to Timestamp
        let last_synced = Timestamp::from(last_synced_i64 as u64);

        Ok(AccountRow {
            pubkey,
            settings,
            last_synced,
            nip65_relays,
            inbox_relays,
            key_package_relays,
        })
    }
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("pubkey", &self.pubkey)
            .field("settings", &self.settings)
            .field("last_synced", &self.last_synced)
            .field("nip65_relays", &self.nip65_relays)
            .field("inbox_relays", &self.inbox_relays)
            .field("key_package_relays", &self.key_package_relays)
            .field("inbox_relays", &self.inbox_relays)
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
    pub(crate) fn new(data_dir: &Path) -> core::result::Result<(Account, Keys), AccountError> {
        tracing::debug!(target: "whitenoise::accounts::new", "Generating new keypair");
        let keys = Keys::generate();

        let nostr_mls = Self::create_nostr_mls(keys.public_key, data_dir)?;

        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            last_synced: Timestamp::zero(),
            nip65_relays: Self::default_relays(),
            inbox_relays: Self::default_relays(),
            key_package_relays: Self::default_relays(),
            nostr_mls,
        };

        Ok((account, keys))
    }

    pub fn default_relays() -> DashSet<RelayUrl> {
        let mut relays = Vec::new();
        if cfg!(debug_assertions) {
            relays.push("ws://localhost:8080");
            relays.push("ws://localhost:7777");
        } else {
            relays.push("wss://relay.damus.io");
            relays.push("wss://relay.primal.net");
            relays.push("wss://nos.lol");
        }
        relays
            .iter()
            .map(|url| RelayUrl::parse(url).unwrap())
            .collect()
    }

    fn create_nostr_mls(
        pubkey: PublicKey,
        data_dir: &Path,
    ) -> core::result::Result<Arc<Mutex<NostrMls<NostrMlsSqliteStorage>>>, AccountError> {
        let mls_storage_dir = data_dir.join("mls").join(pubkey.to_hex());
        let storage = NostrMlsSqliteStorage::new(mls_storage_dir)?;
        Ok(Arc::new(Mutex::new(NostrMls::new(storage))))
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
        let (account, keys) = Account::new(&self.config.data_dir)?;

        // Save the account to the database
        self.save_account(&account).await?;

        // Add the keys to the secret store
        self.secrets_store.store_private_key(&keys)?;

        self.login(keys.secret_key().to_secret_hex()).await
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

        let account = match self.load_account(&pubkey).await {
            Ok(account) => {
                tracing::debug!(target: "whitenoise::login", "Account found");
                account
            }
            Err(WhitenoiseError::AccountNotFound) => {
                tracing::debug!(target: "whitenoise::login", "Account not found, adding from keys");
                let account = self.add_account_from_keys(&keys).await?;
                account
            }
            Err(e) => return Err(e),
        };

        self.connect_account_relays(&account).await?;

        // Add the account to the in-memory accounts list
        {
            let mut accounts = self.write_accounts().await;
            accounts.insert(account.pubkey, account.clone());
        }

        self.publish_account_relay_info(&account).await?;

        self.setup_subscriptions(&account).await?;

        self.publish_key_package_for_account(&account).await?;

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
    pub async fn get_account(&self, pubkey: &PublicKey) -> Result<Account> {
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
            sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts ORDER BY last_synced DESC")
                .fetch_all(&self.database.pool)
                .await?;

        if accounts.is_empty() {
            tracing::debug!(target: "whitenoise::load_accounts", "No accounts found in database");
            return Ok(HashMap::new());
        }

        let data_dir = &self.config.data_dir;

        let mut accounts_map = HashMap::new();

        for account_row in accounts {
            let nostr_mls = Account::create_nostr_mls(account_row.pubkey, data_dir)?;

            let account = Account {
                pubkey: account_row.pubkey,
                settings: account_row.settings,
                last_synced: account_row.last_synced,
                nip65_relays: account_row.nip65_relays.into_iter().collect(),
                inbox_relays: account_row.inbox_relays.into_iter().collect(),
                key_package_relays: account_row.key_package_relays.into_iter().collect(),
                nostr_mls,
            };
            // Add the account to the HashMap first, then trigger background fetch
            accounts_map.insert(account.pubkey, account.clone());

            // Add account relays to the client
            let groups_and_relays = tokio::task::spawn_blocking({
                // clone whatever you need into the closure…
                let account = account.clone();
                move || -> core::result::Result<_, nostr_mls::error::Error> {
                    // this runs on a dedicated “blocking” thread,
                    // so we can call blocking_lock() on the tokio::Mutex
                    let nostr_mls = account.nostr_mls.lock().unwrap();

                    let mut all_relays = Vec::new();
                    let groups = nostr_mls.get_groups()?;
                    for group in groups {
                        let relays = nostr_mls.get_relays(&group.mls_group_id)?;
                        all_relays.push(relays);
                    }
                    Ok(all_relays)
                }
            })
            .await
            .map_err(WhitenoiseError::from)??;

            for group_relays in groups_and_relays {
                self.nostr.add_relays(group_relays).await?;
            }
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
    pub async fn load_account(&self, pubkey: &PublicKey) -> Result<Account> {
        let account_row =
            sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts WHERE pubkey = ?")
                .bind(pubkey.to_hex().as_str())
                .fetch_one(&self.database.pool)
                .await
                .map_err(|_| WhitenoiseError::AccountNotFound)?;

        Ok(Account {
            pubkey: account_row.pubkey,
            settings: account_row.settings,
            last_synced: account_row.last_synced,
            nip65_relays: account_row.nip65_relays.into_iter().collect(),
            inbox_relays: account_row.inbox_relays.into_iter().collect(),
            key_package_relays: account_row.key_package_relays.into_iter().collect(),
            nostr_mls: Account::create_nostr_mls(account_row.pubkey, &self.config.data_dir)?,
        })
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

        let mut nip65_relays = self
            .fetch_relays_from(Account::default_relays(), keys.public_key, RelayType::Nostr)
            .await?;
        if nip65_relays.is_empty() {
            nip65_relays = Account::default_relays();
        }

        // Step 3: Create account struct and save to database
        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            last_synced: Timestamp::zero(),
            nip65_relays,
            inbox_relays: Account::default_relays(),
            key_package_relays: Account::default_relays(),
            nostr_mls: Account::create_nostr_mls(keys.public_key(), &self.config.data_dir)?,
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

        let nip65_urls: Vec<_> = account
            .nip65_relays
            .iter()
            .map(|relay_url| relay_url.to_string())
            .collect();
        let inbox_urls: Vec<_> = account
            .inbox_relays
            .iter()
            .map(|relay_url| relay_url.to_string())
            .collect();
        let key_package_urls: Vec<_> = account
            .key_package_relays
            .iter()
            .map(|relay_url| relay_url.to_string())
            .collect();

        let result = sqlx::query(
            "INSERT INTO accounts (pubkey, settings, nip65_relays, inbox_relays, key_package_relays, last_synced)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(pubkey) DO UPDATE SET
                settings = excluded.settings,
                nip65_relays = excluded.nip65_relays,
                inbox_relays = excluded.inbox_relays,
                key_package_relays = excluded.key_package_relays,
                last_synced = excluded.last_synced",
        )
        .bind(account.pubkey.to_hex())
        .bind(&serde_json::to_string(&account.settings)?)
        .bind(&serde_json::to_string(&nip65_urls)?)
        .bind(&serde_json::to_string(&inbox_urls)?)
        .bind(&serde_json::to_string(&key_package_urls)?)
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

    pub async fn load_account_settings(&self, pubkey: &PublicKey) -> Result<AccountSettings> {
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

    pub(crate) async fn encoded_key_package(
        &self,
        account: &Account,
    ) -> Result<(String, [Tag; 4])> {
        let nostr_mls = &*account.nostr_mls.lock().unwrap();
        let result = nostr_mls
            .create_key_package_for_event(&account.pubkey, account.key_package_relays.clone())
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
        let (encoded_key_package, tags) = self.encoded_key_package(account).await?;

        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package).tags(tags);

        let result = self
            .nostr
            .publish_event_builder_with_signer(
                key_package_event_builder,
                account.key_package_relays.clone(),
                signer,
            )
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
                let nostr_mls = &*account.nostr_mls.lock().unwrap();
                let key_package = nostr_mls.parse_key_package(event)?;
                nostr_mls.delete_key_package_from_storage(&key_package)?;
            }

            let builder = EventBuilder::delete(EventDeletionRequest::new().id(event.id));

            self.nostr
                .publish_event_builder_with_signer(
                    builder,
                    account.key_package_relays.clone(),
                    signer,
                )
                .await?;
        } else {
            tracing::warn!(target: "whitenoise::delete_key_package_from_relays_for_account", "Key package event not found for account: {}", account.pubkey.to_hex());
            return Ok(());
        }

        Ok(())
    }

    pub(crate) async fn setup_subscriptions(&self, account: &Account) -> Result<()> {
        let mut group_relays = Vec::new();
        let groups: Vec<group_types::Group>;
        {
            let nostr_mls = &*account.nostr_mls.lock().unwrap();
            groups = nostr_mls.get_groups()?;
            // Collect all relays from all groups into a single vector
            for group in &groups {
                let relays = nostr_mls.get_relays(&group.mls_group_id)?;
                group_relays.extend(relays);
            }
            // Remove duplicates by sorting and deduplicating
            group_relays.sort();
            group_relays.dedup();
        };

        let nostr_group_ids = groups
            .into_iter()
            .map(|group| hex::encode(group.nostr_group_id))
            .collect::<Vec<String>>();

        // Use the signer-aware subscription setup method
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        self.nostr
            .setup_account_subscriptions_with_signer(
                account.pubkey,
                account.nip65_relays.clone(),
                account.inbox_relays.clone(),
                group_relays.into_iter().collect(),
                nostr_group_ids,
                keys,
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::{path::PathBuf, sync::Arc};

    use nostr::key::PublicKey;
    use nostr_mls::NostrMls;
    use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
    use std::sync::Mutex;
    use tempfile::TempDir;

    pub fn data_dir() -> PathBuf {
        TempDir::new().unwrap().path().to_path_buf()
    }

    pub fn create_nostr_mls(pubkey: PublicKey) -> Arc<Mutex<NostrMls<NostrMlsSqliteStorage>>> {
        super::Account::create_nostr_mls(pubkey, &data_dir()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;

    #[tokio::test]
    #[ignore]
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
    async fn test_from_row_implementation() {
        use sqlx::SqlitePool;

        // Create an in-memory database for testing
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Apply the accounts table schema
        sqlx::query(
            "CREATE TABLE accounts (
                pubkey TEXT PRIMARY KEY,
                settings JSONB NOT NULL,
                nip65_relays TEXT NOT NULL,
                inbox_relays TEXT NOT NULL,
                key_package_relays TEXT NOT NULL,
                last_synced INTEGER NOT NULL
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a test account
        let test_pubkey = Keys::generate().public_key();
        let test_settings = serde_json::to_string(&AccountSettings::default()).unwrap();
        let test_relay_urls: Vec<RelayUrl> = Account::default_relays().into_iter().collect();
        let test_relay_str: Vec<_> = test_relay_urls.iter().map(|url| url.to_string()).collect();
        let relays_str = serde_json::to_string(&test_relay_str).unwrap();
        let test_timestamp = 1234567890u64;

        sqlx::query(
            "INSERT INTO accounts (pubkey, settings, nip65_relays, inbox_relays, key_package_relays, last_synced) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_settings)
        .bind(&relays_str)
        .bind(&relays_str)
        .bind(&relays_str)
        .bind(test_timestamp as i64)
        .execute(&pool)
        .await
        .unwrap();

        // Test FromRow implementation by querying the account
        let account: AccountRow = sqlx::query_as("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        // Verify the account was correctly parsed
        assert_eq!(account.pubkey, test_pubkey);
        assert_eq!(account.settings, AccountSettings::default());
        assert_eq!(account.last_synced.as_u64(), test_timestamp);
        assert_eq!(account.nip65_relays, test_relay_urls);
        assert_eq!(account.inbox_relays, test_relay_urls);
        assert_eq!(account.key_package_relays, test_relay_urls);
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
}
