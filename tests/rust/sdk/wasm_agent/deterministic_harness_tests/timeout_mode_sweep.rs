#[test]
#[serial_test::serial]
fn timeout_mode_is_deterministic_via_sweep() {
    let session_id = init(
        &serde_json::json!({
            "session_id": "timeout-replay-session",
            "max_steps": 10,
            "session_timeout_secs": 1,
            "max_in_memory_sessions": 8
        })
        .to_string(),
    )
    .unwrap();

    let prepared = prepare_user_turn(
        &serde_json::json!({
            "prompt": "hello",
            "session_id": session_id,
            "correlation_id": "trace-timeout"
        })
        .to_string(),
    )
    .unwrap();
    let _ = commit_llm_response(&prepared, "ok").unwrap();

    let archived =
        sweep_idle_sessions(Some(chrono::Utc::now().timestamp_millis() + 2_000)).unwrap();
    assert_eq!(archived, 1);

    let events: serde_json::Value =
        serde_json::from_str(&drain_events("timeout-replay-session").unwrap()).unwrap();
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "session_archived"),
        "session_archived event should be emitted after timeout sweep"
    );
}
