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
        let fresh = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
        Ok(fresh.last_synced_at)
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
                let refreshed = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
                match (before, refreshed.last_synced_at) {
                    (None, Some(_)) => Ok(()),
                    (Some(b), Some(a)) if a > b => Ok(()),
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
                let after = context
                    .whitenoise
                    .find_account_by_pubkey(&pubkey)
                    .await?
                    .last_synced_at;
                if after == before {
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

    async fn publish_account_follow_event(
        &self,
        context: &ScenarioContext,
        pubkey: PublicKey,
    ) -> Result<(), WhitenoiseError> {
        let account_owned = context.whitenoise.find_account_by_pubkey(&pubkey).await?;
        let nsec = context
            .whitenoise
            .export_account_nsec(&account_owned)
            .await?;
        let keys = Keys::parse(&nsec)?;
        let client = create_test_client(&context.dev_relays, keys).await?;
        let contact = Keys::generate().public_key();
        publish_follow_list(&client, &[contact]).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        client.disconnect().await;
        Ok(())
    }

    async fn publish_global_metadata_event(
        &self,
        context: &ScenarioContext,
    ) -> Result<(), WhitenoiseError> {
        let ext = Keys::generate();
        let client = create_test_client(&context.dev_relays, ext).await?;
        let metadata = Metadata {
            name: Some("No-op for account sync".to_string()),
            ..Default::default()
        };
        client
            .send_event_builder(EventBuilder::metadata(&metadata))
            .await?;
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
        match self.mode {
            Mode::AccountFollowEvent => {
                self.publish_account_follow_event(context, pubkey).await?;
                self.assert_advanced(
                    context,
                    pubkey,
                    before,
                    "wait last_synced_at advance on account follow event",
                )
                .await?;
            }
            Mode::GlobalMetadataEvent => {
                self.publish_global_metadata_event(context).await?;
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
