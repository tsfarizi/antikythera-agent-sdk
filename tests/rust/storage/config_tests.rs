use antikythera_storage::config::StorageConfig;

#[test]
fn test_config_defaults() {
    let config = StorageConfig::default();

    assert_eq!(config.backend, "filesystem");
    assert_eq!(config.data_dir, std::path::PathBuf::from("./data/sessions"));
    assert_eq!(
        config.backup_dir,
        std::path::PathBuf::from("./data/backups")
    );
    assert_eq!(config.mode, "embedded");

    assert!(config.cache.enabled);
    assert_eq!(config.cache.max_sessions, 512);
    assert_eq!(config.cache.ttl_seconds, 3600);
    assert_eq!(config.cache.eviction_policy, "both");

    assert!(config.backup.enabled);
    assert_eq!(config.backup.mode, "realtime");
    assert_eq!(config.backup.sync_interval_seconds, 30);
    assert!(config.backup.verify_before_delete);

    assert_eq!(config.mongodb.uri, "mongodb://localhost:27017");
    assert_eq!(config.mongodb.database, "antikythera");
    assert_eq!(config.mongodb.collection, "sessions");

    assert_eq!(config.postgres.host, "localhost");
    assert_eq!(config.postgres.port, 5432);
    assert_eq!(config.postgres.database, "antikythera");
    assert_eq!(config.postgres.user, "postgres");

    assert!(!config.sse_backup.enabled);
    assert_eq!(config.sse_backup.bind, "0.0.0.0:8081");
}

#[test]
fn test_config_from_toml() {
    let toml_str = r#"
backend = "mongodb"
mode = "standalone"
data_dir = "/custom/data"
backup_dir = "/custom/backups"

[cache]
enabled = false
max_sessions = 256
ttl_seconds = 1800
eviction_policy = "lru"

[backup]
enabled = false
mode = "interval"
sync_interval_seconds = 60
verify_before_delete = false

[mongodb]
uri = "mongodb://remote:27017"
database = "mydb"
collection = "mycol"
"#;

    let config = StorageConfig::from_toml(toml_str).unwrap();

    assert_eq!(config.backend, "mongodb");
    assert_eq!(config.mode, "standalone");
    assert_eq!(config.data_dir, std::path::PathBuf::from("/custom/data"));
    assert_eq!(
        config.backup_dir,
        std::path::PathBuf::from("/custom/backups")
    );

    assert!(!config.cache.enabled);
    assert_eq!(config.cache.max_sessions, 256);
    assert_eq!(config.cache.ttl_seconds, 1800);
    assert_eq!(config.cache.eviction_policy, "lru");

    assert!(!config.backup.enabled);
    assert_eq!(config.backup.mode, "interval");
    assert_eq!(config.backup.sync_interval_seconds, 60);
    assert!(!config.backup.verify_before_delete);

    assert_eq!(config.mongodb.uri, "mongodb://remote:27017");
    assert_eq!(config.mongodb.database, "mydb");
    assert_eq!(config.mongodb.collection, "mycol");
}

#[test]
/// Empty TOML should use all defaults.
///
fn test_config_from_toml_minimal() {
    let config = StorageConfig::from_toml("").unwrap();
    assert_eq!(config.backend, "filesystem");
    assert_eq!(config.mode, "embedded");
}

#[test]
fn test_config_is_filesystem() {
    let config = StorageConfig::default();
    assert!(config.is_filesystem());
    assert!(!config.is_mongodb());
    assert!(!config.is_postgres());
}

#[test]
fn test_config_is_standalone() {
    let mut config = StorageConfig::default();
    assert!(!config.is_standalone());
    assert!(config.is_embedded());

    config.mode = "standalone".to_string();
    assert!(config.is_standalone());
    assert!(!config.is_embedded());
}

#[test]
fn test_config_invalid_toml() {
    let result = StorageConfig::from_toml("this is not valid {{{ toml");
    assert!(result.is_err());
}

#[test]
fn test_config_from_toml_postgres() {
    let toml_str = r#"
backend = "postgres"

[postgres]
host = "db.example.com"
port = 5433
database = "prod_db"
user = "admin"
password = "secret"
"#;

    let config = StorageConfig::from_toml(toml_str).unwrap();
    assert!(config.is_postgres());
    assert_eq!(config.postgres.host, "db.example.com");
    assert_eq!(config.postgres.port, 5433);
    assert_eq!(config.postgres.database, "prod_db");
    assert_eq!(config.postgres.user, "admin");
    assert_eq!(config.postgres.password, "secret");
}
