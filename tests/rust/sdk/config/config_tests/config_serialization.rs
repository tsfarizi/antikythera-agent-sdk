#[test]
fn test_config_serialization_roundtrip() {
    let config = AppConfig::default();

    let toml_str = config_to_toml(&config).expect("Failed to serialize");
    let loaded = config_from_toml(&toml_str).expect("Failed to deserialize");

    assert_eq!(config.agent.max_steps, loaded.agent.max_steps);
    assert_eq!(config.agent.verbose, loaded.agent.verbose);
}
