#[test]
fn middleware_policy_denies_blocked_tool() {
    let middleware =
        HostHookMiddleware::new(HookRegistry::new().with_policy_hook(Arc::new(ToolPolicy)));
    let context = HookContext::new(
        CallerContext::new().with_user_id("user-a"),
        HookOperation::ToolCall,
    );

    let decision = middleware
        .authorize_tool(&context, "blocked")
        .expect("policy evaluation should succeed");

    assert_eq!(decision, PolicyDecision::Deny);
}
