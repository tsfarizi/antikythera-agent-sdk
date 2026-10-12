//! Domain-native dynamic value type.
//!
//! Replaces serde_json::Value in domain entities so the domain ring
//! has zero dependency on serialization frameworks.

use serde::{Deserialize, Serialize};

/// A dynamic value that can represent any JSON-like structure.
/// This is the domain's own representation — adapters convert
/// to/from serde_json::Value at the infrastructure boundary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DynamicValue {
    #[default]
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<DynamicValue>),
    Object(Vec<(String, DynamicValue)>),
}

impl DynamicValue {
    pub fn is_null(&self) -> bool {
        matches!(self, DynamicValue::Null)
    }
    pub fn as_str(&self) -> Option<&str> {
        if let DynamicValue::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let DynamicValue::Number(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let DynamicValue::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }
    pub fn as_array(&self) -> Option<&[DynamicValue]> {
        if let DynamicValue::Array(a) = self {
            Some(a)
        } else {
            None
        }
    }
    pub fn as_object(&self) -> Option<&[(String, DynamicValue)]> {
        if let DynamicValue::Object(o) = self {
            Some(o)
        } else {
            None
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, DynamicValue::Array(_))
    }
    pub fn is_object(&self) -> bool {
        matches!(self, DynamicValue::Object(_))
    }

    pub fn get(&self, key: &str) -> Option<&DynamicValue> {
        if let DynamicValue::Object(pairs) = self {
            pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/dynamic_value.tests.rs"]
mod tests;
