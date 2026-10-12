//! WASM Agent Processor
//!
//! Processes LLM responses and determines next action.
//! WASM does NOT call LLM APIs - host handles that.
//! The host is responsible for normalizing provider-native formats before calling
//! commit_llm_response.

use super::types::*;
use crate::sdk_logging::get_sdk_logger;
use antikythera_log::LogLevel;
use std::collections::HashMap;

/// Transition FSM state and log the change.
fn transition_and_log(
    state: &mut AgentState,
    from: AgentFsmState,
    to: AgentFsmState,
) -> Result<(), String> {
    state
        .fsm_state
        .transition_to(to)
        .map(|_| {
            get_sdk_logger(&state.session_id).log_with_source(
                LogLevel::Debug,
                "processor",
                format!(
                    "FSM transition: {} -> {} | step={}",
                    from, to, state.current_step
                ),
            );
        })
        .map_err(|e| e.to_string())
}

/// Process LLM response and determine next action.
///
/// Accepts the framework generic JSON format or plain text.
/// The host must normalize provider-native output (OpenAI choices, Gemini
/// candidates, Anthropic content arrays) before calling this function.
/// FSM: LlmStreaming -> LlmCommitted (runner set LlmStreaming before calling).
///
pub fn process_llm_response(
    state: &mut AgentState,
    llm_response_content: &str,
) -> Result<AgentAction, String> {
    if state.is_max_steps_exceeded() {
        return Err(format!(
            "Max steps exceeded: {} >= {}",
            state.current_step, state.config.max_steps
        ));
    }

    transition_and_log(
        state,
        AgentFsmState::LlmStreaming,
        AgentFsmState::LlmCommitted,
    )?;

    let parsed: serde_json::Value = match serde_json::from_str(llm_response_content) {
        Ok(value) => value,
        Err(_) => {
            transition_and_log(state, AgentFsmState::LlmCommitted, AgentFsmState::Final)?;
            return Ok(AgentAction::Final {
                response: serde_json::Value::String(llm_response_content.to_string()),
            });
        }
    };

    if let Some(tool_action) = parse_generic_tool_action(&parsed)? {
        match &tool_action {
            AgentAction::CallTool { .. } => {
                state.current_step += 1;
                transition_and_log(
                    state,
                    AgentFsmState::LlmCommitted,
                    AgentFsmState::ToolRequested,
                )?;
            }
            AgentAction::Final { .. } => {
                transition_and_log(state, AgentFsmState::LlmCommitted, AgentFsmState::Final)?;
            }
            AgentAction::Retry { .. } => {
                transition_and_log(state, AgentFsmState::LlmCommitted, AgentFsmState::Idle)?;
            }
        }
        return Ok(tool_action);
    }

    if let Some(final_response) = parse_final_response(&parsed) {
        transition_and_log(state, AgentFsmState::LlmCommitted, AgentFsmState::Final)?;
        return Ok(AgentAction::Final {
            response: final_response,
        });
    }

    Err("Could not derive action from LLM response. \
         Host must normalize provider-native output to the framework generic JSON format."
        .to_string())
}

fn parse_generic_tool_action(parsed: &serde_json::Value) -> Result<Option<AgentAction>, String> {
    let action = match parsed.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return Ok(None),
    };

    match action {
        "call_tool" => {
            let tool = parsed
                .get("tool")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'tool' field")?
                .to_string();
            let input = parsed
                .get("input")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            Ok(Some(AgentAction::CallTool { tool, input }))
        }
        "final" => {
            let response = parsed
                .get("response")
                .or_else(|| parsed.get("content"))
                .cloned()
                .ok_or("Missing 'response' field")?;
            Ok(Some(AgentAction::Final { response }))
        }
        "retry" => {
            let error = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("retry requested")
                .to_string();
            Ok(Some(AgentAction::Retry { error }))
        }
        _ => Err(format!("Unknown action: {}", action)),
    }
}

fn parse_final_response(parsed: &serde_json::Value) -> Option<serde_json::Value> {
    parsed
        .get("response")
        .or_else(|| parsed.get("content"))
        .and_then(|v| if v.is_array() { None } else { Some(v.clone()) })
}

/// Process tool execution result and build next prompt
pub fn process_tool_result(
    state: &mut AgentState,
    tool_name: &str,
    success: bool,
    output: serde_json::Value,
    error: Option<String>,
) -> Result<String, String> {
    let result = if success {
        output.clone()
    } else {
        let err_msg = error.clone().unwrap_or_else(|| "Unknown error".to_string());
        serde_json::json!({"error": err_msg})
    };

    state.record_tool_result(tool_name.to_string(), result);

    let message = if success {
        format!(
            "Tool '{}' executed successfully.\nResult: {}",
            tool_name,
            serde_json::to_string_pretty(&output).unwrap_or_default()
        )
    } else {
        let err_msg = error.clone().unwrap_or_else(|| "Unknown error".to_string());
        format!("Tool '{}' failed.\nError: {}", tool_name, err_msg)
    };

    state.add_message(AgentMessage {
        role: "tool".to_string(),
        content: message.clone(),
        tool_call: None,
        tool_result: Some(ToolResult {
            name: tool_name.to_string(),
            success,
            output,
            error,
            step_id: state.current_step,
        }),
    });

    transition_and_log(
        state,
        AgentFsmState::ToolRequested,
        AgentFsmState::ToolResultProcessed,
    )?;

    Ok(message)
}

/// Build system prompt for LLM
pub fn build_system_prompt(template: &str, variables: &PromptVariables) -> String {
    variables.render(template)
}

/// Build messages for LLM API (for host to use)
pub fn build_llm_messages(system_prompt: &str, state: &AgentState) -> Vec<HashMap<String, String>> {
    let mut messages = Vec::new();

    messages.push(HashMap::from([
        ("role".to_string(), "system".to_string()),
        ("content".to_string(), system_prompt.to_string()),
    ]));

    if let Some(summary) = &state.rolling_summary {
        messages.push(HashMap::from([
            ("role".to_string(), "system".to_string()),
            (
                "content".to_string(),
                format!(
                    "Conversation summary v{}: {}",
                    summary.version, summary.text
                ),
            ),
        ]));
    }

    for msg in &state.message_history {
        let mut message = HashMap::from([
            ("role".to_string(), msg.role.clone()),
            ("content".to_string(), msg.content.clone()),
        ]);

        if let Some(tool_call) = &msg.tool_call {
            message.insert(
                "tool_calls".to_string(),
                serde_json::to_string(&tool_call).unwrap_or_default(),
            );
        }

        messages.push(message);
    }

    messages
}

/// Validate JSON against schema
pub fn validate_json_schema(
    schema: &serde_json::Value,
    data: &serde_json::Value,
) -> Result<(), String> {
    if data.is_null() {
        return Err("JSON is null".to_string());
    }

    if let Some(required) = schema.get("required").and_then(|v| v.as_array())
        && let Some(obj) = data.as_object()
    {
        for field in required {
            if let Some(field_str) = field.as_str()
                && !obj.contains_key(field_str)
            {
                return Err(format!("Missing required field: {}", field_str));
            }
        }
    }

    Ok(())
}

/// Validate a tool call against the registered tool definitions.
///
/// When the registry is empty (not yet populated by the host), validation is
/// skipped so sessions that pre-date tool registration are unaffected.
pub fn validate_tool_call(
    registry: &ToolRegistry,
    tool_name: &str,
    arguments: &serde_json::Value,
) -> Result<(), ToolValidationError> {
    if !registry.is_populated() {
        return Ok(());
    }
    registry.validate_call(tool_name, arguments)
}
