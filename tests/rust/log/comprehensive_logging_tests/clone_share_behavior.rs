

#[test]
fn test_logger_clone_shares_buffer() {
    let logger1 = Logger::new("test");
    let logger2 = logger1.clone();

    logger1.info("from logger1");
    logger2.info("from logger2");

    assert_eq!(logger1.len(), 2);
    assert_eq!(logger2.len(), 2);

    let batch = logger1.get_logs(&LogFilter::new());
    assert_eq!(batch.entries.len(), 2);
}

#[test]
fn test_logger_clear_affects_all_clones() {
    let logger1 = Logger::new("test");
    logger1.info("msg1");
    logger1.info("msg2");

    let logger2 = logger1.clone();
    assert_eq!(logger2.len(), 2);

    logger1.clear();
    assert_eq!(logger2.len(), 0);
}
