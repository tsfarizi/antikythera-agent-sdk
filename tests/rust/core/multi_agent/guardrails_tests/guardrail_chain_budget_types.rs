use std::sync::Arc;

use antikythera_core::application::agent::multi_agent::{
    AgentProfile, AgentTask, BudgetGuardrail, BudgetSnapshot, CancellationGuardrail, ErrorKind,
    GuardrailChain, GuardrailContext, GuardrailRejection, GuardrailStage, RateLimitGuardrail,
    TaskGuardrail, TaskResult, TimeoutGuardrail,
};

fn profile() -> AgentProfile {
    AgentProfile {
        id: "agent-1".to_string(),
        name: "Agent One".to_string(),
        role: "general".to_string(),
        system_prompt: None,
        max_steps: Some(8),
    }
}

fn context() -> GuardrailContext {
    GuardrailContext::new(
        BudgetSnapshot {
            max_concurrent_tasks: Some(2),
            max_total_steps: Some(20),
            max_total_tasks: Some(5),
            consumed_steps: 4,
            dispatched_tasks: 2,
        },
        false,
        1,
        "auto",
    )
}

#[test]
fn guardrail_stage_as_str_is_stable() {
    assert_eq!(GuardrailStage::PreCheck.as_str(), "pre_check");
    assert_eq!(GuardrailStage::MidCheck.as_str(), "mid_check");
    assert_eq!(GuardrailStage::PostCheck.as_str(), "post_check");
}

#[test]
fn guardrail_rejection_constructor_sets_fields() {
    let rejection = GuardrailRejection::new(
        "budget",
        GuardrailStage::PreCheck,
        ErrorKind::BudgetExhausted,
        "too many steps",
    );
    assert_eq!(rejection.guardrail_name, "budget");
    assert_eq!(rejection.stage, GuardrailStage::PreCheck);
    assert_eq!(rejection.error_kind, ErrorKind::BudgetExhausted);
    assert_eq!(rejection.message, "too many steps");
}

#[test]
fn guardrail_context_detects_step_budget_exhaustion() {
    let exhausted = GuardrailContext::new(
        BudgetSnapshot {
            max_concurrent_tasks: None,
            max_total_steps: Some(10),
            max_total_tasks: None,
            consumed_steps: 10,
            dispatched_tasks: 0,
        },
        false,
        1,
        "auto",
    );
    assert!(exhausted.step_budget_exhausted());
}

#[test]
fn guardrail_context_detects_task_budget_exhaustion() {
    let exhausted = GuardrailContext::new(
        BudgetSnapshot {
            max_concurrent_tasks: None,
            max_total_steps: None,
            max_total_tasks: Some(2),
            consumed_steps: 0,
            dispatched_tasks: 2,
        },
        false,
        1,
        "auto",
    );
    assert!(exhausted.task_budget_exhausted());
}

#[test]
fn empty_guardrail_chain_is_empty() {
    let chain = GuardrailChain::new();
    assert!(chain.is_empty());
    assert_eq!(chain.len(), 0);
}

#[test]
fn guardrail_chain_with_guardrail_increments_len() {
    let chain = GuardrailChain::new().with_guardrail(Arc::new(TimeoutGuardrail::new(5_000)));
    assert_eq!(chain.len(), 1);
    assert!(!chain.is_empty());
}

#[test]
fn timeout_guardrail_allows_lower_timeout() {
    let task = AgentTask::new("work").with_timeout_ms(1_000);
    let result = TimeoutGuardrail::new(5_000).pre_check(&task, &profile(), &context());
    assert!(result.is_ok());
}

#[test]
fn timeout_guardrail_rejects_missing_timeout_when_required() {
    let task = AgentTask::new("work");
    let err = TimeoutGuardrail::new(5_000)
        .require_timeout()
        .pre_check(&task, &profile(), &context())
        .expect_err("missing timeout should be rejected");
    assert_eq!(err.error_kind, ErrorKind::Permanent);
}

#[test]
fn timeout_guardrail_rejects_timeout_above_limit() {
    let task = AgentTask::new("work").with_timeout_ms(10_000);
    let err = TimeoutGuardrail::new(5_000)
        .pre_check(&task, &profile(), &context())
        .expect_err("timeout above limit should be rejected");
    assert_eq!(err.guardrail_name, "timeout");
}

#[test]
fn budget_guardrail_allows_task_within_limit() {
    let task = AgentTask::new("work").with_budget_steps(6);
    let result =
        BudgetGuardrail::new()
            .with_max_task_steps(8)
            .pre_check(&task, &profile(), &context());
    assert!(result.is_ok());
}

#[test]
fn budget_guardrail_requires_explicit_budget_when_configured() {
    let task = AgentTask::new("work");
    let err = BudgetGuardrail::new()
        .require_explicit_budget()
        .pre_check(&task, &profile(), &context())
        .expect_err("missing budget should be rejected");
    assert_eq!(err.error_kind, ErrorKind::Permanent);
}

#[test]
fn budget_guardrail_rejects_task_over_limit() {
    let task = AgentTask::new("work").with_budget_steps(12);
    let err = BudgetGuardrail::new()
        .with_max_task_steps(8)
        .pre_check(&task, &profile(), &context())
        .expect_err("over-budget task should be rejected");
    assert_eq!(err.error_kind, ErrorKind::BudgetExhausted);
}

#[test]
fn budget_guardrail_rejects_exhausted_orchestrator_budget() {
    let ctx = GuardrailContext::new(
        BudgetSnapshot {
            max_concurrent_tasks: None,
            max_total_steps: Some(4),
            max_total_tasks: None,
            consumed_steps: 4,
            dispatched_tasks: 0,
        },
        false,
        1,
        "auto",
    );
    let err = BudgetGuardrail::new()
        .pre_check(&AgentTask::new("work"), &profile(), &ctx)
        .expect_err("exhausted budget should reject");
    assert_eq!(err.error_kind, ErrorKind::BudgetExhausted);
}

#[test]
fn budget_guardrail_can_ignore_exhausted_orchestrator_budget() {
    let ctx = GuardrailContext::new(
        BudgetSnapshot {
            max_concurrent_tasks: None,
            max_total_steps: Some(4),
            max_total_tasks: None,
            consumed_steps: 4,
            dispatched_tasks: 0,
        },
        false,
        1,
        "auto",
    );
    let result = BudgetGuardrail::new()
        .allow_exhausted_orchestrator()
        .pre_check(&AgentTask::new("work"), &profile(), &ctx);
    assert!(result.is_ok());
}

#[test]
fn budget_guardrail_post_check_rejects_result_above_limit() {
    let result = TaskResult::success(
        "task-1".to_string(),
        "agent-1".to_string(),
        serde_json::json!("ok"),
        9,
        "session-1".to_string(),
    );
    let err = BudgetGuardrail::new()
        .with_max_task_steps(8)
        .post_check(&AgentTask::new("work"), &profile(), &result, &context())
        .expect_err("result above limit should reject");
    assert_eq!(err.stage, GuardrailStage::PostCheck);
}

#[test]
fn rate_limit_guardrail_allows_first_n_requests() {
    let guardrail = RateLimitGuardrail::new(2, 10_000);
    let task = AgentTask::new("work");
    assert!(guardrail.pre_check(&task, &profile(), &context()).is_ok());
    assert!(guardrail.pre_check(&task, &profile(), &context()).is_ok());
}

#[test]
fn rate_limit_guardrail_rejects_when_window_is_full() {
    let guardrail = RateLimitGuardrail::new(1, 10_000);
    let task = AgentTask::new("work");
    assert!(guardrail.pre_check(&task, &profile(), &context()).is_ok());
    let err = guardrail
        .pre_check(&task, &profile(), &context())
        .expect_err("second request should be rate limited");
    assert_eq!(err.error_kind, ErrorKind::Transient);
}

#[test]
fn cancellation_guardrail_allows_active_context() {
    let result =
        CancellationGuardrail::new().pre_check(&AgentTask::new("work"), &profile(), &context());
    assert!(result.is_ok());
}

#[test]
fn cancellation_guardrail_rejects_cancelled_pre_check() {
    let cancelled = GuardrailContext::new(context().budget_snapshot, true, 1, "auto");
    let err = CancellationGuardrail::new()
        .pre_check(&AgentTask::new("work"), &profile(), &cancelled)
        .expect_err("cancelled context should reject");
    assert_eq!(err.error_kind, ErrorKind::Cancelled);
}

#[test]
fn cancellation_guardrail_rejects_cancelled_mid_check() {
    let cancelled = GuardrailContext::new(context().budget_snapshot, true, 2, "auto");
    let err = CancellationGuardrail::new()
        .mid_check(&AgentTask::new("work"), &profile(), &cancelled)
        .expect_err("cancelled context should reject");
    assert_eq!(err.stage, GuardrailStage::MidCheck);
}

#[test]
fn guardrail_chain_stops_on_first_rejection() {
    let chain = GuardrailChain::new()
        .with_guardrail(Arc::new(TimeoutGuardrail::new(5_000).require_timeout()))
        .with_guardrail(Arc::new(BudgetGuardrail::new().with_max_task_steps(8)));
    let err = chain
        .check_pre(&AgentTask::new("work"), &profile(), &context())
        .expect_err("first guardrail should reject");
    assert_eq!(err.guardrail_name, "timeout");
}

#[test]
fn guardrail_chain_runs_post_checks() {
    let chain = GuardrailChain::new()
        .with_guardrail(Arc::new(BudgetGuardrail::new().with_max_task_steps(4)));
    let result = TaskResult::success(
        "task-1".to_string(),
        "agent-1".to_string(),
        serde_json::json!("ok"),
        5,
        "session-1".to_string(),
    );
    let err = chain
        .check_post(&AgentTask::new("work"), &profile(), &result, &context())
        .expect_err("post-check should reject oversized result");
    assert_eq!(err.guardrail_name, "budget");
}
