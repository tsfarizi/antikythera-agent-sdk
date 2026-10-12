use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::logging::{OrchestratorLogger, SessionContext};
use tokio::time::sleep;

use super::super::budget::OrchestratorBudget;
use super::super::cancellation::CancellationToken;
use super::super::guardrails::{GuardrailChain, GuardrailContext, GuardrailRejection};
use super::super::registry::AgentProfile;
use super::super::task::{
    AgentTask, ErrorKind, RetryCondition, RoutingDecision, TaskExecutionMetadata, TaskResult,
};
use crate::application::agent::{Agent, AgentOptions};
use crate::application::client::McpClient;
use crate::application::model_provider::ModelProvider;

/// Captures execution-context values cloned into async closures.
#[derive(Clone)]
pub(super) struct ExecuteTaskRuntime {
    pub(super) routing_decision: RoutingDecision,
    pub(super) execution_mode: String,
    pub(super) cancel_token: CancellationToken,
    pub(super) budget: OrchestratorBudget,
    pub(super) guardrails: GuardrailChain,
    pub(super) concurrency_wait_ms: u64,
}

/// Execute a single task against a pre-resolved agent profile.
///
/// This free function is used both by the sequential `dispatch` path and
/// inside the closures passed to the scheduler.  It is intentionally
/// free-standing so it can be cloned into async closures without carrying an
/// orchestrator reference.
///
/// Execution enforces the following limits:
///
/// - Deadline pre-check (fail fast before any work starts)
/// - Cancellation check (cancel token from orchestrator)
/// - `budget_steps` cap (min of task budget and profile max_steps)
/// - Per-task retry with `RetryCondition` gate
/// - Full `TaskExecutionMetadata` including `RoutingDecision`
/// - `ErrorKind` classification on every failure path
///
/// Timeouts are classified as transient failures. Retriable failures keep the
/// same task identity, and the last attempted failure supplies the final error
/// and classification when the retry policy is exhausted.
pub(super) async fn execute_task<P: ModelProvider>(
    client: Arc<McpClient<P>>,
    task: AgentTask,
    profile: AgentProfile,
    runtime: ExecuteTaskRuntime,
) -> TaskResult {
    let started = Instant::now();
    let ExecuteTaskRuntime {
        routing_decision,
        execution_mode,
        cancel_token,
        budget,
        guardrails,
        concurrency_wait_ms,
    } = runtime;
    let max_steps = task.max_steps.or(profile.max_steps).unwrap_or(8);
    let budgeted_max_steps = task
        .budget_steps
        .map(|b| b.min(max_steps))
        .unwrap_or(max_steps);
    let retry_policy = task.retry_policy.clone().unwrap_or_default();

    let pre_context = GuardrailContext::new(
        budget.snapshot(),
        cancel_token.is_cancelled(),
        0,
        execution_mode.clone(),
    );

    if let Err(rejection) = guardrails.check_pre(&task, &profile, &pre_context) {
        return guardrail_failure_result(
            &task,
            &profile.id,
            &routing_decision,
            &execution_mode,
            concurrency_wait_ms,
            rejection,
        );
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as i64;

    if let Some(deadline) = task.deadline_unix_ms
        && now_ms >= deadline
    {
        let metadata = TaskExecutionMetadata {
            deadline_exceeded: true,
            execution_mode: Some(execution_mode),
            correlation_id: task.correlation_id.clone(),
            routing_decision: Some(routing_decision),
            concurrency_wait_ms,
            error_kind: Some(ErrorKind::DeadlineExceeded),
            ..TaskExecutionMetadata::default()
        };
        return TaskResult::failure_with_kind(
            task.task_id,
            profile.id,
            "Task deadline exceeded before execution".to_string(),
            ErrorKind::DeadlineExceeded,
        )
        .with_metadata(metadata);
    }

    if cancel_token.is_cancelled() {
        let metadata = TaskExecutionMetadata {
            cancelled: true,
            execution_mode: Some(execution_mode),
            correlation_id: task.correlation_id.clone(),
            routing_decision: Some(routing_decision),
            concurrency_wait_ms,
            error_kind: Some(ErrorKind::Cancelled),
            ..TaskExecutionMetadata::default()
        };
        return TaskResult::failure_with_kind(
            task.task_id,
            profile.id,
            "Task cancelled before execution".to_string(),
            ErrorKind::Cancelled,
        )
        .with_metadata(metadata);
    }

    let mut attempt: u8 = 0;

    let (error_msg, error_kind) = loop {
        let mid_context = GuardrailContext::new(
            budget.snapshot(),
            cancel_token.is_cancelled(),
            attempt.saturating_add(1),
            execution_mode.clone(),
        );

        if let Err(rejection) = guardrails.check_mid(&task, &profile, &mid_context) {
            return guardrail_failure_result(
                &task,
                &profile.id,
                &routing_decision,
                &execution_mode,
                concurrency_wait_ms,
                rejection,
            );
        }

        if cancel_token.is_cancelled() {
            let metadata = TaskExecutionMetadata {
                attempt_count: attempt,
                duration_ms: started.elapsed().as_millis() as u64,
                cancelled: true,
                retry_applied: attempt > 1,
                execution_mode: Some(execution_mode),
                correlation_id: task.correlation_id,
                routing_decision: Some(routing_decision),
                concurrency_wait_ms,
                error_kind: Some(ErrorKind::Cancelled),
                ..TaskExecutionMetadata::default()
            };
            return TaskResult::failure_with_kind(
                task.task_id,
                profile.id,
                "Task cancelled during execution".to_string(),
                ErrorKind::Cancelled,
            )
            .with_metadata(metadata);
        }

        attempt = attempt.saturating_add(1);
        let ctx = SessionContext::new(
            task.session_id
                .as_deref()
                .unwrap_or(&SessionContext::default().into_session_id()),
        );
        let log = OrchestratorLogger::from_context(&ctx);
        let agent = Agent::new(client.clone());
        let options = AgentOptions {
            system_prompt: profile.system_prompt.clone(),
            session_id: task.session_id.clone(),
            max_steps: budgeted_max_steps,
            attachments: Vec::new(),
            event_sender: None,
        };

        log.info(format!(
            "Dispatching task to agent | task_id={} agent_id={} attempt={}",
            task.task_id, profile.id, attempt
        ));

        let run_future = agent.run(task.input.clone(), options);
        let run_result = if let Some(timeout_ms) = task.timeout_ms {
            match tokio::time::timeout(Duration::from_millis(timeout_ms), run_future).await {
                Ok(result) => result,
                Err(_) => {
                    let should_retry = attempt <= retry_policy.max_retries
                        && retry_policy.condition != RetryCondition::Never;

                    if should_retry {
                        if retry_policy.backoff_ms > 0 {
                            sleep(Duration::from_millis(retry_policy.backoff_ms)).await;
                        }
                        continue;
                    }

                    let metadata = TaskExecutionMetadata {
                        attempt_count: attempt,
                        duration_ms: started.elapsed().as_millis() as u64,
                        timed_out: true,
                        retry_applied: attempt > 1,
                        execution_mode: Some(execution_mode),
                        correlation_id: task.correlation_id,
                        routing_decision: Some(routing_decision),
                        concurrency_wait_ms,
                        error_kind: Some(ErrorKind::Transient),
                        ..TaskExecutionMetadata::default()
                    };
                    return TaskResult::failure_with_kind(
                        task.task_id,
                        profile.id,
                        format!("Task timed out after {} ms", timeout_ms),
                        ErrorKind::Transient,
                    )
                    .with_metadata(metadata);
                }
            }
        } else {
            run_future.await
        };

        match run_result {
            Ok(outcome) => {
                log.info(format!(
                    "Task completed | task_id={} agent_id={}",
                    task.task_id, profile.id
                ));
                let metadata = TaskExecutionMetadata {
                    attempt_count: attempt,
                    duration_ms: started.elapsed().as_millis() as u64,
                    retry_applied: attempt > 1,
                    execution_mode: Some(execution_mode.clone()),
                    correlation_id: task.correlation_id.clone(),
                    routing_decision: Some(routing_decision.clone()),
                    concurrency_wait_ms,
                    ..TaskExecutionMetadata::default()
                };
                let result = TaskResult::success(
                    task.task_id.clone(),
                    profile.id.clone(),
                    outcome.response,
                    outcome.steps.len(),
                    outcome.session_id,
                )
                .with_metadata(metadata);

                let post_context = GuardrailContext::new(
                    budget.snapshot(),
                    cancel_token.is_cancelled(),
                    attempt,
                    execution_mode.clone(),
                );

                if let Err(rejection) =
                    guardrails.check_post(&task, &profile, &result, &post_context)
                {
                    return guardrail_failure_result(
                        &task,
                        &profile.id,
                        &routing_decision,
                        &execution_mode,
                        concurrency_wait_ms,
                        rejection,
                    );
                }

                return result;
            }
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_agent_error(&err_str);

                log.warn(format!(
                    "Task failed | task_id={} agent_id={} error={} attempt={} kind={:?}",
                    task.task_id, profile.id, err_str, attempt, kind
                ));

                let should_retry = attempt <= retry_policy.max_retries
                    && match retry_policy.condition {
                        RetryCondition::Never => false,
                        RetryCondition::Always => true,
                        RetryCondition::OnTransient => kind == ErrorKind::Transient,
                    };

                if should_retry {
                    if retry_policy.backoff_ms > 0 {
                        sleep(Duration::from_millis(retry_policy.backoff_ms)).await;
                    }
                    continue;
                }
                break (err_str, kind);
            }
        }
    };
    let metadata = TaskExecutionMetadata {
        attempt_count: attempt,
        duration_ms: started.elapsed().as_millis() as u64,
        retry_applied: attempt > 1,
        execution_mode: Some(execution_mode),
        correlation_id: task.correlation_id,
        routing_decision: Some(routing_decision),
        concurrency_wait_ms,
        error_kind: Some(error_kind.clone()),
        ..TaskExecutionMetadata::default()
    };

    TaskResult::failure_with_kind(task.task_id, profile.id, error_msg, error_kind)
        .with_metadata(metadata)
}

/// Classify an agent error string as transient or permanent.
///
/// This is a best-effort heuristic; callers can always override retry behaviour
/// via [`super::super::task::TaskRetryPolicy::condition`].
fn classify_agent_error(error: &str) -> ErrorKind {
    let lower = error.to_lowercase();
    if lower.contains("timeout")
        || lower.contains("rate limit")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("429")
        || lower.contains("connection")
        || lower.contains("network")
        || lower.contains("temporarily")
    {
        ErrorKind::Transient
    } else {
        ErrorKind::Permanent
    }
}

pub(super) fn guardrail_failure_result(
    task: &AgentTask,
    agent_id: &str,
    routing_decision: &RoutingDecision,
    execution_mode: &str,
    concurrency_wait_ms: u64,
    rejection: GuardrailRejection,
) -> TaskResult {
    let metadata = TaskExecutionMetadata {
        attempt_count: 0,
        duration_ms: 0,
        cancelled: rejection.error_kind == ErrorKind::Cancelled,
        retry_applied: false,
        execution_mode: Some(execution_mode.to_string()),
        correlation_id: task.correlation_id.clone(),
        routing_decision: Some(routing_decision.clone()),
        concurrency_wait_ms,
        budget_exhausted: rejection.error_kind == ErrorKind::BudgetExhausted,
        error_kind: Some(rejection.error_kind.clone()),
        guardrail_name: Some(rejection.guardrail_name.clone()),
        guardrail_stage: Some(rejection.stage.as_str().to_string()),
        ..TaskExecutionMetadata::default()
    };

    TaskResult::failure_with_kind(
        task.task_id.clone(),
        agent_id.to_string(),
        rejection.message,
        rejection.error_kind,
    )
    .with_metadata(metadata)
}
