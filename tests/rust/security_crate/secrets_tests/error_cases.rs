#[test]
fn secret_not_found() {
    let manager = SecretManager::from_config().unwrap();
    assert!(matches!(manager.get_secret("nonexistent"), Err(SecretManagerError::SecretNotFound(_))));
}

#[test]
fn disabled_secrets_reject_store() {
    let config = SecretsConfig { enabled: false, ..Default::default() };
    let manager = SecretManager::new(config).unwrap();
    assert!(matches!(manager.store_secret("k", b"v"), Err(SecretManagerError::InvalidConfig(_))));
}

#[test]
fn rotation_on_nonexistent_secret() {
    let manager = SecretManager::from_config().unwrap();
    assert!(matches!(manager.rotate_secret("nonexistent", b"v"), Err(SecretManagerError::SecretNotFound(_))));
}
