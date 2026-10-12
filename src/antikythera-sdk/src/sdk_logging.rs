//! SDK Logging Module
//!
//! Unified logging for all SDK operations with automatic source module tracking.
//! Captures all FFI interactions and SDK functionality.

use antikythera_log::{LogBatch, LogEntry, LogFilter, LogLevel, Logger};
use std::sync::{Arc, LazyLock, Mutex};

/// Global logger storage for SDK
static SDK_LOGGERS: LazyLock<Mutex<std::collections::HashMap<String, Arc<Logger>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Get or create a logger for an SDK session
pub fn get_sdk_logger(session_id: &str) -> Arc<Logger> {
    let mut loggers = SDK_LOGGERS.lock().unwrap();

    loggers
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Logger::new(session_id)))
        .clone()
}

/// Clear all SDK loggers
pub fn clear_sdk_loggers() {
    let mut loggers = SDK_LOGGERS.lock().unwrap();
    loggers.clear();
}

/// Config FFI module logger
/// Module-Specific Loggers (with automatic source tracking).
///
pub struct ConfigFfiLogger {
    logger: Arc<Logger>,
}

impl ConfigFfiLogger {
    pub fn new(session_id: &str) -> Self {
        Self {
            logger: get_sdk_logger(session_id),
        }
    }

    pub fn ffi_call(&self, function: &str, args: &str) {
        let context = format!("{{\"function\": \"{}\", \"args\": {}}}", function, args);
        self.logger
            .log_with_context(LogLevel::Debug, format!("FFI call: {}", function), context);
    }

    pub fn ffi_result(&self, function: &str, success: bool, result_size: usize) {
        let context = format!(
            "{{\"function\": \"{}\", \"success\": {}, \"result_size\": {}}}",
            function, success, result_size
        );
        self.logger.log_with_context(
            LogLevel::Debug,
            format!("FFI result: {}", function),
            context,
        );
    }

    pub fn ffi_error(&self, function: &str, error: &str) {
        let context = format!(
            "{{\"function\": \"{}\", \"error\": \"{}\"}}",
            function, error
        );
        self.logger
            .log_with_context(LogLevel::Error, format!("FFI error: {}", function), context);
    }

    pub fn config_loaded(&self, source: &str, size: usize) {
        let context = format!("{{\"source\": \"{}\", \"size\": {}}}", source, size);
        self.logger
            .log_with_context(LogLevel::Info, "Config loaded", context);
    }

    pub fn config_saved(&self, path: &str, size: usize) {
        let context = format!("{{\"path\": \"{}\", \"size\": {}}}", path, size);
        self.logger
            .log_with_context(LogLevel::Info, "Config saved", context);
    }

    pub fn provider_added(&self, provider_id: &str) {
        self.logger.log_with_context(
            LogLevel::Info,
            "Provider added",
            format!("{{\"provider_id\": \"{}\"}}", provider_id),
        );
    }

    pub fn provider_removed(&self, provider_id: &str) {
        self.logger.log_with_context(
            LogLevel::Info,
            "Provider removed",
            format!("{{\"provider_id\": \"{}\"}}", provider_id),
        );
    }

    pub fn prompt_updated(&self, prompt_name: &str) {
        self.logger.log_with_context(
            LogLevel::Info,
            "Prompt updated",
            format!("{{\"prompt_name\": \"{}\"}}", prompt_name),
        );
    }

    pub fn agent_config_changed(&self, field: &str, value: &str) {
        let context = format!("{{\"field\": \"{}\", \"value\": \"{}\"}}", field, value);
        self.logger
            .log_with_context(LogLevel::Info, "Agent config changed", context);
    }
}

/// Query SDK logs with filter
pub fn query_sdk_logs(session_id: &str, filter: &LogFilter) -> LogBatch {
    if let Some(logger) = SDK_LOGGERS.lock().unwrap().get(session_id) {
        logger.get_logs(filter)
    } else {
        LogBatch::new(Vec::new(), 0, false)
    }
}

/// Get latest SDK logs
pub fn get_latest_sdk_logs(session_id: &str, count: usize) -> Vec<LogEntry> {
    if let Some(logger) = SDK_LOGGERS.lock().unwrap().get(session_id) {
        logger.get_latest(count)
    } else {
        Vec::new()
    }
}

/// Get SDK logs as JSON
pub fn get_sdk_logs_json(session_id: &str, filter: &LogFilter) -> Result<String, String> {
    if let Some(logger) = SDK_LOGGERS.lock().unwrap().get(session_id) {
        logger.get_logs_json(filter)
    } else {
        Ok(r#"{"entries":[],"total_count":0,"has_more":false}"#.to_string())
    }
}

/// Subscribe to real-time SDK log stream
pub fn subscribe_sdk_logs(session_id: &str) -> Option<antikythera_log::LogSubscriber> {
    SDK_LOGGERS
        .lock()
        .unwrap()
        .get(session_id)
        .map(|l| l.subscribe())
}

/// Clear SDK logs
pub fn clear_sdk_session_logs(session_id: &str) {
    if let Some(logger) = SDK_LOGGERS.lock().unwrap().get(session_id) {
        logger.clear();
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-sdk/sdk_logging.tests.rs"]
mod tests;
