use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

impl Whitenoise {
    pub async fn handle_giftwrap(&self, account: &Account, event: Event) -> Result<()> {
        tracing::info!(
            target: "whitenoise::event_handlers::handle_giftwrap",
            "Giftwrap received for account: {} - processing not yet implemented",
            account.pubkey.to_hex()
        );

        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        let unwrapped = extract_rumor(&keys, &event).await.map_err(|e| {
            WhitenoiseError::Configuration(format!("Failed to decrypt giftwrap: {}", e))
        })?;

        match unwrapped.rumor.kind {
            Kind::MlsWelcome => {
                self.process_welcome(account, event, unwrapped.rumor)
                    .await?;
            }
            _ => {
                tracing::debug!(
                    target: "whitenoise::event_handlers::handle_giftwrap",
                    "Received unhandled giftwrap of kind {:?}",
                    unwrapped.rumor.kind
                );
            }
        }

        Ok(())
    }

    async fn process_welcome(
        &self,
        account: &Account,
        event: Event,
        rumor: UnsignedEvent,
    ) -> Result<()> {
        // Process the welcome message - lock scope is minimal
        {
            let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
            nostr_mls
                .process_welcome(&event.id, &rumor)
                .map_err(WhitenoiseError::NostrMlsError)?;
            tracing::debug!(target: "whitenoise::event_processor::process_welcome", "Processed welcome event");
        } // nostr_mls lock released here

        let key_package_event_id: Option<EventId> = rumor
            .tags
            .iter()
            .find(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .and_then(|tag| tag.content())
            .and_then(|content| EventId::parse(content).ok());

        if let Some(key_package_event_id) = key_package_event_id {
            self.delete_key_package_from_relays_for_account(
                account,
                &key_package_event_id,
                false, // For now we don't want to delete the key packages from MLS storage
            )
            .await?;
            tracing::debug!(target: "whitenoise::event_processor::process_welcome", "Deleted used key package from relays");

            self.publish_key_package_for_account(account).await?;
            tracing::debug!(target: "whitenoise::event_processor::process_welcome", "Published new key package");
        } else {
            tracing::warn!(target: "whitenoise::event_processor::process_welcome", "No key package event id found in welcome event");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;

    // Builds a real MLS Welcome rumor for `member_pubkey` by creating a group with `creator_account`
    async fn build_welcome_giftwrap(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkey: PublicKey,
    ) -> Event {
        // Fetch a real key package event for the member from relays
        let key_pkg_event = whitenoise
            .nostr
            .fetch_user_key_package(
                member_pubkey,
                creator_account
                    .key_package_relays(&whitenoise)
                    .await
                    .unwrap(),
            )
            .await
            .unwrap()
            .expect("member must have a published key package");

        // Create the group via nostr_mls directly to obtain welcome rumor
        let (welcome_rumor, _unused_keys) = tokio::task::spawn_blocking({
            let creator_account = creator_account.clone();
            let key_pkg_event = key_pkg_event.clone();
            let nostr_mls = Account::create_nostr_mls(creator_account.pubkey, &whitenoise.config.data_dir).unwrap();
            move || -> core::result::Result<(UnsignedEvent, Keys), nostr_mls::error::Error> {

                let create_group_result = nostr_mls.create_group(
                    &creator_account.pubkey,
                    vec![key_pkg_event],
                    vec![creator_account.pubkey],
                    create_nostr_group_config_data(),
                )?;

                let rumor = create_group_result
                    .welcome_rumors
                    .first()
                    .expect("welcome rumor exists")
                    .clone();

                // Return rumor plus a dummy Keys placeholder; will not be used outside
                Ok((rumor, Keys::generate()))
            }
        })
        .await
        .unwrap()
        .unwrap();

        // Use the creator's real keys as signer to build the giftwrap
        let creator_signer = whitenoise
            .secrets_store
            .get_nostr_keys_for_pubkey(&creator_account.pubkey)
            .unwrap();

        EventBuilder::gift_wrap(&creator_signer, &member_pubkey, welcome_rumor, vec![])
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_handle_giftwrap_welcome_success() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create creator and one member account; setup publishes key packages and contacts
        let creator_account = whitenoise.create_identity().await.unwrap();
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_account = members[0].0.clone();

        // Build a real MLS Welcome giftwrap addressed to the member
        let giftwrap_event =
            build_welcome_giftwrap(&whitenoise, &creator_account, member_account.pubkey).await;

        // Member should successfully process welcome
        let result = whitenoise
            .handle_giftwrap(&member_account, giftwrap_event)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_giftwrap_non_welcome_ok() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        // Build a non-welcome rumor and giftwrap it to the account
        let mut rumor = UnsignedEvent::new(
            account.pubkey,
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "not a welcome".to_string(),
        );
        rumor.ensure_id();

        // Any signer works; encryption targets receiver's pubkey
        let sender_keys = create_test_keys();
        let giftwrap_event = EventBuilder::gift_wrap(&sender_keys, &account.pubkey, rumor, vec![])
            .await
            .unwrap();

        let result = whitenoise.handle_giftwrap(&account, giftwrap_event).await;
        assert!(result.is_ok());
    }
}
