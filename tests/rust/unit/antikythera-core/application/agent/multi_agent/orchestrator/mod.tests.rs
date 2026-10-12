use super::*;
use crate::application::agent::multi_agent::budget::OrchestratorBudget;
use crate::application::agent::multi_agent::task::AgentTask;

/// Regression test: `dispatch_many` must reject tasks when the step budget
/// is exhausted.  Before the fix, only the task-budget guard was checked in
/// `dispatch_many`; the step-budget guard was missing.
#[tokio::test]
async fn dispatch_many_rejects_when_step_budget_exhausted() {
    let budget = OrchestratorBudget::new().with_max_total_steps(5);
    budget.record_steps(5);
    assert!(budget.is_step_budget_exhausted());

    let sem: Option<Arc<Semaphore>> = None;
    let default_retry = RetryCondition::Always;
    let task = AgentTask::new("should be rejected");

    let result = prepare_dispatch(task, &budget, &default_retry, &sem).await;
    assert!(result.is_err(), "expected step-budget rejection");

    let err = result.unwrap_err();
    assert!(!err.success);
    assert_eq!(err.error_kind, Some(ErrorKind::BudgetExhausted));
    assert!(err.metadata.budget_exhausted);
    let err_msg = err.error.unwrap();
    assert!(
        err_msg.contains("step budget exhausted"),
        "error should mention step budget, got: {err_msg}"
    );
}

#[tokio::test]
/// Dispatch the one allowed task.
///
async fn prepare_dispatch_rejects_when_task_budget_exhausted() {
    let budget = OrchestratorBudget::new().with_max_total_tasks(1);
    budget.record_task_dispatch();
    assert!(budget.is_task_budget_exhausted());

    let sem: Option<Arc<Semaphore>> = None;
    let default_retry = RetryCondition::Always;
    let task = AgentTask::new("should be rejected");

    let result = prepare_dispatch(task, &budget, &default_retry, &sem).await;
    assert!(result.is_err(), "expected task-budget rejection");

    let err = result.unwrap_err();
    assert!(!err.success);
    assert_eq!(err.error_kind, Some(ErrorKind::BudgetExhausted));
}

#[tokio::test]
async fn prepare_dispatch_sets_default_retry_policy() {
    let budget = OrchestratorBudget::new();
    let sem: Option<Arc<Semaphore>> = None;
    let default_retry = RetryCondition::OnTransient;
    let task = AgentTask::new("test");

    let result = prepare_dispatch(task, &budget, &default_retry, &sem)
        .await
        .unwrap();

    let policy = result.task.retry_policy.unwrap();
    assert_eq!(policy.condition, RetryCondition::OnTransient);
    assert_eq!(policy.max_retries, 0);
    assert_eq!(policy.backoff_ms, 0);
}

#[tokio::test]
async fn prepare_dispatch_preserves_existing_retry_policy() {
    let budget = OrchestratorBudget::new();
    let sem: Option<Arc<Semaphore>> = None;
    let default_retry = RetryCondition::Never;
    let task = AgentTask::new("test").with_retry_policy(TaskRetryPolicy {
        max_retries: 3,
        backoff_ms: 100,
        condition: RetryCondition::Always,
    });

    let result = prepare_dispatch(task, &budget, &default_retry, &sem)
        .await
        .unwrap();

    let policy = result.task.retry_policy.unwrap();
    assert_eq!(policy.max_retries, 3);
    assert_eq!(policy.backoff_ms, 100);
    assert_eq!(policy.condition, RetryCondition::Always);
}

#[tokio::test]
async fn prepare_dispatch_succeeds_when_budget_available() {
    let budget = OrchestratorBudget::new().with_max_total_steps(10);
    let sem: Option<Arc<Semaphore>> = None;
    let default_retry = RetryCondition::Always;
    let task = AgentTask::new("should succeed");

    let result = prepare_dispatch(task, &budget, &default_retry, &sem).await;
    assert!(result.is_ok(), "expected dispatch to succeed");
}
