use super::*;
use serde_json::json;

#[test]
fn test_extract_result_data_plain_json() {
    let input = json!({"status": "ok", "data": 42});
    let result = ResponseEmbedder::extract_result_data(&input);
    assert_eq!(result, input);
}

#[test]
fn test_extract_result_data_jsonrpc_result() {
    let input = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"status": "ok", "data": 42}
    });
    let result = ResponseEmbedder::extract_result_data(&input);
    assert_eq!(result, json!({"status": "ok", "data": 42}));
}

#[test]
fn test_extract_result_data_jsonrpc_no_result_filters_fields() {
    let input = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "custom_field": "value"
    });
    let result = ResponseEmbedder::extract_result_data(&input);
    assert_eq!(result, json!({"custom_field": "value"}));
}

#[test]
fn test_extract_result_data_mcp_content() {
    let input = json!({
        "content": [
            {"type": "text", "text": "{\"status\": \"ok\"}"}
        ]
    });
    let result = ResponseEmbedder::extract_result_data(&input);
    assert_eq!(result, json!({"status": "ok"}));
}

#[test]
fn test_extract_result_data_mcp_content_plain_text() {
    let input = json!({
        "content": [
            {"type": "text", "text": "hello world"}
        ]
    });
    let result = ResponseEmbedder::extract_result_data(&input);
    assert_eq!(result, json!("hello world"));
}

#[test]
fn test_extract_result_data_non_object_passthrough() {
    let input = json!("just a string");
    let result = ResponseEmbedder::extract_result_data(&input);
    assert_eq!(result, json!("just a string"));
}

#[test]
/// Extract_result_data extracts the "result" field from the output.
///
fn test_embed_tool_results_step_reference() {
    let steps = vec![AgentStep {
        tool: "test_tool".into(),
        input: json!({}),
        success: true,
        output: json!({"result": "step output data"}),
        message: None,
    }];
    let response = json!({"answer": "step_0"});
    let result = ResponseEmbedder::embed_tool_results_sync(response.clone(), &steps);
    assert_eq!(result, json!({"answer": "step output data"}));
}

#[test]
fn test_embed_tool_results_no_match() {
    let steps = vec![AgentStep {
        tool: "test_tool".into(),
        input: json!({}),
        success: true,
        output: json!("data"),
        message: None,
    }];
    let response = json!({"answer": "no references here"});
    let result = ResponseEmbedder::embed_tool_results_sync(response.clone(), &steps);
    assert_eq!(result, json!({"answer": "no references here"}));
}

#[test]
fn test_embed_tool_results_nested_objects() {
    let steps = vec![AgentStep {
        tool: "t".into(),
        input: json!({}),
        success: true,
        output: json!("output_val"),
        message: None,
    }];
    let response = json!({"outer": {"inner": "step_0"}});
    let result = ResponseEmbedder::embed_tool_results_sync(response, &steps);
    assert_eq!(result, json!({"outer": {"inner": "output_val"}}));
}

#[test]
fn test_embed_tool_results_array() {
    let steps = vec![AgentStep {
        tool: "t".into(),
        input: json!({}),
        success: true,
        output: json!("val"),
        message: None,
    }];
    let response = json!(["step_0", "other"]);
    let result = ResponseEmbedder::embed_tool_results_sync(response, &steps);
    assert_eq!(result, json!(["val", "other"]));
}

#[test]
fn test_embed_tool_results_result_prefix() {
    let steps = vec![AgentStep {
        tool: "t".into(),
        input: json!({}),
        success: true,
        output: json!("data_from_step"),
        message: None,
    }];
    let response = json!("result_0");
    let result = ResponseEmbedder::embed_tool_results_sync(response, &steps);
    assert_eq!(result, json!("data_from_step"));
}
