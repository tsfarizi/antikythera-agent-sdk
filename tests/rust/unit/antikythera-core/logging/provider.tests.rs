use super::*;

#[test]
fn test_provider_instantiation() {
    let _provider = AntikytheraLogProvider;
}

#[test]
/// Usize is always >= 0; just verify the call succeeds.
///
fn test_logger_count_returns_number() {
    let provider = AntikytheraLogProvider;
    let count = provider.logger_count();
    let _ = count;
}

#[test]
/// Logger should be usable without panic.
///
fn test_get_logger_returns_logger() {
    let provider = AntikytheraLogProvider;
    let logger = provider.get_logger("test-session");
    logger.log_info("test message".to_string());
}

#[test]
fn test_clear_all_loggers_no_panic() {
    let provider = AntikytheraLogProvider;
    provider.clear_all_loggers();
}
