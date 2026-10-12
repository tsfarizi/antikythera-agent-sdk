use antikythera_macros::ToolDef;
use serde::Serialize;

#[derive(ToolDef, Serialize)]
#[tool(name = "test_tool", description = "A test tool")]
struct TestTool {
    #[tool_param(description = "Input text")]
    input: String,
    #[tool_param(description = "Optional count", required = false)]
    count: Option<i32>,
}

#[test]
fn test_tool_name() {
    assert_eq!(TestTool::TOOL_NAME, "test_tool");
}

#[test]
fn test_tool_description() {
    assert_eq!(TestTool::TOOL_DESCRIPTION, "A test tool");
}

#[test]
fn test_tool_schema() {
    let schema = TestTool::json_schema();
    assert!(schema.is_object());
    assert_eq!(schema["properties"]["input"]["type"], "string");
    assert_eq!(schema["properties"]["input"]["description"], "Input text");
    assert_eq!(schema["properties"]["count"]["type"], "integer");
    assert_eq!(
        schema["properties"]["count"]["description"],
        "Optional count"
    );
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("input"))
    );
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("count"))
    );
}

#[test]
fn test_tool_definition() {
    let def = TestTool::definition();
    assert_eq!(def["name"], "test_tool");
    assert_eq!(def["description"], "A test tool");
    assert!(def["input_schema"].is_object());
    assert!(def["parameters"].as_array().unwrap().is_empty());
}

#[derive(Serialize)]
#[allow(dead_code)]
struct ToolTargetConfig {
    host: String,
}

#[derive(ToolDef, Serialize)]
#[tool(
    name = "custom_ref_tool",
    description = "Tool with a custom nested type"
)]
struct CustomRefTool {
    #[tool_param(description = "Target configuration")]
    target: ToolTargetConfig,
    #[tool_param(description = "Optional overrides")]
    overrides: Option<ToolTargetConfig>,
}

#[test]
/// Custom struct type maps to $ref, not the old "string" default.
///
/// Option<Custom> resolves through the same mapping.
///
/// Placeholder definition is emitted alongside the $ref.
///
/// Target is required; overrides (Option) is not.
///
fn test_custom_type_field_is_ref() {
    let schema = CustomRefTool::json_schema();

    assert_eq!(
        schema["properties"]["target"]["$ref"],
        "#/definitions/ToolTargetConfig"
    );
    assert_eq!(
        schema["properties"]["overrides"]["$ref"],
        "#/definitions/ToolTargetConfig"
    );

    let defs = schema["definitions"].as_object().unwrap();
    assert_eq!(defs["ToolTargetConfig"]["type"], "object");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("target")));
    assert!(!required.contains(&serde_json::json!("overrides")));
}

#[derive(ToolDef, Serialize)]
#[tool(name = "default_tool", description = "Tool with a serde default field")]
struct DefaultTool {
    #[tool_param(description = "Name")]
    name: String,
    #[tool_param(description = "Timeout in seconds")]
    #[serde(default)]
    timeout: u32,
}

#[test]
fn test_serde_default_not_required() {
    let schema = DefaultTool::json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));
    assert!(!required.contains(&serde_json::json!("timeout")));
    assert_eq!(schema["properties"]["timeout"]["type"], "integer");
}

#[derive(ToolDef, Serialize)]
#[tool(
    name = "override_tool",
    description = "Tool with explicit required override"
)]
struct OverrideTool {
    #[tool_param(description = "Retry limit", required = true)]
    #[serde(default)]
    retries: u32,
}

#[test]
fn test_explicit_required_overrides_serde_default() {
    let schema = OverrideTool::json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("retries")));
}
