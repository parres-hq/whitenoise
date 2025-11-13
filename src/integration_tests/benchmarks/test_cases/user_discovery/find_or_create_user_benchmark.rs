use std::time::{Duration, Instant};

use async_trait::async_trait;
use nostr_sdk::PublicKey;

use crate::WhitenoiseError;
use crate::integration_tests::benchmarks::BenchmarkTestCase;
use crate::integration_tests::core::ScenarioContext;
use crate::whitenoise::users::UserSyncMode;

/// Benchmark test case for measuring find_or_create_user_by_pubkey performance
pub struct FindOrCreateUserBenchmark {
    sync_mode: UserSyncMode,
    pubkeys: Vec<PublicKey>,
}

impl FindOrCreateUserBenchmark {
    pub fn new(sync_mode: UserSyncMode, pubkeys: Vec<PublicKey>) -> Self {
        assert!(!pubkeys.is_empty(), "pubkeys cannot be empty");
        Self { sync_mode, pubkeys }
    }

    pub fn with_blocking_mode(pubkeys: Vec<PublicKey>) -> Self {
        Self::new(UserSyncMode::Blocking, pubkeys)
    }

    pub fn with_background_mode(pubkeys: Vec<PublicKey>) -> Self {
        Self::new(UserSyncMode::Background, pubkeys)
    }
}

#[async_trait]
impl BenchmarkTestCase for FindOrCreateUserBenchmark {
    async fn run_iteration(
        &self,
        context: &mut ScenarioContext,
    ) -> Result<Duration, WhitenoiseError> {
        // Get pubkey for this iteration (cycle through the list)
        let pubkey = &self.pubkeys[context.tests_count as usize % self.pubkeys.len()];

        // Time the find_or_create_user_by_pubkey call
        let start = Instant::now();
        context
            .whitenoise
            .find_or_create_user_by_pubkey(pubkey, self.sync_mode)
            .await?;
        let duration = start.elapsed();

        // Increment test count for next iteration
        context.tests_count += 1;

        Ok(duration)
    }
}
