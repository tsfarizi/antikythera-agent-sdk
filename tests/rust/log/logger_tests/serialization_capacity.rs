#[test]
fn test_log_entry_serialization() {
    let entry = LogEntry::new(LogLevel::Info, "Test")
        .with_session("session-1")
        .with_source("wasm-agent")
        .with_sequence(42);

    let json = entry.to_json().unwrap();
    let restored = LogEntry::from_json(&json).unwrap();

    assert_eq!(restored.level, LogLevel::Info);
    assert_eq!(restored.message, "Test");
    assert_eq!(restored.session_id, Some("session-1".to_string()));
    assert_eq!(restored.sequence, 42);
}


#[test]
fn test_log_buffer_capacity() {
    let logger = Logger::with_capacity("test-capacity", 5);

    for i in 0..10 {
        logger.info(format!("msg-{}", i));
    }

    assert_eq!(logger.len(), 5);

    let latest = logger.get_latest(5);
    assert_eq!(latest[0].message, "msg-5");
    assert_eq!(latest[4].message, "msg-9");
}
