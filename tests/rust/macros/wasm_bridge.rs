use antikythera_macros::WasmBridge;
use serde::{Deserialize, Serialize};

/// Marker trait for types that have WASM bridge support.
///
/// Types implementing this trait can be converted between native Rust
/// and WASM-compatible representations via JSON serialization.
pub trait WasmBridge: Sized {
    /// The name of this type on the WASM side.
    const WASM_TYPE_NAME: &'static str;

    /// The bridge target this type is bound to.
    const BRIDGE_TARGET: &'static str;

    /// Convert from the core representation to a JSON-friendly form.
    fn to_json_value(&self) -> Result<serde_json::Value, String>;

    /// Convert from a JSON-friendly form back to the core representation.
    fn from_json_value(value: serde_json::Value) -> Result<Self, String>;
}

#[derive(WasmBridge, Serialize, Deserialize)]
struct TestBridge {
    name: String,
    value: i32,
}

#[derive(WasmBridge, Serialize, Deserialize)]
#[bridge(target = "wasm")]
struct TestBridgeExplicit {
    name: String,
    value: i32,
}

#[derive(WasmBridge, Serialize, Deserialize)]
enum TestEnum {
    Unit,
    Tuple(String, i32),
    Struct { x: f64, y: f64 },
}

#[test]
fn test_wasm_type_name() {
    assert_eq!(<TestBridge as WasmBridge>::WASM_TYPE_NAME, "TestBridge");
}

#[test]
fn test_wasm_type_name_explicit_target() {
    assert_eq!(
        <TestBridgeExplicit as WasmBridge>::WASM_TYPE_NAME,
        "TestBridgeExplicit"
    );
    assert_eq!(<TestBridgeExplicit as WasmBridge>::BRIDGE_TARGET, "wasm");
}

#[test]
fn test_default_bridge_target() {
    assert_eq!(<TestBridge as WasmBridge>::BRIDGE_TARGET, "wasm");
}

#[test]
fn test_to_json_value() {
    let item = TestBridge {
        name: "test".to_string(),
        value: 42,
    };
    let json = item.to_json_value().unwrap();
    assert_eq!(json["name"], "test");
    assert_eq!(json["value"], 42);
}

#[test]
fn test_from_json_value() {
    let json = serde_json::json!({"name": "test", "value": 42});
    let item = TestBridge::from_json_value(json).unwrap();
    assert_eq!(item.name, "test");
    assert_eq!(item.value, 42);
}

#[test]
fn test_roundtrip() {
    let original = TestBridge {
        name: "roundtrip".to_string(),
        value: 99,
    };
    let json = original.to_json_value().unwrap();
    let restored = TestBridge::from_json_value(json).unwrap();
    assert_eq!(original.name, restored.name);
    assert_eq!(original.value, restored.value);
}

#[test]
fn test_enum_roundtrip() {
    let original = TestEnum::Struct { x: 1.5, y: 2.5 };
    let json = original.to_json_value().unwrap();
    let restored = TestEnum::from_json_value(json).unwrap();
    match restored {
        TestEnum::Struct { x, y } => {
            assert!((x - 1.5).abs() < f64::EPSILON);
            assert!((y - 2.5).abs() < f64::EPSILON);
        }
        _ => panic!("Expected Struct variant"),
    }
}

#[test]
fn test_from_json_value_invalid() {
    let json = serde_json::json!({"invalid": true});
    let result = TestBridge::from_json_value(json);
    assert!(result.is_err());
}
