

/// Build a minimal JSON config string.
fn config_json() -> String {
    r#"{"max_steps":10}"#.to_string()
}

/// Build a `prepare_user_turn` request JSON for a given session/prompt.
fn prepare_request(session_id: &str, prompt: &str) -> String {
    serde_json::json!({
        "prompt": prompt,
        "session_id": session_id,
        "system_prompt": "You are a helpful assistant",
        "force_json": false
    })
    .to_string()
}
