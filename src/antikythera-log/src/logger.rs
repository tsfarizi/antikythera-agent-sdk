//! Logger Implementation

use crate::entries::*;
use std::sync::{Arc, Mutex};

/// Thread-safe log buffer
#[derive(Clone, Debug)]
pub struct LogBuffer {
    entries: Arc<Mutex<Vec<LogEntry>>>,
    sequence: Arc<Mutex<u64>>,
    max_capacity: usize,
}

/// Main logger
#[derive(Clone, Debug)]
pub struct Logger {
    session_id: String,
    buffer: LogBuffer,
    #[cfg(feature = "subscriber")]
    pub(crate) subscribers: Arc<std::sync::Mutex<Vec<crate::subscriber::LogSender>>>,
}

impl LogBuffer {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::with_capacity(max_capacity))),
            sequence: Arc::new(Mutex::new(0)),
            max_capacity,
        }
    }

    /// Add a log entry
    pub fn push(&self, entry: LogEntry) {
        let mut seq = self.sequence.lock().unwrap_or_else(|e| e.into_inner());
        *seq += 1;
        let mut entry = entry;
        entry.sequence = *seq;

        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.push(entry);

        if entries.len() > self.max_capacity {
            let drain_count = entries.len() - self.max_capacity;
            entries.drain(0..drain_count);
        }
    }

    /// Get all logs matching filter
    pub fn get_logs(&self, filter: &LogFilter) -> LogBatch {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());

        let filtered: Vec<_> = entries
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();

        let total_count = filtered.len();

        let offset = filter.offset.unwrap_or(0);
        let limit = filter.limit.unwrap_or(total_count);
        let start = offset.min(total_count);
        let end = (start + limit).min(total_count);

        let has_more = end < total_count;
        let entries = filtered[start..end].to_vec();

        LogBatch::new(entries, total_count, has_more)
    }

    /// Get latest N logs
    pub fn get_latest(&self, count: usize) -> Vec<LogEntry> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let len = entries.len();
        if len <= count {
            entries.clone()
        } else {
            entries[len - count..].to_vec()
        }
    }

    /// Clear all logs
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Get total log count
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }
}

impl Logger {
    /// Create a new logger
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            buffer: LogBuffer::new(10000),
            #[cfg(feature = "subscriber")]
            subscribers: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Create logger with custom buffer capacity
    pub fn with_capacity(session_id: impl Into<String>, capacity: usize) -> Self {
        Self {
            session_id: session_id.into(),
            buffer: LogBuffer::new(capacity),
            #[cfg(feature = "subscriber")]
            subscribers: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Log at DEBUG level
    pub fn debug(&self, message: impl Into<String>) {
        self.log(LogLevel::Debug, message);
    }

    /// Log at INFO level
    pub fn info(&self, message: impl Into<String>) {
        self.log(LogLevel::Info, message);
    }

    /// Log at WARN level
    pub fn warn(&self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message);
    }

    /// Log at ERROR level
    pub fn error(&self, message: impl Into<String>) {
        self.log(LogLevel::Error, message);
    }

    /// Log at specific level
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        let entry = LogEntry::new(level, message).with_session(&self.session_id);

        self.buffer.push(entry.clone());

        #[cfg(feature = "subscriber")]
        self.notify_subscribers(entry);
    }

    /// Log with source module
    pub fn log_with_source(
        &self,
        level: LogLevel,
        source: impl Into<String>,
        message: impl Into<String>,
    ) {
        let entry = LogEntry::new(level, message)
            .with_session(&self.session_id)
            .with_source(source);

        self.buffer.push(entry.clone());

        #[cfg(feature = "subscriber")]
        self.notify_subscribers(entry);
    }

    /// Log with context
    pub fn log_with_context(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        context: impl Into<String>,
    ) {
        let entry = LogEntry::new(level, message)
            .with_session(&self.session_id)
            .with_context(context);

        self.buffer.push(entry.clone());

        #[cfg(feature = "subscriber")]
        self.notify_subscribers(entry);
    }

    /// Get logs matching filter
    pub fn get_logs(&self, filter: &LogFilter) -> LogBatch {
        self.buffer.get_logs(filter)
    }

    /// Get latest N logs
    pub fn get_latest(&self, count: usize) -> Vec<LogEntry> {
        self.buffer.get_latest(count)
    }

    /// Get all logs as JSON
    pub fn get_logs_json(&self, filter: &LogFilter) -> Result<String, String> {
        let batch = self.buffer.get_logs(filter);
        batch.to_json()
    }

    /// Clear all logs
    pub fn clear(&self) {
        self.buffer.clear();
    }

    /// Get log count
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[cfg(feature = "subscriber")]
    /// Subscriber Methods (only available with "subscriber" feature).
    ///
    fn notify_subscribers(&self, entry: LogEntry) {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        let mut to_remove = Vec::new();

        for (i, tx) in subscribers.iter().enumerate() {
            if tx.send(entry.clone()).is_err() {
                to_remove.push(i);
            }
        }

        for i in to_remove.into_iter().rev() {
            subscribers.remove(i);
        }
    }

    /// Subscribe to real-time log stream
    #[cfg(feature = "subscriber")]
    pub fn subscribe(&self) -> crate::subscriber::LogSubscriber {
        crate::subscriber::LogSubscriber::new(self)
    }
}
