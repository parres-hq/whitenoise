use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::sync::{Arc, Mutex};

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
pub(crate) struct AccountRow {
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

    pub(crate) fn groups_nostr_group_ids(&self) -> core::result::Result<Vec<String>, AccountError> {
        let nostr_mls_guard = self
            .nostr_mls
            .lock()
            .map_err(|_| AccountError::NostrMlsNotInitialized)?;

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
    use std::sync::{Arc, Mutex};

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
            nostr_mls: Arc::new(Mutex::new(None)),
        };

        let debug_str = format!("{:?}", account);
        assert!(debug_str.contains("Account"));
        assert!(debug_str.contains(&keys.public_key().to_hex()));
        assert!(debug_str.contains("<REDACTED>"));
        assert!(!debug_str.contains("NostrMls"));
    }

    #[test]
    fn test_groups_nostr_group_ids_when_nostr_mls_none() {
        let keys = Keys::generate();
        let account = Account {
            pubkey: keys.public_key(),
            settings: AccountSettings::default(),
            onboarding: OnboardingState::default(),
            last_synced: Timestamp::zero(),
            nostr_mls: Arc::new(Mutex::new(None)),
        };

        let group_ids = account.groups_nostr_group_ids().unwrap();
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
                nostr_mls: Arc::new(Mutex::new(None)),
            };
            (account, keys)
        }
    }
}
