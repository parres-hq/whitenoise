use std::time::Duration;

/// Configuration for benchmark execution
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub iterations: u32,
    pub warmup_iterations: u32,
    pub cooldown_between_iterations: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            iterations: 100,
            warmup_iterations: 10,
            cooldown_between_iterations: Duration::from_millis(50),
        }
    }
}
