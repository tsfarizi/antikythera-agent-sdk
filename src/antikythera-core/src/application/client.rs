//! # MCP Client Module
//!
//! This module provides the core MCP client implementation for communicating
//! with AI language models. It handles chat sessions, tool execution, and
//! conversation management.
//!
//! ## Key Types
//!
//! - [`McpClient`] - Main client for model communication
//! - [`ClientConfig`] - Configuration for the client
//! - [`ChatRequest`] - Request parameters for a chat
//! - [`ChatResult`] - Response from a chat request
//!
//! ## Example
//!
//! ```no_run,ignore
//! use antikythera_core::application::client::{McpClient, ClientConfig, ChatRequest};
//!
//! async fn example() {
//!     // Client setup would go here
//! }
//! ```
//!
//! # Architecture Note
//!
//! This module is **periphery-bound**: it will eventually migrate to a
//! reference host application. The core agent runner should depend on port
//! traits from `ports::` instead of this concrete client.

mod client_chat;
mod client_lifecycle;
mod client_tools;

use super::session_store::{DEFAULT_MAX_SESSIONS, SessionStore};
use super::tooling::{
    ServerManager, ToolServerInterface, TransportFactory, transport::McpTransport,
};
use crate::application::config::{AppConfig, PromptsConfig, ServerConfig, ToolConfig};
use crate::domain::types::{ChatMessage, MessagePart};
use crate::infrastructure::model::{ModelError, ModelProvider, ModelRequest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

/// Client configuration for the MCP client.
///
/// This struct holds all settings needed to initialize and run the client,
/// including provider settings, tools, servers, and prompt configurations.
///
/// Use the builder pattern methods (`with_*`) to construct the configuration:
///
/// ```no_run,ignore
/// use antikythera_core::client::ClientConfig;
///
/// let config = ClientConfig::new("gemini", "gemini-2.0-flash")
///     .with_system_prompt("You are a helpful assistant.");
/// ```
#[derive(Clone)]
pub struct ClientConfig {
    /// Default provider ID to use
    pub default_provider: String,
    /// Default model name
    pub default_model: String,
    /// Optional system prompt override
    pub default_system_prompt: Option<String>,
    /// Available tools from MCP servers
    pub tools: Vec<ToolConfig>,
    /// MCP server configurations
    pub servers: Vec<ServerConfig>,
    /// Configurable prompts for agent behavior
    pub prompts: PromptsConfig,
    /// Pre-built builtin transports keyed by server name (registered after ServerManager init)
    pub builtin_transports: HashMap<String, Arc<dyn McpTransport>>,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("default_provider", &self.default_provider)
            .field("default_model", &self.default_model)
            .field("default_system_prompt", &self.default_system_prompt)
            .field("tools", &self.tools)
            .field("servers", &self.servers)
            .field("prompts", &self.prompts)
            .field("builtin_transports_count", &self.builtin_transports.len())
            .finish()
    }
}

impl ClientConfig {
    /// Create a new client configuration with the specified provider and model.
    pub fn new(default_provider: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            default_provider: default_provider.into(),
            default_model: default_model.into(),
            default_system_prompt: None,
            tools: Vec::new(),
            servers: Vec::new(),
            prompts: PromptsConfig::default(),
            builtin_transports: HashMap::new(),
        }
    }

    /// Set the default system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.default_system_prompt = Some(prompt.into());
        self
    }

    /// Set the available tools.
    pub fn with_tools(mut self, tools: Vec<ToolConfig>) -> Self {
        self.tools = tools;
        self
    }

    /// Set the MCP server configurations.
    pub fn with_servers(mut self, servers: Vec<ServerConfig>) -> Self {
        self.servers = servers;
        self
    }

    /// Set the prompts configuration.
    pub fn with_prompts(mut self, prompts: PromptsConfig) -> Self {
        self.prompts = prompts;
        self
    }

    /// Register a pre-built builtin transport for the given server name.
    pub fn with_builtin_transport(
        mut self,
        server_name: impl Into<String>,
        transport: Arc<dyn McpTransport>,
    ) -> Self {
        self.builtin_transports
            .insert(server_name.into(), transport);
        self
    }

    /// Get the prompt template from prompts config.
    pub fn prompt_template(&self) -> &str {
        self.prompts.template()
    }

    /// Convert to AppConfig for compatibility.
    pub fn to_app_config(&self) -> AppConfig {
        let mut config = AppConfig::default();
        config.set_default_provider(&self.default_provider);
        config.set_model(&self.default_model);
        config.system_prompt = self.default_system_prompt.clone();
        config.tools = self.tools.clone();
        config.servers = self.servers.clone();
        config.prompts = self.prompts.clone();
        config
    }
}

/// Request parameters for a chat interaction.
#[derive(Debug, Default)]
pub struct ChatRequest {
    /// The user's message/prompt
    pub prompt: String,
    /// Optional file/image attachments
    pub attachments: Vec<MessagePart>,
    /// Optional system prompt override
    pub system_prompt: Option<String>,
    /// Session ID for conversation continuity
    pub session_id: Option<String>,
    /// Raw mode - bypass all config system prompts and templates
    /// Used for direct model queries without context injection
    pub raw_mode: bool,
    /// Skip template composition - use system_prompt as-is
    /// Used by Agent runner which composes its own complete system prompt
    pub bypass_template: bool,
    /// Force JSON mode - requests the LLM to output valid JSON
    pub force_json: bool,
}

/// Result from a chat interaction.
///
/// Contains the model's response along with metadata about
/// the interaction.
#[derive(Debug, Clone)]
pub struct ChatResult {
    /// The model's response content
    pub content: String,
    /// Session ID for this conversation
    pub session_id: String,
    /// Provider used for this request
    pub provider: String,
    /// Model used for this request
    pub model: String,
    /// Debug/execution logs
    pub logs: Vec<String>,
}

/// Prepared host-facing model request.
///
/// The host owns the actual LLM API call. This struct captures the exact
/// request payload plus the session metadata needed to commit the response
/// back into the client's internal history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedChatTurn {
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub model_request: ModelRequest,
    pub user_message: ChatMessage,
    pub logs: Vec<String>,
}

/// Error returned by [`McpClient`] operations.
///
/// Wraps [`ModelError`] — the only error path today is a model provider
/// failure. Use [`McpError::user_message`] to get a human-readable string
/// suitable for display in the TUI or CLI output.
#[derive(Debug, Error)]
pub enum McpError {
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl McpError {
    pub fn user_message(&self) -> String {
        match self {
            McpError::Model(err) => err.user_message(),
        }
    }
}

/// Read-only snapshot of the active [`McpClient`] configuration.
///
/// Produced by [`McpClient::config_snapshot`] and passed to the TUI so the
/// UI can display current provider, model, tools, and server bindings without
/// holding a lock on the client.
#[derive(Debug, Clone)]
pub struct ClientConfigSnapshot {
    pub model: String,
    pub default_provider: String,
    pub system_prompt: Option<String>,
    pub prompt_template: String,
    pub tools: Vec<ToolConfig>,
    pub servers: Vec<ServerConfig>,
    /// Full TOML representation of the config, shown in the Settings overlay.
    pub raw: String,
}

/// Core MCP client that owns session history, provider dispatch, and server connectivity.
///
/// `McpClient<P>` is generic over any [`ModelProvider`] implementation, making
/// it usable across native, WASM, and FFI deployment targets without change.
/// Callers supply any concrete `P` — including host-delegating providers that
/// forward the LLM call outward via FFI — keeping this type agnostic to both
/// the runtime environment and the underlying model infrastructure.
///
/// # Session model
///
/// Each session is identified by a `session_id` string.  History is stored
/// in-memory in a `Mutex<SessionStore>` and is scoped to the lifetime of this
/// instance.  The store evicts least-recently-used sessions once
/// `max_sessions` is exceeded (default: `DEFAULT_MAX_SESSIONS`).
/// Use [`McpClient::prune_session`] to trim old messages before a request
/// when the conversation grows long.
pub struct McpClient<P: ModelProvider> {
    pub(crate) provider: P,
    pub(crate) config: ClientConfig,
    pub(crate) sessions: Mutex<SessionStore>,
    pub(crate) server_bridge: Arc<dyn ToolServerInterface>,
}

impl<P: ModelProvider> McpClient<P> {
    /// Construct a new client from a provider and a [`ClientConfig`].
    ///
    /// A [`ServerManager`] is created from `config.servers` and stored as the
    /// active [`ToolServerInterface`].  Session history starts empty with a
    /// default LRU capacity of `DEFAULT_MAX_SESSIONS`.
    pub fn new(
        provider: P,
        config: ClientConfig,
        factory: Option<Box<dyn TransportFactory>>,
    ) -> Self {
        let factory = factory.unwrap_or_else(|| Box::new(NoOpTransportFactory));
        let server_manager = Arc::new(ServerManager::new(config.servers.clone(), factory));
        for (name, transport) in &config.builtin_transports {
            server_manager.register_builtin_transport(name, transport.clone());
        }
        let bridge: Arc<dyn ToolServerInterface> = server_manager;
        Self {
            provider,
            config,
            sessions: Mutex::new(SessionStore::new(DEFAULT_MAX_SESSIONS)),
            server_bridge: bridge,
        }
    }
}

/// A no-op transport factory that fails at runtime if transport creation is needed.
/// Used as a placeholder when no factory is provided.
struct NoOpTransportFactory;

#[async_trait::async_trait]
impl TransportFactory for NoOpTransportFactory {
    async fn create(
        &self,
        config: &ServerConfig,
    ) -> Result<crate::application::tooling::ServerInstance, super::tooling::error::ToolInvokeError>
    {
        Err(super::tooling::error::ToolInvokeError::NotConfigured {
            server: format!("{}: no transport factory configured", config.name),
        })
    }
}
