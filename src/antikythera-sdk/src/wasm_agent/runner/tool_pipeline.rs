//! Tool execution pipeline for WASM agent runner.
//!
//! Handles tool registration, prompt generation, result processing,
//! and context policy management.
//!
//! See also: [`super::llm_stream`], [`super::session_lifecycle`].

use antikythera_log::LogLevel;

use super::runner_types::*;
use super::{AgentRunnerError, AgentRunnerRuntime, now_unix_ms, wasm_log};
use crate::wasm_agent::processor::process_tool_result;
use crate::wasm_agent::types::{
    AgentFsmState, ContextPolicy, StreamEventKind, ToolRegistry, ToolResult,
};

/// Tool registration, prompting, result processing, and context policy.
impl AgentRunnerRuntime {
    /// Registers a set of tools from a JSON array definition.
    ///
    /// Replaces any previously registered tools and returns the count of
    /// tools now available.
    pub(super) fn register_tools(&mut self, tools_json: &str) -> Result<u32, AgentRunnerError> {
        self.known_tools = ToolRegistry::from_json(tools_json)?;
        let count = self.known_tools.len();
        wasm_log(
            "runtime",
            LogLevel::Info,
            &format!("{count} tools registered"),
        );
        Ok(count as u32)
    }

    /// Generates the tool description block for inclusion in system prompts.
    ///
    /// Returns an empty string if no tools are registered.
    pub(super) fn get_tools_prompt(&self) -> Result<String, AgentRunnerError> {
        let block = self.known_tools.to_prompt_block().unwrap_or_default();
        Ok(block)
    }

    /// Processes a tool execution result, updating session state.
    ///
    /// Validates the FSM is in `ToolRequested` state (forcing a transition
    /// if needed), delegates to [`crate::wasm_agent::processor::process_tool_result`],
    /// and returns a JSON object with the session ID, step number, next
    /// assistant message, and the tool result.
    /// On the wasm path the tool result may be replaced by the `handle-tool-result` hook below,
    /// which re-parses the output from the replaced input; the pre-hook parse is only needed on
    /// native builds.
    ///
    /// Logic-hooks: `handle-tool-result` is consulted before default tool-result processing.
    /// Passthrough keeps the reported tool result; an override replaces it. A hook error aborts
    /// (fail-closed).
    ///
    /// FSM guard: ensure we're in ToolRequested state before processing tool result.
    ///
    /// FSM transitions are handled inside process_tool_result (processor). Runner only ensures
    /// the correct pre-condition (ToolRequested) via the guard above.
    ///
    pub(super) fn process_tool_result(
        &mut self,
        session_id: &str,
        tool_result_json: &str,
    ) -> Result<String, AgentRunnerError> {
        let _ = self.sweep_idle_sessions(now_unix_ms())?;
        let input: ToolResultInput = serde_json::from_str(tool_result_json)
            .map_err(|e| AgentRunnerError::ToolFailed(format!("Invalid tool-result-json: {e}")))?;

        wasm_log(
            session_id,
            LogLevel::Debug,
            &format!("Processing tool result for '{}'", input.tool_name),
        );

        #[cfg(not(all(feature = "component", target_family = "wasm")))]
        let output: serde_json::Value = serde_json::from_str(&input.output_json)
            .map_err(|e| AgentRunnerError::ToolFailed(format!("Invalid tool output_json: {e}")))?;

        let runtime = self.ensure_session(session_id);
        runtime.touch(now_unix_ms());

        #[cfg(all(feature = "component", target_family = "wasm"))]
        let (input, output) = {
            let input = super::logic_hooks::apply_handle_tool_result_override(
                &runtime.state,
                tool_result_json,
                &input,
            )?;
            let output: serde_json::Value =
                serde_json::from_str(&input.output_json).map_err(|e| {
                    AgentRunnerError::ToolFailed(format!("Invalid tool output_json: {e}"))
                })?;
            (input, output)
        };

        if runtime.state.fsm_state != AgentFsmState::ToolRequested {
            let fsm_before = runtime.state.fsm_state;
            wasm_log(
                session_id,
                LogLevel::Warn,
                &format!(
                    "FSM guard: state is '{}' expected 'tool_requested', forcing transition",
                    fsm_before
                ),
            );
            let _ = runtime
                .state
                .fsm_state
                .transition_to(AgentFsmState::ToolRequested);
            wasm_log(
                session_id,
                LogLevel::Debug,
                &format!(
                    "FSM transition (forced): {} -> {}",
                    fsm_before, runtime.state.fsm_state
                ),
            );
        }

        let next_message = process_tool_result(
            &mut runtime.state,
            &input.tool_name,
            input.success,
            output.clone(),
            input.error_message.clone(),
        )?;

        runtime.telemetry.counters.tool_results += 1;
        if !input.success {
            runtime.telemetry.counters.tool_errors += 1;
            wasm_log(
                session_id,
                LogLevel::Error,
                &format!(
                    "Tool '{}' failed: {}",
                    input.tool_name,
                    input.error_message.as_deref().unwrap_or("unknown error")
                ),
            );
        }
        runtime.emit_event(
            StreamEventKind::ToolResult,
            input
                .correlation_id
                .clone()
                .or_else(|| runtime.telemetry.correlation_id.clone()),
            serde_json::json!({
                "tool": input.tool_name,
                "success": input.success,
            }),
        );

        let result = ToolResult {
            name: input.tool_name,
            success: input.success,
            output,
            error: input.error_message,
            step_id: runtime.state.current_step,
        };

        serde_json::to_string(&serde_json::json!({
            "session_id": runtime.state.session_id,
            "step": runtime.state.current_step,
            "next_message": next_message,
            "tool_result": result,
        }))
        .map_err(|e| {
            AgentRunnerError::Internal(format!("Failed to encode tool processing result: {e}"))
        })
    }

    /// Resolves the context policy for a given turn request.
    ///
    /// Uses the per-request policy if present, otherwise falls back to the
    /// runtime default.
    pub(super) fn resolve_policy(&self, request: &PrepareUserTurnInput) -> ContextPolicy {
        if let Some(policy) = &request.context_policy {
            return policy.clone();
        }
        self.default_config.context_policy.clone()
    }

    /// Updates the runtime's default context policy from a JSON payload.
    ///
    /// Returns `Ok(true)` on success. Subsequent turns will use the new
    /// policy unless overridden per-request.
    pub(super) fn set_context_policy(
        &mut self,
        policy_json: &str,
    ) -> Result<bool, AgentRunnerError> {
        let input: ContextPolicyUpdateInput = serde_json::from_str(policy_json).map_err(|e| {
            AgentRunnerError::ConfigurationFailed(format!("Invalid context-policy-json: {e}"))
        })?;
        self.default_config.context_policy = input.policy;
        wasm_log("runtime", LogLevel::Debug, "Context policy updated");
        Ok(true)
    }
}
