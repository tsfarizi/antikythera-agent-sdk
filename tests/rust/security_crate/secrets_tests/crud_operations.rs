#[test]
fn store_and_retrieve_secret() {
    let manager = SecretManager::from_config().unwrap();
    manager.store_secret("k1", b"secret-value").unwrap();
    let retrieved = manager.get_secret("k1").unwrap();
    assert_eq!(retrieved, b"secret-value");
}

#[test]
fn delete_secret() {
    let manager = SecretManager::from_config().unwrap();
    manager.store_secret("k2", b"value").unwrap();
    manager.delete_secret("k2").unwrap();
    assert!(manager.get_secret("k2").is_err());
}

#[test]
fn list_secrets() {
    let manager = SecretManager::from_config().unwrap();
    manager.store_secret("a", b"1").unwrap();
    manager.store_secret("b", b"2").unwrap();
    let keys = manager.list_secrets();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"a".to_string()));
    assert!(keys.contains(&"b".to_string()));
}

#[test]
fn get_metadata() {
    let manager = SecretManager::from_config().unwrap();
    manager.store_secret("k3", b"v").unwrap();
    let meta = manager.get_metadata("k3").unwrap();
    assert_eq!(meta.id, "k3");
    assert_eq!(meta.version, 1);
    assert!(meta.active);
}
