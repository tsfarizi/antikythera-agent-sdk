//! Integration tests verifying `antikythera-macros` derives work across crate boundaries
//! when used within the `antikythera-ports` crate's test context.

#![allow(dead_code)]

use antikythera_macros::{JsonSchema, PortValidate, ToolDef};
use antikythera_ports::types::{LogEntry, LogFilter};
use antikythera_ports::{AppLogger, LogProvider, LogQueryPort};

#[derive(PortValidate)]
#[implements(AppLogger)]
struct ValidatedLogger;

impl AppLogger for ValidatedLogger {
    fn log_info(&self, _message: String) {}
    fn log_warn(&self, _message: String) {}
    fn log_error(&self, _message: String) {}
    fn log_debug(&self, _message: String) {}
}

#[test]
/// If this compiles, PortValidate confirmed ValidatedLogger implements AppLogger.
///
fn port_validate_compiles_with_app_logger() {
    let logger = ValidatedLogger;
    logger.log_info("test".to_string());
}

#[derive(ToolDef, serde::Serialize)]
#[tool(
    name = "ports_test_tool",
    description = "Tool defined in ports integration test"
)]
/// ToolDef: verify macro works in ports crate context.
///
struct PortsTestTool {
    #[tool_param(description = "Query string")]
    query: String,
    #[tool_param(description = "Max results", required = false)]
    max_results: Option<u32>,
}

#[test]
fn tool_def_cross_crate_constants() {
    assert_eq!(PortsTestTool::TOOL_NAME, "ports_test_tool");
    assert_eq!(
        PortsTestTool::TOOL_DESCRIPTION,
        "Tool defined in ports integration test"
    );
}

#[test]
fn tool_def_cross_crate_schema() {
    let schema = PortsTestTool::json_schema();
    assert!(schema.is_object());
    assert_eq!(schema["properties"]["query"]["type"], "string");
    assert_eq!(schema["properties"]["max_results"]["type"], "integer");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("query")));
    assert!(!required.contains(&serde_json::json!("max_results")));
}

#[test]
fn tool_def_cross_crate_definition() {
    let def = PortsTestTool::definition();
    assert_eq!(def["name"], "ports_test_tool");
    assert!(def["input_schema"].is_object());
}

#[derive(ToolDef)]
#[tool(
    name = "ports_custom_tool",
    description = "Tool with a cross-crate custom field"
)]
/// ToolDef struct with a cross-crate custom field type: maps to $ref.
///
struct PortsCustomTool {
    #[tool_param(description = "Log filter to apply")]
    filter: LogFilter,
}

#[test]
fn tool_def_custom_type_is_ref() {
    let schema = PortsCustomTool::json_schema();
    assert_eq!(
        schema["properties"]["filter"]["$ref"],
        "#/definitions/LogFilter"
    );

    let defs = schema["definitions"].as_object().unwrap();
    assert_eq!(defs["LogFilter"]["type"], "object");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("filter")));
}

#[derive(ToolDef, serde::Serialize)]
#[tool(
    name = "ports_default_tool",
    description = "Tool with a serde default field"
)]
/// ToolDef struct with a #`serde(default)` non-Option field: not required.
///
struct PortsDefaultTool {
    #[tool_param(description = "Query string")]
    query: String,
    #[tool_param(description = "Limit")]
    #[serde(default)]
    limit: u32,
}

#[test]
fn tool_def_serde_default_not_required() {
    let schema = PortsDefaultTool::json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("query")));
    assert!(!required.contains(&serde_json::json!("limit")));
    assert_eq!(schema["properties"]["limit"]["type"], "integer");
}

#[derive(JsonSchema, serde::Serialize)]
/// JsonSchema: verify macro works in ports crate context.
///
struct PortsConfig {
    host: String,
    port: u16,
    #[serde(default)]
    debug: bool,
}

#[test]
fn json_schema_cross_crate() {
    let schema = PortsConfig::json_schema();
    assert!(schema.is_object());
    assert_eq!(schema["properties"]["host"]["type"], "string");
    assert_eq!(schema["properties"]["port"]["type"], "integer");
    assert_eq!(schema["properties"]["debug"]["type"], "boolean");

    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("host")));
    assert!(required.contains(&serde_json::json!("port")));
    assert!(!required.contains(&serde_json::json!("debug")));
}

#[test]
/// Trait accessibility: verify port traits are importable and well-formed.
///
/// Compile-time checks that port traits are importable and have expected shape.
///
fn port_traits_are_accessible() {
    fn _assert_app_logger<T: AppLogger>() {}
    fn _assert_log_provider<T: LogProvider>() {}
    fn _assert_log_query<T: LogQueryPort>() {}
}

#[test]
/// Verify that ports crate types depend on domain types correctly.
///
fn port_types_use_domain_types() {
    let entry = LogEntry::new(antikythera_ports::types::LogLevel::Info, "integration test");
    assert_eq!(entry.level, antikythera_ports::types::LogLevel::Info);
    assert_eq!(entry.message, "integration test");

    let filter = LogFilter::new()
        .min_level(antikythera_ports::types::LogLevel::Warn)
        .limit(10);
    assert!(!filter.matches(&entry));
}
