//! Server Manager - manages MCP server connections.
//!
//! Handles STDIO, HTTP, and Builtin transport connections.

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use super::envelope::{
    ToolCallEnvelope, ToolResultEnvelope, validate_tool_call_envelope,
    validate_tool_result_envelope,
};
use super::error::ToolInvokeError;
use super::interface::{ServerToolInfo, ToolServerInterface};
use super::transport::McpTransport;
use antikythera_config::ServerConfig;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Unified server instance that wraps any MCP transport.
pub struct ServerInstance {
    transport: Arc<dyn McpTransport>,
}

impl ServerInstance {
    /// Create a new ServerInstance wrapping a transport.
    pub fn new(transport: Arc<dyn McpTransport>) -> Self {
        Self { transport }
    }

    async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value, ToolInvokeError> {
        self.transport.call_tool(tool, arguments).await
    }

    async fn instructions(&self) -> Option<String> {
        self.transport.instructions().await
    }

    async fn tool_metadata(&self, tool: &str) -> Option<ServerToolInfo> {
        self.transport.tool_metadata(tool).await
    }
}

/// Trait for creating transport instances from server configs.
///
/// Implementations live in the CLI crate and handle the actual transport creation.
#[async_trait]
pub trait TransportFactory: Send + Sync {
    /// Create a `ServerInstance` from a `ServerConfig`.
    async fn create(&self, config: &ServerConfig) -> Result<ServerInstance, ToolInvokeError>;
}

pub struct ServerManager {
    configs: HashMap<String, ServerConfig>,
    instances: Mutex<HashMap<String, ServerInstance>>,
    factory: Box<dyn TransportFactory>,
}

impl ServerManager {
    pub fn new(configs: Vec<ServerConfig>, factory: Box<dyn TransportFactory>) -> Self {
        let configs = configs
            .into_iter()
            .map(|cfg| (cfg.name.clone(), cfg))
            .collect();
        Self {
            configs,
            instances: Mutex::new(HashMap::new()),
            factory,
        }
    }

    /// Pre-register a builtin transport instance.
    ///
    /// Builtin transports are created externally (e.g. by the CLI or host)
    /// with tool definitions and handlers, then injected into the manager.
    /// This avoids `ensure_instance` constructing an empty transport.
    pub fn register_builtin_transport(&self, name: &str, transport: Arc<dyn McpTransport>) {
        let mut instances = match self.instances.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!(
                    server = name,
                    error = %e,
                    "ServerManager instances lock poisoned in register_builtin_transport"
                );
                return;
            }
        };
        instances.insert(name.to_string(), ServerInstance::new(transport));
    }

    /// Get config and create instance.
    ///
    async fn ensure_instance(&self, server: &str) -> Result<(), ToolInvokeError> {
        if server.is_empty() {
            return Err(ToolInvokeError::NotConfigured {
                server: server.to_string(),
            });
        }

        {
            let instances = match self.instances.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::warn!(
                        server = server,
                        error = %e,
                        "ServerManager instances lock poisoned in ensure_instance"
                    );
                    return Err(ToolInvokeError::Transport {
                        server: server.to_string(),
                        message: format!("ServerManager lock poisoned: {}", e),
                    });
                }
            };
            if instances.contains_key(server) {
                return Ok(());
            }
        }

        let config =
            self.configs
                .get(server)
                .cloned()
                .ok_or_else(|| ToolInvokeError::NotConfigured {
                    server: server.to_string(),
                })?;

        let instance = self.factory.create(&config).await?;

        let mut instances = match self.instances.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!(
                    server = server,
                    error = %e,
                    "ServerManager instances lock poisoned in ensure_instance after create"
                );
                return Err(ToolInvokeError::Transport {
                    server: server.to_string(),
                    message: format!("ServerManager lock poisoned: {}", e),
                });
            }
        };
        instances.insert(server.to_string(), instance);
        Ok(())
    }

    fn get_instance(&self, server: &str) -> Option<ServerInstance> {
        let instances = match self.instances.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!(
                    server = server,
                    error = %e,
                    "ServerManager instances lock poisoned in get_instance"
                );
                return None;
            }
        };
        instances
            .get(server)
            .map(|i| ServerInstance::new(Arc::clone(&i.transport)))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ToolServerInterface for ServerManager {
    async fn invoke_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<Value, ToolInvokeError> {
        let call_env = ToolCallEnvelope {
            tool: tool.to_string(),
            arguments: arguments.clone(),
            correlation_id: None,
        };
        validate_tool_call_envelope(&call_env).map_err(|e| ToolInvokeError::Transport {
            server: server.to_string(),
            message: e.to_transport_message("call"),
        })?;

        self.ensure_instance(server).await?;
        let instance = self
            .get_instance(server)
            .ok_or_else(|| ToolInvokeError::NotConfigured {
                server: server.to_string(),
            })?;

        let output = instance.call_tool(tool, arguments).await;
        match output {
            Ok(value) => {
                let result_env = ToolResultEnvelope {
                    tool: tool.to_string(),
                    success: true,
                    output: value.clone(),
                    error: None,
                    correlation_id: None,
                };
                validate_tool_result_envelope(&result_env).map_err(|e| {
                    ToolInvokeError::Transport {
                        server: server.to_string(),
                        message: e.to_transport_message("result"),
                    }
                })?;
                Ok(value)
            }
            Err(err) => {
                let result_env = ToolResultEnvelope {
                    tool: tool.to_string(),
                    success: false,
                    output: Value::Null,
                    error: Some(err.to_string()),
                    correlation_id: None,
                };
                validate_tool_result_envelope(&result_env).map_err(|e| {
                    ToolInvokeError::Transport {
                        server: server.to_string(),
                        message: e.to_transport_message("result"),
                    }
                })?;
                Err(err)
            }
        }
    }

    async fn server_instructions(&self, server: &str) -> Option<String> {
        match self.ensure_instance(server).await {
            Ok(()) => {
                if let Some(instance) = self.get_instance(server) {
                    instance.instructions().await
                } else {
                    None
                }
            }
            Err(err) => {
                tracing::warn!(
                    server = server,
                    error = %err,
                    "Failed to fetch server instructions"
                );
                None
            }
        }
    }

    async fn tool_metadata(&self, server: &str, tool: &str) -> Option<ServerToolInfo> {
        match self.ensure_instance(server).await {
            Ok(()) => {
                if let Some(instance) = self.get_instance(server) {
                    instance.tool_metadata(tool).await
                } else {
                    None
                }
            }
            Err(err) => {
                tracing::warn!(
                    server = server,
                    tool = tool,
                    error = %err,
                    "Failed to fetch tool metadata"
                );
                None
            }
        }
    }
}
