//! Split by concern to keep file size manageable and improve readability.
//!
//! Centralized tests for multi-agent production hardening types.
//!
//! These are pure-logic tests that do not require a live LLM or MCP server.
//! They cover: `AgentTask` builder, serde roundtrips, `TaskRetryPolicy`,
//! `TaskExecutionMetadata` defaults, `TaskResult` constructors,
//! `PipelineResult` aggregation, `budget_steps` guardrail semantics, and
//! deadline pre-check logic.

use antikythera_core::application::agent::multi_agent::task::{
    AgentTask, ErrorKind, PipelineResult, RetryCondition, RoutingDecision, TaskExecutionMetadata,
    TaskResult, TaskRetryPolicy,
};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

include!("hardening_tests/agent_task_builder.rs");
include!("hardening_tests/task_retry_policy.rs");
include!("hardening_tests/task_execution_metadata.rs");
include!("hardening_tests/task_result_constructors.rs");
include!("hardening_tests/pipeline_result_aggregation.rs");
include!("hardening_tests/budget_steps_guardrail.rs");
include!("hardening_tests/deadline_precheck.rs");
include!("hardening_tests/cancellation_token_core.rs");
include!("hardening_tests/orchestrator_budget_core.rs");
include!("hardening_tests/sdk_hardening_types.rs");
include!("hardening_tests/retry_condition_error_kind.rs");
include!("hardening_tests/routing_decision.rs");
include!("hardening_tests/agent_router_names.rs");
