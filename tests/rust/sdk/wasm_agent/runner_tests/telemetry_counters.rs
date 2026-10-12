

#[test]
#[serial_test::serial]
/// Telemetry counters increment per turn.
///
fn telemetry_counters_increment_per_turn() {
    let session_id = init(&config_json()).unwrap();

    let prepared = prepare_user_turn(&prepare_request(&session_id, "turn 1")).unwrap();
    commit_llm_response(&prepared, "answer 1").unwrap();

    let prepared = prepare_user_turn(&prepare_request(&session_id, "turn 2")).unwrap();
    commit_llm_response(&prepared, "answer 2").unwrap();

    let telemetry_json = get_telemetry_snapshot(&session_id).unwrap();
    let t: serde_json::Value = serde_json::from_str(&telemetry_json).unwrap();

    assert!(
        t["counters"]["turns_prepared"].as_u64().unwrap_or(0) >= 2,
        "turns_prepared should be at least 2"
    );
    assert!(
        t["counters"]["llm_commits"].as_u64().unwrap_or(0) >= 2,
        "llm_commits should be at least 2"
    );
}
