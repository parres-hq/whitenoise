use crate::{relays::RelayType, Whitenoise, WhitenoiseError};

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

    #[error("No active account found")]
    NoActiveAccount,

    #[error("Nostr MLS error: {0}")]
    NostrMlsError(#[from] nostr_mls::Error),

    #[error("Nostr MLS SQLite storage error: {0}")]
    NostrMlsSqliteStorageError(#[from] nostr_mls_sqlite_storage::error::Error),

    #[error("Nostr MLS not initialized")]
    NostrMlsNotInitialized,
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct ActiveAccount {
    pub pubkey: String,
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
pub struct AccountOnboarding {
    pub inbox_relays: bool,
    pub key_package_relays: bool,
    pub publish_key_package: bool,
}

/// This is an intermediate struct representing an account in the database
#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct AccountRow {
    pub pubkey: String,
    pub metadata: String,   // JSON string
    pub settings: String,   // JSON string
    pub onboarding: String, // JSON string
    pub relays: String,     // JSON string
    pub nwc: String,        // JSON string
    pub last_used: u64,
    pub last_synced: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub pubkey: PublicKey,
    pub metadata: Metadata,
    pub settings: AccountSettings,
    pub onboarding: AccountOnboarding,
    pub relays: AccountRelays,
    pub nwc: AccountNwc,
    pub last_used: Timestamp,
    pub last_synced: Timestamp,
    pub contacts: Vec<PublicKey>,
    #[serde(skip)]
    pub nostr_mls: Arc<Mutex<Option<NostrMls<NostrMlsSqliteStorage>>>>,
    pub groups: Vec<group_types::Group>,
    pub weclomes: Vec<welcome_types::Welcome>,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("pubkey", &self.pubkey)
            .field("metadata", &self.metadata)
            .field("settings", &self.settings)
            .field("onboarding", &self.onboarding)
            .field("last_used", &self.last_used)
            .field("last_synced", &self.last_synced)
            .field("relays", &self.relays)
            .field("nwc", &self.nwc)
            .field("contacts", &self.contacts)
            .field("nostr_mls", &"<REDACTED>")
            .field("groups", &self.groups)
            .field("weclomes", &self.weclomes)
            .finish()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AccountRelays {
    pub nostr_relays: Vec<RelayUrl>,
    pub inbox_relays: Vec<RelayUrl>,
    pub key_package_relays: Vec<RelayUrl>,
}

impl AccountRelays {
    pub fn get_relays(&self, relay_type: RelayType) -> Vec<RelayUrl> {
        match relay_type {
            RelayType::Nostr => self.nostr_relays.clone(),
            RelayType::Inbox => self.inbox_relays.clone(),
            RelayType::KeyPackage => self.key_package_relays.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AccountNwc {
    pub nwc_url: String,
    pub balance: u64,
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
        // Create a new account with a generated keypair
        tracing::debug!(target: "whitenoise::accounts::new", "Generating new keypair");
        let keys = Keys::generate();

        let mut account = Account {
            pubkey: keys.public_key(),
            metadata: Metadata::default(),
            settings: AccountSettings::default(),
            onboarding: AccountOnboarding::default(),
            last_used: Timestamp::now(),
            last_synced: Timestamp::zero(),
            relays: AccountRelays::default(),
            nwc: AccountNwc::default(),
            contacts: Vec::new(),
            nostr_mls: Arc::new(Mutex::new(None)),
            groups: Vec::new(),
            weclomes: Vec::new(),
        };

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

        tracing::debug!(target: "whitenoise::accounts::new", "Generated petname: {}", petname);
        // Update account metadata with petname - metadata fields expect Option<String>
        account.metadata.name = Some(petname.clone());
        account.metadata.display_name = Some(petname);

        Ok((account, keys))
    }
}

impl Whitenoise {
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
    pub(crate) async fn save_account(&self, account: &Account) -> Result<Account, WhitenoiseError> {
        tracing::debug!(
            target: "whitenoise::accounts::save_account",
            "Beginning save transaction for pubkey: {}",
            account.pubkey.to_hex()
        );

        let mut txn = self.database.pool.begin().await?;

        let result = sqlx::query(
            "INSERT INTO accounts (pubkey, metadata, settings, onboarding, relays, nwc, last_used, last_synced)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(pubkey) DO UPDATE SET
                metadata = excluded.metadata,
                settings = excluded.settings,
                onboarding = excluded.onboarding,
                relays = excluded.relays,
                nwc = excluded.nwc,
                last_used = excluded.last_used,
                last_synced = excluded.last_synced"
        )
        .bind(account.pubkey.to_hex())
        .bind(&serde_json::to_string(&account.metadata)?)
        .bind(&serde_json::to_string(&account.settings)?)
        .bind(&serde_json::to_string(&account.onboarding)?)
        .bind(&serde_json::to_string(&account.relays)?)
        .bind(&serde_json::to_string(&account.nwc)?)
        .bind(account.last_used.to_string())
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
    ) -> Result<Account, WhitenoiseError> {
        tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Starting onboarding process");

        // Set onboarding flags
        account.onboarding.inbox_relays = true;
        account.onboarding.key_package_relays = true;

        let default_relays = self
            .nostr
            .relays()
            .await
            .keys()
            .cloned()
            .collect::<Vec<RelayUrl>>();

        // Update relays in database
        account.relays.nostr_relays = default_relays.clone();
        account.relays.inbox_relays = default_relays.clone();
        account.relays.key_package_relays = default_relays;

        // Publish the metadata event to Nostr
        let metadata_json = serde_json::to_string(&account.metadata)?;
        let event = EventBuilder::new(Kind::Metadata, metadata_json);

        let keys = self.get_nostr_keys_for_pubkey(&account.pubkey.to_hex())?;
        self.nostr.set_signer(keys).await;
        let result = self.nostr.send_event_builder(event.clone()).await?;
        tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Published metadata event to Nostr: {:?}", result);
        self.nostr.unset_signer().await;

        // Also publish relay lists to Nostr
        self.publish_relay_list_for_account(account, RelayType::Nostr)
            .await?;
        self.publish_relay_list_for_account(account, RelayType::Inbox)
            .await?;
        self.publish_relay_list_for_account(account, RelayType::KeyPackage)
            .await?;

        // Publish key package to key package relays
        match self.publish_key_package_for_account(account).await {
            Ok(_) => {
                account.onboarding.publish_key_package = true;
                self.save_account(account).await?;
                tracing::debug!(target: "whitenoise::accounts::onboard_new_account", "Published key package to relays");
            }
            Err(e) => {
                account.onboarding.publish_key_package = false;
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
    /// using the relays stored in the account. If there are no relays of the specified type,
    /// the method returns early.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose relay list will be published.
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
        relay_type: RelayType,
    ) -> Result<(), WhitenoiseError> {
        let relays = account.relays.get_relays(relay_type);
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
        let keys = self.get_nostr_keys_for_pubkey(&account.pubkey.to_hex())?;
        self.nostr.set_signer(keys).await;
        let result = self.nostr.send_event_builder(event.clone()).await?;
        tracing::debug!(target: "whitenoise::accounts::publish_relay_list", "Published relay list event to Nostr: {:?}", result);
        self.nostr.unset_signer().await;

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
    ) -> Result<(), WhitenoiseError> {
        let mut encoded_key_package: Option<String> = None;
        let mut tags: Option<[Tag; 4]> = None;
        let key_package_relays = account.relays.get_relays(RelayType::KeyPackage);

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

        let signer = self.get_nostr_keys_for_pubkey(&account.pubkey.to_hex())?;
        self.nostr.set_signer(signer).await;
        if encoded_key_package.is_some() && tags.is_some() {
            let key_package_event_builder =
                EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package.unwrap())
                    .tags(tags.unwrap());

            let result = self
                .nostr
                .send_event_builder_to(key_package_relays, key_package_event_builder.clone())
                .await?;
            tracing::debug!(target: "whitenoise::accounts::publish_key_package_for_account", "Published key package to relays: {:?}", result);
        }

        self.nostr.unset_signer().await;
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
        assert!(account.metadata.name.is_some());
        assert!(account.metadata.display_name.is_some());
        // Check defaults
        assert!(account.settings.dark_theme);
        assert!(!account.settings.dev_mode);
        assert!(!account.settings.lockdown_mode);
        assert!(!account.onboarding.inbox_relays);
        assert!(!account.onboarding.key_package_relays);
        assert!(!account.onboarding.publish_key_package);
    }

    #[test]
    fn test_account_relays_get_relays() {
        let nostr = RelayUrl::parse("wss://relay.nostr.example").unwrap();
        let inbox = RelayUrl::parse("wss://inbox.nostr.example").unwrap();
        let key_package = RelayUrl::parse("wss://keypkg.nostr.example").unwrap();
        let relays = AccountRelays {
            nostr_relays: vec![nostr.clone()],
            inbox_relays: vec![inbox.clone()],
            key_package_relays: vec![key_package.clone()],
        };
        assert_eq!(relays.get_relays(RelayType::Nostr), vec![nostr]);
        assert_eq!(relays.get_relays(RelayType::Inbox), vec![inbox]);
        assert_eq!(relays.get_relays(RelayType::KeyPackage), vec![key_package]);
    }
}
