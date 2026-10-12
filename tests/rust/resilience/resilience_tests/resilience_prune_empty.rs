#[test]
fn resilience_manager_estimate_tokens_is_consistent() {
    let t1 = ResilienceManager::estimate_tokens("hello");
    let t2 = ResilienceManager::estimate_tokens("hello");
    assert_eq!(t1, t2, "token estimation must be deterministic");
    assert!(t1 > 0);
}


#[test]
fn resilience_manager_prune_messages_json_handles_empty_array() {
    let result = ResilienceManager::prune_messages_json("[]", 1000, 100);
    assert!(result.is_ok());
    let pruned: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
    assert!(pruned.is_empty());
}
