use antikythera_core::application::client::{ChatRequest, ClientConfig, McpClient};
use antikythera_core::domain::types::MessagePart;
use antikythera_core::infrastructure::model::{
    ModelError, ModelProvider, ModelRequest, ModelResponse,
};
use async_trait::async_trait;

struct MockProvider {
    response: String,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ModelProvider for MockProvider {
    async fn chat(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse::new(
            format!(
                "{}:{}",
                request.session_id.unwrap_or_default(),
                self.response
            ),
            None,
        ))
    }
}

#[tokio::test]
async fn prepare_and_complete_chat_preserve_session_history() {
    let client = McpClient::new(
        MockProvider {
            response: "siap".to_string(),
        },
        ClientConfig::new("host", "gpt-host"),
        None,
    );

    let first = client
        .chat(ChatRequest {
            prompt: "halo".to_string(),
            attachments: Vec::new(),
            system_prompt: None,
            session_id: None,
            raw_mode: false,
            bypass_template: false,
            force_json: false,
        })
        .await
        .unwrap();

    let prepared = client
        .prepare_chat(ChatRequest {
            prompt: "lanjut".to_string(),
            attachments: Vec::new(),
            system_prompt: None,
            session_id: Some(first.session_id.clone()),
            raw_mode: false,
            bypass_template: false,
            force_json: false,
        })
        .await;

    assert_eq!(prepared.session_id, first.session_id);
    assert!(prepared.model_request.messages.len() >= 3);
    assert!(
        prepared
            .model_request
            .messages
            .iter()
            .any(|message| message.content() == "halo")
    );
    assert!(
        prepared
            .model_request
            .messages
            .iter()
            .any(|message| message.content().contains("siap"))
    );
    assert_eq!(
        prepared.model_request.messages.last().unwrap().content(),
        "lanjut"
    );
}

#[tokio::test]
async fn chat_preserves_attachments_through_session() {
    let client = McpClient::new(
        MockProvider {
            response: "berhasil".to_string(),
        },
        ClientConfig::new("host", "gpt-host"),
        None,
    );

    let result = client
        .chat(ChatRequest {
            prompt: "lihat lampiran".to_string(),
            attachments: vec![MessagePart::file("a.txt", "text/plain", "ZGF0YQ==")],
            system_prompt: None,
            session_id: Some("sess-attach".to_string()),
            raw_mode: false,
            bypass_template: false,
            force_json: false,
        })
        .await
        .unwrap();

    let follow_up = client
        .prepare_chat(ChatRequest {
            prompt: "cek riwayat".to_string(),
            attachments: Vec::new(),
            system_prompt: None,
            session_id: Some(result.session_id.clone()),
            raw_mode: false,
            bypass_template: false,
            force_json: false,
        })
        .await;

    assert!(
        follow_up
            .model_request
            .messages
            .iter()
            .any(|message| message.has_attachments() && message.content() == "lihat lampiran")
    );
    assert!(
        follow_up
            .model_request
            .messages
            .iter()
            .any(|message| message.content().contains("berhasil"))
    );
    assert_eq!(
        follow_up.model_request.messages.last().unwrap().content(),
        "cek riwayat"
    );
}
