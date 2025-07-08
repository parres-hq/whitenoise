use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::*;

impl Whitenoise {
    /// Fetches a specific welcome invitation by its event ID.
    ///
    /// This method retrieves a welcome invitation event that was sent to the user.
    /// Welcome invitations are used to invite users to join group chats through the MLS protocol.
    /// The method:
    /// 1. Parses and validates the welcome event ID
    /// 2. Retrieves the user's account
    /// 3. Fetches the specific welcome invitation from the MLS protocol store
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the user who received the welcome invitation
    /// * `welcome_event_id` - The Nostr event ID of the welcome invitation (as a hex string)
    ///
    /// # Returns
    ///
    /// Returns `Ok(Welcome)` containing the welcome invitation details if found,
    /// or an error if:
    /// - The welcome event ID cannot be parsed as a valid Nostr event ID
    /// - The user account is not found
    /// - Nostr MLS is not initialized for the account
    /// - The welcome invitation with the specified event ID is not found
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use nostr::PublicKey;
    /// # use crate::whitenoise::Whitenoise;
    /// # async fn example(whitenoise: &Whitenoise, pubkey: &PublicKey) -> Result<(), Box<dyn std::error::Error>> {
    /// let welcome_event_id = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    /// let welcome = whitenoise.fetch_welcome(pubkey, welcome_event_id.to_string()).await?;
    /// println!("Found welcome invitation for group");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch_welcome(
        &self,
        pubkey: &PublicKey,
        welcome_event_id: String,
    ) -> Result<welcome_types::Welcome> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = self.fetch_account(pubkey).await?;

        let nostr_mls_guard = account.nostr_mls.lock().await;
        let nostr_mls = nostr_mls_guard
            .as_ref()
            .ok_or_else(|| WhitenoiseError::NostrMlsNotInitialized)?;
        let welcome = nostr_mls
            .get_welcome(&welcome_event_id)?
            .ok_or(WhitenoiseError::WelcomeNotFound)?;
        Ok(welcome)
    }

    /// Fetches all pending welcome invitations for a user.
    ///
    /// This method retrieves all pending welcome invitations that have been sent to the user
    /// but have not yet been accepted or declined. Welcome invitations are used to invite users
    /// to join group chats through the MLS protocol. The method:
    /// 1. Retrieves the user's account
    /// 2. Gets all pending welcome invitations from the MLS protocol store
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the user whose welcome invitations to fetch
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<Welcome>)` containing all pending welcome invitations for the user,
    /// or an error if:
    /// - The user account is not found
    /// - Nostr MLS is not initialized for the account
    /// - There is an error retrieving the welcome invitations from the MLS store
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use nostr::PublicKey;
    /// # use crate::whitenoise::Whitenoise;
    /// # async fn example(whitenoise: &Whitenoise, pubkey: &PublicKey) -> Result<(), Box<dyn std::error::Error>> {
    /// let welcomes = whitenoise.fetch_welcomes(pubkey).await?;
    /// println!("Found {} pending welcome invitations", welcomes.len());
    /// for welcome in welcomes {
    ///     println!("Welcome invitation received");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch_welcomes(&self, pubkey: &PublicKey) -> Result<Vec<welcome_types::Welcome>> {
        let account = self.fetch_account(pubkey).await?;

        let nostr_mls_guard = account.nostr_mls.lock().await;
        let nostr_mls = nostr_mls_guard
            .as_ref()
            .ok_or_else(|| WhitenoiseError::NostrMlsNotInitialized)?;
        let welcomes = nostr_mls.get_pending_welcomes()?;
        Ok(welcomes)
    }

    /// Accepts a welcome invitation to join a group.
    ///
    /// This method processes a welcome event to join a group chat. When accepting a welcome:
    /// 1. Parses and validates the welcome event ID
    /// 2. Retrieves the user's account and Nostr keys
    /// 3. Accepts the welcome invitation through the MLS protocol
    /// 4. Collects all group relays and sets up message subscriptions
    ///
    /// After successfully accepting the welcome, the user will be able to participate
    /// in the group chat and receive messages.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the user accepting the welcome
    /// * `welcome_event_id` - The Nostr event ID of the welcome invitation (as a hex string)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the welcome was successfully accepted and subscriptions were set up,
    /// or an error if:
    /// - The welcome event ID cannot be parsed
    /// - The user account is not found
    /// - Nostr MLS is not initialized for the account
    /// - The welcome event is not found
    /// - Setting up group message subscriptions fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use nostr::PublicKey;
    /// # use crate::whitenoise::Whitenoise;
    /// # async fn example(whitenoise: &Whitenoise, pubkey: &PublicKey) -> Result<(), Box<dyn std::error::Error>> {
    /// let welcome_event_id = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    /// whitenoise.accept_welcome(pubkey, welcome_event_id.to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn accept_welcome(&self, pubkey: &PublicKey, welcome_event_id: String) -> Result<()> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = self.fetch_account(pubkey).await?;
        let keys = self.secrets_store.get_nostr_keys_for_pubkey(pubkey)?;

        let group_ids: Vec<String>;
        let mut group_relays = Vec::new();
        let nostr_mls_guard = account.nostr_mls.lock().await;

        let nostr_mls = nostr_mls_guard
            .as_ref()
            .ok_or_else(|| WhitenoiseError::NostrMlsNotInitialized)?;
        let welcome = nostr_mls.get_welcome(&welcome_event_id)?;
        if let Some(welcome) = welcome {
            nostr_mls.accept_welcome(&welcome)?;

            let groups = nostr_mls.get_groups()?;
            group_ids = groups
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
        } else {
            return Err(WhitenoiseError::WelcomeNotFound);
        }

        self.nostr
            .setup_group_messages_subscriptions_with_signer(*pubkey, group_relays, group_ids, keys)
            .await
            .map_err(WhitenoiseError::from)?;

        Ok(())
    }

    /// Declines a welcome invitation to join a group.
    ///
    /// This method rejects a welcome event invitation to join a group chat. When declining:
    /// 1. Parses and validates the welcome event ID
    /// 2. Retrieves the user's account
    /// 3. Declines the welcome invitation through the MLS protocol
    ///
    /// After declining, the welcome invitation will be marked as rejected and cannot
    /// be accepted later. The user will not join the group chat.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the user declining the welcome
    /// * `welcome_event_id` - The Nostr event ID of the welcome invitation (as a hex string)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the welcome was successfully declined, or an error if:
    /// - The welcome event ID cannot be parsed
    /// - The user account is not found
    /// - Nostr MLS is not initialized for the account
    /// - The welcome event is not found
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use nostr::PublicKey;
    /// # use crate::whitenoise::Whitenoise;
    /// # async fn example(whitenoise: &Whitenoise, pubkey: &PublicKey) -> Result<(), Box<dyn std::error::Error>> {
    /// let welcome_event_id = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    /// whitenoise.decline_welcome(pubkey, welcome_event_id.to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn decline_welcome(
        &self,
        pubkey: &PublicKey,
        welcome_event_id: String,
    ) -> Result<()> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = self.fetch_account(pubkey).await?;

        let nostr_mls_guard = account.nostr_mls.lock().await;
        let nostr_mls = nostr_mls_guard
            .as_ref()
            .ok_or_else(|| WhitenoiseError::NostrMlsNotInitialized)?;
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
    // use std::{thread::sleep, time::Duration};

    use std::{thread::sleep, time::Duration};

    use super::*;
    use crate::whitenoise::test_utils::*;

    #[tokio::test]
    #[ignore]
    async fn test_receive_welcomes() {
        let whitenoise = test_get_whitenoise().await;
        let (creator_account, _creator_keys) = setup_login_account(whitenoise).await;

        // Setup member accounts
        let member_accounts = setup_multiple_test_accounts(whitenoise, &creator_account, 2).await;
        let member_pubkeys: Vec<PublicKey> =
            member_accounts.iter().map(|(acc, _)| acc.pubkey).collect();

        // Setup admin accounts (creator + one member as admin)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];
        let config = create_nostr_group_config_data();

        let group = whitenoise
            .create_group(
                &creator_account,
                member_pubkeys.clone(),
                admin_pubkeys.clone(),
                config,
            )
            .await;
        assert!(group.is_ok());

        let result1 = whitenoise
            .fetch_welcomes(&creator_account.pubkey)
            .await
            .unwrap();
        assert!(result1.is_empty()); // creator should not receive welcome messages

        let admin_key = &member_accounts[0].1;
        let regular_key = &member_accounts[1].1;

        let account = whitenoise
            .login(admin_key.secret_key().to_secret_hex())
            .await
            .unwrap();
        // Give some time for the event processor to process welcome messages
        sleep(Duration::from_secs(3));
        let result = whitenoise.fetch_welcomes(&account.pubkey).await.unwrap();
        assert!(!result.is_empty());

        let account = whitenoise
            .login(regular_key.secret_key().to_secret_hex())
            .await
            .unwrap();
        // Give some time for the event processor to process welcome messages
        // sleep(Duration::from_secs(10));
        let result = whitenoise.fetch_welcomes(&account.pubkey).await.unwrap();
        assert!(!result.is_empty());
    }
}
