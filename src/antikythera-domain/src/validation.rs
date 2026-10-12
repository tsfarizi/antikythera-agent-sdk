//! Domain validation rules.
//!
//! Business rules that validate inputs against the MCP specification.

/// Validates a tool name against the MCP spec naming rules:
/// - Length 1-128 characters
/// - Allowed characters: A-Z, a-z, 0-9, underscore (_), hyphen (-), dot (.)
/// - No spaces, commas, or other special characters
pub fn validate_tool_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("tool name must not be empty".to_string());
    }
    if name.len() > 128 {
        return Err(format!(
            "tool name exceeds 128 characters (length: {})",
            name.len()
        ));
    }
    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' && ch != '.' {
            return Err(format!(
                "tool name contains invalid character '{}'. Allowed: A-Z, a-z, 0-9, underscore, hyphen, dot",
                ch
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/validation.tests.rs"]
mod tests;
