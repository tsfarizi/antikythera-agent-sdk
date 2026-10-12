//! Telemetry types for structured observability.
//!
//! These types are the canonical source for caller context and telemetry events.
//! `antikythera-core` and `antikythera-observability` re-export from here.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Caller context — propagated through all framework operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CallerContext {
    /// Unique ID for this request/session (for end-to-end tracing)
    pub correlation_id: Option<String>,
    /// User ID or service principal
    pub user_id: Option<String>,
    /// Tenant or organization ID
    pub tenant_id: Option<String>,
    /// Request source (CLI, REST, gRPC, WASM, etc.)
    pub source: Option<String>,
    /// Custom metadata propagated by the host
    pub metadata: Option<HashMap<String, String>>,
}

impl CallerContext {
    /// Create a new caller context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set correlation ID for tracing.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set user ID.
    pub fn with_user_id(mut self, id: impl Into<String>) -> Self {
        self.user_id = Some(id.into());
        self
    }

    /// Set tenant ID.
    pub fn with_tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    /// Set request source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Add custom metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if self.metadata.is_none() {
            self.metadata = Some(HashMap::new());
        }
        if let Some(ref mut meta) = self.metadata {
            meta.insert(key.into(), value.into());
        }
        self
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Returns a correlation ID if present, otherwise generates a deterministic
    /// fallback using timestamp-based entropy.
    pub fn ensure_correlation_id(&mut self) -> String {
        if let Some(value) = self.correlation_id.clone() {
            return value;
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let generated = format!("corr-{}", now_ms);
        self.correlation_id = Some(generated.clone());
        generated
    }
}

/// Telemetry event — structured observability data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Event type (e.g., "agent_step", "tool_call", "llm_request")
    pub event_type: String,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// Timestamp (Unix epoch milliseconds)
    pub timestamp_ms: u64,
    /// Event-specific attributes
    pub attributes: HashMap<String, serde_json::Value>,
}

impl TelemetryEvent {
    /// Create a new telemetry event.
    pub fn new(
        event_type: impl Into<String>,
        correlation_id: Option<String>,
        session_id: Option<String>,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            event_type: event_type.into(),
            correlation_id,
            session_id,
            timestamp_ms: now_ms,
            attributes: HashMap::new(),
        }
    }

    /// Add an attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Build flat string attributes suitable for metric exporters.
    pub fn metric_attributes(&self) -> HashMap<String, String> {
        let mut attrs = HashMap::new();
        attrs.insert("event_type".to_string(), self.event_type.clone());

        if let Some(correlation_id) = &self.correlation_id {
            attrs.insert("correlation_id".to_string(), correlation_id.clone());
        }
        if let Some(session_id) = &self.session_id {
            attrs.insert("session_id".to_string(), session_id.clone());
        }

        attrs
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/telemetry.tests.rs"]
mod tests;
