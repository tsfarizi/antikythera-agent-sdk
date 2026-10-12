#[test]
fn to_raw_toml_includes_prompt_template() {
    let config = AppConfig::default();
    let raw = config.to_raw_toml();

    assert!(raw.contains("prompt_template"));
    assert!(!raw.contains("system_prompt"));
}
