#![allow(dead_code)]

use antikythera_macros::JsonSchema;

#[derive(JsonSchema)]
struct PrimitiveStruct {
    name: String,
    count: i32,
    score: f64,
    active: bool,
}

#[test]
/// All fields should be required (no #`serde(default)`).
///
fn test_primitive_types() {
    let schema = PrimitiveStruct::json_schema();
    assert!(schema.is_object());
    assert_eq!(schema["type"], "object");

    let props = schema["properties"].as_object().unwrap();
    assert_eq!(props["name"]["type"], "string");
    assert_eq!(props["count"]["type"], "integer");
    assert_eq!(props["score"]["type"], "number");
    assert_eq!(props["active"]["type"], "boolean");

    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 4);
    assert!(required.contains(&serde_json::json!("name")));
    assert!(required.contains(&serde_json::json!("count")));
    assert!(required.contains(&serde_json::json!("score")));
    assert!(required.contains(&serde_json::json!("active")));
}

#[derive(JsonSchema)]
struct WithOption {
    required_field: String,
    optional_field: Option<String>,
}

#[test]
/// Option<String> should resolve to {"type": "string"}.
///
fn test_option_not_required() {
    let schema = WithOption::json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("required_field")));
    assert!(!required.contains(&serde_json::json!("optional_field")));

    assert_eq!(schema["properties"]["optional_field"]["type"], "string");
}

#[derive(JsonSchema)]
struct WithVec {
    tags: Vec<String>,
    scores: Vec<i32>,
}

#[test]
fn test_vec_array() {
    let schema = WithVec::json_schema();
    assert_eq!(schema["properties"]["tags"]["type"], "array");
    assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
    assert_eq!(schema["properties"]["scores"]["type"], "array");
    assert_eq!(schema["properties"]["scores"]["items"]["type"], "integer");
}

#[derive(JsonSchema)]
struct WithDefault {
    name: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    count: Option<i32>,
}

#[test]
fn test_serde_default_not_required() {
    let schema = WithDefault::json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));
    assert!(!required.contains(&serde_json::json!("enabled")));
    assert!(!required.contains(&serde_json::json!("count")));
}

#[derive(JsonSchema)]
struct Inner {
    value: String,
}

#[derive(JsonSchema)]
struct Outer {
    name: String,
    inner: Inner,
}

#[test]
/// Inner should be a $ref.
///
fn test_nested_struct_ref() {
    let schema = Outer::json_schema();

    assert_eq!(schema["properties"]["inner"]["$ref"], "#/definitions/Inner");

    let defs = schema["definitions"].as_object().unwrap();
    assert!(defs.contains_key("Inner"));
    assert_eq!(defs["Inner"]["type"], "object");
}

#[derive(JsonSchema)]
struct ServerConfig {
    host: String,
}

#[derive(JsonSchema)]
struct DbConfig {
    url: String,
}

#[derive(JsonSchema)]
struct AppConfig {
    server: ServerConfig,
    db: DbConfig,
    name: String,
}

#[test]
fn test_multiple_nested_types() {
    let schema = AppConfig::json_schema();
    let defs = schema["definitions"].as_object().unwrap();
    assert!(defs.contains_key("ServerConfig"));
    assert!(defs.contains_key("DbConfig"));

    assert_eq!(
        schema["properties"]["server"]["$ref"],
        "#/definitions/ServerConfig"
    );
    assert_eq!(schema["properties"]["db"]["$ref"], "#/definitions/DbConfig");
}

#[derive(JsonSchema)]
struct ProviderConfig {
    name: String,
}

#[derive(JsonSchema)]
struct WithNestedVec {
    providers: Vec<ProviderConfig>,
}

#[test]
fn test_vec_of_nested() {
    let schema = WithNestedVec::json_schema();
    assert_eq!(schema["properties"]["providers"]["type"], "array");
    assert_eq!(
        schema["properties"]["providers"]["items"]["$ref"],
        "#/definitions/ProviderConfig"
    );

    let defs = schema["definitions"].as_object().unwrap();
    assert!(defs.contains_key("ProviderConfig"));
}

#[derive(JsonSchema)]
struct WithOptionalNested {
    name: String,
    #[serde(default)]
    config: Option<ServerConfig>,
}

#[test]
/// Option<ServerConfig> should resolve to $ref.
///
/// Config should NOT be in required.
///
fn test_option_of_nested() {
    let schema = WithOptionalNested::json_schema();

    assert_eq!(
        schema["properties"]["config"]["$ref"],
        "#/definitions/ServerConfig"
    );

    let required = schema["required"].as_array().unwrap();
    assert!(!required.contains(&serde_json::json!("config")));

    let defs = schema["definitions"].as_object().unwrap();
    assert!(defs.contains_key("ServerConfig"));
}

#[derive(JsonSchema)]
struct WithOptionalNestedVec {
    items: Option<Vec<WithOptionalNested>>,
}

#[test]
/// Option<Vec<T>> resolves to an array whose items carry the $ref.
///
/// The only field is optional, so the required list is omitted entirely.
///
fn test_option_of_vec_of_nested() {
    let schema = WithOptionalNestedVec::json_schema();

    assert_eq!(schema["properties"]["items"]["type"], "array");
    assert_eq!(
        schema["properties"]["items"]["items"]["$ref"],
        "#/definitions/WithOptionalNested"
    );

    assert!(schema.get("required").is_none());

    let defs = schema["definitions"].as_object().unwrap();
    assert!(defs.contains_key("WithOptionalNested"));
}

#[derive(JsonSchema)]
struct IntegerTypes {
    a: i8,
    b: i16,
    c: i32,
    d: i64,
    e: u8,
    f: u16,
    g: u32,
    h: u64,
}

#[test]
fn test_integer_variants() {
    let schema = IntegerTypes::json_schema();
    let props = schema["properties"].as_object().unwrap();
    for (_name, prop) in props {
        assert_eq!(prop["type"], "integer");
    }
}

#[derive(JsonSchema)]
struct RestServerConfig {
    host: String,
    port: u16,
}

#[derive(JsonSchema)]
struct ModelConfig {
    name: String,
    temperature: f64,
}

#[derive(JsonSchema)]
struct SecurityConfig {
    enabled: bool,
}

#[derive(JsonSchema)]
struct FullExample {
    server: RestServerConfig,
    providers: Vec<ProviderConfig>,
    model: ModelConfig,
    #[serde(default)]
    security: SecurityConfig,
    #[serde(default)]
    custom: Option<String>,
}

#[test]
fn test_full_spec_example() {
    let schema = FullExample::json_schema();

    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 3);
    assert!(required.contains(&serde_json::json!("server")));
    assert!(required.contains(&serde_json::json!("providers")));
    assert!(required.contains(&serde_json::json!("model")));

    assert!(!required.contains(&serde_json::json!("security")));
    assert!(!required.contains(&serde_json::json!("custom")));

    assert_eq!(
        schema["properties"]["server"]["$ref"],
        "#/definitions/RestServerConfig"
    );
    assert_eq!(schema["properties"]["providers"]["type"], "array");
    assert_eq!(
        schema["properties"]["providers"]["items"]["$ref"],
        "#/definitions/ProviderConfig"
    );
    assert_eq!(
        schema["properties"]["model"]["$ref"],
        "#/definitions/ModelConfig"
    );
    assert_eq!(
        schema["properties"]["security"]["$ref"],
        "#/definitions/SecurityConfig"
    );
    assert_eq!(schema["properties"]["custom"]["type"], "string");

    let defs = schema["definitions"].as_object().unwrap();
    assert!(defs.contains_key("RestServerConfig"));
    assert!(defs.contains_key("ProviderConfig"));
    assert!(defs.contains_key("ModelConfig"));
    assert!(defs.contains_key("SecurityConfig"));
}
