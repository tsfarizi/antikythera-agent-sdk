//! Session Export/Import
//!
//! Export and import sessions with consistent JSON format.

use crate::session::Session;
use antikythera_log::SessionLogger;
use serde::{Deserialize, Serialize};

/// Session export data with versioning for consistency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionExport {
    /// Export format version
    pub version: u32,
    /// Session data
    pub session: Session,
    /// Export timestamp
    pub exported_at: String,
    /// Optional notes
    pub notes: Option<String>,
}

impl SessionExport {
    /// Current export format version
    pub const VERSION: u32 = 1;

    /// Create export from session
    pub fn from_session(session: Session) -> Self {
        Self {
            version: Self::VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            session,
            notes: None,
        }
    }

    /// Get the session
    pub fn into_session(self) -> Session {
        self.session
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| {
            SessionLogger::new(&self.session.id)
                .error(format!("Session export serialize error: {}", e));
            format!("Serialize error: {}", e)
        })
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, String> {
        let export: SessionExport = serde_json::from_str(json).map_err(|e| {
            SessionLogger::new("export").error(format!("Session export deserialize error: {}", e));
            format!("Deserialize error: {}", e)
        })?;

        if export.version != Self::VERSION {
            SessionLogger::new("export").error(format!(
                "Unsupported export version: {}. Expected: {}",
                export.version,
                Self::VERSION
            ));
            return Err(format!(
                "Unsupported export version: {}. Expected: {}",
                export.version,
                Self::VERSION
            ));
        }

        Ok(export)
    }
}

/// Multiple sessions export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchExport {
    /// Export format version
    pub version: u32,
    /// Exported sessions
    pub sessions: Vec<SessionExport>,
    /// Export timestamp
    pub exported_at: String,
    /// Optional notes
    pub notes: Option<String>,
}

impl BatchExport {
    /// Current batch export format version
    pub const VERSION: u32 = 1;

    /// Create batch export from sessions
    pub fn from_sessions(sessions: Vec<Session>) -> Self {
        Self {
            version: Self::VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            sessions: sessions
                .into_iter()
                .map(SessionExport::from_session)
                .collect(),
            notes: None,
        }
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get sessions
    pub fn into_sessions(self) -> Vec<Session> {
        self.sessions
            .into_iter()
            .map(|e| e.into_session())
            .collect()
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| {
            SessionLogger::new("export").error(format!("Batch export serialize error: {}", e));
            format!("Serialize error: {}", e)
        })
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, String> {
        let export: BatchExport = serde_json::from_str(json).map_err(|e| {
            SessionLogger::new("export").error(format!("Batch export deserialize error: {}", e));
            format!("Deserialize error: {}", e)
        })?;

        if export.version != Self::VERSION {
            SessionLogger::new("export").error(format!(
                "Unsupported batch export version: {}. Expected: {}",
                export.version,
                Self::VERSION
            ));
            return Err(format!(
                "Unsupported batch export version: {}. Expected: {}",
                export.version,
                Self::VERSION
            ));
        }

        Ok(export)
    }
}
