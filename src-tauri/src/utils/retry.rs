use std::future::Future;
use tokio::time::{sleep, Duration};

#[derive(Debug)]
pub enum GeneralRetryError<E: std::fmt::Display> {
    MaxRetriesExceeded {
        last_error: E,
        operation_description: String,
        attempts_made: u32,
    },
}

impl<E: std::fmt::Display> std::fmt::Display for GeneralRetryError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneralRetryError::MaxRetriesExceeded {
                last_error,
                operation_description,
                attempts_made,
            } => write!(
                f,
                "Operation '{}' failed after {} attempts. Last error: {}",
                operation_description, attempts_made, last_error
            ),
        }
    }
}

pub async fn execute_with_retry<F, Fut, T, E>(
    operation_description: String,
    max_attempts: u32,
    initial_delay: Duration,
    backoff_factor: u32,
    mut attempt_fn: F,
    mut progress_fn: impl FnMut(u32, u32, Duration, &E),
) -> Result<T, GeneralRetryError<E>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display + Clone,
{
    if max_attempts == 0 {
        panic!("max_attempts must be at least 1 for execute_with_retry");
    }
    let mut last_error: Option<E> = None;
    let mut current_delay_for_next_sleep = initial_delay;
    for attempt_num in 1..=max_attempts {
        match attempt_fn().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e.clone());
                if attempt_num == max_attempts {
                    break;
                }
                progress_fn(attempt_num, max_attempts, current_delay_for_next_sleep, &e);
                sleep(current_delay_for_next_sleep).await;
                if backoff_factor > 0 {
                    current_delay_for_next_sleep *= backoff_factor;
                }
            }
        }
    }
    Err(GeneralRetryError::MaxRetriesExceeded {
        last_error: last_error.expect("last_error should be Some if all attempts failed"),
        operation_description,
        attempts_made: max_attempts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_retry_success_first_try() {
        let attempts_queue = Arc::new(Mutex::new(vec![Ok(123)]));
        let progress_calls = Arc::new(Mutex::new(Vec::new()));

        let attempt_fn = move || {
            let res = attempts_queue
                .lock()
                .unwrap()
                .pop()
                .expect("Mock attempt_fn queue exhausted");
            async move { res }
        };
        let progress_calls_clone = Arc::clone(&progress_calls);
        let progress_fn = move |attempt, max, delay, err_msg: &String| {
            progress_calls_clone
                .lock()
                .unwrap()
                .push((attempt, max, delay, err_msg.clone()));
        };

        let result = execute_with_retry(
            "test_op".to_string(),
            3,
            Duration::from_millis(1),
            2,
            attempt_fn,
            progress_fn,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 123);
        assert!(progress_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_retry_success_after_retries() {
        let attempts_queue = Arc::new(Mutex::new(vec![
            Ok(123),
            Err::<u32, String>("fail 2".to_string()),
            Err::<u32, String>("fail 1".to_string()),
        ]));
        let progress_calls = Arc::new(Mutex::new(Vec::new()));

        let attempt_fn = move || {
            let res = attempts_queue
                .lock()
                .unwrap()
                .pop()
                .expect("Mock attempt_fn queue exhausted");
            async move { res }
        };
        let progress_calls_clone = Arc::clone(&progress_calls);
        let progress_fn = move |attempt, max, delay, err_msg: &String| {
            progress_calls_clone
                .lock()
                .unwrap()
                .push((attempt, max, delay, err_msg.clone()));
        };

        let result = execute_with_retry(
            "test_op_retry_success".to_string(),
            3,
            Duration::from_millis(10),
            2,
            attempt_fn,
            progress_fn,
        )
        .await;

        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), 123);

        let calls = progress_calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            (1, 3, Duration::from_millis(10), "fail 1".to_string())
        );
        assert_eq!(
            calls[1],
            (2, 3, Duration::from_millis(20), "fail 2".to_string())
        );
    }

    #[tokio::test]
    async fn test_retry_max_retries_exceeded() {
        let attempts_queue = Arc::new(Mutex::new(vec![
            Err::<u32, String>("fail 3".to_string()),
            Err::<u32, String>("fail 2".to_string()),
            Err::<u32, String>("fail 1".to_string()),
        ]));
        let progress_calls = Arc::new(Mutex::new(Vec::new()));
        let op_desc = "test_op_max_fail".to_string();

        let attempt_fn = move || {
            let res = attempts_queue
                .lock()
                .unwrap()
                .pop()
                .expect("Mock attempt_fn queue exhausted");
            async move { res }
        };
        let progress_calls_clone = Arc::clone(&progress_calls);
        let progress_fn = move |attempt, max, delay, err_msg: &String| {
            progress_calls_clone
                .lock()
                .unwrap()
                .push((attempt, max, delay, err_msg.clone()));
        };

        let result = execute_with_retry(
            op_desc.clone(),
            3,
            Duration::from_millis(5),
            2,
            attempt_fn,
            progress_fn,
        )
        .await;

        assert!(result.is_err());
        match result.err().unwrap() {
            GeneralRetryError::MaxRetriesExceeded {
                last_error,
                operation_description,
                attempts_made,
            } => {
                assert_eq!(last_error, "fail 3");
                assert_eq!(operation_description, op_desc);
                assert_eq!(attempts_made, 3);
            }
        }

        let calls = progress_calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            (1, 3, Duration::from_millis(5), "fail 1".to_string())
        );
        assert_eq!(
            calls[1],
            (2, 3, Duration::from_millis(10), "fail 2".to_string())
        );
    }

    #[tokio::test]
    async fn test_retry_no_retries_on_max_attempts_1_success() {
        let attempts_queue = Arc::new(Mutex::new(vec![Ok(1)]));
        let progress_calls = Arc::new(Mutex::new(Vec::new()));

        let attempt_fn = move || {
            let res = attempts_queue.lock().unwrap().pop().unwrap();
            async move { res }
        };
        let progress_calls_clone = Arc::clone(&progress_calls);
        let progress_fn = move |a, b, c, d: &String| {
            progress_calls_clone
                .lock()
                .unwrap()
                .push((a, b, c, d.clone()));
        };

        let result = execute_with_retry(
            "test_single_attempt_success".to_string(),
            1,
            Duration::from_millis(1),
            1,
            attempt_fn,
            progress_fn,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
        assert!(progress_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_retry_no_retries_on_max_attempts_1_failure() {
        let attempts_queue = Arc::new(Mutex::new(vec![Err::<u32, String>("fail".to_string())]));
        let progress_calls = Arc::new(Mutex::new(Vec::new()));
        let op_name = "test_single_attempt_fail".to_string();

        let attempt_fn = move || {
            let res = attempts_queue.lock().unwrap().pop().unwrap();
            async move { res }
        };
        let progress_calls_clone = Arc::clone(&progress_calls);
        let progress_fn = move |a, b, c, d: &String| {
            progress_calls_clone
                .lock()
                .unwrap()
                .push((a, b, c, d.clone()));
        };

        let result = execute_with_retry(
            op_name.clone(),
            1,
            Duration::from_millis(1),
            1,
            attempt_fn,
            progress_fn,
        )
        .await;

        assert!(result.is_err());
        match result.err().unwrap() {
            GeneralRetryError::MaxRetriesExceeded {
                last_error,
                operation_description,
                attempts_made,
            } => {
                assert_eq!(last_error, "fail");
                assert_eq!(operation_description, op_name);
                assert_eq!(attempts_made, 1);
            }
        }
        assert!(progress_calls.lock().unwrap().is_empty());
    }
}
