

#[test]
#[serial_test::serial]
/// Plain-text response -> Final action.
///
fn plain_text_commit_returns_final_action() {
    let session_id = init(&config_json()).unwrap();
    let prepared = prepare_user_turn(&prepare_request(&session_id, "hello")).unwrap();

    let result_json = commit_llm_response(&prepared, "plain response").unwrap();
    let value: serde_json::Value = serde_json::from_str(&result_json).unwrap();

    assert_eq!(value["action"], "final");
    assert_eq!(value["content"], "plain response");
}
