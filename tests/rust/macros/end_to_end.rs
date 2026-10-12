//! End-to-end tests for antikythera-macros derive macros.
//!
//! These tests verify that the macros work correctly when used
//! from outside the macro crate (cross-crate boundary).

#![allow(dead_code)]

use antikythera_macros::{FsmComplete, JsonSchema, PortValidate, ToolDef, WasmBridge};
use serde::{Deserialize, Serialize};

/// Marker trait for types that have WASM bridge support.
/// The macro generates `impl WasmBridge for T` — this trait must be importable.
/// WasmBridge trait: must be in scope for the macro-generated impl.
///
pub trait WasmBridge: Sized {
    const WASM_TYPE_NAME: &'static str;
    const BRIDGE_TARGET: &'static str;
    fn to_json_value(&self) -> Result<serde_json::Value, String>;
    fn from_json_value(value: serde_json::Value) -> Result<Self, String>;
}

#[derive(ToolDef, Serialize)]
#[tool(name = "e2e_test_tool", description = "End-to-end test tool")]
struct E2eTestTool {
    #[tool_param(description = "Input text")]
    input: String,
    #[tool_param(description = "Optional count", required = false)]
    count: Option<i32>,
}

#[test]
fn e2e_tool_def_constants() {
    assert_eq!(E2eTestTool::TOOL_NAME, "e2e_test_tool");
    assert_eq!(E2eTestTool::TOOL_DESCRIPTION, "End-to-end test tool");
}

#[test]
fn e2e_tool_def_schema() {
    let schema = E2eTestTool::json_schema();
    assert!(schema.is_object());
    assert_eq!(schema["properties"]["input"]["type"], "string");
    assert_eq!(schema["properties"]["input"]["description"], "Input text");
    assert_eq!(schema["properties"]["count"]["type"], "integer");
    assert_eq!(
        schema["properties"]["count"]["description"],
        "Optional count"
    );

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("input")));
    assert!(!required.contains(&serde_json::json!("count")));
}

#[test]
fn e2e_tool_def_definition() {
    let def = E2eTestTool::definition();
    assert_eq!(def["name"], "e2e_test_tool");
    assert_eq!(def["description"], "End-to-end test tool");
    assert!(def["input_schema"].is_object());
}

#[derive(WasmBridge, Serialize, Deserialize)]
struct E2eBridge {
    name: String,
    value: i32,
}

#[test]
fn e2e_wasm_bridge_type_name() {
    assert_eq!(<E2eBridge as WasmBridge>::WASM_TYPE_NAME, "E2eBridge");
}

#[test]
fn e2e_wasm_bridge_target() {
    assert_eq!(<E2eBridge as WasmBridge>::BRIDGE_TARGET, "wasm");
}

#[test]
fn e2e_wasm_bridge_roundtrip() {
    let item = E2eBridge {
        name: "test".to_string(),
        value: 42,
    };
    let json = item.to_json_value().unwrap();
    assert_eq!(json["name"], "test");
    assert_eq!(json["value"], 42);

    let restored = E2eBridge::from_json_value(json).unwrap();
    assert_eq!(restored.name, "test");
    assert_eq!(restored.value, 42);
}

#[test]
fn e2e_wasm_bridge_invalid_json() {
    let json = serde_json::json!({"invalid": true});
    let result = E2eBridge::from_json_value(json);
    assert!(result.is_err());
}

#[derive(JsonSchema, Serialize)]
struct E2eConfig {
    name: String,
    #[serde(default)]
    optional: Option<i32>,
}

#[test]
fn e2e_json_schema_structure() {
    let schema = E2eConfig::json_schema();
    assert!(schema.is_object());
    assert_eq!(schema["type"], "object");
}

#[test]
fn e2e_json_schema_properties() {
    let schema = E2eConfig::json_schema();
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("name"));
    assert!(props.contains_key("optional"));
    assert_eq!(props["name"]["type"], "string");
    assert_eq!(props["optional"]["type"], "integer");
}

#[test]
fn e2e_json_schema_required() {
    let schema = E2eConfig::json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));
    assert!(!required.contains(&serde_json::json!("optional")));
}

#[derive(FsmComplete)]
#[fsm_transitions(
    A => [B],
    B => [C],
    C => [A]
)]
enum E2eFsm {
    A,
    B,
    C,
}

#[test]
/// If this compiles, FsmComplete validated the transition matrix at compile time.
///
fn e2e_fsm_compiles() {
    let _ = std::mem::size_of::<E2eFsm>();
}

#[test]
fn e2e_fsm_variants_exist() {
    let a = E2eFsm::A;
    let b = E2eFsm::B;
    let c = E2eFsm::C;
    assert!(matches!(a, E2eFsm::A));
    assert!(matches!(b, E2eFsm::B));
    assert!(matches!(c, E2eFsm::C));
}

trait E2ePort: Send + Sync {
    fn execute(&self, input: &str) -> String;
}

#[derive(PortValidate)]
#[implements(E2ePort)]
struct E2ePortImpl {
    label: String,
}

impl E2ePort for E2ePortImpl {
    fn execute(&self, input: &str) -> String {
        format!("{}: {}", self.label, input)
    }
}

#[test]
/// If this compiles, PortValidate confirmed E2ePortImpl implements E2ePort.
///
fn e2e_port_validate_compiles() {
    let impl_ = E2ePortImpl {
        label: "test".to_string(),
    };
    assert_eq!(impl_.execute("hello"), "test: hello");
}

#[derive(JsonSchema, Serialize, Deserialize)]
/// Cross-macro interaction: multiple derives on the same type.
///
struct E2eMultiDerive {
    key: String,
    #[serde(default)]
    value: Option<u64>,
}

#[test]
fn e2e_multi_derive_json_schema() {
    let schema = E2eMultiDerive::json_schema();
    assert!(schema.is_object());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("key")));
    assert!(!required.contains(&serde_json::json!("value")));
}

#[test]
fn e2e_multi_derive_serialization() {
    let item = E2eMultiDerive {
        key: "test".to_string(),
        value: Some(42),
    };
    let json = serde_json::to_value(&item).unwrap();
    assert_eq!(json["key"], "test");
    assert_eq!(json["value"], 42);

    let restored: E2eMultiDerive = serde_json::from_value(json).unwrap();
    assert_eq!(restored.key, "test");
    assert_eq!(restored.value, Some(42));
}

#[derive(ToolDef)]
#[tool(name = "contract_weather", description = "Get weather for a city")]
/// ToolDefinition contract: definition() must deserialize into the real consumer type used by the
/// WASM agent tool registry.
///
struct ContractWeatherTool {
    #[tool_param(description = "City name")]
    city: String,
    #[tool_param(description = "Units (metric/imperial)")]
    units: Option<String>,
}

#[test]
/// The canonical output must deserialize into the consumer's ToolDefinition without any field
/// loss: name + description are required, everything else (title, parameters, input_schema,
/// output_schema, annotations, execution) carries #`serde(default)`.
///
/// Required_params() is derived from input_schema`"required"`.
///
/// To_prompt_line() renders a reasonable prompt line with required markers.
///
fn tool_definition_parses_as_consumer_type() {
    let def = ContractWeatherTool::definition();

    let tool: antikythera_sdk::wasm_agent::types::tool_registry::ToolDefinition =
        serde_json::from_value(def).expect("definition() must parse as consumer ToolDefinition");

    assert_eq!(tool.name, "contract_weather");
    assert_eq!(tool.description, "Get weather for a city");
    assert!(tool.parameters.is_empty());

    let required = tool.required_params();
    assert!(required.contains(&"city"));
    assert!(!required.contains(&"units"));

    let line = tool.to_prompt_line();
    assert!(line.contains("contract_weather"));
    assert!(line.contains("city*"));
}
