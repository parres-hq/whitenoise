use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::nostr_manager::NostrManagerError;

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

impl<'r, R> sqlx::FromRow<'r, R> for Account
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> Result<Self, sqlx::Error> {
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
    pub(crate) async fn new() -> core::result::Result<(Account, Keys), AccountError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
        let (account1, keys1) = Account::new().await.unwrap();
        let (account2, keys2) = Account::new().await.unwrap();

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
}
