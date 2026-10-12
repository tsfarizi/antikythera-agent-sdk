

#[test]
fn test_serialization_roundtrip_empty_fields() {
    let entry = LogEntry::new(LogLevel::Debug, "test");

    let json = entry.to_json().unwrap();
    let restored = LogEntry::from_json(&json).unwrap();

    assert_eq!(restored.level, entry.level);
    assert_eq!(restored.message, entry.message);
    assert_eq!(restored.session_id, entry.session_id);
    assert_eq!(restored.source, entry.source);
    assert_eq!(restored.context, entry.context);
}

#[test]
fn test_serialization_roundtrip_all_fields() {
    let entry = LogEntry::new(LogLevel::Warn, "msg")
        .with_session("s1")
        .with_source("src1")
        .with_context(r#"{"key":"value"}"#)
        .with_sequence(999);

    let json = entry.to_json().unwrap();
    let restored = LogEntry::from_json(&json).unwrap();

    assert_eq!(restored.level, LogLevel::Warn);
    assert_eq!(restored.message, "msg");
    assert_eq!(restored.session_id, Some("s1".to_string()));
    assert_eq!(restored.source, Some("src1".to_string()));
    assert_eq!(restored.context, Some(r#"{"key":"value"}"#.to_string()));
    assert_eq!(restored.sequence, 999);
}

#[test]
/// Should gracefully handle invalid JSON.
///
fn test_invalid_json_deserialization() {
    let invalid_json = r#"{"level":"invalid","message":"x"}"#;
    let result = LogEntry::from_json(invalid_json);

    assert!(result.is_err());
}

#[test]
/// Sequences should be monotonically increasing.
///
/// Should have 100 unique sequences (10 threads x 10 messages).
///
fn test_sequence_ordering_on_concurrent_logs() {
    let logger = Arc::new(Logger::new("sequence-test"));
    let barrier = Arc::new(Barrier::new(10));

    let mut handles = vec![];
    for _ in 0..10 {
        let logger_clone = logger.clone();
        let barrier_clone = barrier.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();
            for i in 0..10 {
                logger_clone.info(format!("msg-{}", i));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let batch = logger.get_logs(&LogFilter::new());
    let sequences: Vec<u64> = batch.entries.iter().map(|e| e.sequence).collect();

    for i in 1..sequences.len() {
        assert!(sequences[i] >= sequences[i - 1]);
    }

    assert_eq!(sequences.len(), 100);
}

#[test]
/// With limit < total, should indicate more.
///
/// With limit >= total, should not indicate more.
///
fn test_log_batch_has_more_flag() {
    let logger = Logger::new("test");

    for i in 0..100 {
        logger.info(format!("msg-{}", i));
    }

    let filter = LogFilter::new().limit(10);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.entries.len(), 10);
    assert!(batch.has_more);

    let filter = LogFilter::new().limit(1000);
    let batch = logger.get_logs(&filter);
    assert_eq!(batch.entries.len(), 100);
    assert!(!batch.has_more);
}
