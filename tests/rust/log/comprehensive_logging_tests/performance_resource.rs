

#[test]
/// Should complete in reasonable time (adjust threshold as needed).
///
fn test_rapid_sequential_logging() {
    let logger = Logger::with_capacity("perf-test", 100_000);
    let start = Instant::now();

    for i in 0..100_000 {
        logger.info(format!("msg-{}", i));
    }

    let elapsed = start.elapsed();
    let per_second = (100_000.0 / elapsed.as_secs_f64()) as u64;

    assert_eq!(logger.len(), 100_000);
    cli_print!("Logged 100k messages in {:?} ({} msg/sec)", elapsed, per_second);

    assert!(elapsed.as_secs() < 10, "Logging too slow: {:?}", elapsed);
}

#[test]
fn test_large_batch_retrieval() {
    let logger = Logger::with_capacity("large-batch", 10_000);

    for i in 0..10_000 {
        logger.info(format!("msg-{}", i));
    }

    let start = Instant::now();
    let batch = logger.get_logs(&LogFilter::new());
    let elapsed = start.elapsed();

    assert_eq!(batch.entries.len(), 10_000);
    cli_print!("Retrieved 10k entries in {:?}", elapsed);
    assert!(elapsed.as_millis() < 1000, "Retrieval too slow");
}

#[test]
fn test_filter_performance() {
    let logger = Logger::with_capacity("filter-perf", 50_000);

    for i in 0..50_000 {
        let level = match i % 4 {
            0 => LogLevel::Debug,
            1 => LogLevel::Info,
            2 => LogLevel::Warn,
            _ => LogLevel::Error,
        };
        logger.log(level, format!("msg-{}", i));
    }

    let start = Instant::now();
    let filter = LogFilter::new().min_level(LogLevel::Warn);
    let batch = logger.get_logs(&filter);
    let elapsed = start.elapsed();

    assert!(!batch.entries.is_empty());
    cli_print!("Filtered 50k entries in {:?}", elapsed);
    assert!(elapsed.as_millis() < 500, "Filter too slow");
}

#[test]
fn test_json_serialization_performance() {
    let logger = Logger::with_capacity("json-perf", 10_000);

    for i in 0..10_000 {
        logger.log_with_source(
            LogLevel::Info,
            format!("src-{}", i % 100),
            format!("msg-{}", i),
        );
    }

    let start = Instant::now();
    let json = logger.get_logs_json(&LogFilter::new()).unwrap();
    let elapsed = start.elapsed();

    assert!(!json.is_empty());
    cli_print!("Serialized 10k entries to JSON in {:?}", elapsed);
    assert!(elapsed.as_millis() < 2000, "JSON serialization too slow");
}
