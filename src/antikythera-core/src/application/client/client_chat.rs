use super::super::prompt_composer::PromptComposer;
use super::{ChatRequest, ChatResult, McpClient, McpError, PreparedChatTurn};
use crate::domain::types::{ChatMessage, MessagePart, MessageRole};
use crate::infrastructure::model::{ModelParams, ModelProvider, ModelRequest, ModelResponse};
use crate::logging::ChatLogger;

impl<P: ModelProvider> McpClient<P> {
    /// Assemble a [`PreparedChatTurn`] without calling the model.
    ///
    /// Loads session history, optionally applies `bypass_template` or
    /// `raw_mode`, builds the system prompt from the template, and
    /// constructs the outgoing [`ModelRequest`].  The result can be
    /// inspected or handed to [`Self::complete_chat`] when the host
    /// owns the LLM API call.
    pub async fn prepare_chat(&self, request: ChatRequest) -> PreparedChatTurn {
        let provider = self.config.default_provider.clone();
        let model = self.config.default_model.clone();
        let session_id = request.session_id.clone().unwrap_or_else(new_session_id);
        let raw_mode = request.raw_mode;

        let mut logs = Vec::new();
        logs.push(format!("Provider '{provider}' with model '{model}'"));

        let mut messages = Vec::new();

        if raw_mode {
            logs.push("Raw mode: sending request directly to model".to_string());
        } else {
            let history = {
                let start_wait = std::time::Instant::now();
                let sessions = self.sessions.lock().await;
                let elapsed = start_wait.elapsed();
                ChatLogger::new(&session_id).debug(format!(
                    "Acquired session lock for reading history | lock_wait_us={:?}",
                    elapsed.as_micros()
                ));
                sessions.get(session_id.as_str()).unwrap_or_default()
            };
            ChatLogger::new(&session_id).debug(format!(
                "Preparing chat request with prior history | session_id={} history_count={}",
                session_id.as_str(),
                history.len()
            ));

            if !history.is_empty() {
                logs.push(format!(
                    "Previous conversation history: {} messages",
                    history.len()
                ));
            }

            let system_prompt = if request.bypass_template {
                request.system_prompt.unwrap_or_default()
            } else {
                let system = request
                    .system_prompt
                    .or_else(|| self.config.default_system_prompt.clone());
                let composer = PromptComposer::new(&self.config.prompts, &self.config.tools);
                composer.compose(system)
            };

            if !system_prompt.is_empty() {
                logs.push(format!(
                    "System prompt active: {}",
                    Self::summarise(&system_prompt)
                ));
                messages.push(ChatMessage::new(MessageRole::System, system_prompt));
            }
            messages.extend(history.iter().cloned());
        }

        let mut user_parts = vec![MessagePart::text(request.prompt.clone())];
        user_parts.extend(request.attachments.clone());
        let user_message = ChatMessage::with_parts(MessageRole::User, user_parts);
        let prompt_preview = Self::summarise(&request.prompt);
        messages.push(user_message.clone());

        if !request.attachments.is_empty() {
            logs.push(format!(
                "User: {} (with {} attachment(s))",
                prompt_preview,
                request.attachments.len()
            ));
        } else {
            logs.push(format!("User: {prompt_preview}"));
        }

        let mut params = ModelParams::new();
        if request.force_json {
            params.insert(
                "output_format".to_string(),
                serde_json::Value::String("json".to_string()),
            );
            ChatLogger::new(&session_id)
                .debug("force_json=true → ModelRequest.params set with output_format=json");
        }

        PreparedChatTurn {
            session_id: session_id.clone(),
            provider: provider.clone(),
            model: model.clone(),
            model_request: ModelRequest {
                provider: provider.clone(),
                model: model.clone(),
                messages,
                session_id: Some(session_id.clone()),
                params,
            },
            user_message: user_message.clone(),
            logs,
        }
    }

    /// Commit a [`ModelResponse`] to session history and return a [`ChatResult`].
    ///
    /// Both the user message and the model's assistant message are appended to
    /// the in-memory session store under `prepared.session_id` via
    /// `persist_exchange`.
    pub async fn complete_chat(
        &self,
        prepared: PreparedChatTurn,
        response: ModelResponse,
    ) -> Result<ChatResult, McpError> {
        let final_session = response
            .session_id
            .clone()
            .unwrap_or_else(|| prepared.session_id.clone());
        let assistant_message = response.message.clone();
        let response_preview = Self::summarise(&assistant_message.content());

        let mut logs = prepared.logs;
        logs.push(format!("Model: {response_preview}"));

        let log = ChatLogger::new(&final_session);
        log.info(format!(
            "Response received from model provider | session_id={} provider={} model={}",
            final_session.as_str(),
            prepared.provider.as_str(),
            prepared.model.as_str()
        ));
        for entry in &logs {
            log.info(format!(
                "Interaction log | session_id={} entry={}",
                final_session.as_str(),
                entry
            ));
        }

        self.persist_exchange(&final_session, prepared.user_message, assistant_message)
            .await;

        if response.tokens > 0 {
            let sessions = self.sessions.lock().await;
            let _ = sessions
                .manager()
                .record_tokens(&final_session, response.tokens);
        }

        Ok(ChatResult {
            content: response.message.content(),
            session_id: final_session,
            provider: prepared.provider,
            model: prepared.model,
            logs,
        })
    }

    /// Single-method convenience: [`Self::prepare_chat`] → provider dispatch → [`Self::complete_chat`].
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResult, McpError> {
        let prepared = self.prepare_chat(request).await;

        ChatLogger::new(&prepared.session_id).info(format!(
            "Dispatching prepared request to model host | session_id={} provider={} model={}",
            prepared.session_id.as_str(),
            prepared.provider.as_str(),
            prepared.model.as_str()
        ));

        let response = self.provider.chat(prepared.model_request.clone()).await?;
        self.complete_chat(prepared, response).await
    }

    /// Append `user_message` and `assistant` to the in-memory session history.
    ///
    /// If `session_id` has no existing history an entry is created.  The lock
    /// acquisition latency is traced at `DEBUG` level to surface contention
    /// under concurrent multi-agent usage.
    async fn persist_exchange(
        &self,
        session_id: &str,
        user_message: ChatMessage,
        assistant: ChatMessage,
    ) {
        let start_wait = std::time::Instant::now();
        let mut sessions = self.sessions.lock().await;
        let elapsed = start_wait.elapsed();
        ChatLogger::new(session_id).debug(format!(
            "Acquired session lock to persist exchange | lock_wait_us={:?}",
            elapsed.as_micros()
        ));

        sessions.push_messages(session_id, [user_message, assistant]);
        let total_messages = sessions
            .get(session_id)
            .map(|history| history.len())
            .unwrap_or(0);
        ChatLogger::new(session_id).debug(format!(
            "Persisted chat exchange to session history | session_id={} total_messages={}",
            session_id, total_messages
        ));
    }

    /// Prune old non-system messages from `session_id` to fit within `policy`.
    ///
    /// Returns the number of messages removed, or `0` when the session does
    /// not exist or is already within budget.
    pub async fn prune_session(
        &self,
        session_id: &str,
        policy: &crate::application::resilience::ContextWindowPolicy,
    ) -> usize {
        use crate::application::resilience::prune_messages;
        let sessions = self.sessions.lock().await;
        if let Some(history) = sessions.get(session_id) {
            let before = history.len();
            let pruned = prune_messages(&history, policy);
            let removed = before - pruned.len();
            if removed > 0 {
                drop(sessions);
                let mut sessions_mut = self.sessions.lock().await;
                sessions_mut.replace_history(session_id, pruned.clone());
            }
            if removed > 0 {
                ChatLogger::new(session_id).info(format!(
                    "Context window pruned | session_id={} removed={} remaining={}",
                    session_id,
                    removed,
                    pruned.len()
                ));
            }
            removed
        } else {
            0
        }
    }

    pub(crate) fn summarise(text: &str) -> String {
        const SNIPPET_LIMIT: usize = 160;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return "(empty)".to_string();
        }
        let single_line = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut result = String::new();
        let mut chars = single_line.chars();
        for _ in 0..SNIPPET_LIMIT {
            if let Some(ch) = chars.next() {
                result.push(ch);
            } else {
                return result;
            }
        }
        if chars.next().is_some() {
            result.push('…');
        }
        result
    }
}

fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
