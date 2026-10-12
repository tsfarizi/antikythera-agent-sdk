#[test]
fn rotate_secret_returns_newest() {
    let manager = SecretManager::from_config().unwrap();
    manager.store_secret("k4", b"v1").unwrap();
    manager.rotate_secret("k4", b"v2").unwrap();
    manager.rotate_secret("k4", b"v3").unwrap();
    assert_eq!(manager.get_secret("k4").unwrap(), b"v3");
}

#[test]
/// Should only keep v2 and v3.
///
fn max_versions_enforced() {
    let config = SecretsConfig {
        enable_versioning: true,
        max_versions: 2,
        ..Default::default()
    };
    let manager = SecretManager::new(config).unwrap();
    manager.store_secret("k5", b"v1").unwrap();
    manager.rotate_secret("k5", b"v2").unwrap();
    manager.rotate_secret("k5", b"v3").unwrap();
    let meta = manager.get_metadata("k5").unwrap();
    assert!(meta.version >= 2);
}

#[test]
fn needs_rotation_returns_false_initially() {
    let manager = SecretManager::from_config().unwrap();
    manager.store_secret("k6", b"v").unwrap();
    assert!(!manager.needs_rotation("k6").unwrap());
}
