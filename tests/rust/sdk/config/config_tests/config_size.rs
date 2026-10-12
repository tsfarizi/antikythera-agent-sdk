#[test]
/// TOML should produce reasonably small output.
///
/// Should be under 10KB for default config.
///
fn test_config_size() {
    let config = AppConfig::default();
    let toml_str = config_to_toml(&config).expect("Failed to serialize");

    assert!(!toml_str.is_empty());
    assert!(toml_str.len() < 10000);
}
