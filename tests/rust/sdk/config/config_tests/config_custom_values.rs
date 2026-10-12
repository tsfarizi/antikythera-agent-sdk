#[test]
fn test_config_with_custom_values() {
    let mut config = AppConfig::default();
    config.agent.max_steps = 20;
    config.agent.verbose = true;

    let toml_str = config_to_toml(&config).expect("Failed to serialize");
    let loaded = config_from_toml(&toml_str).expect("Failed to deserialize");

    assert_eq!(loaded.agent.max_steps, 20);
    assert!(loaded.agent.verbose);
}
