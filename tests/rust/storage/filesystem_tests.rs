use antikythera_storage::backend::StorageBackend;
use antikythera_storage::backend::filesystem::FilesystemBackend;
use tempfile::tempdir;

#[tokio::test]
async fn test_filesystem_save_and_load() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "sess-001";
    let data = b"{\"key\":\"value\"}".to_vec();

    backend.save(session_id, &data).await.unwrap();
    let loaded = backend.load(session_id).await.unwrap();

    assert_eq!(loaded, Some(data));
}

#[tokio::test]
async fn test_filesystem_delete() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "sess-002";
    let data = b"to be deleted".to_vec();

    backend.save(session_id, &data).await.unwrap();
    assert!(backend.exists(session_id).await.unwrap());

    backend.delete(session_id).await.unwrap();
    assert!(!backend.exists(session_id).await.unwrap());
    assert_eq!(backend.load(session_id).await.unwrap(), None);
}

#[tokio::test]
async fn test_filesystem_list() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    backend.save("alpha", b"a").await.unwrap();
    backend.save("beta", b"b").await.unwrap();
    backend.save("gamma", b"c").await.unwrap();

    let mut ids = backend.list().await.unwrap();
    ids.sort();
    assert_eq!(ids, vec!["alpha", "beta", "gamma"]);
}

#[tokio::test]
async fn test_filesystem_exists() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    assert!(!backend.exists("sess-x").await.unwrap());

    backend.save("sess-x", b"data").await.unwrap();
    assert!(backend.exists("sess-x").await.unwrap());

    backend.delete("sess-x").await.unwrap();
    assert!(!backend.exists("sess-x").await.unwrap());
}

#[tokio::test]
/// Verify backup file exists (check via backup path).
///
/// Sync backup to primary data.
///
async fn test_filesystem_backup_and_sync() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "sess-003";
    let data = b"backup test data".to_vec();

    backend.backup(session_id, &data).await.unwrap();

    let backup_path = backup_dir.join(format!("{session_id}.backup.json"));
    assert!(backup_path.exists());

    backend.sync_backup(session_id).await.unwrap();
    let loaded = backend.load(session_id).await.unwrap();
    assert_eq!(loaded, Some(data));

    backend.delete_backup(session_id).await.unwrap();
    assert!(!backup_path.exists());
}

#[tokio::test]
/// Before save, verify_sync returns false.
///
async fn test_filesystem_verify_sync() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "sess-004";

    assert!(!backend.verify_sync(session_id).await.unwrap());

    backend.save(session_id, b"test").await.unwrap();
    assert!(backend.verify_sync(session_id).await.unwrap());
}

#[tokio::test]
async fn test_filesystem_load_nonexistent() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let result = backend.load("nonexistent").await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
/// Deleting a non-existent session should not error.
///
async fn test_filesystem_delete_nonexistent_is_ok() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    backend.delete("ghost").await.unwrap();
}

#[tokio::test]
async fn test_filesystem_overwrite_session() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let backup_dir = dir.path().join("backups");
    let backend = FilesystemBackend::new(&data_dir, &backup_dir).unwrap();

    let session_id = "sess-overwrite";
    backend.save(session_id, b"v1").await.unwrap();
    backend.save(session_id, b"v2").await.unwrap();

    let loaded = backend.load(session_id).await.unwrap();
    assert_eq!(loaded, Some(b"v2".to_vec()));
}
