use mdk_core::prelude::MessageProcessingResult;
use nostr_sdk::prelude::*;

use crate::whitenoise::{
    Whitenoise,
    accounts::Account,
    error::{Result, WhitenoiseError},
};

impl Whitenoise {
    pub async fn handle_mls_message(&self, account: &Account, event: Event) -> Result<()> {
        tracing::debug!(
          target: "whitenoise::event_handlers::handle_mls_message",
          "Handling MLS message for account: {}",
          account.pubkey.to_hex()
        );

        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        match mdk.process_message(&event) {
            Ok(result) => {
                tracing::debug!(
                  target: "whitenoise::event_handlers::handle_mls_message",
                  "Handled MLS message - Result: {:?}",
                  result
                );

                // Extract and store media references synchronously (fast, ~1-5ms)
                if let Some((group_id, inner_event)) = Self::extract_message_details(&result) {
                    self.media_files()
                        .store_references_from_imeta_tags(&group_id, &account.pubkey, &inner_event)
                        .await?;
                }

                // Background sync for group images (existing pattern)
                if let MessageProcessingResult::Commit { mls_group_id } = result {
                    Whitenoise::background_sync_group_image_cache_if_needed(account, &mls_group_id);
                }
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::event_handlers::handle_mls_message",
                    "MLS message handling failed for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                Err(WhitenoiseError::MdkCoreError(e))
            }
        }
    }

    /// Extracts group_id and inner_event from MessageProcessingResult
    ///
    /// Returns Some if the result contains an application message with inner event content,
    /// None otherwise (e.g., for commits, proposals, or other non-message results).
    fn extract_message_details(
        result: &MessageProcessingResult,
    ) -> Option<(mdk_core::prelude::GroupId, UnsignedEvent)> {
        match result {
            MessageProcessingResult::ApplicationMessage(message) => {
                // The message.event is the decrypted rumor (UnsignedEvent) from the MLS message
                Some((message.mls_group_id.clone(), message.event.clone()))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_handle_mls_message_success() {
        // Arrange: Whitenoise and accounts
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let creator_account = whitenoise.create_identity().await.unwrap();
        // Create one member account, set contact, publish key package
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_pubkey = members[0].0.pubkey;

        // Give time for key package publish to propagate in test relays
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create the group via high-level API
        let _group = whitenoise
            .create_group(
                &creator_account,
                vec![member_pubkey],
                create_nostr_group_config_data(vec![creator_account.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Build a valid MLS group message event for the new group
        let mdk = Account::create_mdk(creator_account.pubkey, &whitenoise.config.data_dir).unwrap();
        let groups = mdk.get_groups().unwrap();
        let group_id = groups
            .first()
            .expect("group must exist")
            .mls_group_id
            .clone();

        let mut inner = UnsignedEvent::new(
            creator_account.pubkey,
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "hello from test".to_string(),
        );
        inner.ensure_id();
        let message_event = mdk.create_message(&group_id, inner).unwrap();

        // Act
        let result = whitenoise
            .handle_mls_message(&creator_account, message_event)
            .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mls_message_error_path() {
        // Arrange: Whitenoise and accounts
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let creator_account = whitenoise.create_identity().await.unwrap();
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_pubkey = members[0].0.pubkey;
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Create the group via high-level API
        let _group = whitenoise
            .create_group(
                &creator_account,
                vec![member_pubkey],
                create_nostr_group_config_data(vec![creator_account.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Create a valid MLS message event for that group
        let mdk = Account::create_mdk(creator_account.pubkey, &whitenoise.config.data_dir).unwrap();
        let groups = mdk.get_groups().unwrap();
        let group_id = groups
            .first()
            .expect("group must exist")
            .mls_group_id
            .clone();
        let mut inner = UnsignedEvent::new(
            creator_account.pubkey,
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "msg".to_string(),
        );
        inner.ensure_id();
        let valid_event = mdk.create_message(&group_id, inner).unwrap();

        // Corrupt it by changing the kind so MLS processing fails
        let mut bad_event = valid_event.clone();
        bad_event.kind = Kind::TextNote;

        // Act
        let result = whitenoise
            .handle_mls_message(&creator_account, bad_event)
            .await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            WhitenoiseError::MdkCoreError(_) => {}
            other => panic!("Expected MdkCoreError, got: {:?}", other),
        }
    }
}
