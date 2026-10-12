//! Integration test: `ToolPlugin` derive wired into `antikythera-toolrunner`.
//!
//! Exercises the full plugin lifecycle — typed definition deserialization,
//! handler invocation, static tool listing, and end-to-end registration in a
//! `ToolRunner` — proving the canonical `definition_json()` shape produced by
//! the derive is accepted by `ToolDefinition` and its registry validation.

use antikythera_macros::ToolPlugin;
use antikythera_toolrunner::ToolRunner;
use serde_json::json;

/// A plugin tool: metadata via `tool`/`tool_param`, handler and definition
/// type via `plugin`.
///
/// Fields exist only as macro metadata carriers — the derive reads their
/// names, types, and attributes; nothing reads their values.
#[derive(ToolPlugin)]
#[allow(dead_code)]
#[tool(name = "multiply", description = "Multiply two numbers")]
#[plugin(
    handler = "multiply_handler",
    definition = "antikythera_toolrunner::ToolDefinition"
)]
struct MultiplyTool {
    #[tool_param(description = "First")]
    a: i32,
    #[tool_param(description = "Second")]
    b: i32,
}

/// Handler bound by the `plugin` attribute. Signature must match
/// `ToolHandlerFn = fn(&serde_json::Value) -> Result<serde_json::Value, String>`.
fn multiply_handler(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a = args
        .get("a")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing or invalid `a`".to_string())?;
    let b = args
        .get("b")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing or invalid `b`".to_string())?;
    Ok(json!(a * b))
}

#[test]
fn plugin_definition_parses_as_tool_definition() {
    let def: antikythera_toolrunner::ToolDefinition = MultiplyTool::definition();
    assert_eq!(def.name, "multiply");
    assert_eq!(def.description, "Multiply two numbers");
    assert_eq!(def.required_params(), vec!["a", "b"]);
}

#[test]
fn plugin_definition_json_is_canonical() {
    let json = MultiplyTool::definition_json();
    assert_eq!(json["name"], "multiply");
    assert_eq!(json["description"], "Multiply two numbers");
    assert!(json["parameters"].as_array().unwrap().is_empty());
    assert_eq!(
        json["input_schema"]["required"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn plugin_invoke_calls_handler() {
    let result = MultiplyTool::invoke(json!({"a": 6, "b": 7}));
    assert_eq!(result, Ok(json!(42)));
}

#[test]
fn plugin_invoke_propagates_handler_error() {
    let result = MultiplyTool::invoke(json!({"a": 6}));
    assert!(result.is_err());
}

#[test]
fn plugin_tools_list_contains_tool_name() {
    assert_eq!(MultiplyTool::PLUGIN_TOOLS, &["multiply"]);
}

#[test]
fn plugin_registers_end_to_end() {
    let mut runner = ToolRunner::new();
    runner.register_tool(MultiplyTool::definition());
    runner.register_handler(MultiplyTool::TOOL_NAME, multiply_handler);

    let result = runner.execute("multiply", json!({"a": 6, "b": 7}));
    assert!(result.is_ok());

    let tool_result = result.unwrap();
    assert!(tool_result.success);
    assert_eq!(tool_result.output, json!(42));
}

#[test]
fn plugin_registration_validates_required_params() {
    let mut runner = ToolRunner::new();
    runner.register_tool(MultiplyTool::definition());
    runner.register_handler(MultiplyTool::TOOL_NAME, multiply_handler);

    let result = runner.execute("multiply", json!({"a": 6}));
    assert!(result.is_err());
}
