struct AllowAuth;

impl AuthHook for AllowAuth {
    fn name(&self) -> &'static str {
        "allow_auth"
    }

    fn authorize(&self, _context: &HookContext) -> Result<(), HookError> {
        Ok(())
    }
}

struct DenyMissingUser;

impl AuthHook for DenyMissingUser {
    fn name(&self) -> &'static str {
        "deny_missing_user"
    }

    fn authorize(&self, context: &HookContext) -> Result<(), HookError> {
        if context.caller.user_id.is_some() {
            Ok(())
        } else {
            Err(HookError::new(self.name(), "user_id is required"))
        }
    }
}

struct InjectCorrelation;

impl CorrelationHook for InjectCorrelation {
    fn name(&self) -> &'static str {
        "inject_correlation"
    }

    fn apply(&self, context: &mut HookContext) -> Result<(), HookError> {
        if context.correlation_id.is_none() {
            *context = context.clone().with_correlation_id("corr-generated");
        }
        context
            .metadata
            .insert("source".to_string(), "hook".to_string());
        Ok(())
    }
}

struct DenyDangerousTool;

impl PolicyDecisionHook for DenyDangerousTool {
    fn name(&self) -> &'static str {
        "deny_dangerous_tool"
    }

    fn decide(&self, input: &PolicyDecisionInput) -> Result<PolicyDecision, HookError> {
        match &input.target {
            PolicyTarget::Tool { tool_name } if tool_name == "delete-all" => {
                Ok(PolicyDecision::Deny)
            }
            _ => Ok(PolicyDecision::Allow),
        }
    }
}

struct AuditAllModels;

impl PolicyDecisionHook for AuditAllModels {
    fn name(&self) -> &'static str {
        "audit_all_models"
    }

    fn decide(&self, input: &PolicyDecisionInput) -> Result<PolicyDecision, HookError> {
        match input.target {
            PolicyTarget::Model { .. } => Ok(PolicyDecision::Audit),
            PolicyTarget::Tool { .. } => Ok(PolicyDecision::Allow),
        }
    }
}

fn caller() -> CallerContext {
    CallerContext::new()
        .with_user_id("user-1")
        .with_source("test")
}

#[test]
fn hook_context_builder_sets_fields() {
    let context = HookContext::new(caller(), HookOperation::AgentRun)
        .with_session_id("sess-1")
        .with_correlation_id("corr-1")
        .with_metadata("env", "test");

    assert_eq!(context.session_id.as_deref(), Some("sess-1"));
    assert_eq!(context.correlation_id.as_deref(), Some("corr-1"));
    assert_eq!(
        context.metadata.get("env").map(String::as_str),
        Some("test")
    );
}

#[test]
fn policy_input_copies_context_state() {
    let context = HookContext::new(caller(), HookOperation::ToolCall)
        .with_session_id("sess-2")
        .with_correlation_id("corr-2")
        .with_metadata("scope", "internal");
    let input = context.policy_input(PolicyTarget::Tool {
        tool_name: "search".to_string(),
    });

    assert_eq!(input.session_id.as_deref(), Some("sess-2"));
    assert_eq!(input.correlation_id.as_deref(), Some("corr-2"));
    assert_eq!(
        input.metadata.get("scope").map(String::as_str),
        Some("internal")
    );
}

#[test]
fn hook_registry_counts_registered_hooks() {
    let registry = HookRegistry::new()
        .with_auth_hook(Arc::new(AllowAuth))
        .with_correlation_hook(Arc::new(InjectCorrelation))
        .with_policy_hook(Arc::new(DenyDangerousTool))
        .with_telemetry_hook(Arc::new(InMemoryTelemetryHook::new()));

    assert_eq!(registry.auth_hook_count(), 1);
    assert_eq!(registry.correlation_hook_count(), 1);
    assert_eq!(registry.policy_hook_count(), 1);
    assert_eq!(registry.telemetry_hook_count(), 1);
    assert!(!registry.is_empty());
}

#[test]
fn hook_registry_default_is_empty() {
    assert!(HookRegistry::new().is_empty());
}

#[test]
fn middleware_prepare_context_runs_auth_and_correlation_hooks() {
    let middleware = HostHookMiddleware::new(
        HookRegistry::new()
            .with_auth_hook(Arc::new(AllowAuth))
            .with_correlation_hook(Arc::new(InjectCorrelation)),
    );

    let prepared = middleware
        .prepare_context(HookContext::new(caller(), HookOperation::AgentRun))
        .expect("prepare_context should succeed");

    assert_eq!(prepared.correlation_id.as_deref(), Some("corr-generated"));
    assert_eq!(
        prepared.metadata.get("source").map(String::as_str),
        Some("hook")
    );
}

#[test]
fn middleware_prepare_context_propagates_auth_error() {
    let middleware =
        HostHookMiddleware::new(HookRegistry::new().with_auth_hook(Arc::new(DenyMissingUser)));

    let error = middleware
        .prepare_context(HookContext::new(
            CallerContext::new(),
            HookOperation::AgentRun,
        ))
        .expect_err("missing user should be rejected");

    assert_eq!(error.hook_name, "deny_missing_user");
}

#[test]
fn authorize_tool_returns_deny_when_any_policy_denies() {
    let middleware = HostHookMiddleware::new(
        HookRegistry::new().with_policy_hook(Arc::new(DenyDangerousTool)),
    );
    let context = HookContext::new(caller(), HookOperation::ToolCall);

    let decision = middleware
        .authorize_tool(&context, "delete-all")
        .expect("policy decision should succeed");

    assert_eq!(decision, PolicyDecision::Deny);
}

#[test]
fn authorize_model_returns_audit_when_policy_requests_it() {
    let middleware =
        HostHookMiddleware::new(HookRegistry::new().with_policy_hook(Arc::new(AuditAllModels)));
    let context = HookContext::new(caller(), HookOperation::ModelRequest);

    let decision = middleware
        .authorize_model(&context, "host", "gpt-host")
        .expect("policy decision should succeed");

    assert_eq!(decision, PolicyDecision::Audit);
}

#[test]
fn authorize_model_defaults_to_allow_when_no_policy_hooks_exist() {
    let middleware = HostHookMiddleware::new(HookRegistry::new());
    let decision = middleware
        .authorize_model(
            &HookContext::new(caller(), HookOperation::ModelRequest),
            "host",
            "gpt",
        )
        .expect("no policy hooks should allow");
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn in_memory_telemetry_hook_records_events() {
    let hook = InMemoryTelemetryHook::new();
    let middleware = HostHookMiddleware::new(
        HookRegistry::new().with_telemetry_hook(Arc::new(hook.clone())),
    );
    let context = HookContext::new(caller(), HookOperation::AgentRun)
        .with_correlation_id("corr-telemetry")
        .with_session_id("sess-telemetry");
    let event = TelemetryEvent::new(
        "agent_start",
        context.correlation_id.clone(),
        context.session_id.clone(),
    );

    middleware
        .emit_event(&context, event)
        .expect("telemetry emit should succeed");

    assert_eq!(hook.snapshot().len(), 1);
    assert_eq!(hook.snapshot()[0].event_type, "agent_start");
}

#[test]
fn hook_error_display_includes_name_and_message() {
    let error = HookError::new("auth", "denied");
    assert_eq!(error.to_string(), "auth: denied");
}

#[test]
fn policy_target_serialization_roundtrip() {
    let target = PolicyTarget::Model {
        provider: "host".to_string(),
        model: "gpt-host".to_string(),
    };
    let json = serde_json::to_string(&target).expect("serialize");
    let restored: PolicyTarget = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored, target);
}

#[test]
fn policy_decision_input_serialization_roundtrip() {
    let input = HookContext::new(caller(), HookOperation::ToolCall)
        .with_correlation_id("corr-3")
        .policy_input(PolicyTarget::Tool {
            tool_name: "search".to_string(),
        });

    let json = serde_json::to_string(&input).expect("serialize");
    let restored: PolicyDecisionInput = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.correlation_id.as_deref(), Some("corr-3"));
}
