

#[test]
#[serial_test::serial]
/// Archived session triggers restore request and streaming progress events.
///
fn archived_session_prepare_requests_restore_and_supports_progress_stream() {
    register_tools("[]").unwrap();

    let cfg = serde_json::json!({
        "max_steps": 10,
        "session_timeout_secs": 3600,
        "max_in_memory_sessions": 1
    })
    .to_string();

    let archived_id = init(&cfg).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&archived_id, "hello")).unwrap();
    commit_llm_response(&prepared, "done").unwrap();
    let _other = init(&cfg).unwrap();

    let err = prepare_user_turn(&prepare_request(&archived_id, "come back")).unwrap_err();
    assert!(err.to_string().contains("archived"));

    let events: serde_json::Value =
        serde_json::from_str(&drain_events(&archived_id).unwrap()).unwrap();
    let kinds: Vec<String> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap_or_default().to_string())
        .collect();

    assert!(kinds.contains(&"session_restore_requested".to_string()));
}
