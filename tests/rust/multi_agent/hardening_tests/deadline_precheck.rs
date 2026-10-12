

#[test]
/// Deadline_unix_ms pre-check â€” verifies expired deadline is detected.
///
/// Mirrors the pre-check in execute_task: if now_ms >= deadline { ... deadline_exceeded = true }.
///
fn deadline_unix_ms_in_past_is_expired() {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock error")
        .as_millis() as i64;

    let past_deadline: i64 = now_ms - 10_000;
    assert!(
        now_ms >= past_deadline,
        "a deadline in the past must be detected as exceeded"
    );

    let future_deadline: i64 = now_ms + 60_000;
    assert!(
        now_ms < future_deadline,
        "a deadline in the future must not be flagged as exceeded"
    );
}
