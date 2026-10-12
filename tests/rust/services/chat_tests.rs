use antikythera_core::application::client::{ChatRequest, ClientConfig, McpClient};
use antikythera_core::application::model_provider::ModelProvider;
use antikythera_core::domain::types::ChatMessage;
use antikythera_core::domain::types::MessageRole;
use antikythera_core::infrastructure::model::{ModelError, ModelRequest, ModelResponse};
use async_trait::async_trait;
use std::sync::Arc;

struct MockModelProvider {
    response_content: String,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ModelProvider for MockModelProvider {
    async fn chat(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            message: ChatMessage::new(MessageRole::Assistant, self.response_content.clone()),
            session_id: None,
            tokens: 0,
        })
    }
}

#[tokio::test]
async fn test_chat_service_raw_mode_no_debug() {
    let provider = MockModelProvider {
        response_content: "Hello world".to_string(),
    };
    let config = ClientConfig::new("test-provider", "test-model");
    let client = Arc::new(McpClient::new(provider, config, None));

    let result = client
        .chat(ChatRequest {
            prompt: "hi".to_string(),
            attachments: vec![],
            system_prompt: None,
            session_id: None,
            raw_mode: true,
            bypass_template: false,
            force_json: false,
        })
        .await;

    assert!(result.is_ok());
    let outcome = result.unwrap();
    assert_eq!(outcome.content, "Hello world");
}

#[tokio::test]
async fn test_chat_service_raw_mode_with_debug() {
    let provider = MockModelProvider {
        response_content: "Hello world".to_string(),
    };
    let config = ClientConfig::new("test-provider", "test-model");
    let client = Arc::new(McpClient::new(provider, config, None));

    let result = client
        .chat(ChatRequest {
            prompt: "hi".to_string(),
            attachments: vec![],
            system_prompt: None,
            session_id: None,
            raw_mode: true,
            bypass_template: false,
            force_json: false,
        })
        .await;

    assert!(result.is_ok());
    let outcome = result.unwrap();
    assert_eq!(outcome.content, "Hello world");
    assert_eq!(outcome.provider, "test-provider");
}
