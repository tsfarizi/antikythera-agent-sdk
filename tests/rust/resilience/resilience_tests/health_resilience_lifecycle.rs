#[test]
fn health_tracker_snapshot_json_contains_all_tracked_components() {
    let mut tracker = HealthTracker::new();
    tracker.record_success("llm", 100);
    tracker.record_success("tools", 50);
    tracker.record_success("cache", 5);

    let json = tracker.snapshot_json();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert_eq!(arr.len(), 3);
    let ids: Vec<&str> = arr
        .iter()
        .map(|v| v["component_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"llm"));
    assert!(ids.contains(&"tools"));
    assert!(ids.contains(&"cache"));
}



#[test]
/// Reset and verify clean slate.
///
fn resilience_manager_full_lifecycle() {
    let mut mgr = ResilienceManager::new();

    assert_eq!(mgr.config().retry.max_attempts, 3);

    let new_config = ResilienceConfig {
        retry: RetryPolicy {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 5_000,
            backoff_factor: 2.0,
        },
        timeout: TimeoutPolicy {
            llm_timeout_ms: 20_000,
            tool_timeout_ms: 5_000,
        },
    };
    let config_json = serde_json::to_string(&new_config).unwrap();
    assert!(mgr.set_config_from_json(&config_json).unwrap());
    assert_eq!(mgr.config().retry.max_attempts, 5);

    mgr.health_mut().record_success("llm", 150);
    mgr.health_mut().record_failure("llm", "timeout");

    let health_json = mgr.get_health_json();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&health_json).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["component_id"].as_str().unwrap(), "llm");

    mgr.reset_health();
    let after_reset: Vec<serde_json::Value> = serde_json::from_str(&mgr.get_health_json()).unwrap();
    assert!(after_reset.is_empty());
}
