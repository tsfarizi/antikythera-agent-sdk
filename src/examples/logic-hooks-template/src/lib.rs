//! # logic-hooks-template
//!
//! Starter crate for a host-authored `logic-hooks` component
//! (`antikythera:agent-sdk/logic-hooks`, world `logic-hooks-component`).
//!
//! ## How to author a hook
//!
//! Edit ONLY the three `custom_*` functions below — the SDK and the WIT
//! export adapter in `component.rs` are not touched:
//!
//! - return `None` -> passthrough: the SDK keeps its default behavior at
//!   this pipeline point (byte-identical to `plugin/antikythera-default-hooks`);
//! - return `Some(json)` -> override: the returned JSON object replaces the
//!   SDK default decision (contract in `wit/antikythera.wit`);
//! - to abort the operation (Err-path), return an error from the matching
//!   `Guest` method in `component.rs` — the `custom_*` functions have no
//!   error channel by design.
//!
//! Build the component:
//!
//! ```text
//! cargo build -p logic-hooks-template --release --target wasm32-wasip2 --no-default-features --features component
//! ```
//!
//! The artifact exports `antikythera:agent-sdk/logic-hooks@1.0.0` and can be
//! composed into the SDK with `wasm-tools compose`.

/// Custom `prepare-turn` behavior.
///
/// Applications implement this hook to override the SDK decision at this pipeline point.
/// `None` = passthrough (SDK default prepared turn).
/// `Some(json)` = override; the object is merged over the SDK default
/// prepared turn (hook fields are authoritative).
pub fn custom_prepare_turn(_request_json: &str, _session_state_json: &str) -> Option<String> {
    None
}

/// Custom `decide-action` behavior.
///
/// Applications implement this hook to override the SDK decision at this pipeline point.
/// `None` = passthrough (SDK default action decision).
/// `Some(json)` = override; the object is merged over the SDK default
/// commit-result envelope (`action`, `content`, ... are authoritative).
pub fn custom_decide_action(_session_state_json: &str, _llm_response_json: &str) -> Option<String> {
    None
}

/// Custom `handle-tool-result` behavior.
///
/// Applications implement this hook to override the SDK decision at this pipeline point.
/// `None` = passthrough (SDK default tool-result processing).
/// `Some(json)` = override; the object replaces the reported tool result.
pub fn custom_handle_tool_result(
    _session_state_json: &str,
    _tool_result_json: &str,
) -> Option<String> {
    None
}

/// WIT export layer for the component world (`logic-hooks-component`).
#[cfg(feature = "component")]
pub mod component;
