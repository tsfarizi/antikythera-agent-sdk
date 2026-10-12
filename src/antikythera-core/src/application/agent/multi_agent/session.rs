//! Multi-session management for the orchestrator.
//!
//! Tracks sessions across agents, supports concurrent sessions,
//! and provides lifecycle management (creation, activity tracking,
//! TTL-based eviction).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::logging::{OrchestratorLogger, SessionContext};

/// Session state tracked by the orchestrator.
#[derive(Debug, Clone)]
pub struct ManagedSession {
    pub session_id: String,
    pub agent_id: String,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub message_count: usize,
    pub metadata: HashMap<String, String>,
}

impl ManagedSession {
    /// Return how long since the last activity.
    pub fn idle_duration(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Return how long since creation.
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Manages multiple sessions across agents.
///
/// Thread-safe via `Arc<Mutex<...>>` — cheap to clone and pass into async
/// closures.
#[derive(Clone)]
pub struct OrchestratorSessionManager {
    sessions: Arc<Mutex<HashMap<String, ManagedSession>>>,
    max_sessions: usize,
    session_ttl: Option<Duration>,
    log: OrchestratorLogger,
}

impl OrchestratorSessionManager {
    /// Create a new session manager with default limits.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            max_sessions: 100,
            session_ttl: None,
            log: OrchestratorLogger::new(&SessionContext::default().into_session_id()),
        }
    }

    /// Set maximum concurrent sessions.
    pub fn with_max_sessions(mut self, max: usize) -> Self {
        self.max_sessions = max;
        self
    }

    /// Set session time-to-live. Sessions idle longer than this are evicted
    /// when the session limit is reached.
    pub fn with_session_ttl(mut self, ttl: Duration) -> Self {
        self.session_ttl = Some(ttl);
        self
    }

    /// Create or get a session for an agent.
    ///
    /// If a session with `session_id` already exists, its `last_activity` is
    /// refreshed and it is returned. Otherwise a new session is created.
    /// Existing session — refresh activity.
    ///
    /// Capacity check — try eviction first.
    ///
    pub fn get_or_create(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<ManagedSession, SessionError> {
        let mut sessions = self.sessions.lock().unwrap();

        if let Some(session) = sessions.get(session_id) {
            let mut session = session.clone();
            session.last_activity = Instant::now();
            sessions.insert(session_id.to_string(), session.clone());
            self.log
                .debug(format!("Session refreshed | session_id={}", session_id));
            return Ok(session);
        }

        if sessions.len() >= self.max_sessions {
            self.evict_expired(&mut sessions);
            if sessions.len() >= self.max_sessions {
                self.log.warn(format!(
                    "Session limit reached | max={} current={}",
                    self.max_sessions,
                    sessions.len()
                ));
                return Err(SessionError::MaxSessionsReached(self.max_sessions));
            }
        }

        let session = ManagedSession {
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            created_at: Instant::now(),
            last_activity: Instant::now(),
            message_count: 0,
            metadata: HashMap::new(),
        };
        sessions.insert(session_id.to_string(), session.clone());
        self.log.debug(format!(
            "Session created | session_id={} agent_id={}",
            session_id, agent_id
        ));
        Ok(session)
    }

    /// Record a message exchange in a session (bumps `message_count` and
    /// `last_activity`).
    pub fn record_message(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(session_id) {
            session.message_count += 1;
            session.last_activity = Instant::now();
        }
    }

    /// Get a snapshot of a session by ID.
    pub fn get(&self, session_id: &str) -> Option<ManagedSession> {
        self.sessions.lock().unwrap().get(session_id).cloned()
    }

    /// List all sessions for a given agent.
    pub fn list_by_agent(&self, agent_id: &str) -> Vec<ManagedSession> {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// List all tracked sessions.
    pub fn list_all(&self) -> Vec<ManagedSession> {
        self.sessions.lock().unwrap().values().cloned().collect()
    }

    /// Remove a session, returning it if it existed.
    pub fn remove(&self, session_id: &str) -> Option<ManagedSession> {
        let removed = self.sessions.lock().unwrap().remove(session_id);
        if removed.is_some() {
            self.log
                .debug(format!("Session removed | session_id={}", session_id));
        }
        removed
    }

    /// Return total tracked session count.
    pub fn count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    /// Evict sessions whose `last_activity` exceeds the configured TTL.
    fn evict_expired(&self, sessions: &mut HashMap<String, ManagedSession>) {
        if let Some(ttl) = self.session_ttl {
            let before = sessions.len();
            sessions.retain(|_, session| session.last_activity.elapsed() < ttl);
            let evicted = before - sessions.len();
            if evicted > 0 {
                self.log
                    .debug(format!("Expired sessions evicted | count={}", evicted));
            }
        }
    }
}

impl Default for OrchestratorSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors produced by session management operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionError {
    #[error("maximum concurrent sessions reached ({0})")]
    MaxSessionsReached(usize),
    #[error("session not found: {0}")]
    SessionNotFound(String),
}
