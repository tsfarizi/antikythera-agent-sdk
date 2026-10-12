

#[test]
#[serial_test::serial]
/// Set_context_policy -- global policy applied in next turn.
///
/// Inline high-threshold policy in every prepare call prevents concurrent init() calls from other
/// tests from mutating the global default_config and triggering early summarization on this
/// session.
///
/// Pump 4 turns so history has 8 messages. Inline policy prevents accidental summarization even
/// if default_config changes.
///
/// Override global policy to summarize after only 4 messages. Since history is already 8
/// messages, this triggers on next prepare.
///
/// Next prepare without inline context_policy uses global override policy.
///
fn set_context_policy_applies_global_policy_on_next_turn() {
    let no_summarize_policy = serde_json::json!({
        "max_history_messages": 20,
        "summarize_after_messages": 20,
        "summary_max_chars": 500,
        "truncation_strategy": "keep_newest"
    });

    let session_id = init(&serde_json::json!({"max_steps": 20}).to_string()).unwrap();

    for i in 0..4 {
        let prepared = prepare_user_turn(
            &serde_json::json!({
                "prompt": format!("message {i}"),
                "session_id": session_id,
                "system_prompt": "assistant",
                "context_policy": no_summarize_policy
            })
            .to_string(),
        )
        .unwrap();
        commit_llm_response(&prepared, &format!("reply {i}")).unwrap();
    }

    let override_ok = set_context_policy(
        &serde_json::json!({
            "policy": {
                "max_history_messages": 4,
                "summarize_after_messages": 4,
                "summary_max_chars": 120,
                "truncation_strategy": "keep_newest"
            }
        })
        .to_string(),
    )
    .unwrap();
    assert!(override_ok);

    prepare_user_turn(
        &serde_json::json!({
            "prompt": "trigger summary",
            "session_id": session_id,
            "system_prompt": "assistant"
        })
        .to_string(),
    )
    .unwrap();

    let state_json = get_state(&session_id).unwrap();
    let state: serde_json::Value = serde_json::from_str(&state_json).unwrap();
    assert!(
        !state["rolling_summary"].is_null(),
        "rolling_summary should have been created by the updated global context policy"
    );
}
