

#[test]
#[serial_test::serial]
/// Capacity pressure auto-archives the oldest inactive session.
///
fn capacity_pressure_archives_oldest_session_and_emits_state_payload() {
    register_tools("[]").unwrap();

    let cfg = serde_json::json!({
        "max_steps": 10,
        "session_timeout_secs": 3600,
        "max_in_memory_sessions": 1
    })
    .to_string();

    let oldest = init(&cfg).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&oldest, "persist me")).unwrap();
    commit_llm_response(&prepared, "ok").unwrap();

    let _newest = init(&cfg).unwrap();

    let state_result = get_state(&oldest);
    assert!(state_result.is_err(), "oldest session should be archived");
    assert!(
        state_result.unwrap_err().to_string().contains("archived"),
        "expected archived-state error"
    );

    let events: serde_json::Value = serde_json::from_str(&drain_events(&oldest).unwrap()).unwrap();
    let archived = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "session_archived")
        .expect("missing session_archived event");

    assert_eq!(archived["payload"]["reason"], "capacity_pressure");
    assert!(
        archived["payload"]["state_json"].as_str().is_some(),
        "archived event should contain state snapshot JSON for host persistence"
    );
}
