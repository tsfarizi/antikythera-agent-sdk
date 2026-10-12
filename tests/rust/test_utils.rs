//! Shared test utilities for the test suite.

use antikythera_core::application::tooling::error::ToolInvokeError;
use antikythera_core::application::tooling::interface::ServerToolInfo;
use antikythera_core::application::tooling::ToolServerInterface;
use antikythera_core::config::AppConfig;
use antikythera_core::domain::types::{ChatMessage, MessageRole};
use antikythera_core::infrastructure::model::traits::ModelProvider;
use antikythera_core::infrastructure::model::types::{ModelError, ModelRequest, ModelResponse};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// A mock ModelProvider for testing that returns configurable responses.
pub struct MockProvider {
    pub responses: Mutex<Vec<Result<ModelResponse, ModelError>>>,
}

impl MockProvider {
    pub fn with_response(response: &str) -> Self {
        Self {
            responses: Mutex::new(vec![Ok(ModelResponse::new(response.to_string(), None))]),
        }
    }

    pub fn with_error(error: ModelError) -> Self {
        Self {
            responses: Mutex::new(vec![Err(error)]),
        }
    }

    pub fn with_responses(responses: Vec<Result<ModelResponse, ModelError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn chat(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut responses = self.responses.lock().unwrap();
        responses.pop().ok_or_else(|| ModelError::InvalidResponse {
            provider: "mock".to_string(),
            reason: "no more mock responses".to_string(),
        })
    }
}

/// A mock ToolServerInterface for testing.
pub struct MockToolServer {
    pub tools: Vec<ServerToolInfo>,
    pub results: Mutex<Vec<Result<Value, ToolInvokeError>>>,
}

impl MockToolServer {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            results: Mutex::new(Vec::new()),
        }
    }

    pub fn with_tool(mut self, name: &str, description: &str) -> Self {
        self.tools.push(ServerToolInfo {
            name: name.to_string(),
            title: None,
            description: Some(description.to_string()),
            icons: None,
            input_schema: None,
            output_schema: None,
            annotations: None,
            execution: None,
        });
        self
    }

    pub fn with_result(self, result: Result<Value, ToolInvokeError>) -> Self {
        self.results.lock().unwrap().push(result);
        self
    }
}

#[async_trait]
impl ToolServerInterface for MockToolServer {
    async fn invoke_tool(
        &self,
        _server: &str,
        _tool: &str,
        _arguments: Value,
    ) -> Result<Value, ToolInvokeError> {
        self.results.lock().unwrap().pop().unwrap_or(Ok(json!({})))
    }

    async fn server_instructions(&self, _server: &str) -> Option<String> {
        None
    }

    async fn tool_metadata(&self, _server: &str, tool: &str) -> Option<ServerToolInfo> {
        self.tools.iter().find(|t| t.name == tool).cloned()
    }
}

/// Create a default test AppConfig.
pub fn test_config() -> AppConfig {
    AppConfig::default()
}

/// Create a ChatMessage with a user role and given text.
pub fn user_message(text: &str) -> ChatMessage {
    ChatMessage::new(MessageRole::User, text)
}

/// Create a ChatMessage with an assistant role and given text.
pub fn assistant_message(text: &str) -> ChatMessage {
    ChatMessage::new(MessageRole::Assistant, text)
}

/// Assert two ChatMessages have the same role and text content.
pub fn assert_messages_match(a: &ChatMessage, b: &ChatMessage) {
    assert_eq!(a.role, b.role, "message role mismatch");
    assert_eq!(a.content(), b.content(), "message content mismatch");
}
