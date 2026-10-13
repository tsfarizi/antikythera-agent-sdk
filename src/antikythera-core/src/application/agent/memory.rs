//! Agent State Model
//!
//! This module defines the **data model** for agent state persistence and the
//! `MemoryProvider` trait that describes the persistence contract.
//!
//! ## Design principle — storage is the host's responsibility
//!
//! The WASM component only **produces** and **consumes** serialized state:
//!
//! - To persist: call the WIT host import `save-state(session_id, state_json)`.
//!   The host decides the backend (filesystem, Redis, GCS, database, etc.).
//! - To restore: call the WIT host import `load-state(session_id)`.
//!   The host returns the bytes it previously stored.
//!
//! No concrete `MemoryProvider` implementation lives inside the WASM component.
//! Concrete backends (filesystem, cloud storage, etc.) are implemented by the
//! host application that embeds the `.wasm` binary via FFI.
//!
//! ## What lives here
//!
//! - `AgentStateSnapshot` — serializable state blob (JSON format)
//! - `ConversationTurn` / `Attachment` / `StateMetadata` — sub-types
//! - `MemoryProvider` — async trait that a host-side adapter can implement
//! - `MemoryError` — error variants shared by the trait and snapshot types

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::logging::{AgentLogger, SessionContext};

/// Current schema version for state serialization
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// Unique identifier for agent context
pub type ContextId = String;

/// Agent state snapshot for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateSnapshot {
    /// Schema version for compatibility
    pub schema_version: u32,
    /// Context identifier
    pub context_id: ContextId,
    /// Agent profile ID
    pub agent_id: String,
    /// Typed FSM state
    pub fsm_state: crate::domain::fsm::AgentFsmState,
    /// Conversation history
    pub history: Vec<crate::domain::message_types::Message>,
    /// Tool execution cache
    pub tool_cache: HashMap<String, serde_json::Value>,
    /// Context variables
    pub context_vars: HashMap<String, String>,
    /// Timestamp of last update (Unix timestamp in seconds)
    pub timestamp: i64,
    /// Execution metadata
    pub metadata: StateMetadata,
}

/// Execution metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateMetadata {
    /// Total steps executed
    pub steps_executed: u32,
    /// Total tokens used
    pub tokens_used: u32,
    /// Last error message if any
    pub last_error: Option<String>,
    /// Custom metadata key-value pairs
    pub custom: HashMap<String, String>,
}

impl AgentStateSnapshot {
    /// Create a new state snapshot
    pub fn new(context_id: ContextId, agent_id: String) -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            context_id,
            agent_id,
            fsm_state: crate::domain::fsm::AgentFsmState::initial(),
            history: Vec::new(),
            tool_cache: HashMap::new(),
            context_vars: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
            metadata: StateMetadata::default(),
        }
    }

    /// Check if snapshot is compatible with current schema
    pub fn is_compatible(&self) -> bool {
        self.schema_version == STATE_SCHEMA_VERSION
    }

    /// Serialize to JSON string (uses default session context).
    pub fn to_json(&self) -> Result<String, MemoryError> {
        let ctx = SessionContext::default();
        self.to_json_ctx(&ctx)
    }

    /// Serialize to JSON string with explicit session context.
    pub fn to_json_ctx(&self, ctx: &SessionContext) -> Result<String, MemoryError> {
        let log = AgentLogger::from_context(ctx);
        log.debug(format!(
            "Saving state | agent_id={} context_id={}",
            self.agent_id, self.context_id
        ));
        serde_json::to_string(self).map_err(|e| MemoryError::Serialization(e.to_string()))
    }

    /// Deserialize from JSON string (uses default session context).
    pub fn from_json(json: &str) -> Result<Self, MemoryError> {
        let ctx = SessionContext::default();
        Self::from_json_ctx(json, &ctx)
    }

    /// Deserialize from JSON string with explicit session context.
    pub fn from_json_ctx(json: &str, ctx: &SessionContext) -> Result<Self, MemoryError> {
        let state: Self =
            serde_json::from_str(json).map_err(|e| MemoryError::Serialization(e.to_string()))?;
        let log = AgentLogger::from_context(ctx);
        log.debug(format!(
            "Loading state | agent_id={} context_id={}",
            state.agent_id, state.context_id
        ));
        Ok(state)
    }

    /// Transition FSM state, returning error if transition is invalid.
    /// Logs the transition at debug level on success, warn level on failure.
    pub fn transition_fsm(
        &mut self,
        next: crate::domain::fsm::AgentFsmState,
    ) -> Result<(), crate::domain::fsm::FsmTransitionError> {
        let ctx = SessionContext::default();
        self.transition_fsm_ctx(next, &ctx)
    }

    /// Transition FSM state with explicit session context.
    pub fn transition_fsm_ctx(
        &mut self,
        next: crate::domain::fsm::AgentFsmState,
        ctx: &SessionContext,
    ) -> Result<(), crate::domain::fsm::FsmTransitionError> {
        let from = self.fsm_state;
        self.fsm_state.transition_to(next).map(|_| {
            let log = AgentLogger::from_context(ctx);
            log.debug(format!(
                "FSM transition: {} -> {} | agent_id={} context_id={}",
                from, next, self.agent_id, self.context_id
            ));
        })
    }
}

/// Memory Provider trait for state persistence
#[async_trait]
pub trait MemoryProvider: Send + Sync {
    /// Provider name for identification
    fn name(&self) -> &str;

    /// Initialize the provider
    async fn initialize(&mut self) -> Result<(), MemoryError>;

    /// Check if provider is ready
    async fn is_ready(&self) -> bool;

    /// Save agent state
    async fn save_state(&self, state: AgentStateSnapshot) -> Result<(), MemoryError>;

    /// Load agent state by context ID
    async fn load_state(
        &self,
        context_id: &ContextId,
    ) -> Result<Option<AgentStateSnapshot>, MemoryError>;

    /// Update existing state
    async fn update_state(&self, state: AgentStateSnapshot) -> Result<(), MemoryError>;

    /// Delete agent state
    async fn delete_state(&self, context_id: &ContextId) -> Result<(), MemoryError>;

    /// Check if state exists
    async fn state_exists(&self, context_id: &ContextId) -> bool;

    /// List all context IDs for an agent
    async fn list_contexts(&self, agent_id: &str) -> Result<Vec<ContextId>, MemoryError>;

    /// Clear all contexts for an agent
    async fn clear_agent_contexts(&self, agent_id: &str) -> Result<(), MemoryError>;

    /// Shutdown the provider gracefully
    async fn shutdown(&self) -> Result<(), MemoryError>;
}

/// Memory provider errors
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("State not found: {0}")]
    NotFound(ContextId),

    #[error("Schema version mismatch: expected {expected}, got {actual}")]
    SchemaMismatch { expected: u32, actual: u32 },

    #[error("Provider not initialized")]
    NotInitialized,

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}
