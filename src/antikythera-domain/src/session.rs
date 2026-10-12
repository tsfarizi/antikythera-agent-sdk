//! Session and Message Types
//!
//! Canonical session domain types living in core so that the session crate
//! can re-export them without creating a cyclic dependency.

use super::message_types::Message;
use serde::{Deserialize, Serialize};

/// Chat session with full history and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID (UUID v4)
    pub id: String,
    /// User ID
    pub user_id: String,
    /// Model used
    pub model: String,
    /// Session title (auto-generated or user-set)
    pub title: Option<String>,
    /// Message history
    pub messages: Vec<Message>,
    /// Session metadata (JSON encoded)
    pub metadata: Option<String>,
    /// Created timestamp
    pub created_at: String,
    /// Last updated timestamp
    pub updated_at: String,
    /// Total token usage
    pub tokens_used: u64,
    /// Total steps in agent flow
    pub total_steps: u32,
    /// Tools used in this session
    pub tools_used: Vec<String>,
}

impl Session {
    /// Create a new session
    pub fn new(user_id: impl Into<String>, model: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        Self {
            id,
            user_id: user_id.into(),
            model: model.into(),
            title: None,
            messages: Vec::new(),
            metadata: None,
            created_at: now.clone(),
            updated_at: now,
            tokens_used: 0,
            total_steps: 0,
            tools_used: Vec::new(),
        }
    }

    /// Add a message
    pub fn add_message(&mut self, message: Message) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
        self.messages.push(message);
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get latest message
    pub fn latest_message(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Set title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    /// Add token usage
    pub fn add_tokens(&mut self, tokens: u64) {
        self.tokens_used += tokens;
    }

    /// Record tool usage
    pub fn record_tool(&mut self, tool_name: &str, step: u32) {
        self.total_steps = step;
        if !self.tools_used.contains(&tool_name.to_string()) {
            self.tools_used.push(tool_name.to_string());
        }
    }

    /// Clear all messages
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.total_steps = 0;
        self.tools_used.clear();
    }
}

/// Lightweight session summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub user_id: String,
    pub model: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub total_steps: u32,
    pub tools_used: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            user_id: session.user_id.clone(),
            model: session.model.clone(),
            title: session.title.clone(),
            message_count: session.messages.len(),
            total_steps: session.total_steps,
            tools_used: session.tools_used.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/session.tests.rs"]
mod tests;
