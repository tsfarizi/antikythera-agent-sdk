use antikythera_core::application::agent::multi_agent::{BudgetSnapshot, OrchestratorBudget};

#[test]
fn new_budget_has_no_limits() {
    let b = OrchestratorBudget::new();
    assert!(b.max_concurrent_tasks.is_none());
    assert!(b.max_total_steps.is_none());
    assert!(b.max_total_tasks.is_none());
    assert!(!b.is_step_budget_exhausted());
    assert!(!b.is_task_budget_exhausted());
}

#[test]
fn record_steps_accumulates() {
    let b = OrchestratorBudget::new().with_max_total_steps(50);
    b.record_steps(20);
    b.record_steps(15);
    assert_eq!(b.consumed_steps(), 35);
    assert!(!b.is_step_budget_exhausted());
    b.record_steps(15);
    assert_eq!(b.consumed_steps(), 50);
    assert!(b.is_step_budget_exhausted());
}

#[test]
fn remaining_steps_counts_down() {
    let b = OrchestratorBudget::new().with_max_total_steps(100);
    b.record_steps(40);
    assert_eq!(b.remaining_steps(), 60);
}

#[test]
fn remaining_steps_unlimited_returns_max() {
    let b = OrchestratorBudget::new();
    assert_eq!(b.remaining_steps(), usize::MAX);
}

#[test]
fn task_budget_exhausted_after_limit() {
    let b = OrchestratorBudget::new().with_max_total_tasks(3);
    b.record_task_dispatch();
    b.record_task_dispatch();
    assert!(!b.is_task_budget_exhausted());
    b.record_task_dispatch();
    assert!(b.is_task_budget_exhausted());
}

#[test]
fn clone_shares_state() {
    let b = OrchestratorBudget::new().with_max_total_steps(100);
    let b2 = b.clone();
    b.record_steps(30);
    assert_eq!(
        b2.consumed_steps(),
        30,
        "clone must observe parent mutations"
    );
}

#[test]
fn snapshot_is_serialisable() {
    let b = OrchestratorBudget::new()
        .with_max_concurrent_tasks(4)
        .with_max_total_steps(200);
    b.record_steps(50);
    let snap = b.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let restored: BudgetSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snap, restored);
    assert_eq!(restored.consumed_steps, 50);
}
