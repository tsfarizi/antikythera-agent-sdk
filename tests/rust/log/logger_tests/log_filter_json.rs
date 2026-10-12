#[test]
fn test_log_filter() {
    let logger = Logger::new("test-filter");

    logger.debug("debug1");
    logger.info("info1");
    logger.warn("warn1");
    logger.error("error1");
    logger.debug("debug2");

    let filter = LogFilter::new().min_level(LogLevel::Warn);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.total_count, 2);
    assert_eq!(batch.entries.len(), 2);

    let filter = LogFilter::new().limit(2);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.entries.len(), 2);
    assert!(batch.has_more);
}


#[test]
fn test_log_json() {
    let logger = Logger::new("test-json");

    logger.info("Test message");

    let batch = logger.get_logs(&LogFilter::new());
    let json = batch.to_json().unwrap();

    assert!(json.contains("Test message"));
    assert!(json.contains("info"));
}
