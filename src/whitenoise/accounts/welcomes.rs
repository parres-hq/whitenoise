use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::*;

impl Whitenoise {
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
    /// # async fn example(whitenoise: &Whitenoise, pubkey: PublicKey) -> Result<(), Box<dyn std::error::Error>> {
    /// let welcome_event_id = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    /// whitenoise.accept_welcome(pubkey, welcome_event_id.to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn accept_welcome(&self, pubkey: PublicKey, welcome_event_id: String) -> Result<()> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = self.fetch_account(&pubkey).await?;
        let keys = self.secrets_store.get_nostr_keys_for_pubkey(&pubkey)?;

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
            .setup_group_messages_subscriptions_with_signer(pubkey, group_relays, group_ids, keys)
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
    /// # async fn example(whitenoise: &Whitenoise, pubkey: PublicKey) -> Result<(), Box<dyn std::error::Error>> {
    /// let welcome_event_id = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    /// whitenoise.decline_welcome(pubkey, welcome_event_id.to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn decline_welcome(&self, pubkey: PublicKey, welcome_event_id: String) -> Result<()> {
        let welcome_event_id = EventId::parse(&welcome_event_id).map_err(|_e| {
            WhitenoiseError::InvalidEvent("Couldn't parse welcome event ID".to_string())
        })?;
        let account = self.fetch_account(&pubkey).await?;

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
