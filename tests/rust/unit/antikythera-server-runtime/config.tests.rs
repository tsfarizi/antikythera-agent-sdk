use super::*;

#[test]
fn default_policy_denies_every_tool() {
    let policy = GatePolicy::deny_all();
    let err = policy.check_tool(Destination::Local, "echo").unwrap_err();
    assert!(err.starts_with("permission: "), "got: {err}");
    assert_eq!(err, "permission: tool 'echo' not in allowlist");
    assert!(policy.check_tool(Destination::Remote, "echo").is_err());
    assert!(policy.check_tool(Destination::Mcp, "echo").is_err());
}

#[test]
/// The allowlist is per destination.
///
fn allowlist_grants_only_listed_tool() {
    let mut policy = GatePolicy::deny_all();
    policy.allow_tool(Destination::Local, "echo");
    assert!(policy.check_tool(Destination::Local, "echo").is_ok());
    let err = policy.check_tool(Destination::Local, "rm").unwrap_err();
    assert!(err.starts_with("permission: "));
    assert!(policy.check_tool(Destination::Remote, "echo").is_err());
}

#[test]
fn hook_gate_denies_by_default() {
    let policy = GatePolicy::deny_all();
    let err = policy.check_hook(HookName::DecideAction).unwrap_err();
    assert!(err.starts_with("permission: "));
    assert_eq!(err, "permission: hook 'decide-action' not allowed");

    let mut policy = GatePolicy::deny_all();
    policy.allow_hook(HookName::DecideAction);
    assert!(policy.check_hook(HookName::DecideAction).is_ok());
    assert!(policy.check_hook(HookName::PrepareTurn).is_err());
}

#[test]
fn llm_quota_denies_at_limit() {
    let mut policy = GatePolicy::deny_all();
    policy.set_llm_quota(3);
    assert!(policy.check_llm(0).is_ok());
    assert!(policy.check_llm(2).is_ok());
    let err = policy.check_llm(3).unwrap_err();
    assert!(err.starts_with("permission: "));
    assert_eq!(err, "permission: llm quota exceeded");

    let policy = GatePolicy::deny_all();
    assert!(policy.check_llm(10_000).is_ok());
}

#[test]
fn hook_name_kebab_case() {
    assert_eq!(HookName::PrepareTurn.as_str(), "prepare-turn");
    assert_eq!(HookName::DecideAction.as_str(), "decide-action");
    assert_eq!(HookName::HandleToolResult.as_str(), "handle-tool-result");
}

#[test]
/// Response-json may itself contain colons; only the FIRST colon splits.
///
/// Whitespace around the name is trimmed.
///
fn server_tool_spec_parses_name_before_first_colon() {
    let spec = ServerToolSpec::parse("server_echo:{\"ok\":true}").unwrap();
    assert_eq!(spec.name, "server_echo");
    assert_eq!(spec.response_json, serde_json::json!({"ok": true}));

    let spec = ServerToolSpec::parse("deep:{\"a:b\":{\"b\":1}}").unwrap();
    assert_eq!(spec.name, "deep");
    assert_eq!(spec.response_json, serde_json::json!({"a:b":{"b":1}}));

    let spec = ServerToolSpec::parse("  padded  :[1,2]").unwrap();
    assert_eq!(spec.name, "padded");
    assert_eq!(spec.response_json, serde_json::json!([1, 2]));
}

#[test]
fn server_tool_spec_rejects_missing_colon_and_bad_json() {
    let err = ServerToolSpec::parse("no-colon").unwrap_err();
    assert!(
        err.contains("expected <name>:<response-json>"),
        "got: {err}"
    );

    let err = ServerToolSpec::parse(":{\"ok\":true}").unwrap_err();
    assert!(err.contains("name must not be empty"), "got: {err}");

    let err = ServerToolSpec::parse("broken:not-json").unwrap_err();
    assert!(
        err.contains("invalid --server-tool response-json"),
        "got: {err}"
    );
    assert!(err.contains("broken"), "got: {err}");
}
