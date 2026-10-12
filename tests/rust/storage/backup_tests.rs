use std::sync::Arc;

use antikythera_storage::backend::StorageBackend;
use antikythera_storage::backend::filesystem::FilesystemBackend;
use antikythera_storage::backup::BackupCoordinator;
use antikythera_storage::backup::verifier;
use antikythera_storage::config::BackupConfig;
use tempfile::tempdir;

#[tokio::test]
/// Verify backup file content matches.
///
async fn test_backup_coordinator_backup() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = Arc::new(FilesystemBackend::new(&data_dir, &backup_dir).unwrap());

    let config = BackupConfig::default();
    let coordinator = BackupCoordinator::new(backend.clone(), config);

    let session_id = "backup-sess-001";
    let data = b"session payload".to_vec();

    coordinator.backup_session(session_id, &data).await.unwrap();

    let backup_path = backup_dir.join(format!("{session_id}.backup.json"));
    assert!(backup_path.exists());

    let contents = std::fs::read(&backup_path).unwrap();
    assert_eq!(contents, data);
}

#[tokio::test]
async fn test_backup_coordinator_is_enabled() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = Arc::new(FilesystemBackend::new(&data_dir, &backup_dir).unwrap());

    let mut config = BackupConfig::default();
    assert!(BackupCoordinator::new(backend.clone(), config.clone()).is_enabled());

    config.enabled = false;
    assert!(!BackupCoordinator::new(backend.clone(), config).is_enabled());
}

#[tokio::test]
/// Save to primary and create backup.
///
/// Verify sync (primary storage has it) then delete backup.
///
/// Backup should be gone, primary still intact.
///
async fn test_backup_verifier_verify_and_delete() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = Arc::new(FilesystemBackend::new(&data_dir, &backup_dir).unwrap());

    let session_id = "verify-sess-001";
    let data = b"verify me".to_vec();

    backend.save(session_id, &data).await.unwrap();
    backend.backup(session_id, &data).await.unwrap();

    let backup_path = backup_dir.join(format!("{session_id}.backup.json"));
    assert!(backup_path.exists());

    let result = verifier::verify_and_delete(backend.as_ref(), session_id)
        .await
        .unwrap();
    assert!(result);

    assert!(!backup_path.exists());
    let loaded = backend.load(session_id).await.unwrap();
    assert_eq!(loaded, Some(data));
}

#[tokio::test]
/// Before saving, verify_sync returns false.
///
async fn test_backup_verifier_verify_sync() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "sync-check";

    let result = verifier::verify_sync(&backend, session_id).await.unwrap();
    assert!(!result);

    backend.save(session_id, b"exists").await.unwrap();
    let result = verifier::verify_sync(&backend, session_id).await.unwrap();
    assert!(result);
}

#[tokio::test]
/// Verify_and_delete should return false since primary doesn't have it.
///
async fn test_backup_verifier_delete_when_not_synced() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "no-sync";
    backend.backup(session_id, b"only backup").await.unwrap();

    let result = verifier::verify_and_delete(&backend, session_id)
        .await
        .unwrap();
    assert!(!result);

    let backup_path = backup_dir.join(format!("{session_id}.backup.json"));
    assert!(backup_path.exists());
}

#[tokio::test]
async fn test_backup_coordinator_config_accessor() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = Arc::new(FilesystemBackend::new(&data_dir, &backup_dir).unwrap());

    let config = BackupConfig {
        mode: "interval".to_string(),
        sync_interval_seconds: 120,
        ..BackupConfig::default()
    };

    let coordinator = BackupCoordinator::new(backend, config.clone());
    assert_eq!(coordinator.config().mode, "interval");
    assert_eq!(coordinator.config().sync_interval_seconds, 120);
}
