//! LLM proxy (R6): pluggable provider resolution. All LLM traffic — from the
//! tool loop, from the `host-imports.call-llm` gate, and from
//! `POST /antikythera/v1/llm/call` — proxies through the server.
//!
//! Default providers wrap the existing `antikythera-provider-ollama` and
//! `antikythera-provider-openai` clients (convention inheritance); streaming
//! is implemented against the provider wire formats so `llm-token` events can
//! be pushed on the control channel.

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use std::collections::HashMap;

use antikythera_core::domain::types::{ChatMessage, MessageRole};
use antikythera_core::infrastructure::model::traits::ModelClient;
use antikythera_core::infrastructure::model::{ModelError, ModelRequest, ModelResponse};
use antikythera_provider_ollama::OllamaClient;
use antikythera_provider_openai::OpenAiClient;
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;

use crate::config::LlmProviderSpec;
use crate::wire::{LlmRequest, LlmResponse};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm: provider '{0}' not configured")]
    ProviderNotFound(String),
    #[error("llm: {0}")]
    Transport(String),
    #[error("llm: provider returned invalid response: {0}")]
    InvalidResponse(String),
}

impl From<ModelError> for LlmError {
    fn from(e: ModelError) -> Self {
        LlmError::Transport(e.to_string())
    }
}

/// Pluggable LLM provider. Concrete variants: stub (deterministic, for tests
/// and smoke), Ollama, OpenAI.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;

    /// Full (non-streaming) call; returns the `llm-response` shape.
    async fn call(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;

    /// Streaming call: each content delta is pushed through `tokens`; the
    /// full response is returned once the provider finishes. The default
    /// implementation emits the whole content as a single token.
    async fn call_stream(
        &self,
        request: LlmRequest,
        tokens: UnboundedSender<String>,
    ) -> Result<LlmResponse, LlmError> {
        let response = self.call(request).await?;
        let _ = tokens.send(response.content.clone());
        Ok(response)
    }
}

/// Build the configured provider map from the runtime config.
pub fn build_providers(
    specs: HashMap<String, LlmProviderSpec>,
) -> Result<HashMap<String, std::sync::Arc<dyn LlmProvider>>, LlmError> {
    let mut providers: HashMap<String, std::sync::Arc<dyn LlmProvider>> = HashMap::new();
    for (name, spec) in specs {
        let provider: std::sync::Arc<dyn LlmProvider> = match spec {
            LlmProviderSpec::Stub { response } => {
                std::sync::Arc::new(StubLlmProvider::new(name.clone(), response))
            }
            LlmProviderSpec::Ollama { endpoint, model } => {
                std::sync::Arc::new(OllamaLlmProvider::new(name.clone(), endpoint, model))
            }
            LlmProviderSpec::OpenAi {
                endpoint,
                api_key,
                model,
            } => std::sync::Arc::new(OpenAiLlmProvider::new(
                name.clone(),
                endpoint,
                api_key,
                model,
            )?),
        };
        providers.insert(name, provider);
    }
    Ok(providers)
}

/// Deterministic provider: returns a fixed content (the framework-generic
/// JSON action envelope the runner consumes) regardless of input.
pub struct StubLlmProvider {
    name: String,
    response: String,
}

impl StubLlmProvider {
    pub fn new(name: String, response: String) -> Self {
        Self { name, response }
    }
}

#[async_trait]
impl LlmProvider for StubLlmProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn call(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse {
            content: self.response.clone(),
            model: request.model.clone(),
            session_id: request.session_id.clone(),
            message_json: None,
            tokens_used: Some(4),
            finish_reason: Some("stop".to_string()),
            raw_response_json: None,
        })
    }

    async fn call_stream(
        &self,
        request: LlmRequest,
        tokens: UnboundedSender<String>,
    ) -> Result<LlmResponse, LlmError> {
        let _ = tokens.send(self.response.clone());
        Ok(LlmResponse {
            content: self.response.clone(),
            model: request.model.clone(),
            session_id: request.session_id.clone(),
            message_json: None,
            tokens_used: Some(4),
            finish_reason: Some("stop".to_string()),
            raw_response_json: None,
        })
    }
}

#[derive(Default)]
struct StreamLines {
    pending: Vec<u8>,
}
impl StreamLines {
    fn push(&mut self, bytes: &[u8], eof: bool) -> Result<Vec<String>, LlmError> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 8 * 1024 * 1024 {
            return Err(LlmError::InvalidResponse(
                "stream record exceeds 8 MiB".into(),
            ));
        }
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let bytes = self.pending.drain(..=end).collect();
            lines.push(
                String::from_utf8(bytes).map_err(|e| LlmError::InvalidResponse(e.to_string()))?,
            );
        }
        if eof && !self.pending.is_empty() {
            lines.push(
                String::from_utf8(std::mem::take(&mut self.pending))
                    .map_err(|e| LlmError::InvalidResponse(e.to_string()))?,
            );
        }
        Ok(lines)
    }
}

/// Ollama provider: non-streaming via `OllamaClient`, streaming via the
/// `/api/chat` NDJSON stream.
pub struct OllamaLlmProvider {
    name: String,
    endpoint: String,
    model: String,
    client: OllamaClient,
    http: reqwest::Client,
}

impl OllamaLlmProvider {
    pub fn new(name: String, endpoint: String, model: String) -> Self {
        let endpoint = endpoint.trim_end_matches('/').to_string();
        Self {
            name,
            client: OllamaClient::with_endpoint(&endpoint, &model),
            endpoint,
            model,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaLlmProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn call(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let model_request = model_request_from_wire(&request, &self.model)?;
        let response = self.client.chat(model_request).await?;
        Ok(model_response_to_wire(response, &request))
    }

    async fn call_stream(
        &self,
        request: LlmRequest,
        tokens: UnboundedSender<String>,
    ) -> Result<LlmResponse, LlmError> {
        let messages = parse_messages(&request.messages_json)?;
        let chat_messages: Vec<Value> = messages
            .iter()
            .map(|m| json!({"role": role_str(m.role), "content": m.content()}))
            .collect();
        let body = json!({
            "model": request.model.as_deref().unwrap_or(&self.model),
            "messages": chat_messages,
            "stream": true,
            "format": if request.force_json { Some("json") } else { None },
            "options": {"temperature": request.temperature, "num_predict": request.max_tokens},
        });
        let url = format!("{}/api/chat", self.endpoint);
        let mut stream = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !stream.status().is_success() {
            let status = stream.status();
            let text = stream.text().await.unwrap_or_default();
            return Err(LlmError::Transport(format!("ollama HTTP {status}: {text}")));
        }
        let mut full = String::new();
        let mut lines = StreamLines::default();
        loop {
            let chunk = stream
                .chunk()
                .await
                .map_err(|e| LlmError::Transport(e.to_string()))?;
            let eof = chunk.is_none();
            for line in lines.push(chunk.as_deref().unwrap_or_default(), eof)? {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(line)
                    .map_err(|e| LlmError::InvalidResponse(e.to_string()))?;
                if let Some(error) = value["error"].as_str() {
                    return Err(LlmError::Transport(error.into()));
                }
                if let Some(delta) = value["message"]["content"].as_str() {
                    full.push_str(delta);
                    let _ = tokens.send(delta.to_string());
                }
                if value["done"].as_bool() == Some(true) {
                    return Ok(LlmResponse {
                        content: full,
                        model: request.model.clone(),
                        session_id: request.session_id.clone(),
                        message_json: None,
                        tokens_used: None,
                        finish_reason: Some("stop".to_string()),
                        raw_response_json: None,
                    });
                }
            }
            if eof {
                break;
            }
        }
        Err(LlmError::InvalidResponse(
            "stream ended without completion marker".into(),
        ))
    }
}

/// OpenAI-compatible provider: non-streaming via `OpenAiClient`, streaming
/// via the chat completions SSE stream.
pub struct OpenAiLlmProvider {
    name: String,
    endpoint: String,
    api_key: String,
    model: String,
    client: OpenAiClient,
    http: reqwest::Client,
}

impl OpenAiLlmProvider {
    pub fn new(
        name: String,
        endpoint: String,
        api_key: String,
        model: String,
    ) -> Result<Self, LlmError> {
        let endpoint = endpoint.trim_end_matches('/').to_string();
        let client = OpenAiClient::with_endpoint(&endpoint, &api_key, &model)?;
        Ok(Self {
            name,
            endpoint,
            api_key,
            model,
            client,
            http: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiLlmProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn call(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let model_request = model_request_from_wire(&request, &self.model)?;
        let response = self.client.chat(model_request).await?;
        Ok(model_response_to_wire(response, &request))
    }

    async fn call_stream(
        &self,
        request: LlmRequest,
        tokens: UnboundedSender<String>,
    ) -> Result<LlmResponse, LlmError> {
        let messages = parse_messages(&request.messages_json)?;
        let chat_messages: Vec<Value> = messages
            .iter()
            .map(|m| json!({"role": role_str(m.role), "content": m.content()}))
            .collect();
        let mut body = json!({
            "model": request.model.as_deref().unwrap_or(&self.model),
            "messages": chat_messages,
            "stream": true,
        });
        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if request.force_json {
            body["response_format"] = json!({"type":"json_object"});
        }
        let url = format!("{}/chat/completions", self.endpoint);
        let mut stream = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !stream.status().is_success() {
            let status = stream.status();
            let text = stream.text().await.unwrap_or_default();
            return Err(LlmError::Transport(format!("openai HTTP {status}: {text}")));
        }
        let mut full = String::new();
        let mut lines = StreamLines::default();
        loop {
            let chunk = stream
                .chunk()
                .await
                .map_err(|e| LlmError::Transport(e.to_string()))?;
            let eof = chunk.is_none();
            for line in lines.push(chunk.as_deref().unwrap_or_default(), eof)? {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    return Ok(LlmResponse {
                        content: full,
                        model: request.model.clone(),
                        session_id: request.session_id.clone(),
                        message_json: None,
                        tokens_used: None,
                        finish_reason: Some("stop".to_string()),
                        raw_response_json: None,
                    });
                }
                let value: Value = serde_json::from_str(data)
                    .map_err(|e| LlmError::InvalidResponse(e.to_string()))?;
                if !value["error"].is_null() {
                    return Err(LlmError::Transport(value["error"].to_string()));
                }
                if let Some(delta) = value["choices"][0]["delta"]["content"].as_str() {
                    full.push_str(delta);
                    let _ = tokens.send(delta.to_string());
                }
            }
            if eof {
                break;
            }
        }
        Err(LlmError::InvalidResponse(
            "stream ended without completion marker".into(),
        ))
    }
}

/// Translate the wire `llm-request` into the core `ModelRequest`.
fn model_request_from_wire(
    request: &LlmRequest,
    default_model: &str,
) -> Result<ModelRequest, LlmError> {
    let messages = parse_messages(&request.messages_json)?;
    let mut params = HashMap::new();
    if let Some(temperature) = request.temperature {
        params.insert("temperature".to_string(), json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        params.insert("max_tokens".to_string(), json!(max_tokens));
    }
    if request.force_json {
        params.insert("output_format".to_string(), json!("json"));
    }
    Ok(ModelRequest {
        provider: request.provider.clone().unwrap_or_default(),
        model: request
            .model
            .clone()
            .unwrap_or_else(|| default_model.to_string()),
        messages,
        session_id: request.session_id.clone(),
        params,
    })
}

fn model_response_to_wire(response: ModelResponse, request: &LlmRequest) -> LlmResponse {
    LlmResponse {
        content: response.message.content(),
        model: request.model.clone(),
        session_id: response.session_id.or_else(|| request.session_id.clone()),
        message_json: None,
        tokens_used: Some(response.tokens as u32),
        finish_reason: Some("stop".to_string()),
        raw_response_json: None,
    }
}

/// Parse the `messages_json` array of `{role, content}` into `ChatMessage`s.
fn parse_messages(messages_json: &str) -> Result<Vec<ChatMessage>, LlmError> {
    let entries: Vec<WireMessage> = serde_json::from_str(messages_json)
        .map_err(|e| LlmError::InvalidResponse(format!("cannot parse messages_json: {e}")))?;
    Ok(entries
        .into_iter()
        .map(|entry| {
            let role = match entry.role.as_str() {
                "system" => MessageRole::System,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::ToolResult,
                _ => MessageRole::User,
            };
            ChatMessage::new(role, entry.content)
        })
        .collect())
}

fn role_str(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::ToolResult => "user",
    }
}

#[derive(serde::Deserialize)]
struct WireMessage {
    role: String,
    content: String,
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/llm.streaming_tests.rs"]
mod streaming_tests;
