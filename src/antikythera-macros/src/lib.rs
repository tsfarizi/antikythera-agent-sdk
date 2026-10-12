//! # Antikythera Macros
//!
//! Proc macros for compile-time reflection and type-safe code generation.
//!
//! ## Available Derives
//!
//! - `ToolDef` — Generate MCP tool definitions from struct field metadata
//! - `ToolPlugin` — Generate a self-contained tool plugin (definition + handler invocation)
//! - `WasmBridge` — Generate WASM↔Core type bridging
//! - `JsonSchema` — Generate JSON Schema from struct definitions
//! - `FsmComplete` — Validate FSM transition completeness
//! - `PortValidate` — Validate port trait implementations at compile time
//!
//! Implementation logic lives in per-derive modules:
//! `tool_def`, `tool_plugin`, `wasm_bridge`, `json_schema`,
//! `fsm_complete`, `port_validate`, with shared helpers in
//! `type_mapping` and `attr_utils`.

mod attr_utils;
mod fsm_complete;
mod json_schema;
mod port_validate;
mod tool_def;
mod tool_plugin;
mod type_mapping;
mod wasm_bridge;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derive macro that generates MCP tool definitions from struct field metadata.
///
/// # Example
///
/// ```ignore
/// #[derive(ToolDef, Serialize)]
/// #[tool(name = "get_weather", description = "Get weather for a city")]
/// struct WeatherTool {
///     #[tool_param(description = "City name")]
///     city: String,
///
///     #[tool_param(description = "Units", required = false)]
///     units: Option<String>,
/// }
/// ```
///
/// Generates:
/// - `TOOL_NAME: &str` — the tool name constant
/// - `TOOL_DESCRIPTION: &str` — the tool description constant
/// - `json_schema() -> serde_json::Value` — the JSON Schema for input parameters
/// - `definition() -> serde_json::Value` — the full MCP tool definition
#[proc_macro_derive(ToolDef, attributes(tool, tool_param))]
pub fn derive_tool_def(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match tool_def::impl_tool_def(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive macro that generates a self-contained tool plugin: a tool
/// definition plus an in-process handler invocation.
///
/// A `ToolPlugin` struct pairs `ToolDef`'s metadata attributes (`tool`,
/// `tool_param`) with a `plugin` attribute that carries the handler path and
/// the host-facing definition type:
///
/// ```ignore
/// #[derive(ToolPlugin)]
/// #[tool(name = "multiply", description = "Multiply two numbers")]
/// #[plugin(handler = "multiply_handler", definition = "antikythera_toolrunner::ToolDefinition")]
/// struct MultiplyTool {
///     #[tool_param(description = "First")]
///     a: i32,
///     #[tool_param(description = "Second")]
///     b: i32,
/// }
///
/// fn multiply_handler(args: &serde_json::Value) -> Result<serde_json::Value, String> { ... }
/// ```
///
/// Generates:
/// - `TOOL_NAME: &'static str`, `TOOL_DESCRIPTION: &'static str`
/// - `json_schema() -> serde_json::Value` — the JSON Schema for input parameters
/// - `definition_json() -> serde_json::Value` — the canonical definition shape
/// - `definition() -> #definition_type` — canonical shape deserialized into the
///   configured definition type (default `serde_json::Value`)
/// - `invoke(args: serde_json::Value) -> Result<serde_json::Value, String>` —
///   calls the configured handler
/// - `PLUGIN_TOOLS: &[&str]` — the tools exported by this plugin
///
/// `handler` is required and must name a function of the shape
/// `fn(&serde_json::Value) -> Result<serde_json::Value, String>`; a missing or
/// malformed `plugin` attribute is a compile error.
///
/// # Conflict with `ToolDef`
///
/// Both derives emit the same constants and methods (`TOOL_NAME`,
/// `TOOL_DESCRIPTION`, `json_schema()`, `definition()`), so a struct must not
/// derive `ToolDef` and `ToolPlugin` simultaneously. `ToolPlugin` is a full
/// replacement for `ToolDef` when the tool carries its own handler.
#[proc_macro_derive(ToolPlugin, attributes(tool, tool_param, plugin))]
pub fn derive_tool_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match tool_plugin::impl_tool_plugin(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive macro that generates WASM↔Core type bridging code.
///
/// Converts between native Rust types and their WASM-compatible representations,
/// handling serialization boundaries and memory ownership transfer.
///
/// # Generated Items
///
/// - An `impl WasmBridge for YourType` block providing JSON serialization helpers
///
/// # Example
///
/// ```ignore
/// #[derive(WasmBridge, Serialize, Deserialize)]
/// #[bridge(target = "wasm")]
/// pub struct ToolCall {
///     pub name: String,
///     pub arguments: serde_json::Value,
/// }
/// ```
///
/// The `WasmBridge` trait must be in scope where this macro is used.
/// In production, it is defined in `antikythera-core` at
/// `antikythera_core::infrastructure::wasm_bridge::WasmBridge` and
/// re-exported at the crate root as `antikythera_core::WasmBridge`.
#[proc_macro_derive(WasmBridge, attributes(bridge))]
pub fn derive_wasm_bridge(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match wasm_bridge::impl_wasm_bridge(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive macro that generates JSON Schema from struct definitions.
///
/// Produces a `json_schema()` method returning the schema as a `serde_json::Value`,
/// useful for runtime type validation and tool parameter documentation.
///
/// # Requirements
///
/// The struct must have named fields. The generated code requires `serde_json` in scope.
///
/// # Rules
///
/// - Fields without `#[serde(default)]` and without `Option<T>` are marked as required.
/// - Primitive types map to their JSON Schema equivalents.
/// - `Vec<T>` maps to `{"type": "array", "items": <schema of T>}`.
/// - `Option<T>` maps to the inner type's schema and is never required.
/// - Nested structs generate `$ref` entries with placeholder definitions.
///
/// # Example
///
/// ```ignore
/// #[derive(JsonSchema)]
/// struct Config {
///     name: String,
///     #[serde(default)]
///     enabled: bool,
/// }
///
/// // Config::json_schema() returns:
/// // {
/// //   "type": "object",
/// //   "properties": {
/// //     "name": {"type": "string"},
/// //     "enabled": {"type": "boolean"}
/// //   },
/// //   "required": ["name"],
/// //   "definitions": {}
/// // }
/// ```
#[proc_macro_derive(JsonSchema, attributes(serde))]
pub fn derive_json_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match json_schema::impl_json_schema(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive macro that validates FSM transition completeness at compile time.
///
/// Ensures every state has a defined transition to all other required states,
/// preventing runtime errors from missing state machine paths.
///
/// # Example
///
/// ```ignore
/// #[derive(FsmComplete)]
/// #[fsm_transitions(
///     Idle => [UserTurnPrepared],
///     UserTurnPrepared => [LlmStreaming],
///     LlmStreaming => [LlmCommitted],
///     LlmCommitted => [ToolRequested, Final, Idle],
///     ToolRequested => [ToolResultProcessed],
///     ToolResultProcessed => [LlmStreaming, Final, Idle],
///     Final => [Idle]
/// )]
/// pub enum AgentFsmState {
///     Idle,
///     UserTurnPrepared,
///     LlmStreaming,
///     LlmCommitted,
///     ToolRequested,
///     ToolResultProcessed,
///     Final,
/// }
/// ```
#[proc_macro_derive(FsmComplete, attributes(fsm_transitions))]
pub fn derive_fsm_complete(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match fsm_complete::impl_fsm_complete(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive macro that generates compile-time validation for port trait implementations.
///
/// Generates a compile-time assertion that the annotated struct implements
/// the specified port trait. If any required method is missing, the compiler
/// will produce an error at the derive site.
///
/// # Example
///
/// ```ignore
/// #[derive(PortValidate)]
/// #[implements(ModelClient)]
/// pub struct OllamaClient { ... }
/// ```
///
/// This generates:
///
/// ```ignore
/// const _: () = {
///     fn _assert_port<T: ?Sized + ModelClient>() {}
///     fn _assert_impl() {
///         _assert_port::<OllamaClient>();
///     }
/// };
/// ```
#[proc_macro_derive(PortValidate, attributes(implements))]
pub fn derive_port_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match port_validate::impl_port_validate(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
