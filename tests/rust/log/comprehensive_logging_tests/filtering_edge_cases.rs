

#[test]
fn test_log_level_parsing() {
    let levels = vec!["debug", "DEBUG", "info", "INFO", "warn", "WARN", "error", "ERROR"];

    for level_str in levels {
        let level = LogLevel::parse_label(level_str);
        assert!(level.is_some(), "Failed to parse: {}", level_str);
    }

    let invalid = LogLevel::parse_label("invalid");
    assert!(invalid.is_none());
}

#[test]
fn test_filter_source_with_empty_string() {
    let logger = Logger::new("test");

    logger.log_with_source(LogLevel::Info, "", "msg1");
    logger.log_with_source(LogLevel::Info, "src", "msg2");

    let filter = LogFilter::new().source("");
    let batch = logger.get_logs(&filter);

    assert_eq!(batch.entries.len(), 1);
    assert_eq!(batch.entries[0].source, Some("".to_string()));
}

#[test]
fn test_multiple_filters_combined() {
    let logger = Logger::new("test");

    for level in &[LogLevel::Debug, LogLevel::Info, LogLevel::Warn, LogLevel::Error] {
        for source in &["src1", "src2", "src3"] {
            logger.log_with_source(*level, *source, format!("msg-{:?}", level));
        }
    }

    let filter = LogFilter::new()
        .min_level(LogLevel::Warn)
        .source("src2")
        .limit(5);

    let batch = logger.get_logs(&filter);

    for entry in &batch.entries {
        assert!(entry.level >= LogLevel::Warn);
        assert_eq!(entry.source, Some("src2".to_string()));
    }
}

#[test]
/// Get all messages in pages of 10.
///
/// Verify order and no duplicates.
///
fn test_pagination_consistency() {
    let logger = Logger::new("test");

    for i in 0..100 {
        logger.info(format!("msg-{:03}", i));
    }

    let mut all_messages = vec![];
    for page in 0..10 {
        let filter = LogFilter::new()
            .offset(page * 10)
            .limit(10);

        let batch = logger.get_logs(&filter);
        all_messages.extend(batch.entries.iter().map(|e| e.message.clone()));
    }

    assert_eq!(all_messages.len(), 100);

    for (i, msg) in all_messages.iter().enumerate().take(100) {
        assert_eq!(msg, &format!("msg-{:03}", i));
    }
}
