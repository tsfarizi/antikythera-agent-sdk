//! Session Log Export/Import
//!
//! Export and import logs tied to specific sessions with consistent format.

use crate::entries::*;
use serde::{Deserialize, Serialize};

/// Logs exported for a specific session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLogExport {
    /// Export format version
    pub version: u32,
    /// Session ID these logs belong to
    pub session_id: String,
    /// Log entries for this session
    pub logs: Vec<LogEntry>,
    /// Export timestamp
    pub exported_at: String,
    /// Optional notes
    pub notes: Option<String>,
}

impl SessionLogExport {
    /// Current export format version
    pub const VERSION: u32 = 1;

    /// Create export from session logs
    pub fn from_logs(session_id: impl Into<String>, logs: Vec<LogEntry>) -> Self {
        Self {
            version: Self::VERSION,
            session_id: session_id.into(),
            logs,
            exported_at: crate::wasm_compat::now_rfc3339(),
            notes: None,
        }
    }

    /// Create with notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Get log count
    pub fn log_count(&self) -> usize {
        self.logs.len()
    }

    /// Get logs
    pub fn into_logs(self) -> Vec<LogEntry> {
        self.logs
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Serialize error: {}", e))
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, String> {
        let export: SessionLogExport =
            serde_json::from_str(json).map_err(|e| format!("Deserialize error: {}", e))?;

        if export.version != Self::VERSION {
            return Err(format!(
                "Unsupported export version: {}. Expected: {}",
                export.version,
                Self::VERSION
            ));
        }

        for entry in &export.logs {
            if let Some(entry_session) = &entry.session_id
                && entry_session != &export.session_id
            {
                return Err(format!(
                    "Session ID mismatch: export has '{}', log has '{}'",
                    export.session_id, entry_session
                ));
            }
        }

        Ok(export)
    }
}

/// Multiple session logs exported together
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchLogExport {
    /// Export format version
    pub version: u32,
    /// Session log exports
    pub sessions: Vec<SessionLogExport>,
    /// Export timestamp
    pub exported_at: String,
    /// Optional notes
    pub notes: Option<String>,
}

impl BatchLogExport {
    /// Current batch export format version
    pub const VERSION: u32 = 1;

    /// Create batch export from session logs
    pub fn from_session_logs(sessions: Vec<SessionLogExport>) -> Self {
        Self {
            version: Self::VERSION,
            exported_at: crate::wasm_compat::now_rfc3339(),
            sessions,
            notes: None,
        }
    }

    /// Create with notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get total log count across all sessions
    pub fn total_log_count(&self) -> usize {
        self.sessions.iter().map(|s| s.logs.len()).sum()
    }

    /// Get session log exports
    pub fn into_sessions(self) -> Vec<SessionLogExport> {
        self.sessions
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Serialize error: {}", e))
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, String> {
        let export: BatchLogExport =
            serde_json::from_str(json).map_err(|e| format!("Deserialize error: {}", e))?;

        if export.version != Self::VERSION {
            return Err(format!(
                "Unsupported export version: {}. Expected: {}",
                export.version,
                Self::VERSION
            ));
        }

        for session in &export.sessions {
            if session.version != SessionLogExport::VERSION {
                return Err(format!(
                    "Session log version mismatch: {} vs expected {}",
                    session.version,
                    SessionLogExport::VERSION
                ));
            }
        }

        Ok(export)
    }
}
