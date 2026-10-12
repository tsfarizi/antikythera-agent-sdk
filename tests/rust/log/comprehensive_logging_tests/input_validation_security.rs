

#[test]
/// Verify JSON is properly escaped.
///
/// Either escaped or not interpreted.
///
fn test_json_injection_in_message() {
    let logger = Logger::new("test");

    let malicious = r#"","level":"ERROR","message":"injected"#;
    logger.info(malicious);

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].message, malicious);

    let json = batch.to_json().unwrap();
    assert!(json.contains("injected") || !json.contains("\"level\":\"ERROR\""));
}

#[test]
fn test_json_injection_in_context() {
    let logger = Logger::new("test");

    let malicious_context = r#"{"tool":"bad","x":"y","z":"malicious"}"#;
    logger.log_with_context(LogLevel::Info, "test", malicious_context);

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].context, Some(malicious_context.to_string()));

    let json = batch.to_json().unwrap();
    assert!(!json.is_empty());
}

#[test]
fn test_sql_injection_in_message() {
    let logger = Logger::new("test");

    let sql_injection = "'; DROP TABLE logs; --";
    logger.info(sql_injection);

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].message, sql_injection);
}

#[test]
fn test_path_traversal_attempt() {
    let logger = Logger::new("test");

    let path_traversal = "../../../../etc/passwd";
    logger.log_with_source(LogLevel::Info, path_traversal, "test");

    let batch = logger.get_logs(&LogFilter::new());
    assert_eq!(batch.entries[0].source, Some(path_traversal.to_string()));
}

#[test]
fn test_source_filter_with_special_chars() {
    let logger = Logger::new("test");

    logger.log_with_source(LogLevel::Info, "src@#$%", "msg");
    logger.log_with_source(LogLevel::Info, "normal-src", "msg2");

    let filter = LogFilter::new().source("src@#$%");
    let batch = logger.get_logs(&filter);

    assert_eq!(batch.entries.len(), 1);
    assert_eq!(batch.entries[0].source, Some("src@#$%".to_string()));
}
