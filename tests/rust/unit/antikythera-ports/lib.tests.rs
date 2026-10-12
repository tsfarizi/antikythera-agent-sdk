use crate::id_generator::{IdGenerator, UuidGenerator};
use crate::tool_server::ToolAnnotations;
use crate::types::{LogBatch, LogEntry, LogFilter, LogLevel};

#[test]
fn log_level_ordering() {
    assert!(LogLevel::Debug < LogLevel::Info);
    assert!(LogLevel::Info < LogLevel::Warn);
    assert!(LogLevel::Warn < LogLevel::Error);
}

#[test]
fn log_level_as_str() {
    assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(LogLevel::Info.as_str(), "INFO");
    assert_eq!(LogLevel::Warn.as_str(), "WARN");
    assert_eq!(LogLevel::Error.as_str(), "ERROR");
}

#[test]
fn log_level_parse_label() {
    assert_eq!(LogLevel::parse_label("DEBUG"), Some(LogLevel::Debug));
    assert_eq!(LogLevel::parse_label("info"), Some(LogLevel::Info));
    assert_eq!(LogLevel::parse_label("WARN"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::parse_label("ERROR"), Some(LogLevel::Error));
    assert_eq!(LogLevel::parse_label("invalid"), None);
}

#[test]
fn log_level_serialization_roundtrip() {
    let json = serde_json::to_string(&LogLevel::Warn).unwrap();
    let restored: LogLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, LogLevel::Warn);
}

#[test]
fn log_entry_serialization_roundtrip() {
    let entry = LogEntry::new(LogLevel::Info, "test message")
        .with_session("s1")
        .with_source("mod")
        .with_sequence(42);
    let json = serde_json::to_string(&entry).unwrap();
    let restored: LogEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.level, LogLevel::Info);
    assert_eq!(restored.message, "test message");
    assert_eq!(restored.session_id.as_deref(), Some("s1"));
    assert_eq!(restored.source.as_deref(), Some("mod"));
    assert_eq!(restored.sequence, 42);
}

#[test]
fn log_filter_matches_level() {
    let filter = LogFilter::new().min_level(LogLevel::Warn);
    let entry_debug = LogEntry::new(LogLevel::Debug, "d");
    let entry_error = LogEntry::new(LogLevel::Error, "e");
    assert!(!filter.matches(&entry_debug));
    assert!(filter.matches(&entry_error));
}

#[test]
fn log_batch_serialization_roundtrip() {
    let batch = LogBatch::new(vec![LogEntry::new(LogLevel::Info, "a")], 1, false);
    let json = serde_json::to_string(&batch).unwrap();
    let restored: LogBatch = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.total_count, 1);
    assert!(!restored.has_more);
}

#[test]
fn uuid_generator_produces_unique_ids() {
    let id_gen = UuidGenerator;
    let a = id_gen.new_id();
    let b = id_gen.new_id();
    assert_ne!(a, b);
    assert!(!a.is_empty());
}

#[test]
fn tool_annotations_default() {
    let ann = ToolAnnotations::default();
    assert!(ann.title.is_none());
    assert!(ann.read_only_hint.is_none());
}
