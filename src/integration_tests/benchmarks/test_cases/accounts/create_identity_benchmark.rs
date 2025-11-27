use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::WhitenoiseError;
use crate::integration_tests::benchmarks::BenchmarkTestCase;
use crate::integration_tests::core::ScenarioContext;

/// Benchmark test case for measuring create_identity performance
///
/// This benchmark measures the time it takes to create a new identity, which includes:
/// - Generating a new keypair
/// - Setting up default relay lists (NIP-65, Inbox, Key Package)
/// - Creating and publishing metadata with a generated petname
/// - Publishing key packages to relays
/// - Setting up subscriptions
pub struct CreateIdentityBenchmark;

impl CreateIdentityBenchmark {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CreateIdentityBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BenchmarkTestCase for CreateIdentityBenchmark {
    async fn run_iteration(
        &self,
        context: &mut ScenarioContext,
    ) -> Result<Duration, WhitenoiseError> {
        // Time the create_identity operation
        let start = Instant::now();

        let account = context.whitenoise.create_identity().await?;

        let duration = start.elapsed();

        // Store the account in context with a unique name for potential cleanup
        let account_name = format!("benchmark_account_{}", context.tests_count);
        context.add_account(&account_name, account);

        // Increment test count for next iteration
        context.tests_count += 1;

        Ok(duration)
    }
}
