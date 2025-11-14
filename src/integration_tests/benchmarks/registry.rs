use crate::integration_tests::benchmarks::scenarios::{
    MessageAggregationBenchmark, MessagingPerformanceBenchmark, UserDiscoveryBenchmark,
};
use crate::integration_tests::benchmarks::{BenchmarkResult, BenchmarkScenario};
use crate::{Whitenoise, WhitenoiseError};
use std::time::{Duration, Instant};

pub struct BenchmarkRegistry;

impl BenchmarkRegistry {
    pub async fn run_all_benchmarks(
        whitenoise: &'static Whitenoise,
    ) -> Result<(), WhitenoiseError> {
        let overall_start = Instant::now();
        let mut results = Vec::new();
        let mut first_error = None;

        tracing::info!("=== Running Performance Benchmarks ===");

        macro_rules! run_benchmark {
            ($benchmark_expr:expr) => {
                match $benchmark_expr.run_benchmark(whitenoise).await {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        tracing::error!("Benchmark failed: {}", e);
                        if first_error.is_none() {
                            first_error = Some(e);
                        }
                    }
                }
            };
        }

        run_benchmark!(MessagingPerformanceBenchmark::default());
        run_benchmark!(MessageAggregationBenchmark::default());
        run_benchmark!(UserDiscoveryBenchmark::with_blocking_mode());
        run_benchmark!(UserDiscoveryBenchmark::with_background_mode());

        Self::print_summary(&results, overall_start.elapsed()).await;

        // Return the first error encountered, if any
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn print_summary(results: &[BenchmarkResult], overall_duration: Duration) {
        tokio::time::sleep(Duration::from_millis(500)).await; // Wait for logs to flush

        if results.is_empty() {
            return;
        }

        tracing::info!("=== Benchmark Results Summary ===");
        tracing::info!("");

        for result in results {
            tracing::info!("Benchmark: {}", result.name);
            tracing::info!("  Iterations:  {}", result.iterations);
            tracing::info!("  Total Time:  {:?}", result.total_duration);
            tracing::info!("");
            tracing::info!("  Statistics:");
            tracing::info!("    Mean:      {:?}", result.mean);
            tracing::info!("    Median:    {:?}", result.median);
            tracing::info!("    Std Dev:   {:?}", result.std_dev);
            tracing::info!("    Min:       {:?}", result.min);
            tracing::info!("    Max:       {:?}", result.max);
            tracing::info!("    P95:       {:?}", result.p95);
            tracing::info!("    P99:       {:?}", result.p99);
            tracing::info!("");
            tracing::info!("  Throughput:  {:.2} ops/sec", result.throughput);
            tracing::info!("");
            tracing::info!("---");
        }

        tracing::info!("");
        tracing::info!("Total Benchmarks: {}", results.len());
        tracing::info!("Overall Duration: {:?}", overall_duration);

        // Give async logging time to flush before program exits
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_registry_exists() {
        // Simple test to verify the registry struct exists
        let _registry = BenchmarkRegistry;
    }
}
