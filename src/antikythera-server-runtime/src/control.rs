//! Control channel: the SSE stream from server to client plus the pending
//! server-initiated requests awaiting a client POST-back.
//!
//! The server never initiates over SSE; it pushes `tool-execution-request`,
//! `hook-request`, `llm-token` and lifecycle envelopes and waits for the
//! POST-back on `POST /antikythera/v1/events/{correlation-id}/response`.
//! Correlation ids expire after a server-defined TTL; a POST-back whose
//! correlation id is unknown (expired or unknown) is ignored and logged.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::{broadcast, oneshot};

use crate::wire::{EventEnvelope, LifecyclePayload, LlmTokenPayload, PostbackBody};

/// Kind of a server-initiated request awaiting a POST-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    Tool,
    Hook,
}

/// A registered pending request.
pub struct Pending {
    pub kind: PendingKind,
    pub deadline: Instant,
    tx: oneshot::Sender<PostbackBody>,
}

/// Broadcast of raw JSON envelopes per connected `client_id`.
pub struct ControlChannel {
    clients: Mutex<HashMap<String, broadcast::Sender<String>>>,
    pending: Mutex<HashMap<String, Pending>>,
}

impl Default for ControlChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlChannel {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Register (or replace, on reconnect) the SSE receiver for a client.
    pub fn register_client(&self, client_id: &str) -> broadcast::Receiver<String> {
        let (tx, rx) = broadcast::channel(256);
        self.clients
            .lock()
            .expect("control channel clients lock poisoned")
            .insert(client_id.to_string(), tx);
        rx
    }

    /// True when a client has registered an SSE receiver.
    pub fn is_client_connected(&self, client_id: &str) -> bool {
        self.clients
            .lock()
            .expect("control channel clients lock poisoned")
            .contains_key(client_id)
    }

    /// Push a raw envelope to the client's SSE stream.
    pub fn push(&self, client_id: &str, envelope: &EventEnvelope) -> bool {
        let json = match serde_json::to_string(envelope) {
            Ok(json) => json,
            Err(_) => return false,
        };
        self.push_raw(client_id, json)
    }

    /// A client with a dropped receiver counts as disconnected; a full buffer is not an error
    /// here (the peer re-pulls on reconnect).
    ///
    fn push_raw(&self, client_id: &str, json: String) -> bool {
        let Some(tx) = self
            .clients
            .lock()
            .expect("control channel clients lock poisoned")
            .get(client_id)
            .cloned()
        else {
            return false;
        };
        tx.send(json).is_ok()
    }

    /// Push a lifecycle signal (e.g. `connected`, `ready`).
    pub fn push_lifecycle(&self, client_id: &str, signal: &str) -> bool {
        self.push(
            client_id,
            &EventEnvelope {
                event_type: "lifecycle".to_string(),
                correlation_id: None,
                session_id: None,
                client_id: client_id.to_string(),
                payload: serde_json::to_value(LifecyclePayload {
                    signal: signal.to_string(),
                })
                .unwrap_or(Value::Null),
            },
        )
    }

    /// Push an `llm-token` event for a streaming LLM call.
    pub fn push_llm_token(&self, client_id: &str, session_id: Option<&str>, chunk: &str) -> bool {
        self.push(
            client_id,
            &EventEnvelope {
                event_type: "llm-token".to_string(),
                correlation_id: None,
                session_id: session_id.map(|s| s.to_string()),
                client_id: client_id.to_string(),
                payload: serde_json::to_value(LlmTokenPayload {
                    session_id: session_id.map(|s| s.to_string()),
                    chunk: chunk.to_string(),
                    correlation_id: None,
                })
                .unwrap_or(Value::Null),
            },
        )
    }

    /// Register a pending request and return the receiver the caller blocks
    /// on. The pending entry is removed when it completes or times out.
    pub fn register_pending(
        &self,
        correlation_id: String,
        kind: PendingKind,
        ttl: Duration,
    ) -> oneshot::Receiver<PostbackBody> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("control channel pending lock poisoned")
            .insert(
                correlation_id,
                Pending {
                    kind,
                    deadline: Instant::now() + ttl,
                    tx,
                },
            );
        rx
    }

    /// Remove a pending request (used by the caller on timeout / error paths).
    pub fn cancel_pending(&self, correlation_id: &str) {
        self.pending
            .lock()
            .expect("control channel pending lock poisoned")
            .remove(correlation_id);
    }

    /// Complete a pending request from a POST-back. Unknown correlation ids
    /// are ignored and reported as `false` so the HTTP layer can log them.
    /// Expired: the waiter already failed closed; drop the sender.
    ///
    pub fn complete_postback(&self, body: PostbackBody) -> bool {
        let mut pending = self
            .pending
            .lock()
            .expect("control channel pending lock poisoned");
        let Some(p) = pending.remove(&body.correlation_id) else {
            return false;
        };
        if p.deadline < Instant::now() {
            return false;
        }
        let _ = p.tx.send(body);
        true
    }

    /// Number of pending requests (test/debug aid).
    pub fn pending_len(&self) -> usize {
        self.pending
            .lock()
            .expect("control channel pending lock poisoned")
            .len()
    }

    /// Build a standard envelope for a peer request.
    pub fn envelope(
        client_id: &str,
        session_id: Option<&str>,
        correlation_id: String,
        event_type: &str,
        payload: Value,
    ) -> EventEnvelope {
        EventEnvelope {
            event_type: event_type.to_string(),
            correlation_id: Some(correlation_id),
            session_id: session_id.map(|s| s.to_string()),
            client_id: client_id.to_string(),
            payload,
        }
    }

    /// Build the wire `hook-request` envelope payload.
    pub fn hook_payload(hook: &str, session_state_json: &str, input_json: &str) -> Value {
        json!({
            "hook": hook,
            "session_state_json": session_state_json,
            "input_json": input_json,
        })
    }
}

/// Shared handle for control-channel operations from host functions and
/// HTTP handlers.
pub type SharedControl = Arc<ControlChannel>;

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/control.tests.rs"]
mod tests;
