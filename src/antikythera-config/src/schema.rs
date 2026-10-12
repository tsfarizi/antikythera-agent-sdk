//! Format-agnostic configuration schema definitions.
//!
//! All struct definitions that represent the on-disk and runtime configuration
//! live here.  Format-specific code (TOML loading, serialization) resides in
//! sibling modules [`crate::app`], [`crate::loader`], and [`crate::serializer`].

use antikythera_domain::security::SecurityConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// REST server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestServerConfig {
    /// Server bind address (e.g., "127.0.0.1:8080")
    #[serde(default = "default_bind")]
    pub bind: String,
    /// CORS allowed origins
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// API documentation servers
    #[serde(default)]
    pub docs: Vec<DocServerConfig>,
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_string()
}

impl Default for RestServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            cors_origins: Vec::new(),
            docs: Vec::new(),
        }
    }
}

/// API documentation server entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocServerConfig {
    pub url: String,
    pub description: String,
}

/// Configurable prompts for agent behavior.
///
/// All fields use `String` (not `Option<String>`) so the struct serializes
/// cleanly with TOML.  Accessor methods return the built-in default
/// when a field is empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsConfig {
    pub template: String,
    pub tool_guidance: String,
    pub fallback_guidance: String,
    pub json_retry_message: String,
    pub tool_result_instruction: String,
    pub agent_instructions: String,
    pub language_instructions: String,
    pub agent_max_steps_error: String,
    pub no_tools_guidance: String,
    /// Field names probed in fallback when the model returns an unknown action.
    /// Defaults to `["response", "content", "message"]` when empty.
    #[serde(default)]
    pub fallback_response_keys: Vec<String>,
}

impl Default for PromptsConfig {
    fn default() -> Self {
        Self {
            template: Self::default_template().to_string(),
            tool_guidance: Self::default_tool_guidance().to_string(),
            fallback_guidance: Self::default_fallback_guidance().to_string(),
            json_retry_message: Self::default_json_retry_message().to_string(),
            tool_result_instruction: Self::default_tool_result_instruction().to_string(),
            agent_instructions: Self::default_agent_instructions().to_string(),
            language_instructions: Self::default_language_instructions().to_string(),
            agent_max_steps_error: Self::default_agent_max_steps_error().to_string(),
            no_tools_guidance: Self::default_no_tools_guidance().to_string(),
            fallback_response_keys: Self::default_fallback_response_keys()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

macro_rules! prompt_accessor {
    ($field:ident, $default_fn:ident) => {
        pub fn $field(&self) -> &str {
            if self.$field.is_empty() {
                Self::$default_fn()
            } else {
                &self.$field
            }
        }
    };
}

impl PromptsConfig {
    prompt_accessor!(template, default_template);
    prompt_accessor!(tool_guidance, default_tool_guidance);
    prompt_accessor!(fallback_guidance, default_fallback_guidance);
    prompt_accessor!(json_retry_message, default_json_retry_message);
    prompt_accessor!(tool_result_instruction, default_tool_result_instruction);
    prompt_accessor!(agent_instructions, default_agent_instructions);
    prompt_accessor!(language_instructions, default_language_instructions);
    prompt_accessor!(agent_max_steps_error, default_agent_max_steps_error);
    prompt_accessor!(no_tools_guidance, default_no_tools_guidance);

    pub fn default_template() -> &'static str {
        "You are a helpful AI assistant.\n\n{{custom_instruction}}\n\n{{language_guidance}}\n\n{{tool_guidance}}"
    }

    pub fn default_tool_guidance() -> &'static str {
        "You have access to the following tools. Use them only when necessary to fulfill the user request:"
    }

    pub fn default_fallback_guidance() -> &'static str {
        "If the request is outside the scope of available tools, apologize politely and explain your limitations."
    }

    pub fn default_json_retry_message() -> &'static str {
        "System Error: Invalid JSON format returned. You MUST respond with EXACTLY one of:\n\n1. Tool call: {\"action\":\"call_tool\",\"tool\":\"TOOL_NAME\",\"input\":{...}}\n2. Final response: {\"action\":\"final\",\"response\":{\"content\":\"your answer\"}}\n\nCRITICAL: Do NOT use tool_calls, function-calling, or any other structured output format. Output ONLY the JSON object — no markdown, no code fences, no explanation."
    }

    pub fn default_tool_result_instruction() -> &'static str {
        "Tool result received above. Respond with the SAME JSON format as before:\n- If you have the answer: {\"action\":\"final\",\"response\":{\"content\":\"your answer\"}}\n- To include tool data, add \"data\":\"step_0\" inside the response object\n- If you need another tool: {\"action\":\"call_tool\",\"tool\":\"TOOL_NAME\",\"input\":{...}}"
    }

    pub fn default_agent_instructions() -> &'static str {
        "You are an autonomous assistant that can call tools to solve user requests.\nAll responses must be valid JSON without commentary or code fences.\nWhen you need to invoke a single tool, respond with: {\"action\":\"call_tool\",\"tool\":\"tool_name\",\"input\":{...}}.\nWhen you need to invoke multiple tools simultaneously, respond with: {\"action\":\"call_tools\",\"tools\":[{\"name\":\"tool1\",\"input\":{...}}, {\"name\":\"tool2\",\"input\":{...}}]}.\nTo obtain the list of available tools, use: {\"action\":\"call_tool\",\"tool\":\"list_tools\",\"input\":{}}.\nWhen you are ready to give the final answer to the user, respond with: {\"action\":\"final\",\"response\":{\"content\":\"...\", \"data\":\"step_N\"}} where 'step_N' refers to the index of a tool call result.\nIf your response includes data from tool calls, put the reference to the tool result in a 'data' field with the value 'step_N' where N is the step number.\nFor example: {\"action\":\"final\",\"response\":{\"content\":\"Here are the latest posts\",\"data\":\"step_0\"}}.\nIMPORTANT: Always return JSON for final responses, never plain text. If you want to include data from a tool call, reference it using the 'data' field with the appropriate step index.\nCRITICAL: Do not repeat or summarize the content of tool results in the 'content' field. Simply mention that the data exists and reference it using the 'data' field. The system will automatically embed the actual data from the tool result.\nABSOLUTELY CRITICAL: Your final response must be a JSON object with 'content' and 'data' fields. Do not return a string as the value of the 'response' field. The 'response' field must contain an object, not a string."
    }

    pub fn default_language_instructions() -> &'static str {
        "Detect the user's language automatically and answer using that same language unless they explicitly request another language.\nDo not call any translation-related tools; handle language understanding internally."
    }

    pub fn default_agent_max_steps_error() -> &'static str {
        "agent exceeded the maximum number of tool interactions"
    }

    pub fn default_no_tools_guidance() -> &'static str {
        "No additional tools are currently configured."
    }

    pub fn default_fallback_response_keys() -> &'static [&'static str] {
        &["response", "content", "message"]
    }

    pub fn fallback_response_keys(&self) -> Vec<&str> {
        if self.fallback_response_keys.is_empty() {
            Self::default_fallback_response_keys().to_vec()
        } else {
            self.fallback_response_keys
                .iter()
                .map(String::as_str)
                .collect()
        }
    }
}

/// Provider definition stored in the config file.
///
/// This is a generic configuration schema — core does not interpret
/// `provider_type` or `endpoint` semantics. Each provider adapter
/// is responsible for interpreting its own configuration fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Provider / Model / Agent configuration.
///
pub struct ProviderConfig {
    /// Unique provider ID used as routing key
    pub id: String,
    /// Provider type discriminator (interpreted by the provider adapter)
    pub provider_type: String,
    /// API endpoint URL
    pub endpoint: String,
    /// API key reference (env-var name or literal key)
    pub api_key: String,
    /// Available models for this provider
    #[serde(default)]
    pub models: Vec<ModelInfo>,
}

/// Metadata for a single model offered by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
}

/// Default provider and model routing stored in the config blob.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Default provider ID
    pub default_provider: String,
    /// Default model name
    pub model: String,
}

/// Agent behavior settings stored in the config blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum tool interaction steps
    pub max_steps: u32,
    /// Verbose logging
    pub verbose: bool,
    /// Auto-execute tools
    pub auto_execute_tools: bool,
    /// Session timeout (seconds)
    pub session_timeout_secs: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            verbose: false,
            auto_execute_tools: true,
            session_timeout_secs: 300,
        }
    }
}

/// Unified application configuration.
///
/// This is the **single source of truth** for both the on-disk TOML file
/// (`app.toml`) and the in-memory runtime representation used by core, CLI, and
/// SDK.
///
/// Fields that only exist at runtime (populated by server discovery or the
/// CLI) are marked `#[serde(skip)]` and default to empty/`None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// REST server bind / CORS / docs settings.
    pub server: RestServerConfig,
    /// LLM provider catalogue.
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// Default provider + model routing.
    pub model: ModelConfig,
    /// All prompt templates.
    pub prompts: PromptsConfig,
    /// Agent tuning knobs.
    #[serde(default)]
    pub agent: AgentConfig,
    /// Security configuration.
    #[serde(default)]
    pub security: SecurityConfig,
    /// Extensible custom key-value pairs.
    #[serde(default)]
    pub custom: HashMap<String, String>,

    /// Optional system prompt override (set by CLI flag or TUI).
    #[serde(skip)]
    pub system_prompt: Option<String>,
    /// Tools synced from MCP servers at startup.
    #[serde(skip)]
    pub tools: Vec<ToolConfig>,
    /// MCP server connection configs (populated by discovery / TOML).
    #[serde(skip)]
    pub servers: Vec<ServerConfig>,
}

impl AppConfig {
    /// Convenience accessor – default provider ID.
    pub fn default_provider(&self) -> &str {
        &self.model.default_provider
    }

    /// Convenience mutable accessor – default provider ID.
    pub fn set_default_provider(&mut self, provider: impl Into<String>) {
        self.model.default_provider = provider.into();
    }

    /// Convenience accessor – model name.
    pub fn model_name(&self) -> &str {
        &self.model.model
    }

    /// Convenience mutable accessor – model name.
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model.model = model.into();
    }

    /// Get the prompt template.
    pub fn prompt_template(&self) -> &str {
        self.prompts.template()
    }
}

/// Transport type for MCP server connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    /// STDIO transport - spawns subprocess
    Stdio,
    /// HTTP transport - connects via HTTP/SSE
    Http,
    /// Builtin transport - runs in-process tool implementations
    Builtin,
}

/// Configuration for an MCP server connection.
///
/// MCP servers can be connected via STDIO (subprocess) or HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique name for this server
    pub name: String,
    /// Transport type
    pub transport: TransportType,
    /// Path to the executable (for STDIO)
    pub command: Option<PathBuf>,
    /// Command line arguments (for STDIO)
    pub args: Vec<String>,
    /// Environment variables (for STDIO)
    pub env: HashMap<String, String>,
    /// Working directory (for STDIO)
    pub workdir: Option<PathBuf>,
    /// URL for HTTP transport
    pub url: Option<String>,
    /// HTTP headers (for HTTP transport)
    pub headers: HashMap<String, String>,
    /// Default timezone for time-related operations
    pub default_timezone: Option<String>,
    /// Default city for location-based operations
    pub default_city: Option<String>,
}

impl ServerConfig {
    /// Check if this is a STDIO transport server.
    pub fn is_stdio(&self) -> bool {
        matches!(self.transport, TransportType::Stdio)
    }

    /// Check if this is an HTTP transport server.
    pub fn is_http(&self) -> bool {
        matches!(self.transport, TransportType::Http)
    }

    /// Check if this is a builtin transport server.
    pub fn is_builtin(&self) -> bool {
        matches!(self.transport, TransportType::Builtin)
    }

    /// Get command path (for STDIO).
    pub fn command(&self) -> Option<&PathBuf> {
        self.command.as_ref()
    }

    /// Get URL (for HTTP).
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

/// Configuration for an available tool.
///
/// Tools are synced from MCP servers and define what capabilities
/// are available to the AI agent.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ToolConfig {
    /// Unique name of the tool (e.g., "get_current_time")
    pub name: String,
    /// Human-readable description of what the tool does
    pub description: Option<String>,
    /// Name of the MCP server that provides this tool
    #[serde(default)]
    pub server: Option<String>,
}
