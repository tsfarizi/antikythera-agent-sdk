use super::*;
use serde_json::json;

#[test]
fn test_format_single_result() {
    let result_str = ToolResultParser::format_single(
        "my_tool".into(),
        json!({"key": "val"}),
        true,
        json!({"output_key": "output_val"}),
        Some("done".into()),
        "Use the result to answer.",
    );
    let parsed: Value = serde_json::from_str(&result_str).expect("must be valid JSON");

    assert_eq!(parsed["instruction"], "Use the result to answer.");
    let tr = &parsed["tool_result"];
    assert_eq!(tr["tool"], "my_tool");
    assert_eq!(tr["success"], true);
    assert_eq!(tr["input"]["key"], "val");
    assert_eq!(tr["output"]["output_key"], "output_val");
    assert_eq!(tr["message"], "done");
}

#[test]
fn test_format_single_result_failure() {
    let result_str = ToolResultParser::format_single(
        "fail_tool".into(),
        json!({}),
        false,
        json!("error message"),
        None,
        "Handle error.",
    );
    let parsed: Value = serde_json::from_str(&result_str).expect("must be valid JSON");
    assert_eq!(parsed["tool_result"]["success"], false);
    assert!(parsed["tool_result"]["message"].is_null());
}

#[test]
fn test_format_parallel_results() {
    let results = vec![
        json!({"tool": "a", "success": true}),
        json!({"tool": "b", "success": false}),
    ];
    let result_str = ToolResultParser::format_parallel(results.clone(), "Combine results.");
    let parsed: Value = serde_json::from_str(&result_str).expect("must be valid JSON");

    assert_eq!(parsed["instruction"], "Combine results.");
    assert!(parsed["tool_results"].is_array());
    assert_eq!(parsed["tool_results"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["tool_results"][0]["tool"], "a");
    assert_eq!(parsed["tool_results"][1]["tool"], "b");
}

#[test]
fn test_single_result_value() {
    let val = ToolResultParser::single_result_value(
        "tool_x".into(),
        json!({"a": 1}),
        true,
        json!("output"),
        None,
    );
    assert_eq!(val["tool"], "tool_x");
    assert_eq!(val["success"], true);
    assert_eq!(val["output"], "output");
    assert!(val["message"].is_null());
}
