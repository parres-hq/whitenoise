use std::time::Duration;

/// Calculate the mean (average) duration from a collection of durations
pub fn calculate_mean(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }

    let total: Duration = durations.iter().sum();
    total / durations.len() as u32
}

/// Calculate the median duration from a collection of durations
///
/// Note: This function will sort the input slice
pub fn calculate_median(durations: &mut [Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }

    durations.sort();

    let len = durations.len();
    if len.is_multiple_of(2) {
        // Even number of elements - average the two middle values
        let mid1 = durations[len / 2 - 1];
        let mid2 = durations[len / 2];
        (mid1 + mid2) / 2
    } else {
        // Odd number of elements - return the middle value
        durations[len / 2]
    }
}

/// Calculate the standard deviation of durations
pub fn calculate_std_dev(durations: &[Duration], mean: Duration) -> Duration {
    if durations.len() <= 1 {
        return Duration::ZERO;
    }

    let mean_nanos = mean.as_nanos() as f64;

    let variance: f64 = durations
        .iter()
        .map(|d| {
            let diff = d.as_nanos() as f64 - mean_nanos;
            diff * diff
        })
        .sum::<f64>()
        / (durations.len() - 1) as f64; // Sample standard deviation (n-1)

    let std_dev_nanos = variance.sqrt();
    Duration::from_nanos(std_dev_nanos as u64)
}

/// Calculate a specific percentile from a collection of durations
///
/// Note: This function will sort the input slice
///
/// # Arguments
/// * `durations` - Slice of durations to calculate percentile from
/// * `percentile` - Percentile to calculate (0.0 to 1.0, e.g., 0.95 for 95th percentile)
pub fn calculate_percentile(durations: &mut [Duration], percentile: f64) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }

    if percentile <= 0.0 {
        return Duration::ZERO;
    }

    if percentile >= 1.0 {
        durations.sort();
        return *durations.last().unwrap();
    }

    durations.sort();

    let index = (percentile * (durations.len() - 1) as f64).ceil() as usize;
    durations[index.min(durations.len() - 1)]
}

/// Calculate throughput (operations per second) given iterations and total duration
pub fn calculate_throughput(iterations: u32, total_duration: Duration) -> f64 {
    if total_duration.is_zero() {
        return 0.0;
    }

    let total_secs = total_duration.as_secs_f64();
    iterations as f64 / total_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_mean() {
        let durations = vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(300),
        ];
        assert_eq!(calculate_mean(&durations), Duration::from_millis(200));
    }

    #[test]
    fn test_calculate_mean_empty() {
        let durations: Vec<Duration> = vec![];
        assert_eq!(calculate_mean(&durations), Duration::ZERO);
    }

    #[test]
    fn test_calculate_median_odd() {
        let mut durations = vec![
            Duration::from_millis(100),
            Duration::from_millis(300),
            Duration::from_millis(200),
        ];
        assert_eq!(calculate_median(&mut durations), Duration::from_millis(200));
    }

    #[test]
    fn test_calculate_median_even() {
        let mut durations = vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(300),
            Duration::from_millis(400),
        ];
        assert_eq!(calculate_median(&mut durations), Duration::from_millis(250));
    }

    #[test]
    fn test_calculate_median_empty() {
        let mut durations: Vec<Duration> = vec![];
        assert_eq!(calculate_median(&mut durations), Duration::ZERO);
    }

    #[test]
    fn test_calculate_std_dev() {
        let durations = vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(300),
        ];
        let mean = calculate_mean(&durations);
        let std_dev = calculate_std_dev(&durations, mean);

        // Expected std dev ≈ 100ms (sample std dev with n-1)
        assert!(std_dev >= Duration::from_millis(95));
        assert!(std_dev <= Duration::from_millis(105));
    }

    #[test]
    fn test_calculate_std_dev_empty() {
        let durations: Vec<Duration> = vec![];
        let mean = Duration::ZERO;
        assert_eq!(calculate_std_dev(&durations, mean), Duration::ZERO);
    }

    #[test]
    fn test_calculate_percentile_95() {
        let mut durations: Vec<Duration> =
            (1..=100).map(|i| Duration::from_millis(i * 10)).collect();

        let p95 = calculate_percentile(&mut durations, 0.95);

        // 95th percentile of 1..=100 should be around 950ms
        assert!(p95 >= Duration::from_millis(940));
        assert!(p95 <= Duration::from_millis(960));
    }

    #[test]
    fn test_calculate_percentile_empty() {
        let mut durations: Vec<Duration> = vec![];
        assert_eq!(calculate_percentile(&mut durations, 0.95), Duration::ZERO);
    }

    #[test]
    fn test_calculate_percentile_edge_cases() {
        let mut durations = vec![Duration::from_millis(100)];

        assert_eq!(calculate_percentile(&mut durations, 0.0), Duration::ZERO);
        assert_eq!(
            calculate_percentile(&mut durations, 1.0),
            Duration::from_millis(100)
        );
        assert_eq!(
            calculate_percentile(&mut durations, 0.5),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn test_calculate_throughput() {
        let iterations = 100;
        let duration = Duration::from_secs(10);
        let throughput = calculate_throughput(iterations, duration);

        assert_eq!(throughput, 10.0);
    }

    #[test]
    fn test_calculate_throughput_zero_duration() {
        let iterations = 100;
        let duration = Duration::ZERO;
        let throughput = calculate_throughput(iterations, duration);

        assert_eq!(throughput, 0.0);
    }

    #[test]
    fn test_calculate_throughput_fractional() {
        let iterations = 100;
        let duration = Duration::from_millis(2500); // 2.5 seconds
        let throughput = calculate_throughput(iterations, duration);

        assert_eq!(throughput, 40.0);
    }
}
