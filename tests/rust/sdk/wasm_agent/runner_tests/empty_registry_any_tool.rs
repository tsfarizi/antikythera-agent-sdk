

#[test]
#[serial_test::serial]
/// Tool registry -- empty registry allows any tool call (backward compat).
///
fn empty_registry_allows_any_tool_call() {
    register_tools("[]").unwrap();

    let session_id = init(&config_json()).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&session_id, "do something")).unwrap();

    let response = serde_json::json!({
        "action": "call_tool",
        "tool": "anything.goes",
        "input": {}
    })
    .to_string();

    let result = commit_llm_response(&prepared, &response);
    assert!(result.is_ok(), "empty registry should allow any tool call");
}
