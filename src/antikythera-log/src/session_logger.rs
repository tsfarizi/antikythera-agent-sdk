//! Session Logger Registry
//!
//! Provides a global registry of per-session [`Logger`] instances and a typed
//! [`SessionLogger`] wrapper that tags every log entry with the `"session"`
//! source module.
//!
//! This module lives in `antikythera-log` (rather than `antikythera-core`)
//! so that downstream crates which cannot take a dependency on
//! `antikythera-core` (e.g. `antikythera-session`, to avoid a cyclic
//! dependency) can still share the same logger registry.

use crate::entries::LogLevel;
use crate::logger::Logger;
use std::sync::{Arc, LazyLock, Mutex};

/// Global logger storage keyed by session id.
///
/// `antikythera-core` re-exports the [`get_logger`] / [`clear_all_loggers`]
/// helpers from `antikythera-core::logging` for backward compatibility. The
/// actual registry lives here so any crate that depends on `antikythera-log`
/// can share the same buffer state.
static LOGGERS: LazyLock<Mutex<std::collections::HashMap<String, Arc<Logger>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Get or create a shared [`Logger`] for the given session id.
///
/// Multiple calls with the same `session_id` return clones of the same
/// `Arc<Logger>`, so all callers write into a single per-session log buffer.
pub fn get_logger(session_id: &str) -> Arc<Logger> {
    let mut loggers = LOGGERS.lock().unwrap_or_else(|e| e.into_inner());

    loggers
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Logger::new(session_id)))
        .clone()
}

/// Clear all loggers in the registry.
pub fn clear_all_loggers() {
    let mut loggers = LOGGERS.lock().unwrap_or_else(|e| e.into_inner());
    loggers.clear();
}

/// Number of session loggers currently registered.
pub fn logger_count() -> usize {
    let loggers = LOGGERS.lock().unwrap_or_else(|e| e.into_inner());
    loggers.len()
}

/// Typed wrapper around a shared [`Logger`] that tags every entry with the
/// `"session"` source module.
///
/// Equivalent to the `SessionLogger` previously defined in
/// `antikythera-core::logging`, but defined in `antikythera-log` so it can
/// be used by `antikythera-session` without introducing a cyclic
/// dependency between `antikythera-core` and `antikythera-session`.
#[derive(Clone, Debug)]
pub struct SessionLogger {
    logger: Arc<Logger>,
}

impl SessionLogger {
    /// Get a `SessionLogger` for the given session id.
    ///
    /// Multiple calls with the same `session_id` share the same underlying
    /// [`Logger`] buffer, so log entries written from different call sites
    /// accumulate in one place.
    pub fn new(session_id: &str) -> Self {
        Self {
            logger: get_logger(session_id),
        }
    }

    /// Log at DEBUG level with the `"session"` source tag.
    pub fn debug(&self, message: impl Into<String>) {
        self.logger
            .log_with_source(LogLevel::Debug, "session", message);
    }

    /// Log at INFO level with the `"session"` source tag.
    pub fn info(&self, message: impl Into<String>) {
        self.logger
            .log_with_source(LogLevel::Info, "session", message);
    }

    /// Log at WARN level with the `"session"` source tag.
    pub fn warn(&self, message: impl Into<String>) {
        self.logger
            .log_with_source(LogLevel::Warn, "session", message);
    }

    /// Log at ERROR level with the `"session"` source tag.
    pub fn error(&self, message: impl Into<String>) {
        self.logger
            .log_with_source(LogLevel::Error, "session", message);
    }
}
