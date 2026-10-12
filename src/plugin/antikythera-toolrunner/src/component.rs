//! Wit-bindgen emits pre-2024-edition export shims; the unsafe-op-in- unsafe-fn lint fires inside
//! the generated code, not our wrapper.
//!
//! Export layer is wasm-gated; dead on native targets.
//!
//! Wasm-only: WIT export names break rustc's ld version script on native cdylibs.
//!
//! WIT export layer for the `tool-registry-component` world.
//!
//! Thin adapter between the WIT world
//! `antikythera:agent-sdk/tool-registry-component` (interface `tool-registry`)
//! and the existing `crate::runner::ToolRunner` plus the `crate::wasm` bridge
//! (`handle_tool_call`, `tool_result_to_json`). No business logic lives here:
//! JSON strings pass through unmodified and `ToolRunnerError` is flattened to
//! `String` via `Display`.
//!
//! Pattern follows `antikythera-sdk/src/wasm_exports.rs`:
//! `wit_bindgen::generate!` against the root wit, a unit struct implementing
//! `exports::antikythera::agent_sdk::<interface>::Guest`, and `export!`.

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

wit_bindgen::generate!({
    world: "tool-registry-component",
    path: "../../../wit/antikythera.wit",
});

use std::sync::OnceLock;

use crate::error::ToolRunnerError;
use crate::runner::ToolRunner;
use crate::types::ToolDefinition;
use crate::wasm::{handle_tool_call, tool_result_to_json};

/// Shared, immutable runner initialized exactly once per component instance.
///
/// The component world carries no state (D4): every exported call is
/// self-contained, so a single `ToolRunner` created at first use suffices —
/// no mutation after init, no locking on the call path.
fn runner() -> &'static ToolRunner {
    static RUNNER: OnceLock<ToolRunner> = OnceLock::new();
    RUNNER.get_or_init(init_runner)
}

/// Reference builtin: `echo` — returns the arguments object verbatim as output.
///
/// Deterministic, pure, no I/O. Registered so `execute-builtin` has a proven
/// non-host execution path in the standalone component; a registry-only
/// component would force every call to the host and leave the builtin lane
/// unverifiable.
fn echo_handler(arguments: &serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(arguments.clone())
}

/// Register_handler keeps the existing definition (no required params), so echo accepts any
/// arguments object.
///
fn init_runner() -> ToolRunner {
    let mut runner = ToolRunner::new();
    runner.register_tool(ToolDefinition {
        name: "echo".to_string(),
        title: Some("Echo".to_string()),
        description: "Reference builtin: returns the arguments object verbatim as JSON output."
            .to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true,
        })),
        ..Default::default()
    });
    runner.register_handler("echo", echo_handler);
    runner
}

struct ToolRegistryComponent;

impl exports::antikythera::agent_sdk::tool_registry::Guest for ToolRegistryComponent {
    fn list_tools_json() -> Result<String, String> {
        runner().registry().to_json().map_err(|e| e.to_string())
    }

    /// ToolRegistry::validate_call keeps its empty-registry accept-all guard untouched; with the
    /// reference builtin registered, unknown tools are rejected here (documented Err case of the
    /// interface).
    ///
    fn validate_tool_call(tool_name: String, arguments_json: String) -> Result<(), String> {
        let arguments: serde_json::Value = serde_json::from_str(&arguments_json)
            .map_err(|e| format!("invalid arguments JSON: {e}"))?;
        runner()
            .registry()
            .validate_call(&tool_name, &arguments)
            .map_err(|e| e.to_string())
    }

    /// Non-builtin tools MUST surface the delegation signal, not a registry NotFound rejection:
    /// execute() validates against the registry (populated by echo) before the handler lookup, so
    /// the builtin check is hoisted here — the flow documented in wasm.rs.
    ///
    fn execute_builtin(
        tool_name: String,
        arguments_json: String,
        step_id: u32,
    ) -> Result<String, String> {
        if !runner().is_builtin(&tool_name) {
            return Err(ToolRunnerError::HostRequired {
                tool: tool_name.clone(),
            }
            .to_string());
        }

        let arguments: serde_json::Value = serde_json::from_str(&arguments_json)
            .map_err(|e| format!("invalid arguments JSON: {e}"))?;
        let result = handle_tool_call(runner(), &tool_name, arguments, step_id)
            .map_err(|e| e.to_string())?;
        Ok(tool_result_to_json(&result))
    }
}

#[cfg(target_family = "wasm")]
export!(ToolRegistryComponent);
