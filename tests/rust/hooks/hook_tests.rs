use std::collections::HashMap;
use std::sync::Arc;

use antikythera_core::application::hooks::{
    AuthHook, CorrelationHook, HookContext, HookError, HookOperation, HookRegistry,
    HostHookMiddleware, InMemoryTelemetryHook, PolicyDecision, PolicyDecisionHook,
    PolicyDecisionInput, PolicyTarget,
};
use antikythera_core::application::observability::{CallerContext, TelemetryEvent};

struct RequireUser;

impl AuthHook for RequireUser {
    fn name(&self) -> &'static str {
        "require_user"
    }

    fn authorize(&self, context: &HookContext) -> Result<(), HookError> {
        if context.caller.user_id.is_some() {
            Ok(())
        } else {
            Err(HookError::new(self.name(), "missing user"))
        }
    }
}

struct AddCorrelation;

impl CorrelationHook for AddCorrelation {
    fn name(&self) -> &'static str {
        "add_correlation"
    }

    fn apply(&self, context: &mut HookContext) -> Result<(), HookError> {
        if context.correlation_id.is_none() {
            context.correlation_id = Some("corr-hook".to_string());
            context.caller.correlation_id = Some("corr-hook".to_string());
        }
        context
            .metadata
            .insert("trace_origin".to_string(), "integration-test".to_string());
        Ok(())
    }
}

struct ToolPolicy;

impl PolicyDecisionHook for ToolPolicy {
    fn name(&self) -> &'static str {
        "tool_policy"
    }

    fn decide(
        &self,
        input: &antikythera_core::application::hooks::PolicyDecisionInput,
    ) -> Result<PolicyDecision, HookError> {
        match &input.target {
            PolicyTarget::Tool { tool_name } if tool_name == "blocked" => Ok(PolicyDecision::Deny),
            _ => Ok(PolicyDecision::Allow),
        }
    }
}

include!("hook_tests/middleware_prepare_context.rs");
include!("hook_tests/middleware_policy_tool.rs");
include!("hook_tests/telemetry_hook_emit.rs");
include!("hook_tests/hook_registry_comprehensive.rs");
