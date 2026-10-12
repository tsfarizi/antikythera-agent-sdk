#[test]
/// Skip if the production config file is not present (e.g., in CI).
///
fn loads_actual_config_if_exists() {
    let config_path = Path::new("app.toml");

    if !config_path.exists() {
        eprintln!("Skipping: app.toml not found");
        return;
    }

    let config = AppConfig::load(Some(config_path)).expect("Failed to load actual config");

    assert!(!config.model_name().is_empty(), "model should not be empty");
    assert!(
        !config.default_provider().is_empty(),
        "default_provider should not be empty"
    );
}
