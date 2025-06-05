use crate::nostr_manager::NostrManagerError;
use crate::{relays::RelayType, Whitenoise};

use std::sync::{Arc, Mutex};

use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Default, sqlx::FromRow)]
pub struct OnboardingState {
    pub inbox_relays: bool,
    pub key_package_relays: bool,
    pub key_package_published: bool,
}

/// This is an intermediate struct representing an account in the database
#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
struct AccountRow {
    pub pubkey: String,
    pub settings: String,   // JSON string
    pub onboarding: String, // JSON string
    pub last_synced: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub pubkey: PublicKey,
    pub settings: AccountSettings,
    pub onboarding: OnboardingState,
    pub last_synced: Timestamp,
    #[serde(skip)]
    #[doc(hidden)]
    pub(crate) nostr_mls: Arc<Mutex<Option<NostrMls<NostrMlsSqliteStorage>>>>,
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
    pub(crate) async fn new() -> Result<(Account, Keys), AccountError> {
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

    pub(crate) fn groups_nostr_group_ids(&self) -> Result<Vec<String>, AccountError> {
        let mut group_ids = vec![];
        {
            let nostr_mls_guard = self.nostr_mls.lock().unwrap();
            if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
                let groups = nostr_mls.get_groups()?;
                group_ids = groups.iter().map(|g| g.nostr_group_id).collect::<Vec<[u8; 32]>>();
            }
        }
        Ok(group_ids.into_iter().map(hex::encode).collect::<Vec<String>>())
    }
}

impl Whitenoise {
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
    pub(crate) async fn find_account_by_pubkey(
        &self,
        pubkey: &PublicKey,
    ) -> Result<Account> {
        let mut txn = self.database.pool.begin().await?;

        let row = sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(pubkey.to_hex().as_str())
            .fetch_one(&mut *txn)
            .await?;

        let account = Account {
            pubkey: PublicKey::parse(row.pubkey.as_str()).map_err(AccountError::PublicKeyError)?,
            settings: serde_json::from_str(&row.settings)?,
            onboarding: serde_json::from_str(&row.onboarding)?,
            last_synced: Timestamp::from(row.last_synced),
            nostr_mls: Arc::new(Mutex::new(None)),
        };

        Ok(account)
    }

    /// Adds a new account to the database using the provided Nostr keys.
    ///
    /// This method initializes an `Account` struct with the given keys, fetches relevant events
    /// (such as metadata and relay lists) from Nostr, and populates the account's fields accordingly.
    /// It also fetches the contact list, updates relay and onboarding information, saves the account
    /// to the database, stores the private key, and initializes the Nostr MLS instance for the account.
    ///
    /// # Arguments
    ///
    /// * `keys` - A reference to the `Keys` struct containing the Nostr keypair for the account.
    ///
    /// # Returns
    ///
    /// Returns the newly created `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if any database operation, event fetching, serialization,
    /// or key storage fails.
    pub(crate) async fn add_account_from_keys(
        &self,
        keys: &Keys,
    ) -> Result<Account> {
        tracing::debug!(target: "whitenoise::accounts", "Adding account for pubkey: {}", keys.public_key().to_hex());

        let mut account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: OnboardingState::default(),
            last_synced: Timestamp::zero(),
            nostr_mls: Arc::new(Mutex::new(None)),
        };

        let onboarding_state = self.load_onboarding_state(keys.public_key()).await?;
        account.onboarding = onboarding_state;

        self.save_account(&account).await?;
        tracing::debug!(target: "whitenoise::accounts::add_account_from_keys", "Account saved to database");

        // Add the keys to the secret store
        self.store_private_key(keys)?;
        tracing::debug!(target: "whitenoise::accounts::add_account_from_keys", "Keys stored in secret store");

        // Trigger fetch of nostr events on another thread
        self.background_fetch_account_data(&account).await?;

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
    pub(crate) async fn save_account(&self, account: &Account) -> Result<Account> {
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
    pub(crate) async fn delete_account(&self, account: &Account) -> Result<()> {
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
    ///
    pub(crate) async fn initialize_nostr_mls_for_account(&self, account: &Account) -> Result<()> {
        // Initialize NostrMls for the account
        let mls_storage_dir = self
            .config
            .data_dir
            .join("mls")
            .join(account.pubkey.to_hex());

        let nostr_mls = NostrMls::new(NostrMlsSqliteStorage::new(mls_storage_dir)?);
        {
            let mut nostr_mls_guard = account.nostr_mls.lock().unwrap();
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
    pub(crate) async fn onboard_new_account(
        &self,
        account: &mut Account,
    ) -> Result<Account> {
        tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Starting onboarding process");

        // Set onboarding flags
        account.onboarding.inbox_relays = true;
        account.onboarding.key_package_relays = true;

        let default_relays = self
            .nostr
            .relays()
            .await?;

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
        let keys = self.get_nostr_keys_for_pubkey(&account.pubkey)?;
        let result = self.nostr.publish_event_builder_with_signer(event.clone(), keys).await?;
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
    pub(crate) async fn publish_relay_list_for_account(
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
        let keys = self.get_nostr_keys_for_pubkey(&account.pubkey)?;
        let result = self.nostr.publish_event_builder_with_signer(event.clone(), keys).await?;
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
    pub(crate) async fn publish_key_package_for_account(
        &self,
        account: &Account,
    ) -> Result<()> {
        let mut encoded_key_package: Option<String> = None;
        let mut tags: Option<[Tag; 4]> = None;
        let key_package_relays = self
            .load_relays(account.pubkey, RelayType::KeyPackage)
            .await?;

        {
            tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "Attempting to acquire nostr_mls lock");
            let nostr_mls_guard = match account.nostr_mls.lock() {
                Ok(guard) => {
                    tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "nostr_mls lock acquired");
                    guard
                }
                Err(_) => {
                    tracing::error!(target: "whitenoise::accounts::publish_key_package_for_account", "Timeout waiting for nostr_mls lock");
                    return Err(AccountError::NostrMlsNotInitialized)?;
                }
            };
            let _result = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
                let (encoded_key_package_value, tags_value) = nostr_mls
                    .create_key_package_for_event(&account.pubkey, key_package_relays.clone())
                    .map_err(AccountError::NostrMlsError)?;
                encoded_key_package = Some(encoded_key_package_value);
                tags = Some(tags_value);
                Ok(())
            } else {
                Err(AccountError::NostrMlsNotInitialized)
            };
        }
        tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "nostr_mls lock released");

        let signer = self.get_nostr_keys_for_pubkey(&account.pubkey)?;
        if encoded_key_package.is_some() && tags.is_some() {
            let key_package_event_builder =
                EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package.unwrap())
                    .tags(tags.unwrap());

            let result = self
                .nostr
                .publish_event_builder_with_signer(key_package_event_builder.clone(), signer)
                .await?;
            tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "Published key package to relays: {:?}", result);
        }

        Ok(())
    }

    pub(crate) async fn background_fetch_account_data(&self, account: &Account) -> Result<()> {
        let group_ids = account.groups_nostr_group_ids()?;
        let nostr = self.nostr.clone();
        let pubkey = account.pubkey;
        let last_synced = account.last_synced;

        tokio::spawn(async move {
            if let Err(e) = nostr.fetch_all_user_data(pubkey, last_synced, group_ids).await {
                tracing::error!("Failed to fetch user data: {}", e);
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_account_new_creates_account_and_keys() {
        let (account, keys) = Account::new().await.unwrap();
        assert_eq!(account.pubkey, keys.public_key());
        // Check defaults
        assert!(account.settings.dark_theme);
        assert!(!account.settings.dev_mode);
        assert!(!account.settings.lockdown_mode);
        assert!(!account.onboarding.inbox_relays);
        assert!(!account.onboarding.key_package_relays);
        assert!(!account.onboarding.key_package_published);
    }
}
