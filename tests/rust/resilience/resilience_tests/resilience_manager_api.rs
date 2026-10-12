#[test]
fn new_manager_has_default_config() {
    let mgr = ResilienceManager::new();
    let config = mgr.config();
    assert_eq!(config.retry.max_attempts, 3);
    assert_eq!(config.timeout.llm_timeout_ms, 30_000);
}

#[test]
fn get_config_json_is_valid_json() {
    let mgr = ResilienceManager::new();
    let json = mgr.get_config_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("retry").is_some());
    assert!(parsed.get("timeout").is_some());
}

#[test]
fn set_config_from_json_updates_policy() {
    let mut mgr = ResilienceManager::new();
    let json = r#"{
        "retry": {"max_attempts": 7, "initial_delay_ms": 100, "max_delay_ms": 5000, "backoff_factor": 1.5},
        "timeout": {"llm_timeout_ms": 60000, "tool_timeout_ms": 5000}
    }"#;
    assert!(mgr.set_config_from_json(json).unwrap());
    assert_eq!(mgr.config().retry.max_attempts, 7);
    assert_eq!(mgr.config().timeout.llm_timeout_ms, 60_000);
}

#[test]
fn set_config_from_invalid_json_returns_error() {
    let mut mgr = ResilienceManager::new();
    assert!(mgr.set_config_from_json("not-json").is_err());
}

#[test]
fn get_health_json_starts_as_empty_array() {
    let mgr = ResilienceManager::new();
    let json = mgr.get_health_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}

#[test]
fn reset_health_clears_tracked_components() {
    let mut mgr = ResilienceManager::new();
    mgr.health_mut().record_success("llm", 200);
    mgr.reset_health();
    let json = mgr.get_health_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}

#[test]
fn estimate_tokens_returns_positive_for_non_empty_text() {
    assert!(ResilienceManager::estimate_tokens("hello world") > 0);
}

#[test]
fn prune_messages_json_roundtrips_valid_input() {
    let messages = vec![
        ChatMessage::new(MessageRole::User, "hello"),
        ChatMessage::new(MessageRole::Assistant, "hi there"),
    ];
    let input_json = serde_json::to_string(&messages).unwrap();
    let result = ResilienceManager::prune_messages_json(&input_json, 10_000, 100);
    assert!(result.is_ok());
    let pruned: Vec<ChatMessage> = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(pruned.len(), 2);
}

#[test]
fn prune_messages_json_returns_error_for_invalid_input() {
    let result = ResilienceManager::prune_messages_json("[invalid", 1000, 100);
    assert!(result.is_err());
}
