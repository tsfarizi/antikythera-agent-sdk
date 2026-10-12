use super::*;
use serial_test::serial;

#[test]
#[serial]
fn get_sdk_logger_returns_logger_for_session() {
    let logger = get_sdk_logger("unique-test-session-1");
    assert_eq!(logger.session_id(), "unique-test-session-1");
}

#[test]
#[serial]
/// Arc::ptr_eq checks they point to the same allocation.
///
fn get_sdk_logger_returns_same_instance_for_same_id() {
    let a = get_sdk_logger("unique-test-session-2");
    let b = get_sdk_logger("unique-test-session-2");
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
#[serial]
/// After clear, querying returns empty.
///
fn clear_sdk_loggers_removes_all() {
    get_sdk_logger("unique-s1");
    get_sdk_logger("unique-s2");
    clear_sdk_loggers();
    let batch = query_sdk_logs("unique-s1", &LogFilter::new());
    assert_eq!(batch.total_count, 0);
}

#[test]
#[serial]
fn query_nonexistent_session_returns_empty() {
    let batch = query_sdk_logs("nonexistent-session-xyz", &LogFilter::new());
    assert_eq!(batch.total_count, 0);
    assert!(batch.entries.is_empty());
}

#[test]
#[serial]
/// No panic = pass; verify logs were recorded.
///
fn config_ffi_logger_logs_without_panic() {
    let logger = ConfigFfiLogger::new("unique-ffi-test");
    logger.ffi_call("test_fn", "{}");
    logger.ffi_result("test_fn", true, 100);
    logger.ffi_error("test_fn", "err");
    logger.config_loaded("file", 1024);
    logger.config_saved("/path", 512);
    logger.provider_added("openai");
    logger.provider_removed("openai");
    logger.prompt_updated("system");
    logger.agent_config_changed("model", "gpt-4");
    let batch = query_sdk_logs("unique-ffi-test", &LogFilter::new());
    assert!(batch.total_count > 0);
}
