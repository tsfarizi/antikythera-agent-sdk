//! Concrete MCP transports (stdio / HTTP) for the `antikythera-tooling`
//! `McpTransport` port. MCP is a third routing destination, always executed
//! server-side (stdio transport is unavailable in the browser).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use antikythera_config::ServerConfig;
use antikythera_tooling::PROTOCOL_VERSION;
use antikythera_tooling::error::ToolInvokeError;
use antikythera_tooling::interface::{ServerToolInfo, ToolAnnotations, ToolExecution};
use antikythera_tooling::transport::McpTransport;
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

fn spawn_error(server: &str, e: std::io::Error) -> ToolInvokeError {
    ToolInvokeError::Spawn {
        server: server.to_string(),
        source: e,
    }
}

fn transport_error(server: &str, message: impl Into<String>) -> ToolInvokeError {
    ToolInvokeError::Transport {
        server: server.to_string(),
        message: message.into(),
    }
}

fn rpc_error(server: &str, value: &Value) -> ToolInvokeError {
    ToolInvokeError::Rpc {
        server: server.to_string(),
        code: value["code"].as_i64().unwrap_or(-1),
        message: value["message"]
            .as_str()
            .unwrap_or("unknown error")
            .to_string(),
    }
}

#[derive(Default)]
struct ServerInfo {
    instructions: Option<String>,
}

/// STDIO transport: spawns the MCP server subprocess and speaks newline
/// delimited JSON-RPC over its stdin/stdout.
pub struct StdioMcpTransport {
    server_name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    workdir: Option<PathBuf>,
    process: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<Option<BufReader<ChildStdout>>>,
    request_lock: tokio::sync::Mutex<()>,
    next_id: AtomicU64,
    connected: AtomicBool,
    info: Mutex<ServerInfo>,
    tools_cache: Mutex<Vec<ServerToolInfo>>,
}

impl StdioMcpTransport {
    pub fn new(config: ServerConfig) -> Self {
        let server_name = config.name.clone();
        Self {
            server_name,
            command: config
                .command
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            args: config.args.clone(),
            env: config.env.clone(),
            workdir: config.workdir.clone(),
            process: Mutex::new(None),
            stdin: Mutex::new(None),
            stdout: Mutex::new(None),
            request_lock: tokio::sync::Mutex::new(()),
            next_id: AtomicU64::new(1),
            connected: AtomicBool::new(false),
            info: Mutex::new(ServerInfo::default()),
            tools_cache: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl McpTransport for StdioMcpTransport {
    async fn connect(&self) -> Result<(), ToolInvokeError> {
        let mut command = Command::new(&self.command);
        command
            .args(&self.args)
            .envs(&self.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        if let Some(workdir) = &self.workdir {
            command.current_dir(workdir);
        }
        let mut child = command
            .spawn()
            .map_err(|e| spawn_error(&self.server_name, e))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| transport_error(&self.server_name, "spawned child has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| transport_error(&self.server_name, "spawned child has no stdout"))?;
        *self.process.lock().expect("mcp process lock poisoned") = Some(child);
        *self.stdin.lock().expect("mcp stdin lock poisoned") = Some(stdin);
        *self.stdout.lock().expect("mcp stdout lock poisoned") = Some(BufReader::new(stdout));

        let result = self
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "antikythera-server-runtime", "version": "1.0.0"},
                }),
            )
            .await?;
        let instructions = result
            .get("instructions")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        *self.info.lock().expect("mcp info lock poisoned") = ServerInfo { instructions };
        let _ = self
            .send_notification("notifications/initialized", json!({}))
            .await;
        self.refresh_tools().await?;
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value, ToolInvokeError> {
        let _guard = self.request_lock.lock().await;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut line = serde_json::to_string(&request)
            .map_err(|e| transport_error(&self.server_name, format!("encode request: {e}")))?;
        line.push('\n');
        self.write_line(&line).await?;
        let mut stdout = {
            let mut guard = self.stdout.lock().expect("mcp stdout lock poisoned");
            guard
                .take()
                .ok_or_else(|| transport_error(&self.server_name, "stdout not connected"))?
        };
        let result = self.read_response(&mut stdout, id).await;
        *self.stdout.lock().expect("mcp stdout lock poisoned") = Some(stdout);
        result
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<(), ToolInvokeError> {
        let request = json!({"jsonrpc": "2.0", "method": method, "params": params});
        let mut line = serde_json::to_string(&request)
            .map_err(|e| transport_error(&self.server_name, format!("encode notification: {e}")))?;
        line.push('\n');
        self.write_line(&line).await
    }

    async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value, ToolInvokeError> {
        let result = self
            .send_request("tools/call", json!({"name": tool, "arguments": arguments}))
            .await?;
        Ok(result)
    }

    async fn instructions(&self) -> Option<String> {
        self.info
            .lock()
            .expect("mcp info lock poisoned")
            .instructions
            .clone()
    }

    async fn tool_metadata(&self, tool: &str) -> Option<ServerToolInfo> {
        self.tools_cache
            .lock()
            .expect("mcp tools cache lock poisoned")
            .iter()
            .find(|t| t.name == tool)
            .cloned()
    }

    async fn list_tools(&self) -> Vec<ServerToolInfo> {
        self.tools_cache
            .lock()
            .expect("mcp tools cache lock poisoned")
            .clone()
    }

    fn server_name(&self) -> &str {
        &self.server_name
    }

    async fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
        let child = {
            let mut guard = self.process.lock().expect("mcp process lock poisoned");
            guard.take()
        };
        if let Some(mut child) = child {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        *self.stdin.lock().expect("mcp stdin lock poisoned") = None;
        *self.stdout.lock().expect("mcp stdout lock poisoned") = None;
    }
}

impl StdioMcpTransport {
    /// Pull `tools/list` into the cache (used after initialize).
    async fn refresh_tools(&self) -> Result<(), ToolInvokeError> {
        let result = self.send_request("tools/list", json!({})).await?;
        let tools = parse_tools_list(&result);
        *self
            .tools_cache
            .lock()
            .expect("mcp tools cache lock poisoned") = tools;
        Ok(())
    }

    /// Write one JSON-RPC line to the child stdin (tokio async write).
    async fn write_line(&self, line: &str) -> Result<(), ToolInvokeError> {
        use tokio::io::AsyncWriteExt;
        let mut stdin = {
            let mut guard = self.stdin.lock().expect("mcp stdin lock poisoned");
            guard
                .take()
                .ok_or_else(|| transport_error(&self.server_name, "stdin not connected"))?
        };
        let result =
            async {
                stdin.write_all(line.as_bytes()).await.map_err(|e| {
                    transport_error(&self.server_name, format!("write request: {e}"))
                })?;
                stdin.flush().await.map_err(|e| {
                    transport_error(&self.server_name, format!("flush request: {e}"))
                })?;
                Ok(())
            }
            .await;
        *self.stdin.lock().expect("mcp stdin lock poisoned") = Some(stdin);
        result
    }

    /// Read newline-delimited responses until the one matching `id`.
    /// Notifications and other ids are skipped.
    ///
    async fn read_response(
        &self,
        stdout: &mut BufReader<ChildStdout>,
        id: u64,
    ) -> Result<Value, ToolInvokeError> {
        loop {
            let mut line = String::new();
            let n = stdout
                .read_line(&mut line)
                .await
                .map_err(|e| transport_error(&self.server_name, format!("read response: {e}")))?;
            if n == 0 {
                return Err(ToolInvokeError::Terminated {
                    server: self.server_name.clone(),
                });
            }
            let value: Value =
                serde_json::from_str(line.trim()).map_err(|e| ToolInvokeError::InvalidJson {
                    server: self.server_name.clone(),
                    source: e,
                })?;
            if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if value.get("error").is_some() {
                    return Err(rpc_error(&self.server_name, &value["error"]));
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }
}

/// HTTP transport: JSON-RPC over a stateless POST endpoint.
pub struct HttpMcpTransport {
    server_name: String,
    url: String,
    headers: Vec<(String, String)>,
    http: reqwest::Client,
    next_id: AtomicU64,
    connected: AtomicBool,
    info: Mutex<ServerInfo>,
    tools_cache: Mutex<Vec<ServerToolInfo>>,
}

impl HttpMcpTransport {
    pub fn new(config: ServerConfig) -> Self {
        let server_name = config.name.clone();
        Self {
            server_name,
            url: config.url.clone().unwrap_or_default(),
            headers: config.headers.clone().into_iter().collect(),
            http: reqwest::Client::new(),
            next_id: AtomicU64::new(1),
            connected: AtomicBool::new(false),
            info: Mutex::new(ServerInfo::default()),
            tools_cache: Mutex::new(Vec::new()),
        }
    }

    async fn post(&self, body: Value) -> Result<Value, ToolInvokeError> {
        let mut request = self.http.post(&self.url).json(&body);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        let response = request
            .send()
            .await
            .map_err(|e| transport_error(&self.server_name, e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(transport_error(
                &self.server_name,
                format!("HTTP {status}: {text}"),
            ));
        }
        response
            .json::<Value>()
            .await
            .map_err(|e| transport_error(&self.server_name, format!("decode response: {e}")))
    }
}

#[async_trait]
impl McpTransport for HttpMcpTransport {
    async fn connect(&self) -> Result<(), ToolInvokeError> {
        let result = self
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "antikythera-server-runtime", "version": "1.0.0"},
                }),
            )
            .await?;
        let instructions = result
            .get("instructions")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        *self.info.lock().expect("mcp info lock poisoned") = ServerInfo { instructions };
        let _ = self
            .send_notification("notifications/initialized", json!({}))
            .await;
        self.refresh_tools().await?;
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value, ToolInvokeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let response = self.post(body).await?;
        if response.get("error").is_some() {
            return Err(rpc_error(&self.server_name, &response["error"]));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<(), ToolInvokeError> {
        let body = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.post(body).await?;
        Ok(())
    }

    async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value, ToolInvokeError> {
        let result = self
            .send_request("tools/call", json!({"name": tool, "arguments": arguments}))
            .await?;
        Ok(result)
    }

    async fn instructions(&self) -> Option<String> {
        self.info
            .lock()
            .expect("mcp info lock poisoned")
            .instructions
            .clone()
    }

    async fn tool_metadata(&self, tool: &str) -> Option<ServerToolInfo> {
        self.tools_cache
            .lock()
            .expect("mcp tools cache lock poisoned")
            .iter()
            .find(|t| t.name == tool)
            .cloned()
    }

    async fn list_tools(&self) -> Vec<ServerToolInfo> {
        self.tools_cache
            .lock()
            .expect("mcp tools cache lock poisoned")
            .clone()
    }

    fn server_name(&self) -> &str {
        &self.server_name
    }

    async fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }
}

impl HttpMcpTransport {
    async fn refresh_tools(&self) -> Result<(), ToolInvokeError> {
        let result = self.send_request("tools/list", json!({})).await?;
        let tools = parse_tools_list(&result);
        *self
            .tools_cache
            .lock()
            .expect("mcp tools cache lock poisoned") = tools;
        Ok(())
    }
}

/// Parse the `tools/list` result into `ServerToolInfo`.
fn parse_tools_list(result: &Value) -> Vec<ServerToolInfo> {
    result
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| {
                    let name = t.get("name")?.as_str()?.to_string();
                    let title = t
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let description = t
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let input_schema = t.get("inputSchema").cloned();
                    let output_schema = t.get("outputSchema").cloned();
                    let annotations = t
                        .get("annotations")
                        .and_then(|v| serde_json::from_value::<ToolAnnotations>(v.clone()).ok());
                    let execution = t
                        .get("execution")
                        .and_then(|v| serde_json::from_value::<ToolExecution>(v.clone()).ok());
                    Some(ServerToolInfo {
                        name,
                        title,
                        description,
                        icons: None,
                        input_schema,
                        output_schema,
                        annotations,
                        execution,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Convert an MCP `ServerToolInfo` into the wire `ToolDefinition`.
pub fn server_info_to_definition(info: &ServerToolInfo) -> crate::wire::ToolDefinition {
    crate::wire::ToolDefinition {
        name: info.name.clone(),
        title: info.title.clone(),
        description: info.description.clone().unwrap_or_default(),
        parameters: Vec::new(),
        input_schema: info.input_schema.clone(),
        output_schema: info.output_schema.clone(),
    }
}

/// Map an MCP `tools/call` result into a wire `ToolExecutionResult`.
pub fn mcp_result_to_execution(
    tool: &str,
    result: Value,
    step_id: u32,
) -> crate::wire::ToolExecutionResult {
    let is_error = result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = result
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    crate::wire::ToolExecutionResult {
        tool_name: tool.to_string(),
        success: !is_error,
        output_json: json!({"content": text}).to_string(),
        error_message: if is_error {
            Some(if text.is_empty() {
                "mcp tool call failed".to_string()
            } else {
                text
            })
        } else {
            None
        },
        step_id,
    }
}

/// Type alias for a shared transport handle.
pub type SharedMcpTransport = Arc<dyn McpTransport>;
