use crate::types::*;
use antikythera_core::domain::types::{ChatMessage, MessageRole};
use antikythera_core::infrastructure::model::traits::ModelClient;
use antikythera_core::infrastructure::model::{ModelError, ModelRequest, ModelResponse};
use antikythera_macros::PortValidate;

#[derive(PortValidate)]
#[implements(antikythera_core::infrastructure::model::traits::ModelClient)]
pub struct OpenAiClient {
    id: String,
    api_key: String,
    endpoint: String,
    client: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, ModelError> {
        if api_key.is_empty() {
            return Err(ModelError::missing_api_key("openai"));
        }
        let id = format!("openai-{}", model);
        Ok(Self {
            id,
            api_key: api_key.to_string(),
            endpoint: "https://api.openai.com/v1".to_string(),
            client: reqwest::Client::new(),
        })
    }

    pub fn with_endpoint(endpoint: &str, api_key: &str, model: &str) -> Result<Self, ModelError> {
        if api_key.is_empty() {
            return Err(ModelError::missing_api_key("openai"));
        }
        let id = format!("openai-{}", model);
        Ok(Self {
            id,
            api_key: api_key.to_string(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        })
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn map_error(&self, e: reqwest::Error) -> ModelError {
        ModelError::network(self.provider_name(), e.to_string())
    }
}

#[async_trait::async_trait]
impl ModelClient for OpenAiClient {
    fn id(&self) -> &str {
        &self.id
    }

    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut api_messages = Vec::new();

        for msg in &request.messages {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::ToolResult => "user",
            };
            api_messages.push(ApiMessage {
                role: role.to_string(),
                content: msg.content(),
            });
        }

        let response_format = request
            .params
            .get("output_format")
            .and_then(|v| v.as_str())
            .map(|_| ResponseFormat {
                format_type: "json_object".to_string(),
            });

        let completion_request = ChatCompletionRequest {
            model: request.model.clone(),
            messages: api_messages,
            response_format,
        };

        let url = format!("{}/chat/completions", self.endpoint);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&completion_request)
            .send()
            .await
            .map_err(|e| self.map_error(e))?;

        let status = response.status();

        if status.as_u16() == 429 {
            return Err(ModelError::network(
                self.provider_name(),
                "Rate limited by API".to_string(),
            ));
        }

        if status.as_u16() == 401 {
            return Err(ModelError::missing_api_key(self.provider_name()));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                return Err(ModelError::network(
                    self.provider_name(),
                    api_err.error.message,
                ));
            }
            return Err(ModelError::network(
                self.provider_name(),
                format!("HTTP {status}: {body}"),
            ));
        }

        let completion: ChatCompletion = response
            .json()
            .await
            .map_err(|e| ModelError::invalid_response(self.provider_name(), e.to_string()))?;

        let content = completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let tokens = completion.usage.map(|u| u.total_tokens as u64).unwrap_or(0);

        Ok(ModelResponse {
            message: ChatMessage::new(MessageRole::Assistant, content),
            session_id: request.session_id,
            tokens,
        })
    }
}
