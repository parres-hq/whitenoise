use thiserror::Error;

use crate::nostr_manager::NostrManagerError;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::relays::Relay;
use crate::whitenoise::users::User;
use crate::whitenoise::Whitenoise;
use crate::RelayType;
use chrono::{DateTime, Utc};
use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;


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

    #[error("Whitenoise not initialized")]
    WhitenoiseNotInitialized,
}

#[derive(Clone)]
pub struct Account {
    pub id: Option<i64>,
    pub pubkey: PublicKey,
    pub user_id: i64,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[doc(hidden)]
    pub(crate) nostr_mls: Arc<Mutex<NostrMls<NostrMlsSqliteStorage>>>,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("id", &self.id)
            .field("pubkey", &self.pubkey)
            .field("user_id", &self.user_id)
            .field("last_synced_at", &self.last_synced_at)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("nostr_mls", &"<REDACTED>")
            .finish()
    }
}

impl Account {
    pub(crate) async fn new(keys: Option<Keys>, data_dir: &Path) -> Result<(Account, Keys)> {
        tracing::debug!(target: "whitenoise::accounts::new", "Generating new keypair");
        let keys = keys.unwrap_or_else(Keys::generate);
        let whitenoise =
            Whitenoise::get_instance().map_err(|_e| AccountError::WhitenoiseNotInitialized)?;

        let mut user = User::new(keys.public_key);
        user = user.save(&whitenoise).await?;

        let account = Account {
            id: None,
            user_id: user.id.unwrap(),
            pubkey: keys.public_key(),
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            nostr_mls: Self::create_nostr_mls(keys.public_key, data_dir)?,
        };

        Ok((account, keys))
    }

    pub(crate) fn create_nostr_mls(
        pubkey: PublicKey,
        data_dir: &Path,
    ) -> core::result::Result<Arc<Mutex<NostrMls<NostrMlsSqliteStorage>>>, AccountError> {
        let mls_storage_dir = data_dir.join("mls").join(pubkey.to_hex());
        let storage = NostrMlsSqliteStorage::new(mls_storage_dir)?;
        Ok(Arc::new(Mutex::new(NostrMls::new(storage))))
    }

    pub(crate) fn load_nostr_group_ids(&self) -> core::result::Result<Vec<String>, AccountError> {
        let groups;
        {
            let nostr_mls = self.nostr_mls.lock().unwrap();
            groups = nostr_mls.get_groups()?;
        }
        Ok(groups
            .iter()
            .map(|g| hex::encode(g.nostr_group_id))
            .collect())
    }

    pub(crate) fn default_relays() -> Vec<RelayUrl> {
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
}

impl Whitenoise {
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
    pub(crate) async fn initialize_accounts(&self) -> Result<HashMap<PublicKey, Account>> {
        tracing::debug!(target: "whitenoise::load_accounts", "Loading all accounts from database");

        let accounts = Account::all(&self).await?;

        if accounts.is_empty() {
            tracing::debug!(target: "whitenoise::load_accounts", "No accounts found in database");
            return Ok(HashMap::new());
        }

        let mut accounts_map = HashMap::new();

        for account in accounts {
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

    /// Creates a new identity (account) for the user.
    ///
    /// This method generates a new keypair, sets up the account with default relay lists,
    /// creates a metadata event with a generated petname, and fully configures the account
    /// for use in Whitenoise.
    ///
    /// # Returns
    ///
    /// Returns the newly created and fully configured `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if any step fails. The operation is atomic with cleanup on failure.
    pub async fn create_identity(&self) -> Result<Account> {
        let keys = Keys::generate();
        tracing::debug!(target: "whitenoise::create_identity", "Generated new keypair: {}", keys.public_key().to_hex());

        let mut account = self.create_base_account_with_private_key(&keys).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Keys stored in secret store");

        self.setup_relays_for_new_account(&mut account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Relays setup");

        self.persist_and_activate_account(&account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Account persisted and activated");

        self.setup_metadata(&account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Metadata setup");

        tracing::debug!(target: "whitenoise::create_identity", "Successfully created new identity: {}", account.pubkey.to_hex());
        Ok(account)
    }

    /// Logs in an existing user using a private key (nsec or hex format).
    ///
    /// This method parses the private key, checks if the account exists locally,
    /// and sets up the account for use. If the account doesn't exist locally,
    /// it treats it as an existing account and fetches data from the network.
    ///
    /// # Arguments
    ///
    /// * `nsec_or_hex_privkey` - The user's private key as a nsec string or hex-encoded string.
    ///
    /// # Returns
    ///
    /// Returns the fully configured `Account` associated with the provided private key on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the private key is invalid or account setup fails.
    pub async fn login(&self, nsec_or_hex_privkey: String) -> Result<Account> {
        let keys = Keys::parse(&nsec_or_hex_privkey)?;
        let pubkey = keys.public_key();
        tracing::debug!(target: "whitenoise::login", "Logging in with pubkey: {}", pubkey.to_hex());

        let mut account = self.create_base_account_with_private_key(&keys).await?;
        tracing::debug!(target: "whitenoise::login", "Keys stored in secret store");

        self.setup_relays_for_existing_account(&mut account).await?;
        tracing::debug!(target: "whitenoise::login", "Relays setup");

        self.persist_and_activate_account(&account).await?;
        tracing::debug!(target: "whitenoise::login", "Account persisted and activated");

        self.background_fetch_account_data(&account).await?;
        tracing::debug!(target: "whitenoise::login", "Background data fetch triggered");

        tracing::debug!(target: "whitenoise::login", "Successfully logged in: {}", account.pubkey.to_hex());
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
        let account = Account::find_by_pubkey(pubkey, &self).await?;
        // Delete the account from the database
        account.delete(&self).await?;

        // Remove the private key from the secret store
        self.secrets_store.remove_private_key_for_pubkey(pubkey)?;

        Ok(())
    }

    async fn create_base_account_with_private_key(&self, keys: &Keys) -> Result<Account> {
        let account = Account::new(Some(keys.clone()), &self.config.data_dir).await?;

        self.secrets_store.store_private_key(keys).map_err(|e| {
            tracing::error!(target: "whitenoise::setup_account", "Failed to store private key: {}", e);
            e
        })?;

        Ok(account.0)
    }

    async fn persist_and_activate_account(&self, account: &Account) -> Result<()> {
        self.persist_account(account).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Account saved to database");
        self.connect_account_relays(account).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Relays connected");
        self.setup_subscriptions(account).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Subscriptions setup");
        self.setup_key_package(account).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Key package setup");
        Ok(())
    }

    async fn setup_metadata(&self, account: &Account) -> Result<()> {
        let petname = petname::petname(2, " ")
            .unwrap_or_else(|| "Anonymous User".to_string())
            .split_whitespace()
            .map(Whitenoise::capitalize_first_letter)
            .collect::<Vec<_>>()
            .join(" ");

        let metadata = Metadata {
            name: Some(petname.clone()),
            display_name: Some(petname),
            ..Default::default()
        };

        account.update_metadata(&metadata, &self).await?;
        tracing::debug!(target: "whitenoise::setup_metadata", "Created and published metadata with petname: {}", metadata.name.as_ref().unwrap_or(&"Unknown".to_string()));
        Ok(())
    }

    async fn persist_account(&self, account: &Account) -> Result<()> {
        account.save(&self).await.map_err(|e| {
            tracing::error!(target: "whitenoise::setup_account", "Failed to save account: {}", e);
            // Try to clean up stored private key
            if let Err(cleanup_err) = self.secrets_store.remove_private_key_for_pubkey(&account.pubkey) {
                tracing::error!(target: "whitenoise::setup_account", "Failed to cleanup private key after account save failure: {}", cleanup_err);
            }
            e
        })?;
        tracing::debug!(target: "whitenoise::setup_account", "Account saved to database");
        Ok(())
    }

    async fn setup_key_package(&self, account: &Account) -> Result<()> {
        let relays = account.key_package_relays(&self).await?;
        let key_package_event = self
            .nostr
            .fetch_user_key_package(account.pubkey, relays)
            .await?;
        if key_package_event.is_none() {
            self.publish_key_package_for_account(account).await?;
            tracing::debug!(target: "whitenoise::setup_account", "Published key package");
        }
        Ok(())
    }

    async fn setup_relays_for_existing_account(&self, account: &mut Account) -> Result<()> {
        let pubkey = account.pubkey;
        let nip65_relays = account.nip65_relays(&self).await?;
        let mut default_relays = Vec::new();
        for relay in Account::default_relays() {
            default_relays.push(Relay::find_by_url(&relay, &self).await?);
        }

        self.fetch_or_publish_default_relays(pubkey, RelayType::Nostr, &default_relays)
            .await?;

        self.fetch_or_publish_default_relays(pubkey, RelayType::Inbox, &nip65_relays)
            .await?;
        self.fetch_or_publish_default_relays(pubkey, RelayType::KeyPackage, &nip65_relays)
            .await?;

        Ok(())
    }

    async fn fetch_or_publish_default_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
        source_relays: &Vec<Relay>,
    ) -> Result<Vec<Relay>> {
        match self
            .fetch_relays_from(source_relays, pubkey, relay_type)
            .await
        {
            Ok(relays) if !relays.is_empty() => Ok(relays),
            _ => {
                let mut default_relays = Vec::new();
                for relay in Account::default_relays() {
                    default_relays.push(Relay::find_by_url(&relay, &self).await?);
                }
                self.publish_relay_list_for_pubkey(
                    pubkey,
                    default_relays.clone(),
                    relay_type,
                    source_relays.clone(),
                )
                .await?;
                Ok(default_relays)
            }
        }
    }

    async fn setup_relays_for_new_account(&self, account: &mut Account) -> Result<()> {
        let mut default_relays = Vec::new();
        for relay in Account::default_relays() {
            default_relays.push(Relay::find_by_url(&relay, &self).await?);
        }

        // New accounts use default relays for all relay types
        for relay_type in [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage] {
            self.publish_relay_list_for_pubkey(
                account.pubkey,
                default_relays.clone(),
                relay_type,
                default_relays.clone(),
            )
            .await?;
        }

        Ok(())
    }

    pub(crate) async fn background_fetch_account_data(&self, account: &Account) -> Result<()> {
        let group_ids = account.load_nostr_group_ids()?;
        let nostr = self.nostr.clone();
        let database = self.database.clone();
        let account_pubkey = account.pubkey;
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account_pubkey)?;
        let last_synced_timestamp = account.last_synced_at.clone()
                .map(|dt| Timestamp::from(dt.timestamp() as u64))
                .unwrap_or_else(|| Timestamp::from(0));

        tokio::spawn(async move {
            tracing::debug!(
                target: "whitenoise::background_fetch_account_data",
                "Starting background fetch for account: {}",
                account_pubkey.to_hex()
            );

            let current_time = Timestamp::now();
            match nostr
                .fetch_all_user_data_to_nostr_cache(signer, last_synced_timestamp, group_ids)
                .await
            {
                Ok(_) => {
                    // Update the last_synced timestamp in the database
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

    pub(crate) async fn setup_subscriptions(&self, account: &Account) -> Result<()> {
        let mut group_relays = HashSet::new();
        let groups: Vec<group_types::Group>;
        {
            let nostr_mls = &*account.nostr_mls.lock().unwrap();
            groups = nostr_mls.get_groups()?;
            // Collect all relays from all groups into a single vector
            for group in &groups {
                let relays = nostr_mls.get_relays(&group.mls_group_id)?;
                for relay in relays {
                    group_relays.insert(relay.clone());
                }
            }
        };
        // We do this in two stages to deduplicate the relays
        let mut group_relays_vec = Vec::new();
        for relay in group_relays {
            group_relays_vec.push(Relay::find_by_url(&relay, &self).await?);
        }

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
                account.nip65_relays(&self).await?,
                account.inbox_relays(&self).await?,
                group_relays_vec,
                nostr_group_ids,
                keys,
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn encoded_key_package(
        &self,
        account: &Account,
    ) -> Result<(String, [Tag; 4])> {
        let nostr_mls = &*account.nostr_mls.lock().unwrap();
        let key_package_relay_urls = account
            .key_package_relays(&self)
            .await?
            .iter()
            .map(|r| r.url.clone())
            .collect::<Vec<RelayUrl>>();
        let result = nostr_mls
            .create_key_package_for_event(&account.pubkey, key_package_relay_urls)
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
                account.key_package_relays(&self).await?,
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
                    account.key_package_relays(&self).await?,
                    signer,
                )
                .await?;
        } else {
            tracing::warn!(target: "whitenoise::delete_key_package_from_relays_for_account", "Key package event not found for account: {}", account.pubkey.to_hex());
            return Ok(());
        }

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
    async fn test_load_accounts() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Test loading empty database
        let accounts = Account::all(&whitenoise).await.unwrap();
        assert!(accounts.is_empty());

        // Create test accounts and save them to database
        let (account1, keys1) = create_test_account().await;
        let (account2, keys2) = create_test_account().await;

        // Save accounts to database
        account1.save(&whitenoise).await.unwrap();
        account2.save(&whitenoise).await.unwrap();

        // Store keys in secrets store (required for background fetch)
        whitenoise.secrets_store.store_private_key(&keys1).unwrap();
        whitenoise.secrets_store.store_private_key(&keys2).unwrap();

        // Load accounts from database
        let loaded_accounts = Account::all(&whitenoise).await.unwrap();
        assert_eq!(loaded_accounts.len(), 2);
        let pubkeys: Vec<PublicKey> = loaded_accounts.iter().map(|a| a.pubkey).collect();
        assert!(pubkeys.contains(&account1.pubkey));
        assert!(pubkeys.contains(&account2.pubkey));

        // Verify account data is correctly loaded
        let loaded_account1 = loaded_accounts
            .iter()
            .find(|a| a.pubkey == account1.pubkey)
            .unwrap();
        assert_eq!(loaded_account1.pubkey, account1.pubkey);
        assert_eq!(loaded_account1.user_id, account1.user_id);
        assert_eq!(loaded_account1.last_synced_at, account1.last_synced_at);
        assert_eq!(loaded_account1.created_at, account1.created_at);
        assert_eq!(loaded_account1.updated_at, account1.updated_at);
    }

    #[tokio::test]
    async fn test_create_identity_publishes_relay_lists() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create a new identity
        let account = whitenoise.create_identity().await.unwrap();

        // Give the events time to be published and processed
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Query the nostr client's database for the published relay list events
        let inbox_relays_filter = Filter::new()
            .author(account.pubkey)
            .kind(Kind::InboxRelays) // kind 10050
            .limit(1);

        let key_package_relays_filter = Filter::new()
            .author(account.pubkey)
            .kind(Kind::MlsKeyPackageRelays) // kind 10051
            .limit(1);

        let key_package_filter = Filter::new()
            .author(account.pubkey)
            .kind(Kind::MlsKeyPackage) // kind 443
            .limit(1);

        // Check that all three event types were published
        let inbox_events = whitenoise
            .nostr
            .client
            .database()
            .query(inbox_relays_filter)
            .await
            .unwrap();

        let key_package_relays_events = whitenoise
            .nostr
            .client
            .database()
            .query(key_package_relays_filter)
            .await
            .unwrap();

        let key_package_events = whitenoise
            .nostr
            .client
            .database()
            .query(key_package_filter)
            .await
            .unwrap();

        // Verify that the relay list events were published
        assert!(
            !inbox_events.is_empty(),
            "Inbox relays list (kind 10050) should be published for new accounts"
        );
        assert!(
            !key_package_relays_events.is_empty(),
            "Key package relays list (kind 10051) should be published for new accounts"
        );
        assert!(
            !key_package_events.is_empty(),
            "Key package (kind 443) should be published for new accounts"
        );

        // Verify the events are authored by the correct pubkey
        if let Some(inbox_event) = inbox_events.first() {
            assert_eq!(inbox_event.pubkey, account.pubkey);
            assert_eq!(inbox_event.kind, Kind::InboxRelays);
        }

        if let Some(key_package_relays_event) = key_package_relays_events.first() {
            assert_eq!(key_package_relays_event.pubkey, account.pubkey);
            assert_eq!(key_package_relays_event.kind, Kind::MlsKeyPackageRelays);
        }

        if let Some(key_package_event) = key_package_events.first() {
            assert_eq!(key_package_event.pubkey, account.pubkey);
            assert_eq!(key_package_event.kind, Kind::MlsKeyPackage);
        }
    }

    /// Helper function to verify that an account has all three relay lists properly configured
    async fn verify_account_relay_lists_setup(whitenoise: &Whitenoise, account: &Account) {
        // Verify all three relay lists are set up with default relays
        let default_relays = Account::default_relays();
        let default_relay_count = default_relays.len();

        // Check relay database state
        assert_eq!(
            account.nip65_relays(&whitenoise).await.unwrap().len(),
            default_relay_count,
            "Account should have default NIP-65 relays configured"
        );
        assert_eq!(
            account.inbox_relays(&whitenoise).await.unwrap().len(),
            default_relay_count,
            "Account should have default inbox relays configured"
        );
        assert_eq!(
            account.key_package_relays(&whitenoise).await.unwrap().len(),
            default_relay_count,
            "Account should have default key package relays configured"
        );

        // Verify that all relay sets contain the same default relays
        // Convert DashSet to Vec to avoid iterator type issues
        let default_relays_vec: Vec<RelayUrl> = default_relays.into_iter().collect();
        let nip65_relay_urls: Vec<RelayUrl> = account
            .nip65_relays(&whitenoise)
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect();
        let inbox_relay_urls: Vec<RelayUrl> = account
            .inbox_relays(&whitenoise)
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect();
        let key_package_relay_urls: Vec<RelayUrl> = account
            .key_package_relays(&whitenoise)
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect();
        for default_relay in default_relays_vec.iter() {
            assert!(
                nip65_relay_urls.contains(default_relay),
                "NIP-65 relays should contain default relay: {}",
                default_relay
            );
            assert!(
                inbox_relay_urls.contains(default_relay),
                "Inbox relays should contain default relay: {}",
                default_relay
            );
            assert!(
                key_package_relay_urls.contains(default_relay),
                "Key package relays should contain default relay: {}",
                default_relay
            );
        }
    }

    /// Helper function to verify that an account has a key package published
    async fn verify_account_key_package_exists(whitenoise: &Whitenoise, account: &Account) {
        // Check if key package exists by trying to fetch it
        let key_package_event = whitenoise
            .nostr
            .fetch_user_key_package(
                account.pubkey,
                account.key_package_relays(&whitenoise).await.unwrap(),
            )
            .await
            .unwrap();

        assert!(
            key_package_event.is_some(),
            "Account should have a key package published to relays"
        );

        // If key package exists, verify it's authored by the correct account
        if let Some(event) = key_package_event {
            assert_eq!(
                event.pubkey, account.pubkey,
                "Key package should be authored by the account's public key"
            );
            assert_eq!(
                event.kind,
                Kind::MlsKeyPackage,
                "Event should be a key package (kind 443)"
            );
        }
    }

    #[tokio::test]
    async fn test_create_identity_sets_up_all_requirements() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create a new identity
        let account = whitenoise.create_identity().await.unwrap();

        // Give the events time to be published and processed
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify all three relay lists are properly configured
        verify_account_relay_lists_setup(&whitenoise, &account).await;

        // Verify key package is published
        verify_account_key_package_exists(&whitenoise, &account).await;
    }

    #[tokio::test]
    async fn test_login_existing_account_sets_up_all_requirements() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create an account through login (simulating an existing account)
        let keys = create_test_keys();
        let account = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Give the events time to be published and processed
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify all three relay lists are properly configured
        verify_account_relay_lists_setup(&whitenoise, &account).await;

        // Verify key package is published
        verify_account_key_package_exists(&whitenoise, &account).await;
    }

    #[tokio::test]
    async fn test_login_with_existing_relay_lists_preserves_them() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // First, create an account and let it publish relay lists
        let keys = create_test_keys();
        let account1 = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Give time for initial setup
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify initial setup is correct
        verify_account_relay_lists_setup(&whitenoise, &account1).await;
        verify_account_key_package_exists(&whitenoise, &account1).await;

        // Logout the account
        whitenoise.logout(&account1.pubkey).await.unwrap();

        // Login again with the same keys (simulating returning user)
        let account2 = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Give time for login process
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify that relay lists are still properly configured
        verify_account_relay_lists_setup(&whitenoise, &account2).await;

        // Verify key package still exists (should not publish a new one)
        verify_account_key_package_exists(&whitenoise, &account2).await;

        // Accounts should be equivalent (same pubkey, same basic setup)
        assert_eq!(
            account1.pubkey, account2.pubkey,
            "Same keys should result in same account"
        );
    }

    #[tokio::test]
    async fn test_multiple_accounts_each_have_proper_setup() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create multiple accounts
        let mut accounts = Vec::new();
        for i in 0..3 {
            let keys = create_test_keys();
            let account = whitenoise
                .login(keys.secret_key().to_secret_hex())
                .await
                .unwrap();
            accounts.push((account, keys));

            tracing::info!("Created account {}: {}", i, accounts[i].0.pubkey.to_hex());
        }

        // Give time for all accounts to be set up
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        // Verify each account has proper setup
        for (i, (account, _)) in accounts.iter().enumerate() {
            tracing::info!("Verifying account {}: {}", i, account.pubkey.to_hex());

            // Verify all three relay lists are properly configured
            verify_account_relay_lists_setup(&whitenoise, account).await;

            // Verify key package is published
            verify_account_key_package_exists(&whitenoise, account).await;
        }

        // Verify accounts are distinct
        for i in 0..accounts.len() {
            for j in i + 1..accounts.len() {
                assert_ne!(
                    accounts[i].0.pubkey, accounts[j].0.pubkey,
                    "Each account should have a unique public key"
                );
            }
        }
    }
}
