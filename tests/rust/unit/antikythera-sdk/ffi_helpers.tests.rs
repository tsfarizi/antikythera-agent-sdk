use super::*;

#[test]
fn to_c_string_and_back_roundtrip() {
    let original = "hello world";
    let ptr = to_c_string(original);
    assert!(!ptr.is_null());
    let result = from_c_string(ptr).unwrap();
    unsafe {
        mcp_free_string(ptr);
    }
    assert_eq!(result, original);
}

#[test]
fn from_c_string_null_returns_error() {
    let result = from_c_string(std::ptr::null());
    assert!(result.is_err());
}

#[test]
fn error_response_produces_valid_json() {
    let ptr = error_response("something went wrong");
    assert!(!ptr.is_null());
    let json_str = from_c_string(ptr).unwrap();
    unsafe {
        mcp_free_string(ptr);
    }
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["success"], false);
    assert_eq!(parsed["error"], "something went wrong");
}

#[test]
fn serialize_result_produces_valid_json() {
    let data = serde_json::json!({"status": "ok", "count": 42});
    let ptr = serialize_result(&data);
    assert!(!ptr.is_null());
    let json_str = from_c_string(ptr).unwrap();
    unsafe {
        mcp_free_string(ptr);
    }
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["count"], 42);
}
