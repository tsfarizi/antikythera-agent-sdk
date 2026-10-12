use super::*;

#[test]
fn logger_new_stores_session_id() {
    let logger = Logger::new("test-session");
    assert_eq!(logger.session_id(), "test-session");
}

#[test]
fn logger_log_levels_record_correctly() {
    let logger = Logger::new("test");
    logger.debug("debug msg");
    logger.info("info msg");
    logger.warn("warn msg");
    logger.error("error msg");

    let logs = logger.get_logs(&LogFilter::new());
    assert_eq!(logs.total_count, 4);
    assert_eq!(logs.entries[0].level, LogLevel::Debug);
    assert_eq!(logs.entries[1].level, LogLevel::Info);
    assert_eq!(logs.entries[2].level, LogLevel::Warn);
    assert_eq!(logs.entries[3].level, LogLevel::Error);
}

#[test]
fn log_buffer_evicts_oldest_on_overflow() {
    let logger = Logger::with_capacity("test", 3);
    logger.info("a");
    logger.info("b");
    logger.info("c");
    logger.info("d");

    let logs = logger.get_logs(&LogFilter::new());
    assert_eq!(logs.total_count, 3);
    assert_eq!(logs.entries[0].message, "b");
    assert_eq!(logs.entries[1].message, "c");
    assert_eq!(logs.entries[2].message, "d");
}

#[test]
fn log_filter_by_level() {
    let logger = Logger::new("test");
    logger.debug("debug");
    logger.info("info");
    logger.error("error");

    let filter = LogFilter::new().min_level(LogLevel::Warn);
    let logs = logger.get_logs(&filter);
    assert_eq!(logs.total_count, 1);
    assert_eq!(logs.entries[0].level, LogLevel::Error);
}

#[test]
fn log_filter_pagination() {
    let logger = Logger::new("test");
    for i in 0..10 {
        logger.info(format!("msg-{i}"));
    }

    let filter = LogFilter::new().offset(3).limit(2);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.entries.len(), 2);
    assert_eq!(batch.entries[0].message, "msg-3");
    assert_eq!(batch.entries[1].message, "msg-4");
    assert!(batch.has_more);
}

#[test]
fn logger_clone_shares_buffer() {
    let logger = Logger::new("test");
    let clone = logger.clone();

    logger.info("from-original");
    clone.info("from-clone");

    let logs = logger.get_logs(&LogFilter::new());
    assert_eq!(logs.total_count, 2);
}

#[test]
fn log_with_source_and_context() {
    let logger = Logger::new("test");
    logger.log_with_source(LogLevel::Info, "agent", "agent started");
    logger.log_with_context(LogLevel::Debug, "processing", "{\"step\": 1}");

    let logs = logger.get_logs(&LogFilter::new());
    assert_eq!(logs.total_count, 2);
    assert_eq!(logs.entries[0].source.as_deref(), Some("agent"));
    assert_eq!(logs.entries[1].context.as_deref(), Some("{\"step\": 1}"));
}

#[test]
fn log_level_from_str() {
    assert_eq!("debug".parse::<LogLevel>().unwrap(), LogLevel::Debug);
    assert_eq!("INFO".parse::<LogLevel>().unwrap(), LogLevel::Info);
    assert_eq!("Warn".parse::<LogLevel>().unwrap(), LogLevel::Warn);
    assert_eq!("ERROR".parse::<LogLevel>().unwrap(), LogLevel::Error);
}

#[test]
fn log_buffer_clear() {
    let logger = Logger::new("test");
    logger.info("a");
    logger.info("b");
    assert_eq!(logger.len(), 2);

    logger.clear();
    assert_eq!(logger.len(), 0);
    assert!(logger.is_empty());
}
