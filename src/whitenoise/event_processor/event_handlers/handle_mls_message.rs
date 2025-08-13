use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

impl Whitenoise {
    pub async fn handle_mls_message(&self, account: &Account, event: Event) -> Result<()> {
        tracing::debug!(
          target: "whitenoise::event_handlers::handle_mls_message",
          "Handling MLS message for account: {}",
          account.pubkey.to_hex()
        );

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
        match nostr_mls.process_message(&event) {
            Ok(result) => {
                tracing::debug!(
                  target: "whitenoise::event_handlers::handle_mls_message",
                  "Handled MLS message - Result: {:?}",
                  result
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::event_handlers::handle_mls_message",
                    "MLS message handling failed for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                Err(WhitenoiseError::NostrMlsError(e))
            }
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
                vec![creator_account.pubkey],
                create_nostr_group_config_data(),
            )
            .await
            .unwrap();

        // Build a valid MLS group message event for the new group
        let message_event = tokio::task::spawn_blocking({
            let account = creator_account.clone();
            move || -> core::result::Result<Event, nostr_mls::error::Error> {
                let nostr_mls =
                    Account::create_nostr_mls(account.pubkey, &whitenoise.config.data_dir).unwrap();
                let groups = nostr_mls.get_groups()?;
                let group_id = groups
                    .first()
                    .expect("group must exist")
                    .mls_group_id
                    .clone();

                let mut inner = UnsignedEvent::new(
                    account.pubkey,
                    Timestamp::now(),
                    Kind::TextNote,
                    vec![],
                    "hello from test".to_string(),
                );
                inner.ensure_id();
                let evt = nostr_mls.create_message(&group_id, inner)?;
                Ok(evt)
            }
        })
        .await
        .unwrap()
        .unwrap();

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
                vec![creator_account.pubkey],
                create_nostr_group_config_data(),
            )
            .await
            .unwrap();

        // Create a valid MLS message event for that group
        let valid_event = tokio::task::spawn_blocking({
            let account = creator_account.clone();
            move || -> core::result::Result<Event, nostr_mls::error::Error> {
                let nostr_mls =
                    Account::create_nostr_mls(account.pubkey, &whitenoise.config.data_dir).unwrap();
                let groups = nostr_mls.get_groups()?;
                let group_id = groups
                    .first()
                    .expect("group must exist")
                    .mls_group_id
                    .clone();
                let mut inner = UnsignedEvent::new(
                    account.pubkey,
                    Timestamp::now(),
                    Kind::TextNote,
                    vec![],
                    "msg".to_string(),
                );
                inner.ensure_id();
                let evt = nostr_mls.create_message(&group_id, inner)?;
                Ok(evt)
            }
        })
        .await
        .unwrap()
        .unwrap();

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
            WhitenoiseError::NostrMlsError(_) => {}
            other => panic!("Expected NostrMlsError, got: {:?}", other),
        }
    }
}
