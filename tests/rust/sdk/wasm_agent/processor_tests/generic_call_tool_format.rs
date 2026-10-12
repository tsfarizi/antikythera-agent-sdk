#[test]
fn generic_call_tool_action_is_parsed() {
    let mut state = fresh_state();
    let response = serde_json::json!({
        "action": "call_tool",
        "tool": "calculator.add",
        "input": {"a": 1, "b": 2}
    })
    .to_string();

    let action = process_llm_response(&mut state, &response).unwrap();
    match action {
        AgentAction::CallTool { tool, input } => {
            assert_eq!(tool, "calculator.add");
            assert_eq!(input["a"], 1);
            assert_eq!(input["b"], 2);
        }
        other => panic!("expected CallTool, got {other:?}"),
    }
}



#[test]
/// Generic final action via "action" field.
///
fn generic_final_action_in_action_field_is_parsed() {
    let mut state = fresh_state();
    let response = serde_json::json!({
        "action": "final",
        "response": "Task complete"
    })
    .to_string();

    let action = process_llm_response(&mut state, &response).unwrap();
    match action {
        AgentAction::Final { response } => {
            assert_eq!(response.as_str().unwrap(), "Task complete");
        }
        other => panic!("expected Final, got {other:?}"),
    }
}



#[test]
/// Shorthand "response" field â†’ Final.
///
fn shorthand_response_field_is_treated_as_final() {
    let mut state = fresh_state();
    let response = serde_json::json!({ "response": "shorthand answer" }).to_string();

    let action = process_llm_response(&mut state, &response).unwrap();
    match action {
        AgentAction::Final { response } => {
            assert_eq!(response.as_str().unwrap(), "shorthand answer");
        }
        other => panic!("expected Final, got {other:?}"),
    }
}
