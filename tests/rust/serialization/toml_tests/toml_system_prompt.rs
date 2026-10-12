#[test]
fn to_raw_toml_handles_system_prompt() {
    let config = AppConfig {
        system_prompt: Some("Be helpful and concise.".to_string()),
        ..AppConfig::default()
    };
    let raw = config.to_raw_toml();

    assert!(raw.contains("system_prompt = \"Be helpful and concise.\""));
}
