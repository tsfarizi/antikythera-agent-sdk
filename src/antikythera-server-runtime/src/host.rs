//! Host implementations for the two host-facing WIT interfaces:
//!
//! - `antikythera:agent-sdk/runtime-hooks@1.0.0` — the composite's single
//!   non-WASI import. Each decision goes to the peer as a `hook-request`
//!   SSE event and is answered by a POST-back (fail-closed on absence/TTL).
//! - `antikythera:agent-sdk/host-imports@1.0.0` — the drop-in logic-core
//!   escape hatch: `call-llm` (quota), `emit-tool-call` (3-destination
//!   routing behind the tool gate), bounded `save-state`/`load-state`,
//!   `log-message` passthrough.
//!
//! Every gate denial surfaces as `permission:`; there is no silent
//! degradation.

use crate::config::HookName;
use crate::wire::{
    LlmRequest, ToolCallEvent, ToolExecutionResult, tool_execution_result_to_runner_input,
};
use crate::wit::HostState;
use crate::wit::antikythera::agent_sdk::host_imports::{
    Host, LlmRequest as VocabLlmRequest, LlmResponse as VocabLlmResponse,
    LogEvent as VocabLogEvent, ToolCallEvent as VocabToolCallEvent,
    ToolExecutionResult as VocabToolExecutionResult,
};

/// Validate a context-id before it becomes a path/state key: rejects empty
/// ids, `.`, `..`, separators, drive/stream syntax, and NUL.
pub fn validate_context_id(context_id: &str) -> Result<(), String> {
    let traversal = context_id.is_empty()
        || context_id == "."
        || context_id.contains("..")
        || context_id.contains('/')
        || context_id.contains('\\')
        || context_id.contains(':')
        || context_id.contains('\0');
    if traversal {
        return Err("permission: invalid context id".to_string());
    }
    Ok(())
}

impl crate::wit::antikythera::agent_sdk::runtime_hooks::Host for HostState {
    fn prepare_turn(
        &mut self,
        request_json: String,
        session_state_json: String,
    ) -> Result<String, String> {
        self.shared
            .request_hook_decision(HookName::PrepareTurn, &session_state_json, &request_json)
    }

    fn decide_action(
        &mut self,
        session_state_json: String,
        llm_response_json: String,
    ) -> Result<String, String> {
        self.shared.request_hook_decision(
            HookName::DecideAction,
            &session_state_json,
            &llm_response_json,
        )
    }

    fn handle_tool_result(
        &mut self,
        session_state_json: String,
        tool_result_json: String,
    ) -> Result<String, String> {
        self.shared.request_hook_decision(
            HookName::HandleToolResult,
            &session_state_json,
            &tool_result_json,
        )
    }
}

impl Host for HostState {
    fn call_llm(&mut self, request: VocabLlmRequest) -> Result<VocabLlmResponse, String> {
        self.shared.check_llm_gate(request.session_id.as_deref())?;
        let provider = self.shared.resolve_provider(request.provider.as_deref())?;
        let wire_request = LlmRequest {
            provider: request.provider,
            model: request.model,
            session_id: request.session_id,
            messages_json: request.messages_json,
            force_json: request.force_json,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            schema_name: request.schema_name,
            metadata_json: request.metadata_json,
        };
        let response = self
            .shared
            .runtime
            .block_on(provider.call(wire_request))
            .map_err(|e| e.to_string())?;
        Ok(VocabLlmResponse {
            content: response.content,
            model: response.model,
            session_id: response.session_id,
            message_json: response.message_json,
            tokens_used: response.tokens_used,
            finish_reason: response.finish_reason,
            raw_response_json: response.raw_response_json,
        })
    }

    fn emit_tool_call(
        &mut self,
        event: VocabToolCallEvent,
    ) -> Result<VocabToolExecutionResult, String> {
        let wire_event = ToolCallEvent {
            tool_name: event.tool_name,
            arguments_json: event.arguments_json,
            session_id: event.session_id,
            step_id: event.step_id,
        };
        let result = self
            .shared
            .runtime
            .block_on(self.shared.router.execute(&wire_event))?;
        Ok(VocabToolExecutionResult {
            tool_name: result.tool_name,
            success: result.success,
            output_json: result.output_json,
            error_message: result.error_message,
            step_id: result.step_id,
        })
    }

    fn log_message(&mut self, event: VocabLogEvent) {
        tracing::info!(
            target: "host-log",
            level = %event.level,
            timestamp = ?event.timestamp,
            "{}",
            event.message
        );
    }

    fn save_state(&mut self, context_id: String, state_json: String) -> Result<(), String> {
        validate_context_id(&context_id)?;
        let mut storage = self
            .shared
            .storage
            .lock()
            .expect("state store lock poisoned");
        let existing = storage.get(&context_id).map(|v| v.len()).unwrap_or(0);
        let projected = storage
            .values()
            .map(|v| v.len())
            .sum::<usize>()
            .saturating_sub(existing)
            .saturating_add(state_json.len());
        if projected > self.shared.storage_capacity_bytes {
            return Err("permission: state store capacity exceeded".to_string());
        }
        storage.insert(context_id, state_json);
        Ok(())
    }

    fn load_state(&mut self, context_id: String) -> Result<Option<String>, String> {
        validate_context_id(&context_id)?;
        let storage = self
            .shared
            .storage
            .lock()
            .expect("state store lock poisoned");
        Ok(storage.get(&context_id).cloned())
    }
}

/// Convenience for the loop owner: map a wire execution result into the
/// runner `ToolResultInput` JSON and feed it to `process-tool-result`.
pub fn feed_tool_result(
    core: &mut crate::core::CoreSession,
    session_id: &str,
    result: &ToolExecutionResult,
    correlation_id: Option<String>,
) -> Result<String, String> {
    let input = tool_execution_result_to_runner_input(result, correlation_id).to_string();
    core.process_tool_result_for_session(session_id, &input)
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/host.tests.rs"]
mod tests;
