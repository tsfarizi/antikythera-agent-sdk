

#[test]
fn sdk_default_retry_condition_applies_to_unconfigured_task() {
    use antikythera_core::application::agent::multi_agent::task::AgentTask;
    use antikythera_sdk::agents::{
        GuardrailOptions, OrchestratorOptions, RetryConditionOption, TimeoutGuardrailOptions,
    };

    let mut task = AgentTask::new("summarize this document");
    let options = OrchestratorOptions {
        default_retry_condition: RetryConditionOption::OnTransient,
        guardrails: GuardrailOptions {
            timeout: Some(TimeoutGuardrailOptions {
                max_timeout_ms: Some(5_000),
                require_explicit_timeout: true,
            }),
            ..GuardrailOptions::default()
        },
        ..OrchestratorOptions::default()
    };

    options.apply_to_task(&mut task);
    let retry = task.retry_policy.expect("retry policy should be injected");
    let retry_json = serde_json::to_value(&retry).expect("serialize retry policy");

    assert_eq!(retry_json["condition"], "on_transient");
    assert!(options.guardrails.timeout.is_some());
}

#[test]
fn sdk_guardrail_options_is_empty_detects_no_guards() {
    use antikythera_sdk::agents::{
        BudgetGuardrailOptions, GuardrailOptions, RateLimitGuardrailOptions,
        TimeoutGuardrailOptions,
    };

    let empty = GuardrailOptions::default();
    assert!(empty.is_empty());

    let options = GuardrailOptions {
        timeout: Some(TimeoutGuardrailOptions {
            max_timeout_ms: Some(3_000),
            require_explicit_timeout: true,
        }),
        budget: Some(BudgetGuardrailOptions {
            max_task_steps: Some(6),
            require_explicit_budget: true,
            allow_exhausted_orchestrator: false,
        }),
        rate_limit: Some(RateLimitGuardrailOptions {
            max_tasks: Some(2),
            window_ms: Some(1_000),
        }),
        cancellation: true,
    };

    assert!(!options.is_empty());
}

#[test]
fn budget_clone_shares_counter_state() {
    use antikythera_core::application::agent::multi_agent::budget::OrchestratorBudget;
    let budget = OrchestratorBudget::new().with_max_total_steps(10);
    let budget2 = budget.clone();
    budget.record_steps(4);
    assert_eq!(budget2.consumed_steps(), 4);
}

#[test]
fn budget_snapshot_reflects_current_state() {
    use antikythera_core::application::agent::multi_agent::budget::OrchestratorBudget;
    let budget = OrchestratorBudget::new()
        .with_max_total_steps(20)
        .with_max_total_tasks(5);
    budget.record_steps(7);
    budget.record_task_dispatch();
    budget.record_task_dispatch();
    let snap = budget.snapshot();
    assert_eq!(snap.consumed_steps, 7);
    assert_eq!(snap.dispatched_tasks, 2);
    assert_eq!(snap.max_total_steps, Some(20));
    assert_eq!(snap.max_total_tasks, Some(5));
}

#[test]
fn budget_snapshot_serde_roundtrip() {
    use antikythera_core::application::agent::multi_agent::budget::OrchestratorBudget;
    let budget = OrchestratorBudget::new().with_max_total_steps(10);
    budget.record_steps(3);
    let snap = budget.snapshot();
    let json = serde_json::to_string(&snap).expect("serialize");
    let restored: antikythera_core::application::agent::multi_agent::budget::BudgetSnapshot =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.consumed_steps, 3);
    assert_eq!(restored.max_total_steps, Some(10));
}
