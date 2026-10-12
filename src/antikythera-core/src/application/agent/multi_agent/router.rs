//! Agent routing strategies for the multi-agent orchestrator.
//!
//! A [`AgentRouter`] determines which registered agent should handle an
//! incoming [`AgentTask`].  Implementors are free to inspect the task's
//! `agent_id` hint, `metadata`, or apply stateful load-balancing logic.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::registry::AgentProfile;
use super::task::AgentTask;

/// Routing contract for multi-agent task dispatch.
///
/// # Implementation notes
///
/// - Implementations must be `Send + Sync` to work across async tasks.
/// - [`AgentRouter::route`] receives an immutable slice of *references* to registered
///   profiles.  The return value is an `Option<&AgentProfile>` pointing into
///   that slice — no allocation needed for the common case.
/// - Returning `None` means the orchestrator will produce a
///   [`TaskResult::failure`] with "No agent available".
///
/// [`TaskResult::failure`]: super::task::TaskResult::failure
pub trait AgentRouter: Send + Sync {
    /// Choose a profile from `profiles` that should handle `task`.
    ///
    /// Return `None` if no suitable agent is registered.
    fn route<'a>(
        &self,
        task: &AgentTask,
        profiles: &[&'a AgentProfile],
    ) -> Option<&'a AgentProfile>;

    /// Short, human-readable name for this router.
    ///
    /// Used in [`RoutingDecision::router_name`] for introspection and
    /// structured logging.  Implementors should return a stable `&'static str`
    /// such as `"direct"`, `"round-robin"`, or `"role"`.
    ///
    /// [`RoutingDecision::router_name`]: super::task::RoutingDecision::router_name
    fn name(&self) -> &str {
        "unknown"
    }

    /// Optional human-readable reason describing *why* `profile` was selected.
    ///
    /// Called by the orchestrator after a successful [`AgentRouter::route`] call to populate
    /// [`RoutingDecision::reason`].  The default implementation returns `None`.
    ///
    /// [`AgentRouter::route`]: AgentRouter::route
    /// [`RoutingDecision::reason`]: super::task::RoutingDecision::reason
    fn routing_reason(&self, _task: &AgentTask, _profile: &AgentProfile) -> Option<String> {
        None
    }
}

/// Routes tasks directly to the agent specified by `task.agent_id`.
///
/// Returns `None` when `task.agent_id` is `None` or the ID is not registered.
///
/// Use this router when callers always specify an explicit target agent.
#[derive(Debug, Default)]
pub struct DirectRouter;

impl AgentRouter for DirectRouter {
    fn route<'a>(
        &self,
        task: &AgentTask,
        profiles: &[&'a AgentProfile],
    ) -> Option<&'a AgentProfile> {
        let id = task.agent_id.as_deref()?;
        profiles.iter().copied().find(|p| p.id == id)
    }

    fn name(&self) -> &str {
        "direct"
    }

    fn routing_reason(&self, task: &AgentTask, profile: &AgentProfile) -> Option<String> {
        Some(format!(
            "explicit agent_id='{}' matched profile id='{}'",
            task.agent_id.as_deref().unwrap_or(""),
            profile.id
        ))
    }
}

/// Distributes tasks evenly across registered agents using a rotating counter.
///
/// Thread-safe: the counter uses an `AtomicUsize` so multiple async tasks can
/// call [`AgentRouter::route`] concurrently.
///
/// [`AgentRouter::route`]: AgentRouter::route
#[derive(Debug, Default)]
pub struct RoundRobinRouter {
    counter: AtomicUsize,
}

impl RoundRobinRouter {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl AgentRouter for RoundRobinRouter {
    fn route<'a>(
        &self,
        _task: &AgentTask,
        profiles: &[&'a AgentProfile],
    ) -> Option<&'a AgentProfile> {
        if profiles.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % profiles.len();
        Some(profiles[idx])
    }

    fn name(&self) -> &str {
        "round-robin"
    }

    fn routing_reason(&self, _task: &AgentTask, profile: &AgentProfile) -> Option<String> {
        Some(format!("round-robin selected agent_id='{}'", profile.id))
    }
}

/// Always routes to the first agent in the registry.
///
/// This is the default router.  It works well for single-agent setups and as
/// a safe fallback when no routing logic is needed.
#[derive(Debug, Default)]
pub struct FirstAvailableRouter;

impl AgentRouter for FirstAvailableRouter {
    fn route<'a>(
        &self,
        _task: &AgentTask,
        profiles: &[&'a AgentProfile],
    ) -> Option<&'a AgentProfile> {
        profiles.first().copied()
    }

    fn name(&self) -> &str {
        "first-available"
    }

    fn routing_reason(&self, _task: &AgentTask, profile: &AgentProfile) -> Option<String> {
        Some(format!(
            "selected first available agent_id='{}'",
            profile.id
        ))
    }
}

/// Routes tasks to an agent whose `role` matches the value stored under a
/// specified metadata key.
///
/// Example: if `metadata_key` is `"role"` and the task has
/// `metadata["role"] = "code-reviewer"`, the router selects the first
/// registered agent with `profile.role == "code-reviewer"`.
///
/// Falls back to [`FirstAvailableRouter`] behaviour when the metadata key is
/// absent or no agent matches.
#[derive(Debug)]
pub struct RoleRouter {
    metadata_key: String,
}

impl RoleRouter {
    pub fn new(metadata_key: impl Into<String>) -> Self {
        Self {
            metadata_key: metadata_key.into(),
        }
    }

    /// Convenience: route on the `"role"` metadata key.
    pub fn on_role() -> Self {
        Self::new("role")
    }
}

impl AgentRouter for RoleRouter {
    fn route<'a>(
        &self,
        task: &AgentTask,
        profiles: &[&'a AgentProfile],
    ) -> Option<&'a AgentProfile> {
        let desired_role = task
            .metadata
            .get(&self.metadata_key)
            .and_then(|v| v.as_str())?;
        profiles.iter().copied().find(|p| p.role == desired_role)
    }

    fn name(&self) -> &str {
        "role"
    }

    fn routing_reason(&self, task: &AgentTask, profile: &AgentProfile) -> Option<String> {
        let role = task
            .metadata
            .get(&self.metadata_key)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        Some(format!("role='{}' matched agent_id='{}'", role, profile.id))
    }
}
