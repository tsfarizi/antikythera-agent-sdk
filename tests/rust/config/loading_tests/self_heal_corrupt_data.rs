#[test]
/// Corrupt data triggers SchemaChanged error — backup is saved, caller decides recovery.
///
fn self_heals_when_toml_data_is_corrupt() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("app.toml");
    fs::write(&path, "this is not valid TOML {{{").expect("write");

    let result = AppConfig::load(Some(&path));
    match result {
        Err(ConfigError::SchemaChanged { backup_path, .. }) => {
            assert!(backup_path.exists(), "backup file should be created");
        }
        other => panic!(
            "expected SchemaChanged error for corrupt config, got: {:?}",
            other
        ),
    }
}
