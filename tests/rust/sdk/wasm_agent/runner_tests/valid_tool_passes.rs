

#[test]
#[serial_test::serial]
/// Tool registry -- valid call passes through when registry populated.
///
fn valid_tool_call_passes_validation() {
    let tools_json = serde_json::json!([
        {
            "name": "db.query",
            "description": "Run a database query",
            "parameters": [
                {"name": "sql", "param_type": "string", "description": "SQL statement", "required": true}
            ]
        }
    ])
    .to_string();
    register_tools(&tools_json).unwrap();

    let session_id = init(&config_json()).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&session_id, "list users")).unwrap();

    let response = serde_json::json!({
        "action": "call_tool",
        "tool": "db.query",
        "input": {"sql": "SELECT * FROM users"}
    })
    .to_string();

    let result = commit_llm_response(&prepared, &response);
    assert!(
        result.is_ok(),
        "valid tool call should pass, got: {:?}",
        result
    );
    let value: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(value["action"], "call_tool");
    assert_eq!(value["tool_name"], "db.query");
}
