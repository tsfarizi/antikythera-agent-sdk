// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::traits::ModelClient;
use super::types::{ModelError, ModelRequest, ModelResponse};
use crate::domain::types::{ChatMessage, MessageRole};

/// Host-facing response envelope for delegated LLM calls.
///
/// The host may either:
/// - return plain text in `text`, or
/// - return a fully structured assistant `message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostModelResponse {
    pub text: Option<String>,
    pub message: Option<ChatMessage>,
    pub session_id: Option<String>,
    pub raw_response_json: Option<String>,
}

impl HostModelResponse {
    pub fn from_text(text: impl Into<String>, session_id: Option<String>) -> Self {
        Self {
            text: Some(text.into()),
            message: None,
            session_id,
            raw_response_json: None,
        }
    }

    pub fn into_model_response(self, provider: &str) -> Result<ModelResponse, ModelError> {
        if let Some(message) = self.message {
            return Ok(ModelResponse {
                message,
                session_id: self.session_id,
                tokens: 0,
            });
        }

        if let Some(text) = self.text {
            return Ok(ModelResponse {
                message: ChatMessage::new(MessageRole::Assistant, text),
                session_id: self.session_id,
                tokens: 0,
            });
        }

        Err(ModelError::invalid_response(
            provider,
            "host response must include either `text` or `message`",
        ))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait HostModelTransport: Send + Sync {
    async fn call_model(&self, request: ModelRequest) -> Result<HostModelResponse, String>;
}

/// `ModelClient` implementation that delegates every LLM call to the host.
pub struct HostModelClient {
    provider_id: String,
    transport: Arc<dyn HostModelTransport>,
}

impl HostModelClient {
    pub fn new(provider_id: impl Into<String>, transport: Arc<dyn HostModelTransport>) -> Self {
        Self {
            provider_id: provider_id.into(),
            transport,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ModelClient for HostModelClient {
    fn id(&self) -> &str {
        &self.provider_id
    }

    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let response = self
            .transport
            .call_model(request)
            .await
            .map_err(|message| ModelError::host_delegate(self.provider_id.clone(), message))?;

        response.into_model_response(&self.provider_id)
    }
}
