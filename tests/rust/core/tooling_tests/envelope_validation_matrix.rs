use antikythera_core::application::tooling::{
    EnvelopeError, ToolCallEnvelope, ToolResultEnvelope, validate_tool_call_envelope,
    validate_tool_result_envelope,
};
use serde_json::json;

#[test]
fn tool_call_envelope_validation_matrix() {
    let valid = ToolCallEnvelope {
        tool: "weather.get".to_string(),
        arguments: json!({"city": "Jakarta"}),
        correlation_id: Some("corr-1".to_string()),
    };
    assert!(validate_tool_call_envelope(&valid).is_ok());

    let missing_tool = ToolCallEnvelope {
        tool: "  ".to_string(),
        arguments: json!({}),
        correlation_id: None,
    };
    assert_eq!(
        validate_tool_call_envelope(&missing_tool),
        Err(EnvelopeError::MissingTool)
    );

    let invalid_args = ToolCallEnvelope {
        tool: "weather.get".to_string(),
        arguments: json!(["unexpected"]),
        correlation_id: None,
    };
    assert_eq!(
        validate_tool_call_envelope(&invalid_args),
        Err(EnvelopeError::InvalidArguments)
    );
}

#[test]
fn tool_result_envelope_validation_matrix() {
    let valid_success = ToolResultEnvelope {
        tool: "weather.get".to_string(),
        success: true,
        output: json!({"ok": true}),
        error: None,
        correlation_id: None,
    };
    assert!(validate_tool_result_envelope(&valid_success).is_ok());

    let valid_failure = ToolResultEnvelope {
        tool: "weather.get".to_string(),
        success: false,
        output: json!(null),
        error: Some("timeout".to_string()),
        correlation_id: None,
    };
    assert!(validate_tool_result_envelope(&valid_failure).is_ok());

    let inconsistent_success = ToolResultEnvelope {
        tool: "weather.get".to_string(),
        success: true,
        output: json!({}),
        error: Some("must be empty".to_string()),
        correlation_id: None,
    };
    assert_eq!(
        validate_tool_result_envelope(&inconsistent_success),
        Err(EnvelopeError::InconsistentResult)
    );

    let inconsistent_failure = ToolResultEnvelope {
        tool: "weather.get".to_string(),
        success: false,
        output: json!({}),
        error: None,
        correlation_id: None,
    };
    assert_eq!(
        validate_tool_result_envelope(&inconsistent_failure),
        Err(EnvelopeError::InconsistentResult)
    );
}

#[test]
fn transport_message_mapping_matrix() {
    let matrix = [
        (
            EnvelopeError::MissingTool,
            "tool must be a non-empty string",
        ),
        (
            EnvelopeError::InvalidArguments,
            "arguments must be a JSON object",
        ),
        (
            EnvelopeError::InconsistentResult,
            "error must be present for failed results and absent for successful ones",
        ),
    ];

    for (error, expected_tail) in matrix {
        let msg = error.to_transport_message("call");
        assert!(msg.starts_with("invalid MCP tool call envelope:"));
        assert!(msg.ends_with(expected_tail));
    }
}
