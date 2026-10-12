//! Split into 13 parts for consistent test organization.
//!
//! Integration tests for the resilience module.
//!
//! Verifies that the public API of `antikythera_core::application::resilience` works
//! correctly end-to-end from an external crate perspective, mirroring the
//! access pattern a host application would use.

use antikythera_core::application::resilience::{
    ContextWindowPolicy, HealthStatus, HealthTracker, InMemoryAuditSink, PolicyAuditEvent,
    PolicyAuditSink, PolicyEventType, ResilienceConfig, ResilienceManager, RetryPolicy,
    TimeoutPolicy, TokenEstimator, prune_messages, with_retry, with_retry_if,
};
use antikythera_core::domain::types::{ChatMessage, MessageRole};

include!("resilience_tests/retry_timeout_serialization.rs");
include!("resilience_tests/token_estimator.rs");
include!("resilience_tests/prune_messages_health.rs");
include!("resilience_tests/health_resilience_lifecycle.rs");
include!("resilience_tests/resilience_prune_empty.rs");
include!("resilience_tests/token_estimation_pruning.rs");
include!("resilience_tests/health_tracker_states.rs");
include!("resilience_tests/retry_timeout_config.rs");
include!("resilience_tests/resilience_manager_api.rs");
include!("resilience_tests/with_retry_if_logic.rs");
include!("resilience_tests/policy_audit.rs");
