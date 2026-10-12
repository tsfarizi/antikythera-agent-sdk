use super::*;

#[test]
/// Step_id is dropped (runner derives it from session state).
///
/// Output_json is required — the runner rejects the non-_json field.
///
fn tool_execution_result_maps_to_runner_input() {
    let result = ToolExecutionResult {
        tool_name: "get_current_time".to_string(),
        success: true,
        output_json: "{\"datetime\":\"2026-08-12T00:00:00Z\"}".to_string(),
        error_message: None,
        step_id: 7,
    };
    let input = tool_execution_result_to_runner_input(&result, Some("corr-1".to_string()));
    assert_eq!(input["tool_name"], "get_current_time");
    assert_eq!(input["success"], true);
    assert_eq!(
        input["output_json"],
        "{\"datetime\":\"2026-08-12T00:00:00Z\"}"
    );
    assert!(input["error_message"].is_null());
    assert_eq!(input["correlation_id"], "corr-1");
    assert!(input.get("step_id").is_none());
    assert!(input.get("output").is_none());
}

#[test]
fn tool_execution_result_failure_keeps_error_and_correlation() {
    let result = ToolExecutionResult {
        tool_name: "rm".to_string(),
        success: false,
        output_json: "{}".to_string(),
        error_message: Some("permission denied".to_string()),
        step_id: 3,
    };
    let input = tool_execution_result_to_runner_input(&result, None);
    assert_eq!(input["tool_name"], "rm");
    assert_eq!(input["success"], false);
    assert_eq!(input["error_message"], "permission denied");
    assert!(input["correlation_id"].is_null());
}

#[test]
fn tool_call_event_wire_names_are_kebab_case() {
    let event = ToolCallEvent {
        tool_name: "echo".to_string(),
        arguments_json: "{}".to_string(),
        session_id: Some("s1".to_string()),
        step_id: 1,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["tool-name"], "echo");
    assert_eq!(json["arguments-json"], "{}");
    assert_eq!(json["session-id"], "s1");
    assert_eq!(json["step-id"], 1);
    assert!(json.get("tool_name").is_none());
}

#[test]
fn tool_execution_result_wire_names_are_kebab_case() {
    let result = ToolExecutionResult {
        tool_name: "echo".to_string(),
        success: true,
        output_json: "{\"x\":1}".to_string(),
        error_message: None,
        step_id: 1,
    };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["tool-name"], "echo");
    assert_eq!(json["output-json"], "{\"x\":1}");
    assert_eq!(json["step-id"], 1);
    assert!(json.get("output_json").is_none());
}

#[test]
fn llm_request_and_response_wire_names_are_snake_case() {
    let request = LlmRequest {
        provider: Some("ollama".to_string()),
        model: Some("gpt-oss:120b-cloud".to_string()),
        session_id: Some("session-123".to_string()),
        messages_json: "[]".to_string(),
        force_json: false,
        temperature: Some(0.7),
        max_tokens: Some(512),
        schema_name: None,
        metadata_json: None,
    };
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["session_id"], "session-123");
    assert_eq!(json["messages_json"], "[]");
    assert_eq!(json["force_json"], false);
    assert_eq!(json["max_tokens"], 512);
    assert!(json.get("session-id").is_none());

    let response = LlmResponse {
        content: "Hello".to_string(),
        model: Some("gpt-oss:120b-cloud".to_string()),
        session_id: Some("session-123".to_string()),
        message_json: None,
        tokens_used: Some(4),
        finish_reason: Some("stop".to_string()),
        raw_response_json: None,
    };
    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["tokens_used"], 4);
    assert_eq!(json["finish_reason"], "stop");
    assert!(json.get("tokens-used").is_none());
}

#[test]
fn envelope_shape_matches_golden() {
    let envelope = EventEnvelope {
        event_type: "tool-execution-request".to_string(),
        correlation_id: Some("corr-0001".to_string()),
        session_id: Some("session-123".to_string()),
        client_id: "client-a".to_string(),
        payload: json!({"tool-name": "get_current_time"}),
    };
    let json = serde_json::to_value(&envelope).unwrap();
    assert_eq!(json["type"], "tool-execution-request");
    assert_eq!(json["correlation_id"], "corr-0001");
    assert_eq!(json["session_id"], "session-123");
    assert_eq!(json["client_id"], "client-a");
    assert_eq!(json["payload"]["tool-name"], "get_current_time");
}
