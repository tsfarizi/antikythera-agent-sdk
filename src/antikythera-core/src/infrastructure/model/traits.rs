//! Model traits

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use super::types::{ModelError, ModelRequest, ModelResponse};
use async_trait::async_trait;

/// Trait for model provider implementations
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ModelProvider: Send + Sync {
    /// Send a chat request to the model provider
    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;
}

/// Trait for individual model clients
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ModelClient: Send + Sync {
    /// Get the client ID
    fn id(&self) -> &str;

    /// Send a chat request
    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;
}
