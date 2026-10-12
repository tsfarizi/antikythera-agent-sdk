//! Canonical log type definitions.
//!
//! These are the single source of truth for log-level, entry, filter, and batch
//! types across the framework.  Downstream crates (`antikythera-log`, etc.)
//! re-export or depend on these definitions rather than maintaining their own.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Log level severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Detailed debugging information.
    Debug = 0,
    /// General informational messages.
    Info = 1,
    /// Warning conditions.
    Warn = 2,
    /// Error conditions.
    Error = 3,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn parse_label(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "DEBUG" => Some(LogLevel::Debug),
            "INFO" => Some(LogLevel::Info),
            "WARN" => Some(LogLevel::Warn),
            "ERROR" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LogLevel::parse_label(s).ok_or_else(|| format!("invalid log level: {s}"))
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single log entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level.
    pub level: LogLevel,
    /// Log message.
    pub message: String,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
    /// Session ID for grouping (ties log to specific session).
    pub session_id: Option<String>,
    /// Source module/component.
    pub source: Option<String>,
    /// Additional context (JSON encoded).
    pub context: Option<String>,
    /// Sequence number for ordering.
    pub sequence: u64,
}

impl LogEntry {
    /// Create a new log entry with the current timestamp.
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            session_id: None,
            source: None,
            context: None,
            sequence: 0,
        }
    }

    /// Set session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set source module.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set context (additional data).
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Set sequence number.
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    /// Format as a human-readable string.
    pub fn format_pretty(&self) -> String {
        let session = self
            .session_id
            .as_ref()
            .map(|s| format!("[{}]", s))
            .unwrap_or_default();

        let source = self
            .source
            .as_ref()
            .map(|s| format!("[{}]", s))
            .unwrap_or_default();

        format!(
            "{} {} {} {} - {}",
            self.timestamp, self.level, session, source, self.message
        )
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Serialize error: {}", e))
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Deserialize error: {}", e))
    }
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_pretty())
    }
}

/// Filter for querying logs.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Minimum log level.
    pub min_level: Option<LogLevel>,
    /// Filter by session ID.
    pub session_id: Option<String>,
    /// Filter by source.
    pub source: Option<String>,
    /// Maximum number of entries to return.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: Option<usize>,
}

impl LogFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min_level(mut self, level: LogLevel) -> Self {
        self.min_level = Some(level);
        self
    }

    pub fn session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Check if a log entry matches this filter.
    pub fn matches(&self, entry: &LogEntry) -> bool {
        if let Some(min_level) = self.min_level
            && entry.level < min_level
        {
            return false;
        }

        if let Some(session_id) = &self.session_id
            && entry.session_id.as_ref() != Some(session_id)
        {
            return false;
        }

        if let Some(source) = &self.source
            && entry.source.as_ref() != Some(source)
        {
            return false;
        }

        true
    }
}

/// Batch of log entries for efficient transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Log Batch (for efficient transfer).
///
pub struct LogBatch {
    /// Log entries.
    pub entries: Vec<LogEntry>,
    /// Total count (for pagination).
    pub total_count: usize,
    /// Has more entries.
    pub has_more: bool,
}

impl LogBatch {
    pub fn new(entries: Vec<LogEntry>, total_count: usize, has_more: bool) -> Self {
        Self {
            entries,
            total_count,
            has_more,
        }
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Serialize error: {}", e))
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Deserialize error: {}", e))
    }
}
