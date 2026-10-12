

#[test]
#[serial_test::serial]
/// Streaming chunks -> commit_llm_stream -> events drained.
///
fn stream_chunks_and_drain_events() {
    let session_id = init(&config_json()).unwrap();
    let prepared = prepare_user_turn(
        &serde_json::json!({
            "prompt": "hello stream",
            "session_id": session_id,
            "system_prompt": "You are a helpful assistant",
            "correlation_id": "corr-stream"
        })
        .to_string(),
    )
    .unwrap();

    append_llm_chunk(&session_id, "{", Some("corr-stream")).unwrap();
    append_llm_chunk(&session_id, r#""response":"ok"}"#, Some("corr-stream")).unwrap();

    let result_json = commit_llm_stream(&prepared).unwrap();
    let value: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    assert_eq!(value["action"], "final");

    let events_json = drain_events(&session_id).unwrap();
    let events: serde_json::Value = serde_json::from_str(&events_json).unwrap();
    assert!(
        events.as_array().unwrap().len() >= 4,
        "expected at least 4 events (prepare, 2 chunks, commit)"
    );
}
