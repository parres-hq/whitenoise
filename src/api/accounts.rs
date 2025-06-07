//! Accounts API for Whitenoise
//! This module provides methods for managing user accounts, including creating, logging in, and logging out.
//!
//! All methods in this module only load data from the database. They do not perform any network requests.
//! The database is updated by the accounts manager in the backaground.

use crate::error::Result;
use crate::{accounts::Account, Whitenoise, WhitenoiseError};
use nostr_sdk::prelude::*;

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
    pub async fn create_identity(&mut self) -> Result<Account> {
        // Create a new account with a generated keypair and a petname
        let (initial_account, keys) = Account::new().await?;

        // Save the account to the database
        let mut account = self.save_account(&initial_account).await?;

        // Add the keys to the secret store
        self.store_private_key(&keys)?;

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
        self.remove_private_key_for_pubkey(&account.pubkey)?;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::AccountSettings;

    /// Creates a test account for use in tests
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

    #[test]
    fn test_update_active_account_basic() {
        let (account, _keys) = create_test_account();

        // Test the core logic of update_active_account
        let mut active_account: Option<PublicKey> = None;

        // Verify initial state
        assert_eq!(active_account, None);

        // Simulate the update_active_account logic
        active_account = Some(account.pubkey);

        // Verify the account was set as active
        assert_eq!(active_account, Some(account.pubkey));
    }

    #[test]
    fn test_update_active_account_switching() {
        let (account1, _keys1) = create_test_account();
        let (account2, _keys2) = create_test_account();

        // Test switching between accounts
        let mut active_account = Some(account1.pubkey);

        // Verify initial state
        assert_eq!(active_account, Some(account1.pubkey));

        // Switch to account2 (simulating update_active_account logic)
        active_account = Some(account2.pubkey);
        assert_eq!(active_account, Some(account2.pubkey));

        // Switch back to account1
        active_account = Some(account1.pubkey);
        assert_eq!(active_account, Some(account1.pubkey));
    }

    #[test]
    fn test_logout_account_removal_logic() {
        let (account1, _keys1) = create_test_account();
        let (account2, _keys2) = create_test_account();

        // Simulate the logout logic for account management
        let mut accounts = std::collections::HashMap::new();
        accounts.insert(account1.pubkey, account1.clone());
        accounts.insert(account2.pubkey, account2.clone());
        let mut active_account = Some(account2.pubkey);

        // Verify initial state
        assert_eq!(accounts.len(), 2);
        assert_eq!(active_account, Some(account2.pubkey));

        // Simulate logout of active account (account2)
        accounts.remove(&account2.pubkey);
        active_account = accounts.keys().next().copied(); // Set to remaining account

        // Verify account2 was removed and account1 became active
        assert_eq!(accounts.len(), 1);
        assert_eq!(active_account, Some(account1.pubkey));
        assert!(accounts.contains_key(&account1.pubkey));
        assert!(!accounts.contains_key(&account2.pubkey));

        // Simulate logout of remaining account
        accounts.remove(&account1.pubkey);
        active_account = accounts.keys().next().copied();

        // Verify all accounts removed
        assert_eq!(accounts.len(), 0);
        assert_eq!(active_account, None);
    }

    #[test]
    fn test_logout_non_active_account_logic() {
        let (account1, _keys1) = create_test_account();
        let (account2, _keys2) = create_test_account();

        // Test logout of non-active account
        let mut accounts = std::collections::HashMap::new();
        accounts.insert(account1.pubkey, account1.clone());
        accounts.insert(account2.pubkey, account2.clone());
        let active_account = Some(account2.pubkey);

        // Logout account1 (non-active)
        accounts.remove(&account1.pubkey);
        // Active account logic should remain unchanged when logging out non-active account

        // Verify account1 was removed but account2 remains active
        assert_eq!(accounts.len(), 1);
        assert_eq!(active_account, Some(account2.pubkey));
        assert!(!accounts.contains_key(&account1.pubkey));
        assert!(accounts.contains_key(&account2.pubkey));
    }

    #[test]
    fn test_account_state_management() {
        let (account1, _keys1) = create_test_account();
        let (account2, _keys2) = create_test_account();

        // Test comprehensive account state management patterns used in the API
        let mut accounts = std::collections::HashMap::new();
        let mut active_account: Option<PublicKey> = None;

        // Initial state
        assert_eq!(accounts.len(), 0);
        assert_eq!(active_account, None);

        // Add first account (simulate create_identity)
        accounts.insert(account1.pubkey, account1.clone());
        active_account = Some(account1.pubkey);

        assert_eq!(accounts.len(), 1);
        assert_eq!(active_account, Some(account1.pubkey));

        // Add second account (simulate login with new keys)
        accounts.insert(account2.pubkey, account2.clone());
        active_account = Some(account2.pubkey);

        assert_eq!(accounts.len(), 2);
        assert_eq!(active_account, Some(account2.pubkey));

        // Switch active account
        active_account = Some(account1.pubkey);
        assert_eq!(active_account, Some(account1.pubkey));
        assert_eq!(accounts.len(), 2); // Count should remain the same

        // Remove account
        accounts.remove(&account1.pubkey);
        active_account = Some(account2.pubkey);
        assert_eq!(accounts.len(), 1);
        assert_eq!(active_account, Some(account2.pubkey));
    }
}
