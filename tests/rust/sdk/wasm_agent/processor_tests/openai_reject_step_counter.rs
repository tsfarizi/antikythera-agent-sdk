#[test]
fn openai_choices_format_is_rejected() {
    let mut state = fresh_state();
    let response = serde_json::json!({
        "choices": [{
            "message": {
                "tool_calls": [{
                    "function": {"name": "weather.get", "arguments": "{\"city\":\"Jakarta\"}"}
                }]
            }
        }]
    })
    .to_string();

    let result = process_llm_response(&mut state, &response);
    assert!(
        result.is_err(),
        "provider-native OpenAI format must be rejected; host must normalize"
    );
}



#[test]
/// Step counter increments only on tool calls, not on final responses.
///
/// Reset FSM to LlmStreaming for the next call.
///
fn step_counter_increments_on_tool_call_not_final() {
    let mut state = fresh_state();
    assert_eq!(state.current_step, 0);

    let tool_call = serde_json::json!({
        "action": "call_tool", "tool": "t", "input": {}
    })
    .to_string();
    process_llm_response(&mut state, &tool_call).unwrap();
    assert_eq!(state.current_step, 1, "step should increment on tool call");

    state.fsm_state = AgentFsmState::LlmStreaming;
    let final_resp = serde_json::json!({ "response": "done" }).to_string();
    process_llm_response(&mut state, &final_resp).unwrap();
    assert_eq!(
        state.current_step, 1,
        "step must NOT increment on final response"
    );
}


/// ToolRegistry -- unknown tool rejected when registry populated.
///
fn make_weather_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::default();
    reg.register(ToolDefinition {
        name: "weather.get".to_string(),
        title: None,
        description: "Get weather".to_string(),
        parameters: vec![ToolParameterSchema {
            name: "city".to_string(),
            param_type: "string".to_string(),
            description: "City name".to_string(),
            required: true,
        }],
        input_schema: None,
        output_schema: None,
        annotations: None,
        execution: None,
    });
    reg
}


#[test]
fn validate_tool_call_rejects_unknown_tool() {
    let registry = make_weather_registry();
    let args = serde_json::json!({"city": "Jakarta"});
    let err = registry.validate_call("flights.book", &args).unwrap_err();
    assert_eq!(
        err,
        ToolValidationError::UnknownTool {
            name: "flights.book".to_string()
        }
    );
}
