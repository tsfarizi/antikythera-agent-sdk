#[test]
fn returns_not_found_when_config_file_missing() {
    let result = AppConfig::load(Some(Path::new("/nonexistent/path/app.toml")));
    assert!(matches!(result, Err(ConfigError::NotFound { .. })));
}
