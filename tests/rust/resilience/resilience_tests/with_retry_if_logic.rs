use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

fn fast_policy(max: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts: max,
        initial_delay_ms: 1,
        max_delay_ms: 5,
        backoff_factor: 1.0,
    }
}

#[tokio::test]
async fn succeeds_on_first_attempt_without_retry() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let result: Result<&str, String> = with_retry(&RetryPolicy::no_retry(), || {
        let c = Arc::clone(&c);
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok("ok")
        }
    })
    .await;
    assert_eq!(result.unwrap(), "ok");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retries_up_to_max_attempts_on_every_failure() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let result: Result<&str, String> = with_retry(&fast_policy(3), || {
        let c = Arc::clone(&c);
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Err("network error".to_string())
        }
    })
    .await;
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
/// Only one attempt — predicate prevented any retries.
///
async fn stops_immediately_when_predicate_returns_false() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let result: Result<&str, String> = with_retry_if(
        &fast_policy(5),
        || {
            let c = Arc::clone(&c);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err("permanent error".to_string())
            }
        },
        |_| false,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
/// 2 failures + 1 success = 3 total calls.
///
async fn succeeds_after_two_transient_failures() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let result: Result<u32, String> = with_retry(&fast_policy(5), || {
        let c = Arc::clone(&c);
        async move {
            let n = c.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err("transient".to_string())
            } else {
                Ok(n)
            }
        }
    })
    .await;
    assert!(result.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
/// Only retry on "retry-me"; bail on "permanent".
///
async fn retry_predicate_is_checked_per_error() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let result: Result<&str, &str> = with_retry_if(
        &fast_policy(10),
        || {
            let c = Arc::clone(&c);
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                match n {
                    0 => Err("retry-me"),
                    _ => Err("permanent"),
                }
            }
        },
        |e| *e == "retry-me",
    )
    .await;
    assert_eq!(result.unwrap_err(), "permanent");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
