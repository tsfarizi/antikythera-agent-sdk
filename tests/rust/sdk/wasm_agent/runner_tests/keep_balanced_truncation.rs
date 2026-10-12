

#[test]
#[serial_test::serial]
/// KeepBalanced truncation -- head and tail of history are retained.
///
/// Inline context_policy in every prepare call isolates this test from concurrent init() calls
/// that overwrite the global default_config.context_policy.
///
/// 3 commit cycles -> 6 messages; policy is passed inline so it cannot be overridden by a
/// concurrent test calling init() with a different policy.
///
/// 4th prepare: history=6, summarize_after=4 -> triggers KeepBalanced truncation.
///
fn keep_balanced_truncation_retains_head_and_tail() {
    let kb_policy = serde_json::json!({
        "max_history_messages": 4,
        "summarize_after_messages": 4,
        "summary_max_chars": 100,
        "truncation_strategy": "keep_balanced"
    });

    let session_id = init(&serde_json::json!({"max_steps": 30}).to_string()).unwrap();

    for i in 0..3 {
        let prepared = prepare_user_turn(
            &serde_json::json!({
                "prompt": format!("msg {i}"),
                "session_id": session_id,
                "system_prompt": "assistant",
                "context_policy": kb_policy
            })
            .to_string(),
        )
        .unwrap();
        commit_llm_response(&prepared, &format!("resp {i}")).unwrap();
    }

    prepare_user_turn(
        &serde_json::json!({
            "prompt": "trigger",
            "session_id": session_id,
            "system_prompt": "assistant",
            "context_policy": kb_policy
        })
        .to_string(),
    )
    .unwrap();

    let state_json = get_state(&session_id).unwrap();
    let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();

    assert!(
        !state["rolling_summary"].is_null(),
        "rolling_summary should have been created"
    );

    let history_len = state["message_history"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(
        history_len <= 4,
        "KeepBalanced should truncate history to at most max_history_messages (4), got {history_len}"
    );
}
