#[test]
fn tool_registry_to_prompt_block_contains_tool_names() {
    let registry = make_weather_registry();
    let block = registry.to_prompt_block().expect("registry is non-empty");
    assert!(block.contains("weather.get"), "block: {block}");
    assert!(
        block.contains("city*"),
        "required param marked with *: {block}"
    );
}


#[test]
fn tool_registry_prompt_block_is_none_when_empty() {
    let empty = ToolRegistry::default();
    assert!(empty.to_prompt_block().is_none());
}



#[test]
fn tool_registry_from_json_round_trip() {
    let json = serde_json::json!([
        {
            "name": "calc.add",
            "description": "Add two numbers",
            "parameters": [
                {"name": "a", "param_type": "number", "description": "First", "required": true},
                {"name": "b", "param_type": "number", "description": "Second", "required": true}
            ]
        }
    ])
    .to_string();

    let registry = ToolRegistry::from_json(&json).unwrap();
    assert!(registry.is_populated());
    assert_eq!(registry.len(), 1);
    assert!(registry.get("calc.add").is_some());
    let names = registry.tool_names();
    assert_eq!(names, vec!["calc.add"]);
}
