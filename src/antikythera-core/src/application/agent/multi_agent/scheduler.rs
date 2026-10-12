//! Task scheduler for the multi-agent orchestrator.
//!
//! [`TaskScheduler`] drives a batch of tasks using an [`ExecutionMode`]
//! policy.  It is generic over both the task type `T` and the executor
//! closure, so the orchestrator can pass pre-resolved `(AgentTask, Profile)`
//! pairs without the scheduler needing to know about them.

use std::future::Future;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Semaphore;

use super::execution::ExecutionMode;
use super::task::TaskResult;

/// Drives batches of tasks according to the configured [`ExecutionMode`].
///
/// # Generic parameters
///
/// The scheduler operates over a generic task type `T` and a generic async
/// executor `F`.  The orchestrator supplies these through the [`run`] method,
/// keeping the scheduler free of agent-specific logic.
///
/// [`run`]: TaskScheduler::run
pub struct TaskScheduler {
    /// How tasks are dispatched.
    pub mode: ExecutionMode,
}

impl TaskScheduler {
    /// Create a scheduler with the given execution mode.
    pub fn new(mode: ExecutionMode) -> Self {
        Self { mode }
    }

    /// Create a scheduler with the default [`ExecutionMode::Auto`] mode.
    pub fn auto() -> Self {
        Self::new(ExecutionMode::Auto)
    }

    /// Execute `tasks` using `executor` according to the configured mode.
    ///
    /// # Mode semantics
    ///
    /// | Mode | Behaviour |
    /// |---|---|
    /// | `Sequential` | Tasks are awaited one at a time in submission order. |
    /// | `Concurrent` | All tasks run as concurrent futures on the calling task's thread (no spawning). |
    /// | `Auto` | Each task is spawned as an independent `tokio::spawn` task. Parallelism is determined by the tokio runtime. |
    /// | `Parallel { workers }` | Same as `Auto` but at most `workers` tasks run simultaneously (semaphore-limited). |
    ///
    /// # Constraints
    ///
    /// - `T: Send + 'static` — required when tasks are moved into spawned tasks.
    /// - `F: Clone + 'static` — the executor is cloned for each spawned task.
    /// - `Fut: Send + 'static` — spawned futures must be sendable across threads.
    ///
    /// Sequential: await tasks one by one in order.
    ///
    /// Concurrent: poll all futures simultaneously, no spawning.
    ///
    /// Auto: spawn each task as an independent tokio task.
    ///
    /// Parallel: spawn tasks with a semaphore-based concurrency limit.
    ///
    /// Collect task IDs before moving tasks into closures.
    ///
    pub async fn run<T, F, Fut>(&self, tasks: Vec<T>, executor: F) -> Vec<TaskResult>
    where
        T: Send + 'static,
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = TaskResult> + Send + 'static,
    {
        if tasks.is_empty() {
            return Vec::new();
        }

        match self.mode {
            ExecutionMode::Sequential => {
                let mut results = Vec::with_capacity(tasks.len());
                for task in tasks {
                    results.push(executor.clone()(task).await);
                }
                results
            }

            ExecutionMode::Concurrent => {
                let futs: FuturesUnordered<_> =
                    tasks.into_iter().map(|t| executor.clone()(t)).collect();
                futs.collect().await
            }

            ExecutionMode::Auto => {
                let handles: Vec<_> = tasks
                    .into_iter()
                    .map(|t| {
                        let exec = executor.clone();
                        tokio::spawn(async move { exec(t).await })
                    })
                    .collect();

                let mut results = Vec::with_capacity(handles.len());
                for handle in handles {
                    match handle.await {
                        Ok(r) => results.push(r),
                        Err(e) => results.push(TaskResult::failure(
                            "unknown".to_string(),
                            "unknown".to_string(),
                            format!("Task panicked: {e}"),
                        )),
                    }
                }
                results
            }

            ExecutionMode::Parallel { workers } => {
                let sem = Arc::new(Semaphore::new(workers.max(1)));

                let handles: Vec<_> = tasks
                    .into_iter()
                    .map(|t| {
                        let exec = executor.clone();
                        let sem = sem.clone();
                        tokio::spawn(async move {
                            let _permit = sem
                                .acquire_owned()
                                .await
                                .expect("scheduler semaphore closed");
                            exec(t).await
                        })
                    })
                    .collect();

                let mut results = Vec::with_capacity(handles.len());
                for handle in handles {
                    match handle.await {
                        Ok(r) => results.push(r),
                        Err(e) => results.push(TaskResult::failure(
                            "unknown".to_string(),
                            "unknown".to_string(),
                            format!("Task panicked: {e}"),
                        )),
                    }
                }
                results
            }
        }
    }
}
