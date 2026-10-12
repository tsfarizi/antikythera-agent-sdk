//! Wasmtime component hosting: the `bindgen!` declaration for the
//! `server-runtime` world and the host state shared with the HTTP layer.
//!
//! The linker wiring mirrors `examples/component-harness`:
//! `bindgen!` plus the generated `ServerRuntime::add_to_linker` for the
//! host-facing interfaces, alongside `wasmtime_wasi::p2::add_to_linker_sync`
//! for WASI. The composite imports exactly one non-WASI interface
//! (`runtime-hooks`); the world also declares `host-imports` so the same
//! linker serves drop-in logic cores that use the escape hatch.

use std::sync::Arc;

use serde_json::Value;

wasmtime::component::bindgen!({
    path: "wit/server-runtime.wit",
    world: "server-runtime",
    require_store_data_send: true,
});

use crate::config::HookName;
use crate::control::{ControlChannel, PendingKind};
use crate::wire::PostbackBody;

/// Shared state between the wasmtime host functions (core thread) and the
/// HTTP/SSE layer (tokio workers).
pub struct SharedState {
    pub auth_token: Option<String>,
    pub allowed_origins: Vec<String>,
    pub engine: Arc<wasmtime::Engine>,
    pub policy: Arc<crate::config::GatePolicy>,
    pub control: Arc<ControlChannel>,
    pub providers: std::collections::HashMap<String, Arc<dyn crate::llm::LlmProvider>>,
    pub default_provider: String,
    pub router: Arc<crate::routing::ToolRouter>,
    /// Bounded `save-state`/`load-state` store: context-id → state JSON.
    pub storage: std::sync::Mutex<std::collections::HashMap<String, String>>,
    pub storage_capacity_bytes: usize,
    /// Per-session LLM call counters for the quota gate.
    pub llm_calls: std::sync::Mutex<std::collections::HashMap<String, u32>>,
    pub runtime: tokio::runtime::Handle,
    pub client_id: String,
    pub session_id: Option<String>,
    pub pending_ttl: std::time::Duration,
}

impl SharedState {
    /// Resolve a provider by request name; falls back to the default.
    pub fn resolve_provider(
        &self,
        name: Option<&str>,
    ) -> Result<Arc<dyn crate::llm::LlmProvider>, String> {
        let key = name.filter(|n| !n.is_empty());
        if let Some(key) = key {
            if let Some(provider) = self.providers.get(key) {
                return Ok(provider.clone());
            }
            if key != self.default_provider {
                return Err(format!("llm: provider '{key}' not configured"));
            }
        }
        self.providers
            .get(&self.default_provider)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "llm: default provider '{}' not configured",
                    self.default_provider
                )
            })
    }

    /// Apply the LLM quota gate for a session and consume one call slot.
    pub fn check_llm_gate(&self, session_id: Option<&str>) -> Result<(), String> {
        if self.policy.llm_quota.is_none() {
            return Ok(());
        }
        let key = session_id.unwrap_or("default").to_string();
        let mut calls = self.llm_calls.lock().expect("llm calls lock poisoned");
        if !calls.contains_key(&key) && calls.len() >= 1024 {
            return Err("permission: llm session capacity exceeded".to_string());
        }
        let used = calls.entry(key).or_insert(0);
        self.policy.check_llm(*used)?;
        *used += 1;
        Ok(())
    }

    /// Push a `hook-request` to the peer and block (on the core thread) for
    /// the POST-back decision. Fail-closed when the client is absent or the
    /// TTL expires: every failure starts with `permission:`.
    pub fn request_hook_decision(
        &self,
        hook: HookName,
        session_state_json: &str,
        input_json: &str,
    ) -> Result<String, String> {
        self.policy.check_hook(hook)?;
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let mut rx = self.control.register_pending(
            correlation_id.clone(),
            PendingKind::Hook,
            self.pending_ttl,
        );
        if !self.control.is_client_connected(&self.client_id) {
            self.control.cancel_pending(&correlation_id);
            return Err(format!(
                "permission: hook '{}' requires a connected client",
                hook.as_str()
            ));
        }
        let envelope = crate::control::ControlChannel::envelope(
            &self.client_id,
            self.session_id.as_deref(),
            correlation_id.clone(),
            "hook-request",
            ControlChannel::hook_payload(hook.as_str(), session_state_json, input_json),
        );
        self.control.push(&self.client_id, &envelope);

        let wait = self
            .runtime
            .block_on(async { tokio::time::timeout(self.pending_ttl, &mut rx).await });
        match wait {
            Ok(Ok(body)) => {
                if !body.ok {
                    return Err(normalize_denial(body.error.unwrap_or_else(|| {
                        format!("permission: hook '{}' rejected by client", hook.as_str())
                    })));
                }
                hook_decision_from_payload(body)
            }
            Ok(Err(_)) | Err(_) => {
                self.control.cancel_pending(&correlation_id);
                Err(format!(
                    "permission: hook '{}' timed out waiting for client response",
                    hook.as_str()
                ))
            }
        }
    }
}

/// Extract the hook decision string from a POST-back payload. The client may
/// send a JSON string (the exact WIT return value) or a JSON object (e.g.
/// `{"passthrough": true}`), which is re-encoded to the string form.
fn hook_decision_from_payload(body: PostbackBody) -> Result<String, String> {
    match &body.payload {
        Value::String(s) => Ok(s.clone()),
        other => serde_json::to_string(other)
            .map_err(|e| format!("permission: hook response payload is not a decision: {e}")),
    }
}

fn normalize_denial(message: String) -> String {
    if message.starts_with("permission:") {
        message
    } else {
        format!("permission: {message}")
    }
}

/// Host state for wasmtime: WASI context + resource table + the shared
/// runtime state.
pub struct HostState {
    pub ctx: wasmtime_wasi::WasiCtx,
    pub table: wasmtime_wasi::ResourceTable,
    pub shared: Arc<SharedState>,
}

impl wasmtime_wasi::WasiView for HostState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl antikythera::agent_sdk::vocabulary::Host for HostState {}
