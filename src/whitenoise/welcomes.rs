use std::collections::HashSet;

use nostr_mls::prelude::*;

use crate::whitenoise::{
    accounts::Account,
    error::{Result, WhitenoiseError},
    relays::Relay,
    Whitenoise,
};

impl Whitenoise {
    /// Finds a specific welcome message by its event ID for a given public key.
    ///
    /// This method retrieves a welcome message that was previously received and stored
    /// in the nostr-mls system. Welcome messages are used to invite users to join
    /// MLS groups in the Nostr ecosystem.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the account to search welcomes for
    /// * `welcome_event_id` - The event ID of the welcome message to find (as a hex string)
    pub async fn find_welcome_by_event_id(
        &self,
        pubkey: &PublicKey,
        welcome_event_id: String,
    ) -> Result<welcome_types::Welcome> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let welcome = nostr_mls
            .get_welcome(&welcome_event_id)?
            .ok_or(WhitenoiseError::WelcomeNotFound)?;
        Ok(welcome)
    }

    /// Retrieves all pending welcome messages for a given public key.
    ///
    /// This method returns a list of all welcome messages that have been received
    /// but not yet accepted or declined by the user. Pending welcomes represent
    /// group invitations that are waiting for the user's response.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the account to get pending welcomes for
    pub async fn pending_welcomes(
        &self,
        pubkey: &PublicKey,
    ) -> Result<Vec<welcome_types::Welcome>> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let welcomes = nostr_mls.get_pending_welcomes()?;
        Ok(welcomes)
    }

    /// Accepts a welcome message and joins the associated MLS group.
    ///
    /// This method processes a pending welcome message by accepting the group invitation
    /// and performing all necessary setup to join the MLS group. This includes:
    /// - Accepting the welcome in the MLS system
    /// - Retrieving group information and relay configurations
    /// - Setting up Nostr subscriptions for group messages
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the account accepting the welcome
    /// * `welcome_event_id` - The event ID of the welcome message to accept (as a hex string)
    pub async fn accept_welcome(&self, pubkey: &PublicKey, welcome_event_id: String) -> Result<()> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        let keys = self.secrets_store.get_nostr_keys_for_pubkey(pubkey)?;

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;

        let welcome = nostr_mls.get_welcome(&welcome_event_id)?;
        let result = if let Some(welcome) = welcome {
            nostr_mls.accept_welcome(&welcome)?;

            let groups = nostr_mls.get_groups()?;
            let mut group_relays = Vec::new();
            let group_ids = groups
                .iter()
                .map(|g| hex::encode(g.nostr_group_id))
                .collect::<Vec<_>>();

            // Collect all relays from all groups into a single vector
            for group in &groups {
                let relays = nostr_mls.get_relays(&group.mls_group_id)?;
                group_relays.extend(relays);
            }

            // Remove duplicates by sorting and deduplicating
            group_relays.sort();
            group_relays.dedup();
            Ok((group_ids, group_relays))
        } else {
            Err(WhitenoiseError::WelcomeNotFound)
        }?;

        let (group_ids, group_relays) = result;

        let mut relays = HashSet::new();
        for relay in group_relays {
            let db_relay = Relay::find_or_create_by_url(&relay, &self.database).await?;
            relays.insert(db_relay);
        }

        self.nostr
            .setup_group_messages_subscriptions_with_signer(
                *pubkey,
                &relays.into_iter().collect::<Vec<_>>(),
                &group_ids,
                keys,
            )
            .await
            .map_err(WhitenoiseError::from)?;

        Ok(())
    }

    /// Declines a welcome message and rejects the group invitation.
    ///
    /// This method processes a pending welcome message by declining the group invitation.
    /// The welcome message will be marked as declined in the MLS system and will no longer
    /// appear in the list of pending welcomes. The user will not join the associated group.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the account declining the welcome
    /// * `welcome_event_id` - The event ID of the welcome message to decline (as a hex string)
    pub async fn decline_welcome(
        &self,
        pubkey: &PublicKey,
        welcome_event_id: String,
    ) -> Result<()> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let welcome = nostr_mls.get_welcome(&welcome_event_id)?;
        if let Some(welcome) = welcome {
            nostr_mls.decline_welcome(&welcome)?;
        } else {
            return Err(WhitenoiseError::WelcomeNotFound);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;

    #[tokio::test]
    #[ignore]
    async fn test_receive_welcomes() {
        let whitenoise = test_get_whitenoise().await;
        let (creator_account, _creator_keys) = setup_login_account(whitenoise).await;

        // Setup member accounts
        let member_accounts = setup_multiple_test_accounts(whitenoise, 2).await;
        let member_pubkeys: Vec<PublicKey> =
            member_accounts.iter().map(|(acc, _)| acc.pubkey).collect();

        // Setup admin accounts (creator + one member as admin)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];
        let config = create_nostr_group_config_data(admin_pubkeys.clone());

        let group = whitenoise
            .create_group(&creator_account, member_pubkeys.clone(), config, None)
            .await;
        assert!(group.is_ok());
        let result1 = whitenoise
            .pending_welcomes(&creator_account.pubkey)
            .await
            .unwrap();
        assert!(result1.is_empty()); // creator should not receive welcome messages
        whitenoise.logout(&creator_account.pubkey).await.unwrap();

        let admin_key = &member_accounts[0].1;
        let regular_key = &member_accounts[1].1;

        tracing::info!("Logging into account {}", admin_key.public_key.to_hex());
        let account = whitenoise
            .login(admin_key.secret_key().to_secret_hex())
            .await
            .unwrap();
        // Give some time for the event processor to process welcome messages
        // sleep(Duration::from_secs(3));
        let result = whitenoise.pending_welcomes(&account.pubkey).await.unwrap();
        assert!(!result.is_empty(), "{:?}", result);
        whitenoise.logout(&admin_key.public_key).await.unwrap();

        tracing::info!("Logging into account {}", regular_key.public_key.to_hex());
        let account = whitenoise
            .login(regular_key.secret_key().to_secret_hex())
            .await
            .unwrap();
        // Give some time for the event processor to process welcome messages
        let result = whitenoise.pending_welcomes(&account.pubkey).await.unwrap();
        assert!(!result.is_empty());
    }
}
