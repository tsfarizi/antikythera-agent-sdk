//! Wit-bindgen emits pre-2024-edition export shims; the unsafe-op-in- unsafe-fn lint fires inside
//! the generated code, not our wrapper.
//!
//! Export layer is wasm-gated; dead on native targets.
//!
//! Wasm-only: WIT export names break rustc's ld version script on native cdylibs.
//!
//! WIT export layer for the `logic-hooks-component` world.
//!
//! Reference default hooks: every exported call returns
//! `Ok({"passthrough": true})`, the exact single-key object the SDK
//! interprets as "use the SDK default behavior" (see the `logic-hooks`
//! contract in `wit/antikythera.wit`). Hooks are stateless decisions — the
//! input JSON is read, never persisted or mutated, and the hook never imports
//! back from the SDK. The default is a constant, so the arguments are
//! intentionally unused (`_`-prefixed).
//!
//! Pattern follows `antikythera-toolrunner/src/component.rs`:
//! `wit_bindgen::generate!` against the root wit, a unit struct implementing
//! `exports::antikythera::agent_sdk::<interface>::Guest`, and `export!`.

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

wit_bindgen::generate!({
    world: "logic-hooks-component",
    path: "../../../wit/antikythera.wit",
});

/// The SDK's passthrough signal: use default behavior at this pipeline point.
const PASSTHROUGH_JSON: &str = "{\"passthrough\": true}";

struct DefaultHooks;

impl exports::antikythera::agent_sdk::logic_hooks::Guest for DefaultHooks {
    fn prepare_turn(_request_json: String, _session_state_json: String) -> Result<String, String> {
        Ok(PASSTHROUGH_JSON.to_string())
    }

    fn decide_action(
        _session_state_json: String,
        _llm_response_json: String,
    ) -> Result<String, String> {
        Ok(PASSTHROUGH_JSON.to_string())
    }

    fn handle_tool_result(
        _session_state_json: String,
        _tool_result_json: String,
    ) -> Result<String, String> {
        Ok(PASSTHROUGH_JSON.to_string())
    }
}

#[cfg(target_family = "wasm")]
export!(DefaultHooks);
