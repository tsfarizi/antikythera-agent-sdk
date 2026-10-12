

#[test]
#[serial_test::serial]
/// Idle sweep archives timed-out sessions deterministically.
///
fn sweep_idle_sessions_archives_timed_out_session() {
    register_tools("[]").unwrap();

    let cfg = serde_json::json!({
        "max_steps": 10,
        "session_timeout_secs": 1,
        "max_in_memory_sessions": 32
    })
    .to_string();

    let session_id = init(&cfg).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&session_id, "hello")).unwrap();
    commit_llm_response(&prepared, "world").unwrap();

    let swept = sweep_idle_sessions(Some(chrono::Utc::now().timestamp_millis() + 2_000)).unwrap();
    assert_eq!(
        swept, 1,
        "exactly one session should be archived by idle sweep"
    );

    let state_result = get_state(&session_id);
    assert!(state_result.is_err());

    let events: serde_json::Value =
        serde_json::from_str(&drain_events(&session_id).unwrap()).unwrap();
    let archived = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "session_archived")
        .expect("missing session_archived event");
    assert_eq!(archived["payload"]["reason"], "idle_timeout");
}
