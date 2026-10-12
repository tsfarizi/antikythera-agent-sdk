//! Server runtime configuration and the default-deny permission gate policy.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use antikythera_config::ServerConfig;

use crate::registry::Destination;

/// A runtime-hook pipeline point whose decisions may be delegated to the
/// peer over the SSE control channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookName {
    PrepareTurn,
    DecideAction,
    HandleToolResult,
}

impl HookName {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookName::PrepareTurn => "prepare-turn",
            HookName::DecideAction => "decide-action",
            HookName::HandleToolResult => "handle-tool-result",
        }
    }
}

/// Default-deny permission gate policy.
///
/// Every denial surfaces as an error whose message starts with `permission:`
/// (repo invariant). Tools are denied unless allowlisted for their
/// destination; hooks are denied unless allowlisted; LLM calls are allowed
/// with an optional per-session quota.
#[derive(Debug, Clone, Default)]
pub struct GatePolicy {
    /// Allowlist of local (server-side) tools. `None` = deny all local tools.
    pub local_tools: Option<HashSet<String>>,
    /// Allowlist of remote (client-side) tools. `None` = deny all remote tools.
    pub remote_tools: Option<HashSet<String>>,
    /// Allowlist of MCP tools. `None` = deny all MCP tools.
    pub mcp_tools: Option<HashSet<String>>,
    /// Hook points allowed to invoke the peer. Empty = deny all hooks.
    pub allowed_hooks: HashSet<HookName>,
    /// Per-session LLM call quota. `None` = unlimited.
    pub llm_quota: Option<u32>,
}

impl GatePolicy {
    /// A policy that denies everything (the default posture).
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Allow a tool for a destination.
    pub fn allow_tool(&mut self, destination: Destination, tool: impl Into<String>) {
        let set = match destination {
            Destination::Local => &mut self.local_tools,
            Destination::Remote => &mut self.remote_tools,
            Destination::Mcp => &mut self.mcp_tools,
        };
        set.get_or_insert_with(HashSet::new).insert(tool.into());
    }

    /// Allow a hook point to delegate to the peer.
    pub fn allow_hook(&mut self, hook: HookName) {
        self.allowed_hooks.insert(hook);
    }

    /// Set the per-session LLM quota.
    pub fn set_llm_quota(&mut self, quota: u32) {
        self.llm_quota = Some(quota);
    }

    /// Check that `tool` may execute at `destination`.
    pub fn check_tool(&self, destination: Destination, tool: &str) -> Result<(), String> {
        let allowed = match destination {
            Destination::Local => self.local_tools.as_ref(),
            Destination::Remote => self.remote_tools.as_ref(),
            Destination::Mcp => self.mcp_tools.as_ref(),
        };
        match allowed {
            Some(set) if set.contains(tool) => Ok(()),
            _ => Err(format!("permission: tool '{tool}' not in allowlist")),
        }
    }

    /// Check that a hook point may invoke the peer.
    pub fn check_hook(&self, hook: HookName) -> Result<(), String> {
        if self.allowed_hooks.contains(&hook) {
            Ok(())
        } else {
            Err(format!("permission: hook '{}' not allowed", hook.as_str()))
        }
    }

    /// Check that a session may issue another LLM call under the quota.
    /// `used` is the number of calls already consumed by the session.
    pub fn check_llm(&self, used: u32) -> Result<(), String> {
        match self.llm_quota {
            Some(quota) if used >= quota => Err("permission: llm quota exceeded".to_string()),
            _ => Ok(()),
        }
    }
}

/// LLM provider specification resolved at server construction.
#[derive(Debug, Clone)]
pub enum LlmProviderSpec {
    /// Deterministic provider used by tests and the smoke binary; returns a
    /// fixed framework-generic response content.
    Stub { response: String },
    /// Local-dev Ollama via the `antikythera-provider-ollama` client.
    Ollama { endpoint: String, model: String },
    /// OpenAI-compatible endpoint via the `antikythera-provider-openai` client.
    OpenAi {
        endpoint: String,
        api_key: String,
        model: String,
    },
}

/// A server-side tool registered from the CLI (`--server-tool <name>:<json>`).
/// Registration is a grant: the tool is both registered in the union registry
/// (so `GET /tools` lists it and routing resolves it to `local`) and allowlisted
/// for the local destination, so `--allow-tool` is NOT required.
#[derive(Debug, Clone)]
pub struct ServerToolSpec {
    pub name: String,
    pub response_json: serde_json::Value,
}

impl ServerToolSpec {
    /// Parse `<name>:<response-json>`; the name is everything before the FIRST
    /// colon, the remainder must parse as JSON (explicit error otherwise).
    pub fn parse(spec: &str) -> Result<Self, String> {
        let colon = spec.find(':').ok_or_else(|| {
            format!("invalid --server-tool '{spec}': expected <name>:<response-json>")
        })?;
        let name = spec[..colon].trim().to_string();
        if name.is_empty() {
            return Err(format!(
                "invalid --server-tool '{spec}': tool name must not be empty"
            ));
        }
        let response_json = serde_json::from_str(&spec[colon + 1..])
            .map_err(|e| format!("invalid --server-tool response-json for tool '{name}': {e}"))?;
        Ok(Self {
            name,
            response_json,
        })
    }
}

/// Server runtime configuration.
#[derive(Debug, Clone)]
pub struct ServerRuntimeConfig {
    /// Required for non-loopback listeners. Protects all wire endpoints.
    pub auth_token: Option<String>,
    /// Browser origins granted access (empty by default).
    pub allowed_origins: Vec<String>,
    /// Path to the composite WASM component.
    pub component_path: PathBuf,
    /// HTTP bind address.
    pub bind_addr: std::net::SocketAddr,
    /// The peer `client_id` the server expects on the SSE control channel.
    pub client_id: String,
    /// Optional session id used in outbound event envelopes.
    pub session_id: Option<String>,
    /// Default-deny permission gate policy.
    pub policy: GatePolicy,
    /// Named LLM providers; `default_provider` selects the loop's provider.
    pub providers: std::collections::HashMap<String, LlmProviderSpec>,
    /// Name of the default LLM provider.
    pub default_provider: String,
    /// Server tools registered via `--server-tool` (registration = grant).
    pub server_tools: Vec<ServerToolSpec>,
    /// MCP server connections, always executed server-side.
    pub mcp_servers: Vec<ServerConfig>,
    /// Cap on total bytes stored via `save-state`/`load-state`.
    pub storage_capacity_bytes: usize,
    /// TTL for server-initiated requests awaiting a POST-back.
    pub pending_ttl: Duration,
}

impl Default for ServerRuntimeConfig {
    fn default() -> Self {
        Self {
            auth_token: None,
            allowed_origins: Vec::new(),
            component_path: PathBuf::from("dist/antikythera-sdk.wasm"),
            bind_addr: "127.0.0.1:8787".parse().expect("valid default bind addr"),
            client_id: "antikythera-client".to_string(),
            session_id: None,
            policy: GatePolicy::deny_all(),
            providers: std::collections::HashMap::new(),
            default_provider: "stub".to_string(),
            server_tools: Vec::new(),
            mcp_servers: Vec::new(),
            storage_capacity_bytes: 1024 * 1024,
            pending_ttl: Duration::from_secs(60),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/config.tests.rs"]
mod tests;
