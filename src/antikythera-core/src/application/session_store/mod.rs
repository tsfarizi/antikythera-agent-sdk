use std::collections::VecDeque;

use crate::domain::message_types::Message;
use crate::domain::session_manager::SessionManager;
use crate::domain::types::ChatMessage;
use crate::domain::types::MessagePart;
use crate::logging::SessionLogger;

/// Default maximum number of concurrent sessions kept in memory.
///
/// When this limit is reached the least-recently-used session is evicted
/// before a new one is created. This prevents unbounded memory growth in
/// long-running deployments with many ephemeral sessions.
pub(crate) const DEFAULT_MAX_SESSIONS: usize = 256;

/// In-memory session store with LRU eviction.
pub(crate) struct SessionStore {
    manager: SessionManager,
    /// Access order: front = least recently used, back = most recently used.
    pub(crate) order: VecDeque<String>,
    /// Maximum number of sessions to retain simultaneously.
    pub(crate) max_sessions: usize,
}

impl SessionStore {
    pub(super) fn new(max_sessions: usize) -> Self {
        Self {
            manager: SessionManager::new(),
            order: VecDeque::new(),
            max_sessions,
        }
    }

    /// Return a reference to the history for `session_id`, or `None`.
    pub(super) fn get(&self, session_id: &str) -> Option<Vec<ChatMessage>> {
        self.manager
            .get_chat_history(session_id)
            .ok()
            .map(|messages| messages.into_iter().map(session_message_to_chat).collect())
    }

    /// Ensure a session exists and mark it as most-recently-used.
    pub(super) fn touch_or_create(&mut self, session_id: &str) {
        self.touch(session_id);
        if !self.manager.has_session(session_id).unwrap_or(false) {
            let _ =
                self.manager
                    .create_session_with_id(session_id.to_string(), "core", "core-default");
        }
    }

    /// Replace the full history for a session.
    pub(super) fn replace_history(&mut self, session_id: &str, messages: Vec<ChatMessage>) {
        self.touch_or_create(session_id);
        let _ = self.manager.clear_session(session_id);
        for message in messages {
            let _ = self
                .manager
                .add_message(session_id, chat_to_session_message(message));
        }
    }

    /// Get the underlying session manager.
    pub(super) fn manager(&self) -> &SessionManager {
        &self.manager
    }

    /// Append `messages` to `session_id`, creating the session if absent.
    pub(super) fn push_messages(
        &mut self,
        session_id: &str,
        messages: impl IntoIterator<Item = ChatMessage>,
    ) {
        self.touch_or_create(session_id);
        for message in messages {
            let _ = self
                .manager
                .add_message(session_id, chat_to_session_message(message));
        }
    }

    /// Move `session_id` to the back of the access-order deque (most recent).
    ///
    /// If the session is new and the store is at capacity, the front entry
    /// (least recently used) is evicted first.
    fn touch(&mut self, session_id: &str) {
        if let Some(pos) = self.order.iter().position(|id| id == session_id) {
            self.order.remove(pos);
        } else if self.order.len() >= self.max_sessions
            && let Some(lru_id) = self.order.pop_front()
        {
            let _ = self.manager.delete_session(&lru_id);
            SessionLogger::new(&lru_id).debug(format!(
                "Evicted LRU session from in-memory store | evicted_session={} active_sessions={}",
                lru_id,
                self.order.len()
            ));
        }
        self.order.push_back(session_id.to_string());
    }
}

fn session_message_to_chat(message: Message) -> ChatMessage {
    let parts = if message.parts.is_empty() {
        vec![MessagePart::text(message.content)]
    } else {
        message.parts
    };
    ChatMessage::with_parts(message.role, parts)
}

fn chat_to_session_message(message: ChatMessage) -> Message {
    Message::with_parts(message.role, message.parts)
}
