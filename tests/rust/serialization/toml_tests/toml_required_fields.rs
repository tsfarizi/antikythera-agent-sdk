#[test]
fn to_raw_toml_contains_required_fields() {
    let config = AppConfig {
        system_prompt: Some("Be helpful and concise.".to_string()),
        ..AppConfig::default()
    };
    let raw = config.to_raw_toml();

    assert!(raw.contains("system_prompt = \"Be helpful and concise.\""));
    assert!(raw.contains("prompt_template"));
}
