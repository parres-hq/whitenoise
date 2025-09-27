use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;
use nostr_sdk::prelude::*;
use std::time::Duration;

pub struct VerifyLastSyncedTimestampTestCase {
    mode: Mode,
}

enum Mode {
    AccountFollowEvent,
    GlobalMetadataEvent,
}

impl VerifyLastSyncedTimestampTestCase {
    pub fn for_account_follow_event() -> Self {
        Self {
            mode: Mode::AccountFollowEvent,
        }
    }

    pub fn for_global_metadata_event() -> Self {
        Self {
            mode: Mode::GlobalMetadataEvent,
        }
    }

    async fn baseline(
        &self,
        context: &ScenarioContext,
        pubkey: PublicKey,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, WhitenoiseError> {
        let account = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
        Ok(account.last_synced_at)
    }

    async fn assert_advanced(
        &self,
        context: &mut ScenarioContext,
        pubkey: PublicKey,
        before: Option<chrono::DateTime<chrono::Utc>>,
        description: &str,
    ) -> Result<(), WhitenoiseError> {
        retry(
            50,
            Duration::from_millis(50),
            || async {
                let account = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
                match (before, account.last_synced_at) {
                    (None, Some(_)) => Ok(()),
                    (Some(before_time), Some(after_time)) if after_time > before_time => Ok(()),
                    _ => Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "last_synced_at not advanced yet"
                    ))),
                }
            },
            description,
        )
        .await
    }

    async fn assert_unchanged(
        &self,
        context: &mut ScenarioContext,
        pubkey: PublicKey,
        before: Option<chrono::DateTime<chrono::Utc>>,
        description: &str,
    ) -> Result<(), WhitenoiseError> {
        retry(
            50,
            Duration::from_millis(50),
            || async {
                let account = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
                if account.last_synced_at == before {
                    Ok(())
                } else {
                    Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "last_synced_at advanced on global-only event"
                    )))
                }
            },
            description,
        )
        .await
    }

    async fn publish_account_follow_event_with_timestamp(
        &self,
        context: &ScenarioContext,
        pubkey: PublicKey,
        event_timestamp: Timestamp,
    ) -> Result<(), WhitenoiseError> {
        let account = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
        let nsec = context.whitenoise.export_account_nsec(&account).await?;
        let keys = Keys::parse(&nsec)?;
        let client = create_test_client(&context.dev_relays, keys.clone()).await?;
        let contact = Keys::generate().public_key();

        let tags = vec![Tag::custom(TagKind::p(), [contact.to_hex()])];
        let event = EventBuilder::new(Kind::ContactList, "")
            .tags(tags)
            .custom_created_at(event_timestamp)
            .sign_with_keys(&keys)
            .map_err(|e| WhitenoiseError::Other(e.into()))?;

        client.send_event(&event).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        client.disconnect().await;
        Ok(())
    }

    async fn publish_global_metadata_event_with_timestamp(
        &self,
        context: &ScenarioContext,
        event_timestamp: Timestamp,
    ) -> Result<(), WhitenoiseError> {
        let keys = Keys::generate();
        let client = create_test_client(&context.dev_relays, keys.clone()).await?;
        let metadata = Metadata {
            name: Some("Test metadata for sync verification".to_string()),
            ..Default::default()
        };

        let event = EventBuilder::metadata(&metadata)
            .custom_created_at(event_timestamp)
            .sign_with_keys(&keys)
            .map_err(|e| WhitenoiseError::Other(e.into()))?;

        client.send_event(&event).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
        client.disconnect().await;
        Ok(())
    }
}

#[async_trait]
impl TestCase for VerifyLastSyncedTimestampTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let pubkey = { context.get_account("subscription_test_account")?.pubkey };
        let before = self.baseline(context, pubkey).await?;

        // Create deterministic base timestamp for this test run
        let base_timestamp = Timestamp::now();

        match self.mode {
            Mode::AccountFollowEvent => {
                // Use base timestamp + 10 seconds for guaranteed advancement
                let event_timestamp = Timestamp::from_secs(base_timestamp.as_u64() + 10);
                self.publish_account_follow_event_with_timestamp(context, pubkey, event_timestamp)
                    .await?;
                self.assert_advanced(
                    context,
                    pubkey,
                    before,
                    "wait last_synced_at advance on account follow event",
                )
                .await?;
            }
            Mode::GlobalMetadataEvent => {
                // Use base timestamp + 5 seconds (should not affect account sync)
                let event_timestamp = Timestamp::from_secs(base_timestamp.as_u64() + 5);
                self.publish_global_metadata_event_with_timestamp(context, event_timestamp)
                    .await?;
                self.assert_unchanged(
                    context,
                    pubkey,
                    before,
                    "ensure last_synced_at unchanged on global metadata",
                )
                .await?;
            }
        }

        Ok(())
    }
}
