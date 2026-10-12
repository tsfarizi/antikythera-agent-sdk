#[test]
fn new_component_after_first_success_is_healthy() {
    let mut tracker = HealthTracker::new();
    tracker.record_success("llm", 100);
    assert_eq!(
        tracker.health_of("llm").unwrap().status,
        HealthStatus::Healthy
    );
}

#[test]
/// 1 failure / 1 total = 100% error rate → Unhealthy.
///
fn first_failure_alone_makes_component_unhealthy() {
    let mut tracker = HealthTracker::new();
    tracker.record_failure("llm", "connection refused");
    assert_eq!(
        tracker.health_of("llm").unwrap().status,
        HealthStatus::Unhealthy
    );
}

#[test]
/// 1 success + 1 failure = 50% error rate → Unhealthy (rate >= 0.5).
///
fn failure_after_success_results_in_unhealthy_at_fifty_percent() {
    let mut tracker = HealthTracker::new();
    tracker.record_success("llm", 100);
    tracker.record_failure("llm", "network timeout");
    assert_eq!(
        tracker.health_of("llm").unwrap().status,
        HealthStatus::Unhealthy
    );
}

#[test]
/// 1 failure / 6 total ≈ 16.7% → Degraded.
///
fn many_successes_after_one_failure_recovers_to_degraded() {
    let mut tracker = HealthTracker::new();
    tracker.record_failure("llm", "transient err");
    for _ in 0..5 {
        tracker.record_success("llm", 50);
    }
    assert_eq!(
        tracker.health_of("llm").unwrap().status,
        HealthStatus::Degraded
    );
}

#[test]
fn overall_status_is_worst_component_status() {
    let mut tracker = HealthTracker::new();
    tracker.record_success("a", 10);
    tracker.record_failure("b", "error");
    assert_eq!(tracker.overall_status(), HealthStatus::Unhealthy);
}

#[test]
fn overall_status_is_healthy_with_no_components() {
    let tracker = HealthTracker::new();
    assert_eq!(tracker.overall_status(), HealthStatus::Healthy);
}

#[test]
fn snapshot_json_is_a_valid_json_array() {
    let mut tracker = HealthTracker::new();
    tracker.record_success("llm", 100);
    tracker.record_failure("tools", "timeout");
    let json = tracker.snapshot_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 2);
}

#[test]
fn reset_clears_all_component_data() {
    let mut tracker = HealthTracker::new();
    tracker.record_success("llm", 100);
    tracker.reset();
    assert!(tracker.health_of("llm").is_none());
    assert_eq!(tracker.overall_status(), HealthStatus::Healthy);
}

#[test]
fn health_of_unknown_component_returns_none() {
    let tracker = HealthTracker::new();
    assert!(tracker.health_of("unknown").is_none());
}

#[test]
fn last_error_is_stored_on_failure() {
    let mut tracker = HealthTracker::new();
    tracker.record_failure("llm", "HTTP 503");
    let health = tracker.health_of("llm").unwrap();
    assert_eq!(health.last_error.as_deref(), Some("HTTP 503"));
}
