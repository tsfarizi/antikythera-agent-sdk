use crate::types::*;
use antikythera_core::domain::types::{ChatMessage, MessageRole};
use antikythera_core::infrastructure::model::traits::ModelClient;
use antikythera_core::infrastructure::model::{ModelError, ModelRequest, ModelResponse};
use antikythera_macros::PortValidate;

#[derive(PortValidate)]
#[implements(antikythera_core::infrastructure::model::traits::ModelClient)]
pub struct OllamaClient {
    id: String,
    endpoint: String,
    client: reqwest::Client,
}

impl OllamaClient {
    pub fn new(model: &str) -> Self {
        Self::with_endpoint("http://127.0.0.1:11434", model)
    }

    pub fn with_endpoint(endpoint: &str, model: &str) -> Self {
        let id = format!("ollama-{}", model);
        Self {
            id,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    fn map_error(&self, e: reqwest::Error) -> ModelError {
        ModelError::network(self.provider_name(), e.to_string())
    }
}

#[async_trait::async_trait]
impl ModelClient for OllamaClient {
    fn id(&self) -> &str {
        &self.id
    }

    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut system_prompt = None;
        let mut messages = Vec::new();

        for msg in &request.messages {
            match msg.role {
                MessageRole::System => {
                    system_prompt = Some(msg.content());
                }
                MessageRole::User => {
                    messages.push(Message {
                        role: "user".to_string(),
                        content: msg.content(),
                    });
                }
                MessageRole::Assistant => {
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: msg.content(),
                    });
                }
                MessageRole::ToolResult => {
                    messages.push(Message {
                        role: "user".to_string(),
                        content: msg.content(),
                    });
                }
            }
        }

        let format = request
            .params
            .get("output_format")
            .and_then(|v| v.as_str())
            .map(|_| "json".to_string());

        let chat_request = ChatRequest {
            model: request.model.clone(),
            messages,
            system: system_prompt,
            stream: false,
            format,
        };

        let url = format!("{}/api/chat", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&chat_request)
            .send()
            .await
            .map_err(|e| self.map_error(e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError::network(
                self.provider_name(),
                format!("HTTP {status}: {body}"),
            ));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| ModelError::invalid_response(self.provider_name(), e.to_string()))?;

        let tokens = chat_response.eval_count.unwrap_or(0) as u64;

        Ok(ModelResponse {
            message: ChatMessage::new(MessageRole::Assistant, chat_response.message.content),
            session_id: request.session_id,
            tokens,
        })
    }
}
