#[test]
#[serial_test::serial]
fn partial_output_stream_is_committed_deterministically() {
    let session_id = init(
        &serde_json::json!({
            "session_id": "partial-stream-session",
            "max_steps": 10
        })
        .to_string(),
    )
    .unwrap();

    let prepared = prepare_user_turn(
        &serde_json::json!({
            "prompt": "stream this",
            "session_id": session_id,
            "correlation_id": "trace-stream"
        })
        .to_string(),
    )
    .unwrap();

    append_llm_chunk(
        "partial-stream-session",
        "{\"response\":\"par",
        Some("trace-stream"),
    )
    .unwrap();
    append_llm_chunk("partial-stream-session", "tial\"}", Some("trace-stream")).unwrap();

    let committed = commit_llm_stream(&prepared).unwrap();
    let value: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(value["action"], "final");

    let events: serde_json::Value =
        serde_json::from_str(&drain_events("partial-stream-session").unwrap()).unwrap();
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "llm_chunk"),
        "llm_chunk events should be present for partial stream"
    );
}
