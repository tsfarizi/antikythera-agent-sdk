//! Model types - Request, Response, and Error types
//!
//! These types define the WASM-safe message contract between core and the host.
//! `ModelError::Network` intentionally uses a plain `String` so that `reqwest`
//! is not part of core's public API surface — HTTP error details are converted
//! to strings by the provider implementation layer (CLI or SDK) before
//! constructing this error.

use crate::domain::types::{ChatMessage, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Provider-agnostic model I/O payload.
///
/// CORE does not interpret parameter semantics — the host (CLI / WASM runtime)
/// is responsible for mapping these opaque hints to provider-specific API fields
/// (e.g., `output_format` → `response_format` for OpenAI or
/// `responseMimeType` for Gemini).
///
/// # Well-known keys (convention, not contract)
///
/// | Key              | Value                    | Meaning                              |
/// |------------------|--------------------------|--------------------------------------|
/// | `output_format`  | `"json"`                 | Request structured JSON output       |
/// | `temperature`    | float                    | Sampling temperature (0.0–2.0)      |
/// | `max_tokens`     | integer                  | Maximum completion tokens            |
///
/// Keys not understood by a provider are silently ignored.
pub type ModelParams = HashMap<String, serde_json::Value>;

/// Model request for LLM chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    pub provider: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub session_id: Option<String>,
    /// Opaque parameters forwarded to the provider implementation.
    /// CORE does not inspect these — the host interprets them.
    #[serde(default)]
    pub params: ModelParams,
}

/// Model response from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub message: ChatMessage,
    pub session_id: Option<String>,
    /// Token usage for this specific response (if provided by the model)
    #[serde(default)]
    pub tokens: u64,
}

impl ModelResponse {
    pub fn new(content: String, session_id: Option<String>) -> Self {
        Self {
            message: ChatMessage::new(MessageRole::Assistant, content),
            session_id,
            tokens: 0,
        }
    }

    pub fn with_tokens(mut self, tokens: u64) -> Self {
        self.tokens = tokens;
        self
    }
}

/// Model errors
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("provider '{provider}' is not configured")]
    ProviderNotFound { provider: String },
    #[error("model '{model}' is not available for provider '{provider}'")]
    ModelNotFound { provider: String, model: String },
    #[error("provider '{provider}' requires an API key")]
    MissingApiKey { provider: String },
    /// Network / HTTP error.  The provider implementation converts the
    /// transport-layer error to a plain string so that `reqwest` is not
    /// referenced in core's public API surface.
    #[error("network error calling provider '{provider}': {message}")]
    Network { provider: String, message: String },
    #[error("provider '{provider}' returned invalid response: {reason}")]
    InvalidResponse { provider: String, reason: String },
    #[error("host-delegated provider '{provider}' failed: {message}")]
    HostDelegate { provider: String, message: String },
    #[error("unsupported model integration: {message}")]
    Unsupported { message: String },
}

impl ModelError {
    pub fn provider_not_found(provider: impl Into<String>) -> Self {
        Self::ProviderNotFound {
            provider: provider.into(),
        }
    }

    pub fn model_not_found(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self::ModelNotFound {
            provider: provider.into(),
            model: model.into(),
        }
    }

    pub fn missing_api_key(provider: impl Into<String>) -> Self {
        Self::MissingApiKey {
            provider: provider.into(),
        }
    }

    /// Build a network error from any displayable error message.
    /// The caller (provider implementation) is responsible for converting
    /// transport-layer errors (e.g., `reqwest::Error`) to a string before
    /// calling this constructor.
    pub fn network(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Network {
            provider: provider.into(),
            message: message.into(),
        }
    }

    pub fn invalid_response(provider: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidResponse {
            provider: provider.into(),
            reason: reason.into(),
        }
    }

    pub fn host_delegate(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::HostDelegate {
            provider: provider.into(),
            message: message.into(),
        }
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported {
            message: message.into(),
        }
    }

    /// User-friendly error message in English
    /// The provider implementation already stringified the transport error.
    ///
    pub fn user_message(&self) -> String {
        match self {
            ModelError::ProviderNotFound { provider } => format!(
                "Model provider '{provider}' not found. Check your client.toml configuration."
            ),
            ModelError::ModelNotFound { provider, model } => {
                format!("Model '{model}' is not available for provider '{provider}'.")
            }
            ModelError::MissingApiKey { provider } => {
                format!("Provider '{provider}' requires an API key.")
            }
            ModelError::Network { provider, message } => {
                format!("Network error calling provider '{provider}': {message}")
            }
            ModelError::InvalidResponse { provider, .. } => {
                format!("Invalid response from provider '{provider}'.")
            }
            ModelError::HostDelegate { provider, message } => {
                format!("Host failed to process model request for '{provider}': {message}")
            }
            ModelError::Unsupported { message } => message.clone(),
        }
    }
}
