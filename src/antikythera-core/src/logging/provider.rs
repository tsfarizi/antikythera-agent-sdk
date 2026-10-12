//! Concrete [`LogProvider`] implementation backed by `antikythera_log`.
//!
//! Bridges the port traits defined in [`crate::application::ports::logging`]
//! to the session-based logger registry maintained by `antikythera_log`.

use antikythera_log::Logger;
use antikythera_log::session_logger;
use antikythera_ports::types::{LogEntry as PortsLogEntry, LogFilter as PortsLogFilter};
use std::sync::Arc;

use crate::application::ports::logging::{AppLogger, LogProvider, LogQueryPort};

/// Adapter that wraps antikythera_log's Logger to implement the AppLogger port.
///
/// Delegates each log-level method to the inner [`Logger`] instance obtained
/// from the session logger registry.
struct LogProviderAdapter(Arc<Logger>);

/// Delegates info-level log calls to the underlying `antikythera_log::Logger`.
impl AppLogger for LogProviderAdapter {
    fn log_info(&self, message: String) {
        self.0.info(message);
    }

    fn log_warn(&self, message: String) {
        self.0.warn(message);
    }

    fn log_error(&self, message: String) {
        self.0.error(message);
    }

    fn log_debug(&self, message: String) {
        self.0.debug(message);
    }
}

/// Concrete LogProvider wrapping antikythera_log's session_logger.
///
/// Provides session-scoped logger instances and delegates log storage and
/// querying to the global `antikythera_log::session_logger` registry.
pub struct AntikytheraLogProvider;

/// Obtains or creates a per-session [`AppLogger`] via the session logger registry.
impl LogProvider for AntikytheraLogProvider {
    fn get_logger(&self, session_id: &str) -> Box<dyn AppLogger> {
        Box::new(LogProviderAdapter(session_logger::get_logger(session_id)))
    }

    fn logger_count(&self) -> usize {
        session_logger::logger_count()
    }

    fn clear_all_loggers(&self) {
        session_logger::clear_all_loggers();
    }
}

/// Convert from `antikythera_log` entry to `antikythera_ports` entry.
///
/// Both types are structurally identical (same fields, same semantics);
/// this bridge exists because the two crates define their own canonical
/// copies of the log types. The port trait uses `antikythera_ports::types`;
/// the logger returns `antikythera_log` entries.
fn log_entry_to_ports(entry: antikythera_log::LogEntry) -> PortsLogEntry {
    PortsLogEntry {
        level: log_level_to_ports(entry.level),
        message: entry.message,
        timestamp: entry.timestamp,
        session_id: entry.session_id,
        source: entry.source,
        context: entry.context,
        sequence: entry.sequence,
    }
}

fn log_level_to_ports(level: antikythera_log::LogLevel) -> antikythera_ports::types::LogLevel {
    match level {
        antikythera_log::LogLevel::Debug => antikythera_ports::types::LogLevel::Debug,
        antikythera_log::LogLevel::Info => antikythera_ports::types::LogLevel::Info,
        antikythera_log::LogLevel::Warn => antikythera_ports::types::LogLevel::Warn,
        antikythera_log::LogLevel::Error => antikythera_ports::types::LogLevel::Error,
    }
}

/// Convert from `antikythera_ports` filter to `antikythera_log` filter.
///
/// The `antikythera_log::Logger::get_logs` method accepts its own `LogFilter`
/// type, so we must translate the port's filter before calling into the logger.
fn ports_filter_to_log(filter: &PortsLogFilter) -> antikythera_log::LogFilter {
    let mut f = antikythera_log::LogFilter::new();
    if let Some(level) = filter.min_level {
        f = f.min_level(ports_level_to_log(level));
    }
    if let Some(ref session_id) = filter.session_id {
        f = f.session(session_id.as_str());
    }
    if let Some(ref source) = filter.source {
        f = f.source(source.as_str());
    }
    if let Some(limit) = filter.limit {
        f = f.limit(limit);
    }
    if let Some(offset) = filter.offset {
        f = f.offset(offset);
    }
    f
}

fn ports_level_to_log(level: antikythera_ports::types::LogLevel) -> antikythera_log::LogLevel {
    match level {
        antikythera_ports::types::LogLevel::Debug => antikythera_log::LogLevel::Debug,
        antikythera_ports::types::LogLevel::Info => antikythera_log::LogLevel::Info,
        antikythera_ports::types::LogLevel::Warn => antikythera_log::LogLevel::Warn,
        antikythera_ports::types::LogLevel::Error => antikythera_log::LogLevel::Error,
    }
}

/// Queries historical log entries from the session logger registry.
impl LogQueryPort for AntikytheraLogProvider {
    fn query_logs(&self, session_id: &str, filter: &PortsLogFilter) -> Vec<PortsLogEntry> {
        let logger = session_logger::get_logger(session_id);
        let log_filter = ports_filter_to_log(filter);
        let batch = logger.get_logs(&log_filter);
        batch.entries.into_iter().map(log_entry_to_ports).collect()
    }

    fn get_latest_logs(&self, session_id: &str, count: usize) -> Vec<PortsLogEntry> {
        let logger = session_logger::get_logger(session_id);
        logger
            .get_latest(count)
            .into_iter()
            .map(log_entry_to_ports)
            .collect()
    }

    fn get_logs_json(&self, session_id: &str, filter: &PortsLogFilter) -> Result<String, String> {
        let logger = session_logger::get_logger(session_id);
        let log_filter = ports_filter_to_log(filter);
        logger.get_logs_json(&log_filter)
    }
}

#[cfg(test)]
#[path = "../../../../tests/rust/unit/antikythera-core/logging/provider.tests.rs"]
mod tests;
