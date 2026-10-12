#[test]
fn test_log_with_source() {
    let logger = Logger::new("test-source");

    logger.log_with_source(LogLevel::Info, "wasm-agent", "Agent started");

    let filter = LogFilter::new().source("wasm-agent");
    let batch = logger.get_logs(&filter);

    assert_eq!(batch.total_count, 1);
    assert_eq!(batch.entries[0].source, Some("wasm-agent".to_string()));
}


#[test]
fn test_log_with_context() {
    let logger = Logger::new("test-context");

    logger.log_with_context(
        LogLevel::Info,
        "Tool called",
        r#"{"tool": "get_weather", "args": {"city": "NYC"}}"#,
    );

    let latest = logger.get_latest(1);
    assert!(latest[0].context.is_some());
    assert!(latest[0].context.as_ref().unwrap().contains("get_weather"));
}
