//! Orchestrator-level concurrency and step budget guardrails.
//!
//! [`OrchestratorBudget`] enforces two independent limits at the orchestrator
//! scope (as opposed to per-task limits):
//!
//! 1. **`max_concurrent_tasks`** — the maximum number of tasks that may be
//!    *executing* simultaneously across the entire orchestrator.  Tasks that
//!    exceed this limit are queued until a slot becomes available.  This is
//!    complementary to `ExecutionMode::Parallel { workers }`, which limits
//!    concurrency inside the scheduler; the budget adds a second layer that
//!    applies regardless of execution mode.
//!
//! 2. **`max_total_steps`** — a cumulative cap on the total number of agent
//!    reasoning steps consumed across *all* tasks dispatched during the
//!    lifetime of the orchestrator.  Once the budget is exhausted, new tasks
//!    are rejected with a `BudgetExhausted` error.
//!
//! Both limits are tracked with lock-free atomics so the budget can be shared
//! between concurrent async tasks without a `Mutex`.
//!
//! # Example
//!
//! ```rust
//! use antikythera_core::application::agent::multi_agent::budget::OrchestratorBudget;
//!
//! let budget = OrchestratorBudget::new()
//!     .with_max_concurrent_tasks(4)
//!     .with_max_total_steps(200);
//!
//! // Record that a task used 12 steps.
//! budget.record_steps(12);
//! assert_eq!(budget.consumed_steps(), 12);
//! assert!(!budget.is_step_budget_exhausted());
//! ```

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use serde::{Deserialize, Serialize};

/// Shared concurrency and step-budget guardrails for the orchestrator.
///
/// Clone this struct freely — all clones share the same atomic counters.
#[derive(Debug, Clone)]
pub struct OrchestratorBudget {
    /// Maximum number of tasks that may run simultaneously.
    /// `None` means unlimited.
    pub max_concurrent_tasks: Option<usize>,

    /// Maximum cumulative agent steps across all tasks.
    /// `None` means unlimited.
    pub max_total_steps: Option<usize>,

    /// Maximum total number of tasks that may be dispatched.
    /// `None` means unlimited.
    pub max_total_tasks: Option<usize>,

    consumed_steps: Arc<AtomicUsize>,
    dispatched_tasks: Arc<AtomicUsize>,
}

impl Default for OrchestratorBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorBudget {
    /// Create an unlimited budget (all guards disabled).
    pub fn new() -> Self {
        Self {
            max_concurrent_tasks: None,
            max_total_steps: None,
            max_total_tasks: None,
            consumed_steps: Arc::new(AtomicUsize::new(0)),
            dispatched_tasks: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Set the concurrent-task limit.
    pub fn with_max_concurrent_tasks(mut self, limit: usize) -> Self {
        self.max_concurrent_tasks = Some(limit);
        self
    }

    /// Set the cumulative step budget.
    pub fn with_max_total_steps(mut self, limit: usize) -> Self {
        self.max_total_steps = Some(limit);
        self
    }

    /// Set the total task dispatch limit.
    pub fn with_max_total_tasks(mut self, limit: usize) -> Self {
        self.max_total_tasks = Some(limit);
        self
    }

    /// Increment the dispatched-task counter and return the new total.
    ///
    /// Call this *before* dispatching a task.  The orchestrator should check
    /// [`is_task_budget_exhausted`] after this returns.
    ///
    /// [`is_task_budget_exhausted`]: OrchestratorBudget::is_task_budget_exhausted
    /// Accounting helpers (called by the orchestrator/scheduler).
    ///
    pub fn record_task_dispatch(&self) -> usize {
        self.dispatched_tasks.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Add `steps` to the cumulative step counter and return the new total.
    pub fn record_steps(&self, steps: usize) -> usize {
        self.consumed_steps.fetch_add(steps, Ordering::Relaxed) + steps
    }

    /// Current cumulative step count.
    pub fn consumed_steps(&self) -> usize {
        self.consumed_steps.load(Ordering::Relaxed)
    }

    /// Current dispatched-task count.
    pub fn dispatched_tasks(&self) -> usize {
        self.dispatched_tasks.load(Ordering::Relaxed)
    }

    /// `true` when the step budget is enabled and exhausted.
    pub fn is_step_budget_exhausted(&self) -> bool {
        self.max_total_steps
            .is_some_and(|max| self.consumed_steps() >= max)
    }

    /// `true` when the total-task budget is enabled and exhausted.
    pub fn is_task_budget_exhausted(&self) -> bool {
        self.max_total_tasks
            .is_some_and(|max| self.dispatched_tasks() >= max)
    }

    /// Remaining steps before the step budget is exhausted.
    /// Returns `usize::MAX` when no limit is set.
    pub fn remaining_steps(&self) -> usize {
        match self.max_total_steps {
            None => usize::MAX,
            Some(max) => max.saturating_sub(self.consumed_steps()),
        }
    }

    /// A serialisable snapshot of the current budget state.
    pub fn snapshot(&self) -> BudgetSnapshot {
        BudgetSnapshot {
            max_concurrent_tasks: self.max_concurrent_tasks,
            max_total_steps: self.max_total_steps,
            max_total_tasks: self.max_total_tasks,
            consumed_steps: self.consumed_steps(),
            dispatched_tasks: self.dispatched_tasks(),
        }
    }
}

/// A point-in-time, serialisable view of an [`OrchestratorBudget`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub max_concurrent_tasks: Option<usize>,
    pub max_total_steps: Option<usize>,
    pub max_total_tasks: Option<usize>,
    pub consumed_steps: usize,
    pub dispatched_tasks: usize,
}
