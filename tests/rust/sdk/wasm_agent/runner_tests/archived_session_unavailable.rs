

#[test]
#[serial_test::serial]
/// Archived session remains archived — no hydration support.
///
/// Session is archived, subsequent prepare should fail.
///
fn archived_session_remains_unavailable_after_archive() {
    register_tools("[]").unwrap();

    let cfg = serde_json::json!({
        "max_steps": 10,
        "session_timeout_secs": 3600,
        "max_in_memory_sessions": 1
    })
    .to_string();

    let archived_id = init(&cfg).unwrap();
    let first = prepare_user_turn(&prepare_request(&archived_id, "message-1")).unwrap();
    commit_llm_response(&first, "reply-1").unwrap();

    let _other = init(&cfg).unwrap();
    let events: serde_json::Value =
        serde_json::from_str(&drain_events(&archived_id).unwrap()).unwrap();
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "session_archived"),
        "session_archived event should be emitted"
    );

    let err = prepare_user_turn(&prepare_request(&archived_id, "message-2")).unwrap_err();
    assert!(err.to_string().contains("archived"));
}
