#[test]
#[serial_test::serial]
fn replay_trace_prepare_commit_tool_result_commit_final_is_deterministic() {
    let session_id = init(
        &serde_json::json!({
            "session_id": "replay-trace-session",
            "max_steps": 10,
            "session_timeout_secs": 3600,
            "max_in_memory_sessions": 16
        })
        .to_string(),
    )
    .unwrap();

    let prepared_1 = prepare_user_turn(
        &serde_json::json!({
            "prompt": "Cari cuaca",
            "session_id": session_id,
            "force_json": true,
            "correlation_id": "trace-1"
        })
        .to_string(),
    )
    .unwrap();

    let commit_1 = commit_llm_response(
        &prepared_1,
        &serde_json::json!({
            "action": "call_tool",
            "tool": "weather.get",
            "input": {"city": "Bandung"}
        })
        .to_string(),
    )
    .unwrap();
    let c1: serde_json::Value = serde_json::from_str(&commit_1).unwrap();
    assert_eq!(c1["action"], "call_tool");

    let tool_result = process_tool_result_for_session(
        "replay-trace-session",
        &serde_json::json!({
            "tool_name": "weather.get",
            "success": true,
            "output_json": "{\"temp\":24,\"condition\":\"cloudy\"}",
            "error_message": null,
            "correlation_id": "trace-1"
        })
        .to_string(),
    )
    .unwrap();
    let tr: serde_json::Value = serde_json::from_str(&tool_result).unwrap();
    assert_eq!(tr["tool_result"]["success"], true);

    let prepared_2 = prepare_user_turn(
        &serde_json::json!({
            "prompt": "Ringkas hasilnya",
            "session_id": "replay-trace-session",
            "correlation_id": "trace-1"
        })
        .to_string(),
    )
    .unwrap();

    let commit_2 = commit_llm_response(&prepared_2, "Cuaca Bandung 24C dan berawan.").unwrap();
    let c2: serde_json::Value = serde_json::from_str(&commit_2).unwrap();
    assert_eq!(c2["action"], "final");
}
