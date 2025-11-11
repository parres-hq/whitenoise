use crate::WhitenoiseError;
use crate::integration_tests::benchmarks::BenchmarkTestCase;
use crate::integration_tests::core::ScenarioContext;
use async_trait::async_trait;
use std::time::{Duration, Instant};

/// Benchmark test case for measuring send_message_to_group performance
pub struct SendMessageBenchmark {
    sender_account: String,
    target_group: String,
    message_prefix: String,
}

impl SendMessageBenchmark {
    pub fn new(sender_account: &str, target_group: &str) -> Self {
        Self {
            sender_account: sender_account.to_string(),
            target_group: target_group.to_string(),
            message_prefix: "Benchmark message".to_string(),
        }
    }

    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.message_prefix = prefix.to_string();
        self
    }
}

#[async_trait]
impl BenchmarkTestCase for SendMessageBenchmark {
    async fn run_iteration(
        &self,
        context: &mut ScenarioContext,
    ) -> Result<Duration, WhitenoiseError> {
        let sender = context.get_account(&self.sender_account)?;
        let group = context.get_group(&self.target_group)?;

        // Create unique message content for each iteration
        let message_content = format!(
            "{} - iteration {}",
            self.message_prefix, context.tests_count
        );

        // Time only the send_message_to_group operation
        let start = Instant::now();

        context
            .whitenoise
            .send_message_to_group(
                sender,
                &group.mls_group_id,
                message_content,
                9, // Kind: text message
                None,
            )
            .await?;

        let duration = start.elapsed();

        // Increment test count for next iteration
        context.tests_count += 1;

        Ok(duration)
    }
}
