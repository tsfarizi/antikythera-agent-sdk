#[test]
fn test_basic_logging() {
    let logger = Logger::new("test-session");

    logger.debug("Debug message");
    logger.info("Info message");
    logger.warn("Warn message");
    logger.error("Error message");

    assert_eq!(logger.len(), 4);
}


#[test]
fn test_log_levels() {
    let logger = Logger::new("test-levels");

    logger.debug("debug");
    logger.info("info");
    logger.warn("warn");
    logger.error("error");

    let latest = logger.get_latest(1);
    assert_eq!(latest[0].level, LogLevel::Error);
    assert_eq!(latest[0].message, "error");
}
