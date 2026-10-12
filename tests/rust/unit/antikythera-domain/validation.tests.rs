use super::*;

#[test]
fn valid_tool_names_accepted() {
    assert!(validate_tool_name("my_tool").is_ok());
    assert!(validate_tool_name("tool-name").is_ok());
    assert!(validate_tool_name("tool.name").is_ok());
    assert!(validate_tool_name("a").is_ok());
    assert!(validate_tool_name("search123").is_ok());
}

#[test]
fn empty_name_rejected() {
    assert!(validate_tool_name("").is_err());
}

#[test]
fn too_long_name_rejected() {
    let long = "a".repeat(129);
    assert!(validate_tool_name(&long).is_err());
}

#[test]
fn invalid_characters_rejected() {
    assert!(validate_tool_name("has space").is_err());
    assert!(validate_tool_name("has,comma").is_err());
    assert!(validate_tool_name("has@at").is_err());
}

#[test]
fn exactly_128_chars_accepted() {
    let name = "a".repeat(128);
    assert!(validate_tool_name(&name).is_ok());
}
