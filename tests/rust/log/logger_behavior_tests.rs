use antikythera_log::{
    BatchLogExport, LogFilter, LogLevel, Logger, SessionLogExport, alog_debug, alog_error_ctx,
    alog_info, alog_info_src, alog_warn,
};

#[test]
fn basic_logging_records_levels_and_session() {
    let logger = Logger::new("session-a");

    logger.debug("debug message");
    logger.info("info message");
    logger.warn("warn message");
    logger.error("error message");

    let entries = logger.get_latest(10);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].level, LogLevel::Debug);
    assert_eq!(entries[1].level, LogLevel::Info);
    assert_eq!(entries[2].level, LogLevel::Warn);
    assert_eq!(entries[3].level, LogLevel::Error);
    assert!(
        entries
            .iter()
            .all(|entry| entry.session_id.as_deref() == Some("session-a"))
    );
}

#[test]
fn source_and_context_are_preserved() {
    let logger = Logger::new("session-b");

    logger.log_with_source(LogLevel::Info, "transport", "connected");
    logger.log_with_context(LogLevel::Error, "tool failed", r#"{"tool":"read"}"#);

    let entries = logger.get_latest(10);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].source.as_deref(), Some("transport"));
    assert_eq!(entries[1].context.as_deref(), Some(r#"{"tool":"read"}"#));
}

#[test]
fn filter_by_level_source_and_pagination_works() {
    let logger = Logger::new("session-c");
    logger.log_with_source(LogLevel::Debug, "agent", "d1");
    logger.log_with_source(LogLevel::Info, "agent", "i1");
    logger.log_with_source(LogLevel::Warn, "transport", "w1");
    logger.log_with_source(LogLevel::Error, "agent", "e1");

    let batch = logger.get_logs(
        &LogFilter::new()
            .min_level(LogLevel::Info)
            .source("agent")
            .limit(1)
            .offset(1),
    );

    assert_eq!(batch.total_count, 2);
    assert_eq!(batch.entries.len(), 1);
    assert!(!batch.has_more);
    assert_eq!(batch.entries[0].message, "e1");
}

#[test]
fn json_roundtrip_works_for_batches_and_entries() {
    let logger = Logger::new("session-d");
    logger.info("hello json");

    let entry = logger.get_latest(1).pop().expect("entry exists");
    let entry_json = entry.to_json().expect("entry json");
    let decoded_entry = antikythera_log::LogEntry::from_json(&entry_json).expect("entry decode");
    assert_eq!(decoded_entry.message, "hello json");

    let batch = logger.get_logs(&LogFilter::new());
    let batch_json = batch.to_json().expect("batch json");
    let decoded_batch = antikythera_log::LogBatch::from_json(&batch_json).expect("batch decode");
    assert_eq!(decoded_batch.total_count, 1);
    assert_eq!(decoded_batch.entries[0].message, "hello json");
}

#[test]
fn session_and_batch_exports_roundtrip() {
    let logger = Logger::new("session-e");
    logger.info("line 1");
    logger.warn("line 2");

    let logs = logger.get_latest(10);
    let export = SessionLogExport::from_logs("session-e", logs.clone()).with_notes("note");
    let export_json = export.to_json().expect("export json");
    let decoded_export = SessionLogExport::from_json(&export_json).expect("decode export");
    assert_eq!(decoded_export.session_id, "session-e");
    assert_eq!(decoded_export.log_count(), 2);

    let batch = BatchLogExport::from_session_logs(vec![decoded_export.clone()]).with_notes("batch");
    let batch_json = batch.to_json().expect("batch json");
    let decoded_batch = BatchLogExport::from_json(&batch_json).expect("decode batch");
    assert_eq!(decoded_batch.session_count(), 1);
    assert_eq!(decoded_batch.total_log_count(), 2);
}

#[test]
fn buffer_capacity_trims_oldest_entries() {
    let logger = Logger::with_capacity("session-f", 2);
    logger.info("first");
    logger.info("second");
    logger.info("third");

    let entries = logger.get_latest(10);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].message, "second");
    assert_eq!(entries[1].message, "third");
}

#[test]
fn convenience_macros_write_logs() {
    let logger = Logger::new("session-g");
    alog_info!(logger, "hello {}", "world");
    alog_debug!(logger, "debug {}", 7);
    alog_warn!(logger, "warn {}", 9);
    alog_info_src!(logger, "agent", "agent {}", "step");
    alog_error_ctx!(logger, "tool failed", r#"{"code":500}"#);

    let entries = logger.get_latest(10);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0].message, "hello world");
    assert_eq!(entries[1].message, "debug 7");
    assert_eq!(entries[2].message, "warn 9");
    assert_eq!(entries[3].source.as_deref(), Some("agent"));
    assert_eq!(entries[4].context.as_deref(), Some(r#"{"code":500}"#));
}

#[test]
fn subscriber_receives_live_entries() {
    let logger = Logger::new("session-h");
    let subscriber = logger.subscribe();

    logger.info("stream me");

    let entry = subscriber
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("entry received");
    assert_eq!(entry.message, "stream me");
    assert_eq!(entry.level, LogLevel::Info);
}
