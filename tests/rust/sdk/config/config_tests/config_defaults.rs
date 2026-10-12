#[test]
fn test_config_default_values() {
    let config = AppConfig::default();

    assert_eq!(config.agent.max_steps, 10);
    assert!(!config.agent.verbose);
    assert!(config.agent.auto_execute_tools);
}
