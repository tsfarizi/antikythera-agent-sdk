//! Filesystem-based storage backend.
//!
//! Sessions are stored as JSON files. This is the default backend
//! and does not require any external database.

use std::path::PathBuf;

use antikythera_macros::PortValidate;
use async_trait::async_trait;

use crate::error::StorageError;

use super::StorageBackend;

fn validate_id(id: &str) -> Result<(), StorageError> {
    let first = id
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(first.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (first.len() == 4
            && (first.starts_with("COM") || first.starts_with("LPT"))
            && first.as_bytes()[3].is_ascii_digit());
    if id.is_empty()
        || id.len() > 256
        || id == "."
        || id == ".."
        || reserved
        || id.ends_with('.')
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(StorageError::Config(
            "invalid filesystem session id".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/rust/unit/antikythera-storage/backend/filesystem.tests.rs"]
mod tests;

/// Replace a complete file only after its contents have been flushed.
fn atomic_write(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Filesystem-based storage backend.
///
/// Sessions are stored as JSON files at `{data_dir}/{session_id}.json`.
/// Backups are stored at `{backup_dir}/{session_id}.backup.json`.
#[derive(PortValidate)]
#[implements(crate::backend::StorageBackend)]
pub struct FilesystemBackend {
    data_dir: PathBuf,
    backup_dir: PathBuf,
}

impl FilesystemBackend {
    /// Create a new filesystem backend, creating directories if they don't exist.
    pub fn new(data_dir: &PathBuf, backup_dir: &PathBuf) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(data_dir)?;
        std::fs::create_dir_all(backup_dir)?;
        Ok(Self {
            data_dir: data_dir.clone(),
            backup_dir: backup_dir.clone(),
        })
    }

    fn session_path(&self, session_id: &str) -> Result<PathBuf, StorageError> {
        validate_id(session_id)?;
        Ok(self.data_dir.join(format!("{session_id}.json")))
    }

    fn backup_path(&self, session_id: &str) -> Result<PathBuf, StorageError> {
        validate_id(session_id)?;
        Ok(self.backup_dir.join(format!("{session_id}.backup.json")))
    }
}

#[async_trait]
impl StorageBackend for FilesystemBackend {
    async fn save(&self, session_id: &str, data: &[u8]) -> Result<(), StorageError> {
        let path = self.session_path(session_id)?;
        let data = data.to_vec();
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || atomic_write(&path_clone, &data))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?
            .map_err(|e| StorageError::Path { path, source: e })
    }

    async fn load(&self, session_id: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let path = self.session_path(session_id)?;
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || std::fs::read(&path_clone))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?
            .map(Some)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(StorageError::Path { path, source: e })
                }
            })
    }

    async fn delete(&self, session_id: &str) -> Result<(), StorageError> {
        let path = self.session_path(session_id)?;
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || match std::fs::remove_file(&path_clone) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Path { path, source: e }),
        })
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?
    }

    async fn list(&self) -> Result<Vec<String>, StorageError> {
        let data_dir = self.data_dir.clone();
        tokio::task::spawn_blocking(move || {
            let mut ids = Vec::new();
            for entry in std::fs::read_dir(&data_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if let Some(id) = name.strip_suffix(".json") {
                    ids.push(id.to_string());
                }
            }
            Ok(ids)
        })
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?
    }

    async fn exists(&self, session_id: &str) -> Result<bool, StorageError> {
        let path = self.session_path(session_id)?;
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || match std::fs::metadata(&path_clone) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::Path { path, source: e }),
        })
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?
    }

    async fn backup(&self, session_id: &str, data: &[u8]) -> Result<(), StorageError> {
        let path = self.backup_path(session_id)?;
        let data = data.to_vec();
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || atomic_write(&path_clone, &data))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?
            .map_err(|e| StorageError::Path { path, source: e })
    }

    async fn sync_backup(&self, session_id: &str) -> Result<(), StorageError> {
        let backup = self.backup_path(session_id)?;
        let target = self.session_path(session_id)?;
        let session_id = session_id.to_string();
        tokio::task::spawn_blocking(move || {
            std::fs::read(&backup).and_then(|data| atomic_write(&target, &data))
        })
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?
        .map_err(|e| {
            StorageError::Backup(format!("failed to sync backup for {session_id}: {e}"))
        })?;
        Ok(())
    }

    async fn verify_sync(&self, session_id: &str) -> Result<bool, StorageError> {
        let path = self.session_path(session_id)?;
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || match std::fs::metadata(&path_clone) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::Path { path, source: e }),
        })
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?
    }

    async fn delete_backup(&self, session_id: &str) -> Result<(), StorageError> {
        let path = self.backup_path(session_id)?;
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || match std::fs::remove_file(&path_clone) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Path { path, source: e }),
        })
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?
    }
}
