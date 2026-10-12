use super::*;

#[tokio::test]
async fn rejects_traversal_for_every_id_operation() {
    let temp = tempfile::tempdir().unwrap();
    let backend =
        FilesystemBackend::new(&temp.path().join("data"), &temp.path().join("backup")).unwrap();
    for id in [
        "../escaped",
        "..\\escaped",
        "/absolute",
        "C:\\absolute",
        "",
        ".",
        "..",
        "NUL",
        "CON.txt",
    ] {
        assert!(backend.save(id, b"bad").await.is_err());
        assert!(backend.load(id).await.is_err());
        assert!(backend.delete(id).await.is_err());
        assert!(backend.backup(id, b"bad").await.is_err());
        assert!(backend.sync_backup(id).await.is_err());
        assert!(backend.delete_backup(id).await.is_err());
        assert!(backend.exists(id).await.is_err());
        assert!(backend.verify_sync(id).await.is_err());
    }
    assert!(!temp.path().join("escaped.json").exists());
}

#[tokio::test]
async fn replaces_complete_files_and_restores_backup() {
    let temp = tempfile::tempdir().unwrap();
    let backend =
        FilesystemBackend::new(&temp.path().join("data"), &temp.path().join("backup")).unwrap();
    backend.save("session-1", b"original").await.unwrap();
    backend.save("session-1", b"replacement").await.unwrap();
    assert_eq!(
        backend.load("session-1").await.unwrap().unwrap(),
        b"replacement"
    );
    backend.backup("session-1", b"backup").await.unwrap();
    backend.sync_backup("session-1").await.unwrap();
    assert_eq!(backend.load("session-1").await.unwrap().unwrap(), b"backup");
    assert_eq!(backend.list().await.unwrap(), vec!["session-1"]);
    assert_eq!(
        std::fs::read_dir(temp.path().join("data")).unwrap().count(),
        1
    );
}
