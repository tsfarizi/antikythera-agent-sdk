
#[test]
fn p95_empty_returns_zero() {
    let values: Vec<u64> = vec![];
    assert_eq!(AgentRunnerRuntime::p95(&values), 0);
}

#[test]
fn p95_single_value() {
    assert_eq!(AgentRunnerRuntime::p95(&[42]), 42);
}

#[test]
fn p95_small_set() {
    let values: Vec<u64> = (1..=100).collect();
    let result = AgentRunnerRuntime::p95(&values);
    assert!(result >= 95, "p95 of 1..=100 should be >= 95, got {result}");
    assert!(result <= 96, "p95 of 1..=100 should be <= 96, got {result}");
}

#[test]
fn p95_known_example() {
    assert_eq!(
        AgentRunnerRuntime::p95(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
        9
    );
}

#[test]
fn p95_larger_set() {
    let values: Vec<u64> = (0..1000).collect();
    let result = AgentRunnerRuntime::p95(&values);
    assert_eq!(result, 949);
}

#[test]
fn new_session_id_has_prefix() {
    let id = new_session_id();
    assert!(
        id.starts_with("session-"),
        "expected 'session-' prefix, got: {id}"
    );
}

#[test]
fn new_session_id_has_two_digits() {
    let id = new_session_id();
    let rest = id.strip_prefix("session-").unwrap();
    let parts: Vec<&str> = rest.splitn(2, '-').collect();
    assert_eq!(
        parts.len(),
        2,
        "expected 'timestamp-seq' format, got: {rest}"
    );
    assert!(
        parts[0].parse::<i64>().is_ok(),
        "timestamp part should be numeric"
    );
    assert!(
        parts[1].parse::<u64>().is_ok(),
        "seq part should be numeric"
    );
}

#[test]
fn new_session_id_generates_unique() {
    let id1 = new_session_id();
    let id2 = new_session_id();
    assert_ne!(id1, id2);
}
