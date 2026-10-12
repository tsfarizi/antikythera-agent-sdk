

#[test]
/// EDGE CASES & BOUNDARY CONDITIONS.
///
fn test_empty_session_id() {
    let logger = Logger::new("");
    logger.info("test message");

    assert_eq!(logger.len(), 1);
    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].session_id, Some("".to_string()));
}

#[test]
fn test_very_long_session_id() {
    let long_id = "x".repeat(100_000);
    let logger = Logger::new(&long_id);
    logger.info("test");

    assert_eq!(logger.session_id(), long_id.as_str());
    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].session_id, Some(long_id));
}

#[test]
fn test_empty_message() {
    let logger = Logger::new("test");
    logger.info("");

    assert_eq!(logger.len(), 1);
    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].message, "");
}

#[test]
fn test_very_long_message() {
    let long_msg = "m".repeat(1_000_000);
    let logger = Logger::new("test");
    logger.info(&long_msg);

    assert_eq!(logger.len(), 1);
    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].message, long_msg);
}

#[test]
fn test_unicode_in_message() {
    let logger = Logger::new("test");
    let messages = vec![
        "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}",
        "\u{4f60}\u{597d}",
        "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}",
        "\u{1f980} Rust \u{1f680}",
        "Emoji test: \u{1f600}\u{1f603}\u{1f604}\u{1f601}\u{1f606}",
        "Combining: \u{00e9} = e + \u{0301}",
        "RTL: \u{202E}RTL\u{202C}",
    ];

    for msg in &messages {
        logger.info(*msg);
    }

    assert_eq!(logger.len(), messages.len());
    let batch = logger.get_logs(&LogFilter::new());
    for (i, msg) in messages.iter().enumerate() {
        assert_eq!(&batch.entries[i].message, msg);
    }
}

#[test]
fn test_unicode_in_session_id() {
    let logger = Logger::new("session_\u{1f31f}_\u{65e5}\u{672c}\u{8a9e}");
    logger.info("test");

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].session_id, Some("session_\u{1f31f}_\u{65e5}\u{672c}\u{8a9e}".to_string()));
}

#[test]
fn test_special_characters_in_message() {
    let logger = Logger::new("test");
    let special_msgs = vec![
        "Quote: \"hello\"",
        "Backslash: \\test\\",
        "Newline: hello\nworld",
        "Tab: hello\tworld",
        "Null byte: test\0null",
        "Control chars: \x01\x02\x03",
    ];

    for msg in &special_msgs {
        logger.info(*msg);
    }

    assert_eq!(logger.len(), special_msgs.len());
}

#[test]
/// With zero capacity, buffer should remain empty.
///
fn test_zero_buffer_capacity() {
    let logger = Logger::with_capacity("test", 0);
    logger.info("msg1");
    logger.info("msg2");

    assert_eq!(logger.len(), 0);
}

#[test]
fn test_single_capacity_buffer() {
    let logger = Logger::with_capacity("test", 1);

    logger.info("msg1");
    assert_eq!(logger.len(), 1);

    logger.info("msg2");
    assert_eq!(logger.len(), 1);

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].message, "msg2");
}

#[test]
fn test_get_latest_with_empty_buffer() {
    let logger = Logger::new("test");
    let latest = logger.get_latest(10);
    assert_eq!(latest.len(), 0);
}

#[test]
fn test_get_latest_exceeds_buffer() {
    let logger = Logger::new("test");
    logger.info("msg1");
    logger.info("msg2");
    logger.info("msg3");

    let latest = logger.get_latest(1000);
    assert_eq!(latest.len(), 3);
}

#[test]
fn test_filter_offset_beyond_range() {
    let logger = Logger::new("test");
    logger.info("msg1");
    logger.info("msg2");

    let filter = LogFilter::new().offset(100).limit(10);
    let batch = logger.get_logs(&filter);

    assert_eq!(batch.entries.len(), 0);
    assert!(!batch.has_more);
}

#[test]
/// Should still indicate more exists.
///
fn test_filter_limit_zero() {
    let logger = Logger::new("test");
    logger.info("msg1");
    logger.info("msg2");

    let filter = LogFilter::new().limit(0);
    let batch = logger.get_logs(&filter);

    assert_eq!(batch.entries.len(), 0);
    assert!(batch.has_more);
}

#[test]
fn test_multiple_clears() {
    let logger = Logger::new("test");

    for _ in 0..3 {
        logger.info("msg");
        assert_eq!(logger.len(), 1);
        logger.clear();
        assert_eq!(logger.len(), 0);
    }
}
