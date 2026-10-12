

#[test]
#[serial_test::serial]
/// Tool registry -- validation blocks unknown tool calls.
///
/// Registers only one known tool.
///
/// LLM tries to call a tool not in the registry.
///
fn unknown_tool_call_returns_error_when_registry_populated() {
    let tools_json = serde_json::json!([
        {
            "name": "weather.get",
            "description": "Get weather",
            "parameters": [
                {"name": "city", "param_type": "string", "description": "City", "required": true}
            ]
        }
    ])
    .to_string();
    register_tools(&tools_json).unwrap();

    let session_id = init(&config_json()).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&session_id, "book a flight")).unwrap();

    let response = serde_json::json!({
        "action": "call_tool",
        "tool": "flights.book",
        "input": {"destination": "Tokyo"}
    })
    .to_string();

    let result = commit_llm_response(&prepared, &response);
    assert!(result.is_err(), "should reject unknown tool call");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("flights.book"),
        "error should mention the unknown tool name, got: {err}"
    );
}
