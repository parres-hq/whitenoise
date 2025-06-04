use crate::{accounts::Account, Whitenoise, WhitenoiseError};

use nostr_mls::NostrMls;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
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
    ///
    /// # Returns
    ///
    /// Returns the newly created `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if any step fails, such as account creation, database save, key storage, or onboarding.
    pub async fn create_identity(&self) -> Result<Account, WhitenoiseError> {
        // Create a new account with a generated keypair and a petname
        let (initial_account, keys) = Account::new().await?;

        // Save the account to the database
        let mut account = self.save_account(&initial_account).await?;

        // Add the keys to the secret store
        self.store_private_key(&keys)?;

        // Initialize NostrMls for the account
        let storage_dir = self
            .config
            .data_dir
            .join("mls")
            .join(account.pubkey.to_hex());
        let nostr_mls = NostrMls::new(NostrMlsSqliteStorage::new(storage_dir)?);
        {
            let mut guard = account.nostr_mls.lock().unwrap();
            *guard = Some(nostr_mls);
        }

        // Onboard the account
        self.onboard_new_account(&mut account).await?;

        Ok(account)
    }

    /// Logs in an existing user using a private key (nsec or hex format).
    ///
    /// This method performs the following steps:
    /// - Parses the provided private key (either nsec or hex format) to obtain the user's keys.
    /// - Attempts to find an existing account in the database matching the public key.
    /// - If the account exists, returns it.
    /// - If the account does not exist, creates a new account from the provided keys and adds it to the database.
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
    pub async fn login(&self, nsec_or_hex_privkey: String) -> Result<Account, WhitenoiseError> {
        let keys = Keys::parse(&nsec_or_hex_privkey)?;

        match self.find_account_by_pubkey(&keys.public_key).await {
            Ok(account) => {
                tracing::debug!(target: "whitenoise::api::accounts::login", "Account found");
                Ok(account)
            }
            _ => {
                tracing::debug!(target: "whitenoise::api::accounts::login", "Account not found, adding from keys");
                let account = self.add_account_from_keys(&keys).await?;
                Ok(account)
            }
        }
    }
}
