use antikythera_log::{LogBatch, LogFilter as LogLogFilter};
use antikythera_ports::types::{LogEntry as PortsLogEntry, LogFilter as PortsLogFilter};
use std::sync::LazyLock;

use super::provider::AntikytheraLogProvider;
use crate::application::ports::logging::LogQueryPort;

/// Static port instance for log queries.
static LOG_QUERY: LazyLock<AntikytheraLogProvider> = LazyLock::new(|| AntikytheraLogProvider);

/// Re-exports for backward compatibility. Prefer using the LogProvider / LogQueryPort traits
/// instead.
///
pub use antikythera_log::session_logger::{
    SessionLogger, clear_all_loggers, get_logger, logger_count,
};

static ACTIVE_SESSION: LazyLock<std::sync::Mutex<String>> =
    LazyLock::new(|| std::sync::Mutex::new("tui".to_string()));

pub fn set_active_session(session_id: &str) {
    let mut s = ACTIVE_SESSION
        .lock()
        .expect("ACTIVE_SESSION lock poisoned in set_active_session");
    *s = session_id.to_string();
}

pub fn get_active_session() -> String {
    ACTIVE_SESSION
        .lock()
        .expect("ACTIVE_SESSION lock poisoned in get_active_session")
        .clone()
}

/// Query logs for a session, returning a LogBatch with pagination.
///
/// This is a direct convenience wrapper around the logger; for trait-based
/// access, use [`LogQueryPort`] via the static `LOG_QUERY` instance.
pub fn query_logs(session_id: &str, filter: &LogLogFilter) -> LogBatch {
    get_logger(session_id).get_logs(filter)
}

/// Get the latest N log entries for a session via the LogQueryPort.
pub fn get_latest_logs(session_id: &str, count: usize) -> Vec<PortsLogEntry> {
    LOG_QUERY.get_latest_logs(session_id, count)
}

/// Get logs as JSON for a session via the LogQueryPort.
pub fn get_logs_json(session_id: &str, filter: &PortsLogFilter) -> Result<String, String> {
    LOG_QUERY.get_logs_json(session_id, filter)
}

/// Subscribe to real-time log stream for a session.
pub fn subscribe_logs(session_id: &str) -> Option<antikythera_log::LogSubscriber> {
    Some(get_logger(session_id).subscribe())
}

/// Clear all logs for a session.
pub fn clear_logs(session_id: &str) {
    get_logger(session_id).clear();
}
