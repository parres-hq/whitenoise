use crate::{accounts::{Account}, Whitenoise, WhitenoiseError};

use nostr_sdk::prelude::*;
use nostr_mls::NostrMls;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;

impl Whitenoise {
    pub async fn create_identity(&self) -> Result<Account, WhitenoiseError> {
        // Create a new account with a generated keypair and a petname
        let (initial_account, keys) = Account::new().await?;

        // Save the account to the database
        let mut account = self.save_account(&initial_account).await?;

        // Add the keys to the secret store
        self.store_private_key(&keys)?;

        // Initialize NostrMls for the account
        let storage_dir = self.config.data_dir.join("mls").join(account.pubkey.to_hex());
        let nostr_mls = NostrMls::new(NostrMlsSqliteStorage::new(storage_dir).unwrap());
        {
            let mut guard = account.nostr_mls.lock().unwrap();
            *guard = Some(nostr_mls);
        }

        // Onboard the account
        self.onboard_new_account(&mut account).await?;

        Ok(account)
    }
}
