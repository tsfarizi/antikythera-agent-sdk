//! K1 tool loop owner: the host-side agent loop over the runner with
//! `auto_execute_tools=false`.
//!
//! init(config) → prepare-user-turn → LLM proxy → commit-llm-response →
//! drain-events → on `call_tool`: execute the tool through routing →
//! process-tool-result-for-session → repeat until `final` or `max_steps`.
//!
//! Builtin in-band tools are distinguished from host-executed tools: the
//! composite executes builtins inside `commit-llm-response`
//! (llm_stream.rs:525-596) and emits the `tool_result` event into the drain;
//! a tool_result already present in the drain after commit means no host
//! execution is needed, otherwise the tool goes through routing.

use serde_json::{Value, json};

use crate::core::CoreSession;
use crate::host::feed_tool_result;
use crate::wire::{LlmRequest, ToolCallEvent};
use crate::wit::SharedState;

/// Parameters for one tool-loop run.
#[derive(Debug, Clone)]
pub struct ToolLoopConfig {
    pub session_id: String,
    pub max_steps: u32,
    /// Per-step user prompts; step `i` uses `prompts[i]` when present, else
    /// the last prompt.
    pub prompts: Vec<String>,
    /// LLM proxy parameters.
    pub provider: String,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub force_json: bool,
    /// When true the union registry is pushed to the runner before the loop.
    pub register_union_tools: bool,
    /// When false the host-supplied `runtime-hooks` interface is never
    /// consulted (the loop runs without a peer client).
    pub runtime_hooks_enabled: bool,
}

impl Default for ToolLoopConfig {
    fn default() -> Self {
        Self {
            session_id: "server-loop".to_string(),
            max_steps: 10,
            prompts: vec!["hello".to_string()],
            provider: "stub".to_string(),
            model: "stub-model".to_string(),
            temperature: None,
            max_tokens: None,
            force_json: false,
            register_union_tools: true,
            runtime_hooks_enabled: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoopOutcome {
    pub session_id: String,
    pub steps: u32,
    pub action: String,
    pub content: Option<String>,
    pub commit_json: Value,
}

/// Run the tool loop on the current thread (the core thread). Blocking: the
/// caller must not be a tokio worker (host functions block on the runtime).
/// Builtin in-band: the composite already emitted a tool_result for this tool inside commit — no
/// host execution needed.
///
pub fn run_tool_loop(
    core: &mut CoreSession,
    shared: &SharedState,
    config: ToolLoopConfig,
) -> Result<LoopOutcome, String> {
    let config_json = json!({
        "session_id": config.session_id,
        "max_steps": config.max_steps,
        "auto_execute_tools": false,
        "runtime_hooks_enabled": config.runtime_hooks_enabled,
    })
    .to_string();
    let session_id = core.init(&config_json)?;

    if config.register_union_tools {
        let union = serde_json::to_string(&shared.router.union_definitions())
            .map_err(|e| format!("tool loop: cannot encode union registry: {e}"))?;
        core.register_tools(&union)?;
    }

    let mut step = 0u32;
    loop {
        if step >= config.max_steps {
            return Err(format!(
                "tool loop: max_steps ({}) exceeded without final action",
                config.max_steps
            ));
        }
        let prompt = config
            .prompts
            .get(step as usize)
            .cloned()
            .or_else(|| config.prompts.last().cloned())
            .unwrap_or_default();
        let request_json = json!({
            "prompt": prompt,
            "session_id": session_id,
            "correlation_id": format!("loop-{step}"),
        })
        .to_string();
        let prepared_json = core.prepare_user_turn(&request_json)?;
        let prepared: Value = serde_json::from_str(&prepared_json)
            .map_err(|e| format!("tool loop: prepared turn is not JSON: {e}"))?;
        let messages_json = prepared["messages_json"]
            .as_str()
            .unwrap_or("[]")
            .to_string();

        shared.check_llm_gate(Some(&session_id))?;
        let llm_request = LlmRequest {
            provider: Some(config.provider.clone()),
            model: Some(config.model.clone()),
            session_id: Some(session_id.clone()),
            messages_json,
            force_json: config.force_json,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            schema_name: None,
            metadata_json: None,
        };
        let provider = shared.resolve_provider(Some(&config.provider))?;
        let llm_response = shared
            .runtime
            .block_on(provider.call(llm_request))
            .map_err(|e| format!("tool loop: llm call failed: {e}"))?;

        let commit_json = core.commit_llm_response(&prepared_json, &llm_response.content)?;
        let commit: Value = serde_json::from_str(&commit_json)
            .map_err(|e| format!("tool loop: commit result is not JSON: {e}"))?;
        let action = commit["action"].as_str().unwrap_or("").to_string();
        let content = commit["content"].as_str().map(|s| s.to_string());

        tracing::debug!(session = %session_id, step, action = %action, "tool loop step");

        match action.as_str() {
            "final" => {
                return Ok(LoopOutcome {
                    session_id,
                    steps: step + 1,
                    action,
                    content,
                    commit_json: commit,
                });
            }
            "call_tool" => {
                let tool = commit["tool_name"].as_str().unwrap_or_default().to_string();
                let tool_input = commit["tool_input"].clone();
                let step_id = commit["step"].as_u64().unwrap_or(0) as u32;

                let events_json = core.drain_events(&session_id)?;
                let events: Value = serde_json::from_str(&events_json)
                    .map_err(|e| format!("tool loop: drained events are not JSON: {e}"))?;

                let in_band = events.as_array().is_some_and(|arr| {
                    arr.iter().any(|event| {
                        event["kind"] == "tool_result"
                            && event["payload"]["tool"].as_str() == Some(tool.as_str())
                    })
                });

                if !in_band {
                    let call_event = ToolCallEvent {
                        tool_name: tool.clone(),
                        arguments_json: tool_input.to_string(),
                        session_id: Some(session_id.clone()),
                        step_id,
                    };
                    let result = shared
                        .runtime
                        .block_on(shared.router.execute(&call_event))
                        .map_err(|e| format!("tool loop: execute '{tool}' failed: {e}"))?;
                    let correlation_id = commit["correlation_id"].as_str().map(|s| s.to_string());
                    feed_tool_result(core, &session_id, &result, correlation_id)
                        .map_err(|e| format!("tool loop: process tool result failed: {e}"))?;
                }
            }
            "retry" => {
                return Err(format!(
                    "tool loop: runner requested retry: {}",
                    content.unwrap_or_default()
                ));
            }
            other => {
                return Err(format!("tool loop: unknown action '{other}'"));
            }
        }
        step += 1;
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/loop_owner.tests.rs"]
mod tests;
