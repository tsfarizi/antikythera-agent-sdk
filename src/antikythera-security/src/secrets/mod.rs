//! Secrets management with versioning and in-memory backend.

pub mod error;
pub mod memory;

pub use error::SecretManagerError;
pub use memory::{MemoryStorage, StoredSecret};

use antikythera_domain::security::{SecretMetadata, SecretsConfig};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Secret manager supporting versioning, rotation, and memory backend.
pub struct SecretManager {
    config: SecretsConfig,
    storage: Arc<Mutex<MemoryStorage>>,
    rotation_task: Option<std::thread::JoinHandle<()>>,
}

impl SecretManager {
    pub fn new(config: SecretsConfig) -> Result<Self, SecretManagerError> {
        if config.storage_backend != "memory" {
            return Err(SecretManagerError::InvalidConfig(format!(
                "Unknown storage backend: {} (only 'memory' is supported)",
                config.storage_backend
            )));
        }

        let storage = Arc::new(Mutex::new(MemoryStorage::new()));

        let rotation_task = if config.auto_rotate {
            let storage_clone = Arc::clone(&storage);
            let interval = Duration::from_secs((config.rotation_interval_hours * 3600) as u64);
            let max_secret_age = config.max_secret_age_hours;

            Some(std::thread::spawn(move || {
                Self::rotation_loop(storage_clone, interval, max_secret_age);
            }))
        } else {
            None
        };

        Ok(Self {
            config,
            storage,
            rotation_task,
        })
    }

    pub fn from_config() -> Result<Self, SecretManagerError> {
        Self::new(SecretsConfig::default())
    }

    /// Store a new version of a secret.
    pub fn store_secret(&self, id: &str, value: &[u8]) -> Result<(), SecretManagerError> {
        if !self.config.enabled {
            tracing::warn!(id, "Secrets management is disabled");
            return Err(SecretManagerError::InvalidConfig(
                "Secrets management is disabled".to_string(),
            ));
        }

        let mut storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in store_secret");

        let entry = storage.secrets.entry(id.to_string()).or_default();
        let version = entry.len() as u32 + 1;

        let metadata = SecretMetadata::new(id.to_string(), version);

        entry.push(StoredSecret {
            value: value.to_vec(),
            metadata,
        });

        if self.config.enable_versioning && entry.len() as u32 > self.config.max_versions {
            entry.remove(0);
        }

        tracing::debug!(id, version, "Secret stored");
        Ok(())
    }

    /// Retrieve the latest active version of a secret.
    pub fn get_secret(&self, id: &str) -> Result<Vec<u8>, SecretManagerError> {
        let storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in get_secret");

        let entry = storage.secrets.get(id).ok_or_else(|| {
            tracing::warn!(id, "Secret not found");
            SecretManagerError::SecretNotFound(id.to_string())
        })?;

        let latest = entry
            .iter()
            .filter(|s| s.metadata.active)
            .max_by_key(|s| s.metadata.version)
            .ok_or_else(|| {
                tracing::warn!(id, "No active version found");
                SecretManagerError::SecretNotFound(id.to_string())
            })?;

        if latest.metadata.is_expired() {
            tracing::warn!(id, "Secret expired");
            return Err(SecretManagerError::SecretExpired(id.to_string()));
        }

        Ok(latest.value.clone())
    }

    /// Delete all versions of a secret.
    pub fn delete_secret(&self, id: &str) -> Result<(), SecretManagerError> {
        let mut storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in delete_secret");

        storage.secrets.remove(id).ok_or_else(|| {
            tracing::warn!(id, "Secret not found for deletion");
            SecretManagerError::SecretNotFound(id.to_string())
        })?;

        tracing::debug!(id, "Secret deleted");
        Ok(())
    }

    /// Rotate a secret: deactivate old versions, store a new one.
    pub fn rotate_secret(&self, id: &str, new_value: &[u8]) -> Result<(), SecretManagerError> {
        let mut storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in rotate_secret");

        let entry = storage.secrets.get_mut(id).ok_or_else(|| {
            tracing::warn!(id, "Secret not found for rotation");
            SecretManagerError::SecretNotFound(id.to_string())
        })?;

        for secret in entry.iter_mut() {
            secret.metadata.active = false;
        }

        let version = entry.len() as u32 + 1;
        let mut metadata = SecretMetadata::new(id.to_string(), version);
        metadata.last_rotated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        entry.push(StoredSecret {
            value: new_value.to_vec(),
            metadata,
        });

        if self.config.enable_versioning && entry.len() as u32 > self.config.max_versions {
            entry.remove(0);
        }

        tracing::debug!(id, version, "Secret rotated");
        Ok(())
    }

    /// Check whether a secret needs rotation based on age.
    pub fn needs_rotation(&self, id: &str) -> Result<bool, SecretManagerError> {
        let storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in needs_rotation");

        let entry = storage.secrets.get(id).ok_or_else(|| {
            tracing::warn!(id, "Secret not found for rotation check");
            SecretManagerError::SecretNotFound(id.to_string())
        })?;

        let latest = entry
            .iter()
            .filter(|s| s.metadata.active)
            .max_by_key(|s| s.metadata.version)
            .ok_or_else(|| {
                tracing::warn!(id, "No active version for rotation check");
                SecretManagerError::SecretNotFound(id.to_string())
            })?;

        Ok(latest
            .metadata
            .needs_rotation(self.config.max_secret_age_hours))
    }

    /// List all secret IDs.
    pub fn list_secrets(&self) -> Vec<String> {
        let storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in list_secrets");
        storage.secrets.keys().cloned().collect()
    }

    /// Get metadata for the latest active version of a secret.
    pub fn get_metadata(&self, id: &str) -> Result<SecretMetadata, SecretManagerError> {
        let storage = self
            .storage
            .lock()
            .expect("SecretManager storage lock poisoned in get_metadata");

        let entry = storage.secrets.get(id).ok_or_else(|| {
            tracing::warn!(id, "Secret not found for metadata");
            SecretManagerError::SecretNotFound(id.to_string())
        })?;

        let latest = entry
            .iter()
            .filter(|s| s.metadata.active)
            .max_by_key(|s| s.metadata.version)
            .ok_or_else(|| {
                tracing::warn!(id, "No active version for metadata");
                SecretManagerError::SecretNotFound(id.to_string())
            })?;

        Ok(latest.metadata.clone())
    }

    /// Current configuration reference.
    pub fn config(&self) -> &SecretsConfig {
        &self.config
    }

    /// Replace config and restart the rotation task if needed.
    pub fn update_config(&mut self, config: SecretsConfig) -> Result<(), SecretManagerError> {
        let rotation_interval_hours = config.rotation_interval_hours;
        let auto_rotate = config.auto_rotate;
        self.config = config;

        if auto_rotate && self.rotation_task.is_none() {
            let storage_clone = Arc::clone(&self.storage);
            let interval = Duration::from_secs((rotation_interval_hours * 3600) as u64);
            let max_secret_age = self.config.max_secret_age_hours;

            self.rotation_task = Some(std::thread::spawn(move || {
                Self::rotation_loop(storage_clone, interval, max_secret_age);
            }));
        }

        Ok(())
    }

    fn rotation_loop(
        storage: Arc<Mutex<MemoryStorage>>,
        interval: Duration,
        max_secret_age_hours: u32,
    ) {
        loop {
            std::thread::sleep(interval);

            let mut guard = storage
                .lock()
                .expect("SecretManager rotation storage lock poisoned");

            for entry in guard.secrets.values_mut() {
                if let Some(latest) = entry
                    .iter_mut()
                    .filter(|s| s.metadata.active)
                    .max_by_key(|s| s.metadata.version)
                    .filter(|s| s.metadata.needs_rotation(max_secret_age_hours))
                {
                    latest.metadata.active = false;
                }
            }
        }
    }
}

impl Drop for SecretManager {
    fn drop(&mut self) {
        self.rotation_task.take();
    }
}

#[async_trait::async_trait]
impl antikythera_ports::SecretStore for SecretManager {
    async fn store_secret(&self, id: &str, secret: &[u8]) -> Result<(), String> {
        SecretManager::store_secret(self, id, secret).map_err(|e| e.to_string())
    }

    async fn get_secret(&self, id: &str) -> Result<Vec<u8>, String> {
        SecretManager::get_secret(self, id).map_err(|e| e.to_string())
    }

    async fn delete_secret(&self, id: &str) -> Result<(), String> {
        SecretManager::delete_secret(self, id).map_err(|e| e.to_string())
    }
}
