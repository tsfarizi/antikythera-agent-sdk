

#[test]
#[serial_test::serial]
/// Structured tool-call response -> CallTool action.
///
fn structured_tool_call_commit_returns_call_tool() {
    register_tools(
        &serde_json::json!([
            {
                "name": "weather.get",
                "description": "Get weather",
                "parameters": [
                    {"name": "city", "param_type": "string", "description": "City", "required": true}
                ]
            }
        ])
        .to_string(),
    )
    .unwrap();

    let session_id = init(&config_json()).unwrap();
    let prepared = prepare_user_turn(
        &serde_json::json!({
            "prompt": "check weather",
            "session_id": session_id,
            "system_prompt": "Use tools if needed",
            "force_json": true
        })
        .to_string(),
    )
    .unwrap();

    let response = serde_json::json!({
        "action": "call_tool",
        "tool": "weather.get",
        "input": {"city": "Jakarta"}
    })
    .to_string();

    let result_json = commit_llm_response(&prepared, &response).unwrap();
    let value: serde_json::Value = serde_json::from_str(&result_json).unwrap();

    assert_eq!(value["action"], "call_tool");
    assert_eq!(value["tool_name"], "weather.get");
    assert_eq!(value["tool_input"]["city"], "Jakarta");
}
