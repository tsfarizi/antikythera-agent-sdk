use super::*;

#[test]
fn default_is_null() {
    assert!(DynamicValue::Null.is_null());
    assert!(DynamicValue::default().is_null());
}

#[test]
fn accessors_return_correct_variants() {
    assert_eq!(DynamicValue::Bool(true).as_bool(), Some(true));
    assert_eq!(DynamicValue::Number(1.5).as_f64(), Some(1.5));
    assert_eq!(DynamicValue::String("hi".into()).as_str(), Some("hi"));
    let arr = DynamicValue::Array(vec![DynamicValue::Null]);
    assert!(arr.as_array().is_some());
    let obj = DynamicValue::Object(vec![("k".into(), DynamicValue::Number(1.0))]);
    assert!(obj.as_object().is_some());
}

#[test]
fn get_on_object_finds_key() {
    let obj = DynamicValue::Object(vec![
        ("a".into(), DynamicValue::Number(1.0)),
        ("b".into(), DynamicValue::String("x".into())),
    ]);
    assert_eq!(obj.get("a").and_then(|v| v.as_f64()), Some(1.0));
    assert_eq!(obj.get("b").and_then(|v| v.as_str()), Some("x"));
    assert!(obj.get("c").is_none());
}

#[test]
/// Null, Bool, Number, and String roundtrip cleanly through JSON.
///
fn serialization_roundtrip_simple_variants() {
    let cases: Vec<DynamicValue> = vec![
        DynamicValue::Null,
        DynamicValue::Bool(true),
        DynamicValue::Bool(false),
        DynamicValue::Number(42.0),
        DynamicValue::String("hello".into()),
        DynamicValue::Array(vec![
            DynamicValue::Number(1.0),
            DynamicValue::String("two".into()),
        ]),
    ];
    for val in &cases {
        let json = serde_json::to_string(val).unwrap();
        let restored: DynamicValue = serde_json::from_str(&json).unwrap();
        assert_eq!(*val, restored, "roundtrip failed for {:?}", val);
    }
}
