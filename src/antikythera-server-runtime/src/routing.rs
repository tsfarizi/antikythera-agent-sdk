//! Tool routing: resolve a tool call to one of three destinations
//! (`local` / `remote` / `mcp`) and execute it behind the permission gate.
//!
//! - `local` — server tools: handler functions registered by the SDK user.
//! - `remote` — client tools: `tool-execution-request` on the SSE control
//!   channel + POST-back, fail-closed on missing client or TTL expiry.
//! - `mcp` — third-party MCP servers via `antikythera-tooling` transports,
//!   always server-side.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use antikythera_config::{ServerConfig, TransportType};
use serde_json::Value;

use crate::config::GatePolicy;
use crate::control::{ControlChannel, PendingKind};
use crate::mcp::{SharedMcpTransport, mcp_result_to_execution, server_info_to_definition};
use crate::registry::{Destination, ToolOwner, UnionRegistry};
use crate::wire::{EventEnvelope, ToolCallEvent, ToolDefinition, ToolExecutionResult};

/// Handler for a server-side (local) tool: arguments object in, result out.
/// A tool failure is NOT an error: return `Ok(Value)` with an error-shaped
/// object, or surface an execution-level failure with `Err(message)`.
pub type LocalToolHandler = dyn Fn(Value) -> Result<Value, String> + Send + Sync;

/// The routing engine shared by host functions, the tool loop, and the HTTP
/// `POST /tools/execute` endpoint.
pub struct ToolRouter {
    registry: Mutex<UnionRegistry>,
    local_handlers: Mutex<HashMap<String, Arc<LocalToolHandler>>>,
    mcp_servers: Mutex<HashMap<String, SharedMcpTransport>>,
    tool_server: Mutex<HashMap<String, String>>,
    control: Arc<ControlChannel>,
    policy: Arc<GatePolicy>,
    runtime: tokio::runtime::Handle,
    client_id: String,
    pending_ttl: Duration,
}

impl ToolRouter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        control: Arc<ControlChannel>,
        policy: Arc<GatePolicy>,
        runtime: tokio::runtime::Handle,
        client_id: String,
        pending_ttl: Duration,
    ) -> Self {
        Self {
            registry: Mutex::new(UnionRegistry::new()),
            local_handlers: Mutex::new(HashMap::new()),
            mcp_servers: Mutex::new(HashMap::new()),
            tool_server: Mutex::new(HashMap::new()),
            control,
            policy,
            runtime,
            client_id,
            pending_ttl,
        }
    }

    /// Register a server-side tool with its local handler.
    pub fn register_local_tool(
        &self,
        definition: ToolDefinition,
        handler: Arc<LocalToolHandler>,
    ) -> Result<(), String> {
        let name = definition.name.clone();
        self.registry
            .lock()
            .expect("tool registry lock poisoned")
            .register(ToolOwner::Server, definition)?;
        self.local_handlers
            .lock()
            .expect("tool handlers lock poisoned")
            .insert(name, handler);
        Ok(())
    }

    /// Register client-side (remote) tool definitions.
    pub fn register_client_tools(&self, definitions: Vec<ToolDefinition>) -> Result<(), String> {
        let mut registry = self.registry.lock().expect("tool registry lock poisoned");
        for definition in definitions {
            registry.register(ToolOwner::Client, definition)?;
        }
        Ok(())
    }

    /// Connect an MCP server and register its tools as `mcp`-owned. Blocking:
    /// must be called from a non-async context (the HTTP server drives async
    /// work on its own threads).
    pub fn connect_mcp_server(&self, config: ServerConfig) -> Result<(), String> {
        let transport: SharedMcpTransport = match config.transport {
            TransportType::Stdio => Arc::new(crate::mcp::StdioMcpTransport::new(config.clone())),
            TransportType::Http => Arc::new(crate::mcp::HttpMcpTransport::new(config.clone())),
            TransportType::Builtin => {
                return Err(
                    "mcp: builtin transport is not supported by the server runtime".to_string(),
                );
            }
        };
        let server_name = transport.server_name().to_string();
        self.runtime.block_on(async {
            transport
                .connect()
                .await
                .map_err(|e| format!("mcp: connect '{server_name}' failed: {e}"))?;
            let tools = transport.list_tools().await;
            let mut registry = self.registry.lock().expect("tool registry lock poisoned");
            let mut tool_server = self.tool_server.lock().expect("tool server lock poisoned");
            for info in tools {
                let name = info.name.clone();
                let definition = server_info_to_definition(&info);
                registry.register(ToolOwner::Mcp, definition)?;
                tool_server.insert(name, server_name.clone());
            }
            drop(tool_server);
            drop(registry);
            self.mcp_servers
                .lock()
                .expect("mcp servers lock poisoned")
                .insert(server_name, transport);
            Ok(())
        })
    }

    pub fn owner_of(&self, tool: &str) -> Option<ToolOwner> {
        self.registry
            .lock()
            .expect("tool registry lock poisoned")
            .owner_of(tool)
    }

    /// Resolve the routing destination for a tool call. Unknown tools are
    /// denied (no owner means no destination).
    pub fn resolve_destination(&self, tool: &str) -> Result<Destination, String> {
        let registry = self.registry.lock().expect("tool registry lock poisoned");
        match registry.owner_of(tool) {
            Some(owner) => Ok(Destination::from(owner)),
            None => Err(format!("permission: tool '{tool}' not in allowlist")),
        }
    }

    /// The union of all registered definitions (for the single
    /// `register-tools` push to the runner).
    pub fn union_definitions(&self) -> Vec<ToolDefinition> {
        self.registry
            .lock()
            .expect("tool registry lock poisoned")
            .definitions()
    }

    /// Peer-facing definitions (`GET /tools`): server- and mcp-owned tools.
    pub fn peer_definitions(&self) -> Vec<ToolDefinition> {
        let registry = self.registry.lock().expect("tool registry lock poisoned");
        let mut defs = registry.definitions_for(ToolOwner::Server);
        defs.extend(registry.definitions_for(ToolOwner::Mcp));
        defs
    }

    /// Execute a tool call with full routing (host-imports `emit-tool-call`
    /// and the tool loop).
    pub async fn execute(&self, event: &ToolCallEvent) -> Result<ToolExecutionResult, String> {
        let destination = self.resolve_destination(&event.tool_name)?;
        self.execute_for(destination, event).await
    }

    /// Execute a tool call that MUST be server- or mcp-owned
    /// (`POST /tools/execute`). Client-owned tools are denied here.
    pub async fn execute_server_owned(
        &self,
        event: &ToolCallEvent,
    ) -> Result<ToolExecutionResult, String> {
        match self.resolve_destination(&event.tool_name)? {
            Destination::Remote => Err(format!(
                "permission: tool '{}' is owned by the client; server cannot execute it",
                event.tool_name
            )),
            destination => self.execute_for(destination, event).await,
        }
    }

    async fn execute_for(
        &self,
        destination: Destination,
        event: &ToolCallEvent,
    ) -> Result<ToolExecutionResult, String> {
        self.policy.check_tool(destination, &event.tool_name)?;
        match destination {
            Destination::Local => self.execute_local(event),
            Destination::Remote => self.execute_remote(event).await,
            Destination::Mcp => self.execute_mcp(event).await,
        }
    }

    fn execute_local(&self, event: &ToolCallEvent) -> Result<ToolExecutionResult, String> {
        let handler = self
            .local_handlers
            .lock()
            .expect("tool handlers lock poisoned")
            .get(&event.tool_name)
            .cloned()
            .ok_or_else(|| format!("tool '{}' has no local handler", event.tool_name))?;
        let arguments: Value = serde_json::from_str(&event.arguments_json).map_err(|e| {
            format!(
                "tool '{}': cannot parse arguments-json: {e}",
                event.tool_name
            )
        })?;
        match handler(arguments) {
            Ok(output) => Ok(ToolExecutionResult {
                tool_name: event.tool_name.clone(),
                success: true,
                output_json: output.to_string(),
                error_message: None,
                step_id: event.step_id,
            }),
            Err(message) => Ok(ToolExecutionResult {
                tool_name: event.tool_name.clone(),
                success: false,
                output_json: "{}".to_string(),
                error_message: Some(message),
                step_id: event.step_id,
            }),
        }
    }

    async fn execute_remote(&self, event: &ToolCallEvent) -> Result<ToolExecutionResult, String> {
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let mut rx = self.control.register_pending(
            correlation_id.clone(),
            PendingKind::Tool,
            self.pending_ttl,
        );
        if !self.control.is_client_connected(&self.client_id) {
            self.control.cancel_pending(&correlation_id);
            return Err(format!(
                "permission: tool '{}' requires a connected client",
                event.tool_name
            ));
        }
        let envelope = EventEnvelope {
            event_type: "tool-execution-request".to_string(),
            correlation_id: Some(correlation_id.clone()),
            session_id: event.session_id.clone(),
            client_id: self.client_id.clone(),
            payload: serde_json::to_value(event).unwrap_or(Value::Null),
        };
        self.control.push(&self.client_id, &envelope);
        match tokio::time::timeout(self.pending_ttl, &mut rx).await {
            Ok(Ok(body)) => {
                if !body.ok {
                    return Err(normalize_denial(body.error.unwrap_or_else(|| {
                        format!("permission: tool '{}' rejected by client", event.tool_name)
                    })));
                }
                let result: ToolExecutionResult =
                    serde_json::from_value(body.payload).map_err(|e| {
                        format!(
                            "tool '{}': client returned invalid tool-execution-result: {e}",
                            event.tool_name
                        )
                    })?;
                Ok(result)
            }
            Ok(Err(_)) => {
                self.control.cancel_pending(&correlation_id);
                Err(format!(
                    "permission: tool '{}' timed out waiting for client response",
                    event.tool_name
                ))
            }
            Err(_) => {
                self.control.cancel_pending(&correlation_id);
                Err(format!(
                    "permission: tool '{}' timed out waiting for client response",
                    event.tool_name
                ))
            }
        }
    }

    async fn execute_mcp(&self, event: &ToolCallEvent) -> Result<ToolExecutionResult, String> {
        let server = self
            .tool_server
            .lock()
            .expect("tool server lock poisoned")
            .get(&event.tool_name)
            .cloned()
            .ok_or_else(|| format!("tool '{}' has no MCP server mapping", event.tool_name))?;
        let transport = self
            .mcp_servers
            .lock()
            .expect("mcp servers lock poisoned")
            .get(&server)
            .cloned()
            .ok_or_else(|| format!("mcp server '{server}' is not connected"))?;
        let arguments: Value = serde_json::from_str(&event.arguments_json).map_err(|e| {
            format!(
                "tool '{}': cannot parse arguments-json: {e}",
                event.tool_name
            )
        })?;
        let result = transport
            .call_tool(&event.tool_name, arguments)
            .await
            .map_err(|e| format!("mcp: tool '{}' failed on '{server}': {e}", event.tool_name))?;
        Ok(mcp_result_to_execution(
            &event.tool_name,
            result,
            event.step_id,
        ))
    }
}

/// A denial message must start with `permission:`; normalize client-provided
/// errors so the invariant holds.
fn normalize_denial(message: String) -> String {
    if message.starts_with("permission:") {
        message
    } else {
        format!("permission: {message}")
    }
}

impl ToolRouter {
    #[cfg(test)]
    fn control_pending_len_for_test(&self) -> usize {
        self.control.pending_len()
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/routing.tests.rs"]
mod tests;
