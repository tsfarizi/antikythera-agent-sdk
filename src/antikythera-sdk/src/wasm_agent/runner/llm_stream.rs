//! LLM streaming pipeline for WASM agent runner.
//!
//! Handles chunked LLM response processing: preparing user turns, appending
//! streaming chunks, and committing the final LLM response into session state.
//!
//! See also: [`super::session_lifecycle`], [`super::tool_pipeline`].

use std::collections::HashMap;

use antikythera_log::LogLevel;

use super::runner_types::*;
use super::{AgentRunnerError, AgentRunnerRuntime, new_session_id, now_unix_ms, wasm_log};
use crate::wasm_agent::processor::{build_llm_messages, process_llm_response, validate_tool_call};
use crate::wasm_agent::types::{
    AgentAction, AgentFsmState, AgentMessage, StreamEventKind, ToolCall,
};

/// LLM streaming pipeline: prepare, chunk, and commit operations.
impl AgentRunnerRuntime {
    /// Prepares a user turn for LLM invocation.
    ///
    /// Validates the request, manages session lifecycle (restore detection,
    /// FSM transitions), builds the message list, and returns a serialized
    /// [`PreparedTurn`] that the host can hand to the LLM provider.
    /// Snapshot the tool block before the mutable session borrow to avoid borrow conflict.
    ///
    /// Validates FSM state for session restore consistency. Restored sessions may carry
    /// mid-operation states (e.g., LlmStreaming, ToolRequested, Final) which are invalid for a
    /// fresh user turn. Reset to Idle.
    ///
    /// FSM: Idle -> UserTurnPrepared -> LlmStreaming.
    ///
    /// Logic-hooks: `prepare-turn` is consulted after the SDK built its default prepared turn.
    /// Passthrough keeps the default byte-identical; an override's present fields are
    /// authoritative, absent fields fall back to the default. A hook error aborts the turn
    /// (fail-closed).
    ///
    pub(super) fn prepare_user_turn(
        &mut self,
        request_json: &str,
    ) -> Result<String, AgentRunnerError> {
        let started_ms = now_unix_ms();
        let input: PrepareUserTurnInput = serde_json::from_str(request_json).map_err(|e| {
            AgentRunnerError::ValidationFailed(format!("Invalid request-json: {e}"))
        })?;

        let now_ms = now_unix_ms();
        let _ = self.sweep_idle_sessions(now_ms)?;

        let tool_block_snapshot = self.known_tools.to_prompt_block();

        let session_id = input.session_id.clone().unwrap_or_else(new_session_id);
        wasm_log(&session_id, LogLevel::Info, "Preparing user turn");

        if !self.sessions.contains_key(&session_id)
            && self.archived_sessions.contains_key(&session_id)
        {
            let archived =
                self.archived_sessions
                    .get(&session_id)
                    .cloned()
                    .unwrap_or(ArchivedSessionRecord {
                        archived_at_ms: now_ms,
                        reason: "unknown".to_string(),
                    });
            self.emit_pending_event(
                &session_id,
                StreamEventKind::SessionRestoreRequested,
                input.correlation_id.clone(),
                serde_json::json!({
                    "reason": archived.reason,
                    "archived_at_ms": archived.archived_at_ms,
                }),
            );
            self.emit_pending_event(
                &session_id,
                StreamEventKind::SessionRestoreProgress,
                input.correlation_id.clone(),
                serde_json::json!({
                    "stage": "requested",
                    "percent": 0,
                    "message": "Host load_state required before this turn can continue"
                }),
            );
            wasm_log(
                &session_id,
                LogLevel::Warn,
                "Session archived, restore required before turn",
            );
            return Err(AgentRunnerError::SessionArchived(format!(
                "Session '{session_id}' archived and not in RAM"
            )));
        }

        let policy = self.resolve_policy(&input);
        let runtime = self.ensure_session(&session_id);
        runtime.touch(now_ms);

        let current_fsm = runtime.state.fsm_state;
        if current_fsm != AgentFsmState::Idle {
            wasm_log(
                &session_id,
                LogLevel::Warn,
                &format!(
                    "Session in non-idle FSM state '{}', resetting to idle for fresh turn",
                    current_fsm
                ),
            );
            runtime.state.fsm_state = AgentFsmState::Idle;
            runtime.emit_event(
                StreamEventKind::FsmStateChanged,
                input.correlation_id.clone(),
                serde_json::json!({
                    "previous_state": current_fsm,
                    "new_state": AgentFsmState::Idle,
                    "reason": "fresh_turn_reset",
                }),
            );
        }

        let fsm_before = runtime.state.fsm_state;
        if let Err(e) = runtime
            .state
            .fsm_state
            .transition_to(AgentFsmState::UserTurnPrepared)
        {
            wasm_log(
                &session_id,
                LogLevel::Warn,
                &format!("FSM transition failed: {e}"),
            );
        } else {
            wasm_log(
                &session_id,
                LogLevel::Debug,
                &format!(
                    "FSM transition: {} -> {}",
                    fsm_before, runtime.state.fsm_state
                ),
            );
            runtime.emit_event(
                StreamEventKind::FsmStateChanged,
                input.correlation_id.clone(),
                serde_json::json!({
                    "from": fsm_before.to_string(),
                    "to": runtime.state.fsm_state.to_string(),
                }),
            );
        }
        let fsm_before = runtime.state.fsm_state;
        if let Err(e) = runtime
            .state
            .fsm_state
            .transition_to(AgentFsmState::LlmStreaming)
        {
            wasm_log(
                &session_id,
                LogLevel::Warn,
                &format!("FSM transition failed: {e}"),
            );
        } else {
            wasm_log(
                &session_id,
                LogLevel::Debug,
                &format!(
                    "FSM transition: {} -> {}",
                    fsm_before, runtime.state.fsm_state
                ),
            );
            runtime.emit_event(
                StreamEventKind::FsmStateChanged,
                input.correlation_id.clone(),
                serde_json::json!({
                    "from": fsm_before.to_string(),
                    "to": runtime.state.fsm_state.to_string(),
                }),
            );
        }

        let summary = Self::maybe_update_summary(&mut runtime.state, &policy);
        if let Some(summary) = &summary {
            runtime.telemetry.counters.context_summaries += 1;
            runtime.emit_event(
                StreamEventKind::SummaryUpdated,
                input.correlation_id.clone(),
                serde_json::json!({
                    "version": summary.version,
                    "source_messages": summary.source_messages,
                }),
            );
        }

        let base_system_prompt = input.system_prompt.clone().unwrap_or_default();
        let system_prompt = if let Some(tool_block) = tool_block_snapshot {
            if base_system_prompt.is_empty() {
                tool_block
            } else {
                format!("{base_system_prompt}\n\n{tool_block}")
            }
        } else {
            base_system_prompt
        };
        let mut messages = build_llm_messages(&system_prompt, &runtime.state);
        messages.push(HashMap::from([
            ("role".to_string(), "user".to_string()),
            ("content".to_string(), input.prompt.clone()),
        ]));

        runtime.telemetry.counters.turns_prepared += 1;
        let prepare_latency_ms = (now_unix_ms() - started_ms) as u64;
        runtime.telemetry.total_prepare_latency_ms += prepare_latency_ms;
        runtime.prepare_latencies_ms.push(prepare_latency_ms);
        runtime.emit_event(
            StreamEventKind::UserTurnPrepared,
            input.correlation_id.clone(),
            serde_json::json!({
                "messages_count": messages.len(),
            }),
        );

        let prepared = PreparedTurn {
            session_id,
            step: runtime.state.current_step,
            prompt: input.prompt,
            system_prompt,
            force_json: input.force_json.unwrap_or(false),
            metadata_json: input.metadata_json,
            correlation_id: input.correlation_id,
            summary_handoff: summary.or_else(|| runtime.state.rolling_summary.clone()),
            messages_json: serde_json::to_string(&messages).map_err(|e| {
                AgentRunnerError::Internal(format!("Failed to encode messages_json: {e}"))
            })?,
        };

        let encoded = serde_json::to_string(&prepared).map_err(|e| {
            AgentRunnerError::Internal(format!("Failed to encode prepared turn: {e}"))
        })?;

        #[cfg(all(feature = "component", target_family = "wasm"))]
        let encoded = super::logic_hooks::apply_prepare_turn_override(
            &runtime.state,
            request_json,
            &encoded,
        )?;

        let _ =
            self.enforce_capacity(Some(&prepared.session_id), prepared.correlation_id.clone())?;

        Ok(encoded)
    }

    /// Appends a single LLM streaming chunk to the session's pending buffer.
    ///
    /// Transitions the FSM to `LlmStreaming` on the first chunk if the
    /// session is still in `UserTurnPrepared`. Returns `Ok(true)` on success.
    /// FSM: UserTurnPrepared -> LlmStreaming (first chunk signals streaming start).
    ///
    pub(super) fn append_llm_chunk(
        &mut self,
        session_id: &str,
        chunk: &str,
        correlation_id: Option<String>,
    ) -> Result<bool, AgentRunnerError> {
        let _ = self.sweep_idle_sessions(now_unix_ms())?;
        let runtime = self.ensure_session(session_id);
        runtime.touch(now_unix_ms());

        if runtime.state.fsm_state == AgentFsmState::UserTurnPrepared {
            let fsm_before = runtime.state.fsm_state;
            if let Err(e) = runtime
                .state
                .fsm_state
                .transition_to(AgentFsmState::LlmStreaming)
            {
                wasm_log(
                    session_id,
                    LogLevel::Warn,
                    &format!("FSM transition failed: {e}"),
                );
            } else {
                wasm_log(
                    session_id,
                    LogLevel::Debug,
                    &format!(
                        "FSM transition: {} -> {}",
                        fsm_before, runtime.state.fsm_state
                    ),
                );
                runtime.emit_event(
                    StreamEventKind::FsmStateChanged,
                    correlation_id.clone(),
                    serde_json::json!({
                        "from": fsm_before.to_string(),
                        "to": runtime.state.fsm_state.to_string(),
                    }),
                );
            }
        }

        runtime.pending_llm_chunks.push(chunk.to_string());
        runtime.telemetry.counters.llm_chunks += 1;
        runtime.emit_event(
            StreamEventKind::LlmChunk,
            correlation_id,
            serde_json::json!({"chunk": chunk}),
        );
        Ok(true)
    }

    /// Commits a complete LLM response into session state.
    ///
    /// Replays the user message, processes the LLM output via
    /// [`crate::wasm_agent::processor::process_llm_response`], handles FSM
    /// transitions, and returns a serialized [`CommitResult`] indicating
    /// whether the response is final, requests a tool call, or requires a retry.
    /// Snapshot the registry before the mutable session borrow to avoid borrow conflict.
    ///
    /// FSM guard: ensure we're in LlmStreaming state before processing LLM response.
    ///
    /// FSM transitions are handled inside process_llm_response (processor). Runner only ensures
    /// the correct pre-condition (LlmStreaming) via the guard above.
    ///
    /// Validates the tool call against the registered registry (no-op if empty).
    ///
    /// Logic-hooks: `decide-action` is consulted before the default action decision is finalized.
    /// Passthrough keeps the SDK default; an override is committed as the action result (e.g.
    /// forcing `final` by returning the SDK's envelope). A hook error aborts (fail-closed).
    ///
    /// After NLL, `runtime`'s borrow on `self` has ended. Try in-process execution for builtin
    /// tools via toolrunner.
    ///
    /// Component path: execute builtin tools through the imported `tool-registry` interface.
    /// `wasm-tools compose` supplies the implementation when the composite is assembled; until
    /// then the Native builds with `feature = "component"` (e.g. the native test suites) compile
    /// the wit-bindgen shims but have no wired host for the `tool-registry` import; calling them
    /// aborts via `unreachable!()`. The import only exists on wasm targets, so the block is gated
    /// on the target family as well: native tool calls are delegated to the host (pre-component
    /// behavior), wasm components execute builtins.
    ///
    pub(super) fn commit_llm_response(
        &mut self,
        prepared_turn_json: &str,
        llm_response_json: &str,
    ) -> Result<String, AgentRunnerError> {
        let _ = self.sweep_idle_sessions(now_unix_ms())?;
        let started_ms = now_unix_ms();
        let prepared: PreparedTurn = serde_json::from_str(prepared_turn_json).map_err(|e| {
            AgentRunnerError::ValidationFailed(format!("Invalid prepared-turn-json: {e}"))
        })?;

        wasm_log(
            &prepared.session_id,
            LogLevel::Debug,
            "Committing LLM response",
        );

        let registry_snapshot = self.known_tools.clone();

        let runtime = self.ensure_session(&prepared.session_id);
        runtime.touch(now_unix_ms());
        runtime.state.add_message(AgentMessage {
            role: "user".to_string(),
            content: prepared.prompt,
            tool_call: None,
            tool_result: None,
        });

        if runtime.state.fsm_state != AgentFsmState::LlmStreaming {
            let fsm_before = runtime.state.fsm_state;
            wasm_log(
                &prepared.session_id,
                LogLevel::Warn,
                &format!(
                    "FSM guard: state is '{}' expected 'llm_streaming', forcing transition",
                    fsm_before
                ),
            );
            let _ = runtime
                .state
                .fsm_state
                .transition_to(AgentFsmState::LlmStreaming);
            wasm_log(
                &prepared.session_id,
                LogLevel::Debug,
                &format!(
                    "FSM transition (forced): {} -> {}",
                    fsm_before, runtime.state.fsm_state
                ),
            );
        }

        let action = process_llm_response(&mut runtime.state, llm_response_json)?;
        runtime.telemetry.counters.llm_commits += 1;
        let commit_latency_ms = (now_unix_ms() - started_ms) as u64;
        runtime.telemetry.total_commit_latency_ms += commit_latency_ms;
        runtime.commit_latencies_ms.push(commit_latency_ms);
        runtime.emit_event(
            StreamEventKind::LlmCommitted,
            prepared.correlation_id.clone(),
            serde_json::json!({"length": llm_response_json.len()}),
        );

        let result = match action {
            AgentAction::Final { response } => {
                let content = if let Some(text) = response.as_str() {
                    text.to_string()
                } else {
                    response.to_string()
                };

                runtime.state.add_message(AgentMessage {
                    role: "assistant".to_string(),
                    content: content.clone(),
                    tool_call: None,
                    tool_result: None,
                });

                runtime.telemetry.counters.final_responses += 1;
                runtime.emit_event(
                    StreamEventKind::FinalResponse,
                    prepared.correlation_id.clone(),
                    serde_json::json!({"content": content}),
                );

                CommitResult {
                    session_id: runtime.state.session_id.clone(),
                    step: runtime.state.current_step,
                    action: "final".to_string(),
                    content: Some(content),
                    tool_name: None,
                    tool_input: None,
                    fsm_state: runtime.state.fsm_state.to_string(),
                }
            }
            AgentAction::CallTool { tool, input } => {
                if let Err(validation_err) = validate_tool_call(&registry_snapshot, &tool, &input) {
                    wasm_log(
                        &prepared.session_id,
                        LogLevel::Error,
                        &format!("Tool validation failed for '{tool}': {validation_err}"),
                    );
                    return Err(AgentRunnerError::ToolFailed(validation_err.to_string()));
                }

                runtime.state.add_message(AgentMessage {
                    role: "assistant".to_string(),
                    content: format!("call_tool:{}", tool),
                    tool_call: Some(ToolCall {
                        name: tool.clone(),
                        arguments: input.clone(),
                        step_id: runtime.state.current_step,
                    }),
                    tool_result: None,
                });

                runtime.telemetry.counters.tool_requests += 1;
                runtime.emit_event(
                    StreamEventKind::ToolRequested,
                    prepared.correlation_id.clone(),
                    serde_json::json!({"tool": tool, "input": input}),
                );

                CommitResult {
                    session_id: runtime.state.session_id.clone(),
                    step: runtime.state.current_step,
                    action: "call_tool".to_string(),
                    content: None,
                    tool_name: Some(tool),
                    tool_input: Some(input),
                    fsm_state: runtime.state.fsm_state.to_string(),
                }
            }
            AgentAction::Retry { error } => {
                runtime.telemetry.counters.llm_retries += 1;
                CommitResult {
                    session_id: runtime.state.session_id.clone(),
                    step: runtime.state.current_step,
                    action: "retry".to_string(),
                    content: Some(error),
                    tool_name: None,
                    tool_input: None,
                    fsm_state: runtime.state.fsm_state.to_string(),
                }
            }
        };

        #[cfg(all(feature = "component", target_family = "wasm"))]
        let result = super::logic_hooks::apply_decide_action_override(
            &runtime.state,
            llm_response_json,
            &result,
        )?;

        wasm_log(
            &prepared.session_id,
            LogLevel::Debug,
            &format!("LLM response committed: action={}", result.action),
        );
        runtime.pending_llm_chunks.clear();

        #[cfg(feature = "toolrunner")]
        {
            if result.action == "call_tool" {
                if let Some(ref runner) = self.toolrunner {
                    let tool = result.tool_name.as_deref().unwrap_or("");
                    let input_val = result
                        .tool_input
                        .as_ref()
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    if let Ok(Some(tr)) = runner.try_execute(tool, input_val) {
                        wasm_log(
                            &prepared.session_id,
                            LogLevel::Debug,
                            &format!("Tool '{}' executed in-process by toolrunner", tool),
                        );
                        self.emit_pending_event(
                            &prepared.session_id,
                            StreamEventKind::ToolResult,
                            prepared.correlation_id.clone(),
                            serde_json::json!({
                                "tool": tr.name,
                                "success": tr.success,
                            }),
                        );
                    }
                }
            }
        }

        #[cfg(all(feature = "component", target_family = "wasm"))]
        {
            if result.action == "call_tool" {
                let tool = result.tool_name.as_deref().unwrap_or("");
                let input_val = result
                    .tool_input
                    .as_ref()
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let arguments_json = input_val.to_string();
                let step_id = result.step;
                match crate::wasm_exports::antikythera::agent_sdk::tool_registry::execute_builtin(
                    tool,
                    &arguments_json,
                    step_id,
                ) {
                    Ok(tool_result_json) => {
                        match serde_json::from_str::<crate::wasm_agent::types::ToolResult>(
                            &tool_result_json,
                        ) {
                            Ok(tr) => {
                                wasm_log(
                                    &prepared.session_id,
                                    LogLevel::Debug,
                                    &format!("Tool '{}' executed by tool-registry component", tool),
                                );
                                self.emit_pending_event(
                                    &prepared.session_id,
                                    StreamEventKind::ToolResult,
                                    prepared.correlation_id.clone(),
                                    serde_json::json!({
                                        "tool": tr.name,
                                        "success": tr.success,
                                    }),
                                );
                            }
                            Err(e) => {
                                wasm_log(
                                    &prepared.session_id,
                                    LogLevel::Error,
                                    &format!(
                                        "Tool '{}' returned malformed result from tool-registry: {e}",
                                        tool
                                    ),
                                );
                                return Err(AgentRunnerError::ToolFailed(format!(
                                    "Tool '{tool}' returned malformed result from tool-registry: {e}"
                                )));
                            }
                        }
                    }
                    Err(msg) if msg.contains("requires host execution") => {
                        wasm_log(
                            &prepared.session_id,
                            LogLevel::Debug,
                            &format!("Tool '{}' delegated to host (not builtin)", tool),
                        );
                    }
                    Err(msg) => {
                        wasm_log(
                            &prepared.session_id,
                            LogLevel::Error,
                            &format!("Tool '{}' rejected by tool-registry: {msg}", tool),
                        );
                        return Err(AgentRunnerError::ToolFailed(msg));
                    }
                }
            }
        }

        serde_json::to_string(&result)
            .map_err(|e| AgentRunnerError::Internal(format!("Failed to encode commit result: {e}")))
    }

    /// Joins all pending LLM chunks and commits them as a single response.
    ///
    /// Convenience wrapper over [`Self::commit_llm_response`] for hosts that
    /// accumulate chunks before committing.
    pub(super) fn commit_llm_stream(
        &mut self,
        prepared_turn_json: &str,
    ) -> Result<String, AgentRunnerError> {
        let prepared: PreparedTurn = serde_json::from_str(prepared_turn_json).map_err(|e| {
            AgentRunnerError::ValidationFailed(format!("Invalid prepared-turn-json: {e}"))
        })?;

        let runtime = self.ensure_session(&prepared.session_id);
        let payload = runtime.pending_llm_chunks.join("");
        self.commit_llm_response(prepared_turn_json, &payload)
    }

    /// Processes an LLM response without committing to session history.
    ///
    /// Similar to [`Self::commit_llm_response`] but does not persist the
    /// user message or the assistant reply into `message_history`. Useful
    /// for dry-run or validation-only flows.
    /// FSM guard: ensure we're in LlmStreaming state before processing.
    ///
    /// Logic-hooks: `decide-action` is consulted before the default action derivation runs.
    /// Passthrough keeps the SDK default; an override is committed as the action result (the same
    /// envelope the SDK produces). A hook error aborts the operation (fail-closed).
    ///
    /// Composed provider passed through. Precedence A1a: consult the host-supplied
    /// `runtime-hooks` at the same pipeline point; it is skipped entirely when
    /// `runtime_hooks_enabled` is false.
    ///
    pub(super) fn process_llm_response(
        &mut self,
        session_id: &str,
        llm_response_json: &str,
    ) -> Result<String, AgentRunnerError> {
        let _ = self.sweep_idle_sessions(now_unix_ms())?;
        let runtime = self.ensure_session(session_id);
        runtime.touch(now_unix_ms());
        wasm_log(session_id, LogLevel::Debug, "Processing LLM response");

        if runtime.state.fsm_state != AgentFsmState::LlmStreaming {
            let fsm_before = runtime.state.fsm_state;
            wasm_log(
                session_id,
                LogLevel::Warn,
                &format!(
                    "FSM guard: state is '{}' expected 'llm_streaming', forcing transition",
                    fsm_before
                ),
            );
            let _ = runtime
                .state
                .fsm_state
                .transition_to(AgentFsmState::LlmStreaming);
            wasm_log(
                session_id,
                LogLevel::Debug,
                &format!(
                    "FSM transition (forced): {} -> {}",
                    fsm_before, runtime.state.fsm_state
                ),
            );
        }

        #[cfg(all(feature = "component", target_family = "wasm"))]
        if let Some(override_value) =
            super::logic_hooks::decide_action_override(&runtime.state, llm_response_json)?
        {
            return serde_json::to_string(&override_value)
                .map_err(|e| AgentRunnerError::Internal(format!("Failed to encode action: {e}")));
        }
        #[cfg(all(feature = "component", target_family = "wasm"))]
        if let Some(override_value) =
            super::runtime_hooks::decide_action_raw_override(&runtime.state, llm_response_json)?
        {
            return serde_json::to_string(&override_value)
                .map_err(|e| AgentRunnerError::Internal(format!("Failed to encode action: {e}")));
        }

        let action = process_llm_response(&mut runtime.state, llm_response_json)?;
        serde_json::to_string(&action)
            .map_err(|e| AgentRunnerError::Internal(format!("Failed to encode action: {e}")))
    }
}
