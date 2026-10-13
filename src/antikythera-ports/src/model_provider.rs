//! Port: Model Provider
//!
//! Application defines this port. Infrastructure implements it.
//! The core agent loop depends only on this trait, not on any concrete provider.

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use async_trait::async_trait;
use std::collections::HashMap;

/// Provider-agnostic model errors.
#[derive(Debug, Clone)]
pub enum ModelError {
    Network(String),
    Authentication(String),
    RateLimited(String),
    InvalidRequest(String),
    ServerError(String),
    Unknown(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "Network error: {}", e),
            Self::Authentication(e) => write!(f, "Authentication error: {}", e),
            Self::RateLimited(e) => write!(f, "Rate limited: {}", e),
            Self::InvalidRequest(e) => write!(f, "Invalid request: {}", e),
            Self::ServerError(e) => write!(f, "Server error: {}", e),
            Self::Unknown(e) => write!(f, "Unknown error: {}", e),
        }
    }
}

impl std::error::Error for ModelError {}

/// Provider-agnostic model request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelRequest {
    pub messages: Vec<serde_json::Value>,
    pub model: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Provider-agnostic model response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<ModelUsage>,
}

/// Token usage statistics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Trait for model provider implementations.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ModelProvider: Send + Sync {
    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;
}

/// Trait for individual model clients.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ModelClient: Send + Sync {
    fn id(&self) -> &str;
    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;
}
