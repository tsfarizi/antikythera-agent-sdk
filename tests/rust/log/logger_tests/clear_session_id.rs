#[test]
fn test_clear_logs() {
    let logger = Logger::new("test-clear");

    logger.info("msg1");
    logger.info("msg2");
    assert_eq!(logger.len(), 2);

    logger.clear();
    assert_eq!(logger.len(), 0);
}


#[test]
fn test_session_id() {
    let logger = Logger::new("my-session");
    assert_eq!(logger.session_id(), "my-session");

    logger.info("test");

    let latest = logger.get_latest(1);
    assert_eq!(latest[0].session_id, Some("my-session".to_string()));
}
