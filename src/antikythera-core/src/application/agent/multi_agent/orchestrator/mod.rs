//! Multi-agent orchestrator.
//!
//! [`MultiAgentOrchestrator`] is the primary entry point for running multiple
//! agents over a shared [`McpClient`].  It combines an [`AgentRegistry`] (for
//! profile look-ups), a [`TaskScheduler`] (for execution-mode policy), and an
//! [`AgentRouter`] (for task → agent mapping) into a single, ergonomic API.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use antikythera_core::application::agent::multi_agent::{
//!     orchestrator::MultiAgentOrchestrator,
//!     registry::AgentProfile,
//!     task::AgentTask,
//!     execution::ExecutionMode,
//! };
//!
//! # async fn example(client: Arc<antikythera_core::application::client::McpClient<impl antikythera_core::application::model_provider::ModelProvider + 'static>>) {
//! let orchestrator = MultiAgentOrchestrator::new(client, ExecutionMode::Auto)
//!     .register_agent(AgentProfile {
//!         id: "reviewer".into(),
//!         name: "Code Reviewer".into(),
//!         role: "code-review".into(),
//!         system_prompt: Some("You are an expert code reviewer.".into()),
//!         max_steps: Some(10),
//!     });
//!
//! let task = AgentTask::new("Review this function for security issues");
//! let result = orchestrator.dispatch(task).await;
//! println!("Success: {}", result.success);
//! # }
//! ```

pub(super) mod runtime;

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Semaphore;

use super::budget::OrchestratorBudget;
use super::cancellation::CancellationToken;
use super::execution::ExecutionMode;
use super::guardrails::GuardrailChain;
use super::registry::{AgentProfile, AgentRegistry};
use super::router::{AgentRouter, FirstAvailableRouter};
use super::scheduler::TaskScheduler;
use super::session::OrchestratorSessionManager;
use super::task::{
    AgentTask, ErrorKind, PipelineResult, RetryCondition, RoutingDecision, TaskExecutionMetadata,
    TaskResult, TaskRetryPolicy,
};
use crate::application::client::McpClient;
use crate::application::model_provider::ModelProvider;
use crate::logging::{OrchestratorLogger, SessionContext};
use runtime::{ExecuteTaskRuntime, execute_task};

/// Outcome of pre-dispatch checks and resource acquisition.
///
/// Holding this struct alive keeps the concurrency slot reserved.
#[derive(Debug)]
struct DispatchPrepared {
    task: AgentTask,
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    concurrency_wait_ms: u64,
}

/// Perform pre-dispatch checks: retry defaults, budget guards, and concurrency
/// slot acquisition.
///
/// This is a free function (not a method) so it can be used both by
/// [`MultiAgentOrchestrator::dispatch`] and inside the async closures
/// passed to the task scheduler in [`MultiAgentOrchestrator::dispatch_many`].
/// 1. Default retry policy — applied only when the task does not define its own.
///
/// 4. Concurrency slot (optional semaphore).
///
async fn prepare_dispatch(
    mut task: AgentTask,
    budget: &OrchestratorBudget,
    default_retry_condition: &RetryCondition,
    concurrency_sem: &Option<Arc<Semaphore>>,
) -> Result<DispatchPrepared, TaskResult> {
    if task.retry_policy.is_none() {
        task.retry_policy = Some(TaskRetryPolicy {
            max_retries: 0,
            backoff_ms: 0,
            condition: default_retry_condition.clone(),
        });
    }

    let dispatch_count = budget.record_task_dispatch();
    if budget.is_task_budget_exhausted() {
        let meta = TaskExecutionMetadata {
            budget_exhausted: true,
            correlation_id: task.correlation_id.clone(),
            error_kind: Some(ErrorKind::BudgetExhausted),
            ..TaskExecutionMetadata::default()
        };
        return Err(TaskResult::failure_with_kind(
            task.task_id.clone(),
            task.agent_id.clone().unwrap_or_default(),
            format!(
                "Orchestrator task budget exhausted (dispatched {})",
                dispatch_count
            ),
            ErrorKind::BudgetExhausted,
        )
        .with_metadata(meta));
    }

    if budget.is_step_budget_exhausted() {
        let meta = TaskExecutionMetadata {
            budget_exhausted: true,
            correlation_id: task.correlation_id.clone(),
            error_kind: Some(ErrorKind::BudgetExhausted),
            ..TaskExecutionMetadata::default()
        };
        return Err(TaskResult::failure_with_kind(
            task.task_id.clone(),
            task.agent_id.clone().unwrap_or_default(),
            format!(
                "Orchestrator step budget exhausted ({} / {} steps consumed)",
                budget.consumed_steps(),
                budget.max_total_steps.unwrap_or(0),
            ),
            ErrorKind::BudgetExhausted,
        )
        .with_metadata(meta));
    }

    let concurrency_wait_start = Instant::now();
    let _permit = if let Some(sem) = concurrency_sem {
        Some(
            sem.clone()
                .acquire_owned()
                .await
                .expect("orchestrator concurrency semaphore closed"),
        )
    } else {
        None
    };
    let concurrency_wait_ms = concurrency_wait_start.elapsed().as_millis() as u64;

    Ok(DispatchPrepared {
        task,
        _permit,
        concurrency_wait_ms,
    })
}

/// Coordinates multiple agents across a shared [`McpClient`].
///
/// # Builder pattern
///
/// ```rust,no_run
/// # use std::sync::Arc;
/// # use antikythera_core::application::agent::multi_agent::{
/// #     orchestrator::MultiAgentOrchestrator,
/// #     registry::AgentProfile,
/// #     execution::ExecutionMode,
/// #     router::RoundRobinRouter,
/// # };
/// # fn doc(client: Arc<antikythera_core::application::client::McpClient<impl antikythera_core::application::model_provider::ModelProvider + 'static>>) {
/// let orchestrator = MultiAgentOrchestrator::new(client, ExecutionMode::Auto)
///     .register_agent(AgentProfile {
///         id: "a1".into(),
///         name: "Agent One".into(),
///         role: "general".into(),
///         system_prompt: None,
///         max_steps: None,
///     })
///     .register_agent(AgentProfile {
///         id: "a2".into(),
///         name: "Agent Two".into(),
///         role: "general".into(),
///         system_prompt: None,
///         max_steps: None,
///     })
///     .with_router(Arc::new(RoundRobinRouter::new()));
/// # }
/// ```
pub struct MultiAgentOrchestrator<P: ModelProvider> {
    registry: AgentRegistry<()>,
    scheduler: TaskScheduler,
    router: Arc<dyn AgentRouter>,
    client: Arc<McpClient<P>>,
    /// Orchestrator-level cancellation — shared with all running tasks.
    cancel_token: CancellationToken,
    /// Orchestrator-level concurrency and step budget guardrails.
    budget: OrchestratorBudget,
    /// Optional semaphore enforcing `budget.max_concurrent_tasks`.
    concurrency_sem: Option<Arc<Semaphore>>,
    /// Default retry condition for tasks without explicit retry policy.
    default_retry_condition: RetryCondition,
    /// Ordered guardrails evaluated around task execution.
    guardrails: GuardrailChain,
    /// Tracks session lifecycle across dispatched tasks.
    session_manager: OrchestratorSessionManager,
    log: OrchestratorLogger,
}

impl<P: ModelProvider + 'static> MultiAgentOrchestrator<P> {
    /// Create a new orchestrator with an explicit execution mode.
    pub fn new(client: Arc<McpClient<P>>, mode: ExecutionMode) -> Self {
        Self {
            registry: AgentRegistry::new(),
            scheduler: TaskScheduler::new(mode),
            router: Arc::new(FirstAvailableRouter),
            client,
            cancel_token: CancellationToken::new(),
            budget: OrchestratorBudget::new(),
            concurrency_sem: None,
            default_retry_condition: RetryCondition::Always,
            guardrails: GuardrailChain::new(),
            session_manager: OrchestratorSessionManager::new(),
            log: OrchestratorLogger::new(&SessionContext::default().into_session_id()),
        }
    }

    /// Create an orchestrator with [`ExecutionMode::Auto`] (recommended default).
    pub fn with_auto_mode(client: Arc<McpClient<P>>) -> Self {
        Self::new(client, ExecutionMode::Auto)
    }

    /// Register an agent profile.
    ///
    /// Profiles with duplicate IDs silently replace the previous entry.
    pub fn register_agent(mut self, profile: AgentProfile) -> Self {
        let id = profile.id.clone();
        let role = profile.role.clone();
        self.registry.register(profile);
        self.log
            .debug(format!("Agent registered | id={} role={}", id, role));
        self
    }

    /// Override the routing strategy.
    pub fn with_router(mut self, router: Arc<dyn AgentRouter>) -> Self {
        self.router = router;
        self
    }

    /// Override the execution mode after construction.
    pub fn with_mode(mut self, mode: ExecutionMode) -> Self {
        self.scheduler = TaskScheduler::new(mode);
        self
    }

    /// Set orchestrator-level budget guardrails.
    ///
    /// The budget is enforced in addition to per-task `budget_steps` and
    /// `ExecutionMode::Parallel { workers }`.  Setting
    /// `OrchestratorBudget::max_concurrent_tasks` installs a semaphore that
    /// limits concurrent executions across *all* dispatch paths.
    pub fn with_budget(mut self, budget: OrchestratorBudget) -> Self {
        self.concurrency_sem = budget
            .max_concurrent_tasks
            .map(|n| Arc::new(Semaphore::new(n.max(1))));
        self.budget = budget;
        self
    }

    /// Set orchestrator-level default retry condition.
    ///
    /// Applied only when a task does not define its own retry policy.
    pub fn with_default_retry_condition(mut self, condition: RetryCondition) -> Self {
        self.default_retry_condition = condition;
        self
    }

    /// Set the entire guardrail chain for this orchestrator.
    pub fn with_guardrails(mut self, guardrails: GuardrailChain) -> Self {
        self.guardrails = guardrails;
        self
    }

    /// Append a single guardrail to the existing chain.
    pub fn with_guardrail(mut self, guardrail: Arc<dyn super::guardrails::TaskGuardrail>) -> Self {
        self.guardrails.push(guardrail);
        self
    }

    /// Set the session manager for this orchestrator.
    pub fn with_session_manager(mut self, manager: OrchestratorSessionManager) -> Self {
        self.session_manager = manager;
        self
    }

    /// Return a reference to the session manager.
    pub fn sessions(&self) -> &OrchestratorSessionManager {
        &self.session_manager
    }

    /// Return the number of registered agent profiles.
    pub fn agent_count(&self) -> usize {
        self.registry.count()
    }

    /// Return the current execution mode.
    pub fn execution_mode(&self) -> ExecutionMode {
        self.scheduler.mode
    }

    /// Return a snapshot of the current budget state.
    pub fn budget_snapshot(&self) -> super::budget::BudgetSnapshot {
        self.budget.snapshot()
    }

    /// Number of guardrails configured for this orchestrator.
    pub fn guardrail_count(&self) -> usize {
        self.guardrails.len()
    }

    /// Signal all running (and future) tasks to stop.
    ///
    /// After calling `cancel`, any task that checks the cancellation token
    /// will receive a [`TaskResult`] with `error_kind = Cancelled`.
    ///
    /// Cancellation is *cooperative* — tasks check the token between retry
    /// iterations, not mid-step.
    pub fn cancel(&self) {
        self.log.warn("Orchestrator cancellation triggered");
        self.cancel_token.cancel();
    }

    /// Returns `true` if [`cancel`] has been called on this orchestrator.
    ///
    /// [`cancel`]: MultiAgentOrchestrator::cancel
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Return a child [`CancellationToken`] that can be stored or passed to
    /// other components.  Cancelling the orchestrator will propagate to all
    /// child tokens.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel_token.child_token()
    }

    /// Dispatch a single task and wait for the result.
    ///
    /// The router is called to resolve the target agent.  If the router
    /// returns `None` a [`TaskResult::failure`] is returned immediately.
    /// Track session if the task declares one.
    ///
    /// Record message exchange for the session.
    ///
    pub async fn dispatch(&self, task: AgentTask) -> TaskResult {
        let prepared = match prepare_dispatch(
            task,
            &self.budget,
            &self.default_retry_condition,
            &self.concurrency_sem,
        )
        .await
        {
            Ok(p) => p,
            Err(result) => return result,
        };
        let task = prepared.task;

        let profiles: Vec<&AgentProfile> = self.registry.list_profiles();
        let candidates = profiles.len();
        let profile = match self.router.route(&task, &profiles) {
            Some(p) => p.clone(),
            None => {
                self.log
                    .error(format!("No agent available | task={}", task.task_id));
                return TaskResult::failure(
                    task.task_id.clone(),
                    task.agent_id.clone().unwrap_or_default(),
                    "No agent available to handle the task".to_string(),
                );
            }
        };

        let routing_decision = RoutingDecision {
            router_name: self.router.name().to_string(),
            selected_agent_id: profile.id.clone(),
            candidates_considered: candidates,
            reason: self.router.routing_reason(&task, &profile),
        };

        self.log.info(format!(
            "Task dispatched | task={} agent={} router={}",
            task.task_id,
            profile.id,
            self.router.name()
        ));

        if let Some(ref sid) = task.session_id {
            let _ = self.session_manager.get_or_create(sid, &profile.id);
        }

        let result = execute_task(
            self.client.clone(),
            task,
            profile,
            ExecuteTaskRuntime {
                routing_decision,
                execution_mode: self.execution_mode().to_string(),
                cancel_token: self.cancel_token.child_token(),
                budget: self.budget.clone(),
                guardrails: self.guardrails.clone(),
                concurrency_wait_ms: prepared.concurrency_wait_ms,
            },
        )
        .await;

        if !result.session_id.is_empty() {
            self.session_manager.record_message(&result.session_id);
        }

        self.log.info(format!(
            "Task execution finished | task_id={} agent={} success={} steps={}",
            result.task_id, result.agent_id, result.success, result.steps_used
        ));

        self.budget.record_steps(result.steps_used);

        result
    }

    /// Dispatch multiple tasks and collect all results.
    ///
    /// Routing is resolved for every task up-front before any task starts
    /// executing.  The actual execution order and degree of parallelism is
    /// determined by the configured [`ExecutionMode`].
    ///
    /// Results are returned in an unspecified order for `Auto` and `Parallel`
    /// modes, and in submission order for `Sequential` and `Concurrent` modes.
    /// Resolve routing for all tasks before entering the scheduler.
    ///
    pub async fn dispatch_many(&self, tasks: Vec<AgentTask>) -> Vec<TaskResult> {
        if tasks.is_empty() {
            self.log.debug("dispatch_many called with empty task list");
            return Vec::new();
        }

        let profiles: Vec<&AgentProfile> = self.registry.list_profiles();
        let candidates = profiles.len();
        let execution_mode = self.execution_mode().to_string();

        let prepared: Vec<(AgentTask, Option<AgentProfile>, RoutingDecision)> = tasks
            .into_iter()
            .map(|task| {
                let profile = self.router.route(&task, &profiles).cloned();
                let routing_decision = match &profile {
                    Some(p) => RoutingDecision {
                        router_name: self.router.name().to_string(),
                        selected_agent_id: p.id.clone(),
                        candidates_considered: candidates,
                        reason: self.router.routing_reason(&task, p),
                    },
                    None => RoutingDecision {
                        router_name: self.router.name().to_string(),
                        selected_agent_id: String::new(),
                        candidates_considered: candidates,
                        reason: Some("No matching agent found".to_string()),
                    },
                };
                (task, profile, routing_decision)
            })
            .collect();

        self.log.info(format!(
            "Dispatching {} tasks | mode={}",
            prepared.len(),
            execution_mode
        ));

        let client = self.client.clone();
        let cancel_token = self.cancel_token.clone();
        let budget = self.budget.clone();
        let concurrency_sem = self.concurrency_sem.clone();
        let default_retry_condition = self.default_retry_condition.clone();
        let guardrails = self.guardrails.clone();

        self.scheduler
            .run(prepared, move |(task, profile, routing_decision)| {
                let client = client.clone();
                let execution_mode = execution_mode.clone();
                let cancel_token = cancel_token.clone();
                let budget = budget.clone();
                let concurrency_sem = concurrency_sem.clone();
                let default_retry_condition = default_retry_condition.clone();
                let guardrails = guardrails.clone();
                async move {
                    let prepared = match prepare_dispatch(
                        task,
                        &budget,
                        &default_retry_condition,
                        &concurrency_sem,
                    )
                    .await
                    {
                        Ok(p) => p,
                        Err(result) => return result,
                    };
                    let task = prepared.task;

                    match profile {
                        None => TaskResult::failure(
                            task.task_id.clone(),
                            task.agent_id.clone().unwrap_or_default(),
                            "No agent profile found for this task".to_string(),
                        )
                        .with_metadata(TaskExecutionMetadata {
                            execution_mode: Some(execution_mode),
                            correlation_id: task.correlation_id,
                            routing_decision: Some(routing_decision),
                            ..TaskExecutionMetadata::default()
                        }),
                        Some(p) => {
                            let result = execute_task(
                                client,
                                task,
                                p,
                                ExecuteTaskRuntime {
                                    routing_decision,
                                    execution_mode,
                                    cancel_token,
                                    budget: budget.clone(),
                                    guardrails,
                                    concurrency_wait_ms: prepared.concurrency_wait_ms,
                                },
                            )
                            .await;
                            budget.record_steps(result.steps_used);
                            result
                        }
                    }
                }
            })
            .await
    }

    /// Execute tasks as a sequential pipeline.
    ///
    /// Each task's output is prepended to the next task's input as context,
    /// enabling "chain-of-thought" style multi-step reasoning across agents.
    ///
    /// The pipeline short-circuits on the first failure: remaining tasks are
    /// not executed and the partial results are returned.
    /// Inject the previous step's output as leading context.
    ///
    pub async fn pipeline(&self, tasks: Vec<AgentTask>) -> PipelineResult {
        if tasks.is_empty() {
            self.log.debug("pipeline called with empty task list");
            return PipelineResult::from_results(Vec::new());
        }

        let mut results = Vec::with_capacity(tasks.len());
        let mut previous_output: Option<String> = None;

        for mut task in tasks {
            if let Some(prev) = previous_output.take() {
                task.input = format!(
                    "Previous step output:\n{prev}\n\n---\nCurrent task:\n{}",
                    task.input
                );
            }

            let result = self.dispatch(task).await;
            let task_id = result.task_id.clone();
            self.log.info(format!(
                "Pipeline step | task={} success={}",
                task_id, result.success
            ));
            let success = result.success;
            let output_str = result.output.to_string();

            results.push(result);

            if !success {
                self.log.warn(format!(
                    "Pipeline short-circuited | failed_task={}",
                    task_id
                ));
                break;
            }

            previous_output = Some(output_str);
        }

        PipelineResult::from_results(results)
    }
}

#[cfg(test)]
#[path = "../../../../../../../tests/rust/unit/antikythera-core/application/agent/multi_agent/orchestrator/mod.tests.rs"]
mod tests;
