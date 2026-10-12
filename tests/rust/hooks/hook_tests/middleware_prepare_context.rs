#[test]
fn middleware_prepare_context_updates_correlation_and_metadata() {
    let middleware = HostHookMiddleware::new(
        HookRegistry::new()
            .with_auth_hook(Arc::new(RequireUser))
            .with_correlation_hook(Arc::new(AddCorrelation)),
    );

    let prepared = middleware
        .prepare_context(
            HookContext::new(
                CallerContext::new().with_user_id("user-a"),
                HookOperation::AgentRun,
            )
            .with_session_id("sess-a"),
        )
        .expect("prepare context should succeed");

    assert_eq!(prepared.correlation_id.as_deref(), Some("corr-hook"));
    assert_eq!(
        prepared.metadata.get("trace_origin").map(String::as_str),
        Some("integration-test")
    );
}
