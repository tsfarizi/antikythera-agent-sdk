

#[test]
/// PANIC SAFETY & ERROR HANDLING.
///
/// Should not panic even with empty buffer.
///
fn test_no_panic_on_empty_log_retrieval() {
    let logger = Logger::new("test");

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries.len(), 0);

    let latest = logger.get_latest(10);
    assert_eq!(latest.len(), 0);
}

#[test]
/// Should handle extreme values without panic.
///
fn test_no_panic_on_extreme_limits() {
    let logger = Logger::new("test");
    logger.info("msg");

    let filter = LogFilter::new().limit(usize::MAX);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.entries.len(), 1);

    let filter = LogFilter::new().offset(usize::MAX);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.entries.len(), 0);
}

#[test]
/// Invalid JSON in context should not panic.
///
fn test_no_panic_on_context_with_invalid_json() {
    let logger = Logger::new("test");

    logger.log_with_context(LogLevel::Info, "msg", "not valid json {");

    assert_eq!(logger.len(), 1);
}

#[test]
fn test_no_panic_on_format_pretty_with_special_chars() {
    let entry = LogEntry::new(LogLevel::Info, "msg\0with\nnull\tand\rspecial")
        .with_session("s\x01\x02")
        .with_source("src\x03\x04");

    let formatted = entry.format_pretty();
    assert!(!formatted.is_empty());
}
