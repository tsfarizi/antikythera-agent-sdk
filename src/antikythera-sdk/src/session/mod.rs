//! Session Management Module
//!
//! Wraps antikythera-session for SDK usage.
//! Integrates with antikythera-log for session-specific logging.

/// Re-export session types from antikythera-session.
///
pub use antikythera_session::{
    BatchExport, Message, MessagePart, MessageRole, Session, SessionExport, SessionSummary,
};

/// Session log export types from antikythera-log.
///
pub use antikythera_log::{BatchLogExport, SessionLogExport};

#[cfg(feature = "component")]
/// From conversions: AgentState/AgentMessage -> Session/Message.
///
impl From<crate::AgentState> for Session {
    fn from(agent: crate::AgentState) -> Self {
        Session {
            id: agent.session_id,
            user_id: String::new(),
            model: String::new(),
            title: None,
            messages: agent
                .message_history
                .into_iter()
                .map(Message::from)
                .collect(),
            metadata: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            tokens_used: 0,
            total_steps: agent.current_step,
            tools_used: agent.tool_results.into_keys().collect(),
        }
    }
}

#[cfg(feature = "component")]
impl From<crate::AgentMessage> for Message {
    fn from(msg: crate::AgentMessage) -> Self {
        let (tool_name, tool_args) = match msg.tool_call {
            Some(ref tc) => (
                Some(tc.name.clone()),
                serde_json::to_string(&tc.arguments).ok(),
            ),
            None => (None, None),
        };

        Message {
            role: match msg.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                "tool" | "tool_result" => MessageRole::ToolResult,
                _ => MessageRole::System,
            },
            parts: if msg.content.is_empty() {
                Vec::new()
            } else {
                vec![MessagePart::text(msg.content.clone())]
            },
            content: msg.content,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_name,
            tool_args,
            step: msg.tool_result.as_ref().map(|tr| tr.step_id),
            metadata: None,
        }
    }
}
