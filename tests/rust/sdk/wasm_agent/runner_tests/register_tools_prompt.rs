

#[test]
#[serial_test::serial]
/// Tool registry -- register_tools and get_tools_prompt.
///
fn register_tools_counts_tools_correctly() {
    let tools_json = serde_json::json!([
        {
            "name": "weather.get",
            "description": "Get current weather for a city",
            "parameters": [
                {"name": "city", "param_type": "string", "description": "City name", "required": true}
            ]
        },
        {
            "name": "calculator.add",
            "description": "Add two numbers",
            "parameters": [
                {"name": "a", "param_type": "number", "description": "First operand", "required": true},
                {"name": "b", "param_type": "number", "description": "Second operand", "required": true}
            ]
        }
    ])
    .to_string();

    let count = register_tools(&tools_json).unwrap();
    assert_eq!(count, 2, "expected 2 tools registered");
}

#[test]
#[serial_test::serial]
fn get_tools_prompt_contains_tool_names() {
    let tools_json = serde_json::json!([
        {
            "name": "search.query",
            "description": "Search the web",
            "parameters": [
                {"name": "query", "param_type": "string", "description": "Search query", "required": true}
            ]
        }
    ])
    .to_string();

    register_tools(&tools_json).unwrap();
    let prompt = get_tools_prompt().unwrap();
    assert!(
        prompt.contains("search.query"),
        "tools prompt should contain tool name 'search.query'"
    );
    assert!(
        prompt.contains("query*"),
        "required param should be marked with '*'"
    );
}
