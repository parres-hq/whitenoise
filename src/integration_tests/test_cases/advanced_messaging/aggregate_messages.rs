use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;

pub struct AggregateMessagesTestCase {
    account_name: String,
    group_name: String,
    expected_min_messages: usize,
}

impl AggregateMessagesTestCase {
    pub fn new(account_name: &str, group_name: &str, expected_min_messages: usize) -> Self {
        Self {
            account_name: account_name.to_string(),
            group_name: group_name.to_string(),
            expected_min_messages,
        }
    }
}

#[async_trait]
impl TestCase for AggregateMessagesTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Aggregating messages for group {} using account: {}",
            self.group_name,
            self.account_name
        );

        let account = context.get_account(&self.account_name)?;
        let group = context.get_group(&self.group_name)?;

        // Wait for message processing with retry logic
        let mut retry_count = 0;
        const MAX_RETRIES: usize = 10;
        const RETRY_DELAY_MS: u64 = 500;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS)).await;

            let messages = context
                .whitenoise
                .fetch_aggregated_messages_for_group(&account.pubkey, &group.mls_group_id)
                .await?;

            if messages.len() >= self.expected_min_messages {
                tracing::info!(
                    "Found expected {} messages after {} retries",
                    messages.len(),
                    retry_count
                );
                break;
            }

            retry_count += 1;
            if retry_count >= MAX_RETRIES {
                tracing::warn!(
                    "Only found {} messages after {} retries, continuing with test",
                    messages.len(),
                    retry_count
                );
                break;
            }

            tracing::debug!(
                "Retry {}/{}: Found {} messages, waiting for {}",
                retry_count,
                MAX_RETRIES,
                messages.len(),
                self.expected_min_messages
            );
        }

        // Fetch aggregated messages
        let aggregated_messages = context
            .whitenoise
            .fetch_aggregated_messages_for_group(&account.pubkey, &group.mls_group_id)
            .await?;

        tracing::info!(
            "Fetched {} aggregated messages (expected at least {})",
            aggregated_messages.len(),
            self.expected_min_messages
        );

        // Verify we have at least the expected number of messages
        if aggregated_messages.len() < self.expected_min_messages {
            tracing::error!("Message aggregation failure details:");
            tracing::error!("  Expected at least: {}", self.expected_min_messages);
            tracing::error!("  Actually got: {}", aggregated_messages.len());
            tracing::error!("  Messages found:");
            for (i, msg) in aggregated_messages.iter().enumerate() {
                tracing::error!(
                    "    {}: {} from {} (deleted: {}, kind: {})",
                    i,
                    msg.content,
                    &msg.author.to_hex()[..8],
                    msg.is_deleted,
                    msg.kind
                );
            }

            panic!(
                "Expected at least {} messages, but got {} after retries. Check logs for details.",
                self.expected_min_messages,
                aggregated_messages.len()
            );
        }

        // Analyze message statistics
        let mut deleted_count = 0;
        let mut reply_count = 0;
        let mut messages_with_reactions = 0;
        let mut total_reactions = 0;

        for message in &aggregated_messages {
            tracing::debug!(
                "Message [{}]: '{}' from {} (deleted: {}, reply: {}, reactions: {})",
                message.id,
                message.content,
                &message.author.to_hex()[..8],
                message.is_deleted,
                message.is_reply,
                message.reactions.user_reactions.len()
            );

            if message.is_deleted {
                deleted_count += 1;
            }

            if message.is_reply {
                reply_count += 1;
            }

            if !message.reactions.user_reactions.is_empty() {
                messages_with_reactions += 1;
                total_reactions += message.reactions.user_reactions.len();

                tracing::debug!("  Reactions on this message:");
                for reaction in &message.reactions.user_reactions {
                    tracing::debug!(
                        "    {} from {} at {}",
                        reaction.emoji,
                        &reaction.user.to_hex()[..8],
                        reaction.created_at
                    );
                }
            }
        }

        tracing::info!("✓ Found {} deleted messages in aggregation", deleted_count);

        tracing::info!(
            "✓ Found {} messages with reactions ({} total reactions)",
            messages_with_reactions,
            total_reactions
        );

        tracing::info!("✓ Found {} reply messages in aggregation", reply_count);

        tracing::info!(
            "✓ Message aggregation completed: {} messages, {} deleted, {} replies, {} with reactions",
            aggregated_messages.len(),
            deleted_count,
            reply_count,
            messages_with_reactions
        );

        Ok(())
    }
}
