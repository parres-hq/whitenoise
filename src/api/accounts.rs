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
