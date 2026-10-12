#[test]
#[serial_test::serial]
fn malformed_tool_result_json_is_rejected() {
    let session_id = init(
        &serde_json::json!({
            "session_id": "malformed-tool-session",
            "max_steps": 10
        })
        .to_string(),
    )
    .unwrap();

    let _prepared = prepare_user_turn(
        &serde_json::json!({
            "prompt": "hi",
            "session_id": session_id
        })
        .to_string(),
    )
    .unwrap();

    let err = process_tool_result_for_session(
        "malformed-tool-session",
        &serde_json::json!({
            "tool_name": "bad.tool",
            "success": true,
            "output_json": "{invalid-json",
            "error_message": null,
            "correlation_id": "trace-malformed"
        })
        .to_string(),
    )
    .unwrap_err();

    assert!(err.to_string().contains("Invalid tool output_json"));
}
