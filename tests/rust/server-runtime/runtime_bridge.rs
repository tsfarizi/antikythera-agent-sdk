//! Runtime Bridge acceptance tests â€” SERVER side of the 2x2 connectivity
//! matrix (core runs on the server).
//!
//! Each test proves one clause of the acceptance invariants 1-7 from the
//! server's point of view, using the real wire protocol where it matters:
//!
//!   1. builtin in-band: composite executes `echo` inside commit; no host
//!      execution, no double execution.
//!   2. server-only local tool: registered handler is executed through the
//!      loop and its output is fed back through process-tool-result.
//!   3. client-only remote tool: SSE `tool-execution-request` + POST-back
//!      roundtrip; the POST-back result is consumed by the loop.
//!   4. runtime-hook roundtrip: `hook-request` over SSE answered by a client
//!      POST-back override; the override is committed.
//!   5. MCP routing: a mock MCP HTTP server's tool executes through the MCP
//!      transport and its result enters the loop.
//!   6. permission denial: a tool outside the allowlist fails with
//!      `permission:`.
//!   7. registry collision: cross-side same-name registration is rejected.
//!   8. LLM quota gate: exceeding the per-session quota denies with
//!      `permission: llm quota exceeded`.
//!   9. `host-imports.emit-tool-call` from a drop-in logic core routes to all
//!      three destinations (local / remote / mcp).
//!  10. wire-shape consistency: HTTP endpoints serialize exactly the golden
//!      shapes in `contracts/shared/wire_protocol.golden.json`.
//!
//! Run: `cargo test -p antikythera-server-runtime --test runtime_bridge`

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use antikythera_config::{ServerConfig, TransportType};
use antikythera_server_runtime::RuntimeServer;
use antikythera_server_runtime::config::{HookName, LlmProviderSpec, ServerRuntimeConfig};
use antikythera_server_runtime::core::CoreSession;
use antikythera_server_runtime::llm::{LlmError, LlmProvider};
use antikythera_server_runtime::loop_owner::{ToolLoopConfig, run_tool_loop};
use antikythera_server_runtime::registry::{Destination, ToolOwner};
use antikythera_server_runtime::routing::ToolRouter;
use antikythera_server_runtime::wire::{LlmRequest, LlmResponse, ToolDefinition};
use async_trait::async_trait;
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio_stream::StreamExt;

fn component_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/antikythera-sdk.wasm")
}

fn logic_core_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-wasip2/release/logic_core_host_example.wasm")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/shared/wire_protocol.golden.json")
}

/// True when the composite is present; prints the SKIP marker otherwise.
fn composite_ready() -> bool {
    let path = component_path();
    if path.exists() {
        true
    } else {
        panic!(
            "Required artifact missing: composite {} not built; run `task build` first",
            path.display()
        )
    }
}

fn stub_config() -> ServerRuntimeConfig {
    ServerRuntimeConfig {
        component_path: component_path(),
        providers: HashMap::from([(
            "stub".to_string(),
            LlmProviderSpec::Stub {
                response: "{\"action\":\"final\",\"content\":\"bridge-complete\"}".to_string(),
            },
        )]),
        default_provider: "stub".to_string(),
        ..ServerRuntimeConfig::default()
    }
}

#[tokio::test]
async fn authenticated_http_and_quota_capacity_fail_closed() {
    let mut config = stub_config();
    config.auth_token = Some("test-runtime-token".into());
    config.allowed_origins = vec!["https://trusted.example".into()];
    config.policy.set_llm_quota(1);
    let server = RuntimeServer::new(config, tokio::runtime::Handle::current()).unwrap();
    for index in 0..1024 {
        server
            .shared
            .check_llm_gate(Some(&format!("session-{index}")))
            .unwrap();
    }
    assert!(
        server
            .shared
            .check_llm_gate(Some("new-session"))
            .unwrap_err()
            .contains("capacity")
    );
    assert!(
        server
            .shared
            .check_llm_gate(Some("session-0"))
            .unwrap_err()
            .contains("quota")
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/antikythera/v1/tools",
        listener.local_addr().unwrap()
    );
    let router = antikythera_server_runtime::http::router(server.shared.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let client = reqwest::Client::new();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
    assert_eq!(
        client
            .get(&url)
            .bearer_auth("test-runtime-token")
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(&url)
            .bearer_auth("test-runtime-token")
            .header("Origin", "https://untrusted.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let allowed = client
        .get(&url)
        .bearer_auth("test-runtime-token")
        .header("Origin", "https://trusted.example")
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), 200);
    assert_eq!(
        allowed.headers()["access-control-allow-origin"],
        "https://trusted.example"
    );
    task.abort();
}

/// Deterministic LLM provider returning a scripted sequence of responses
/// (the last response repeats).
struct ScriptedStub {
    responses: Vec<String>,
    calls: AtomicU32,
}

#[async_trait]
impl LlmProvider for ScriptedStub {
    fn name(&self) -> &str {
        "scripted"
    }

    async fn call(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let i = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
        let content = self
            .responses
            .get(i)
            .or_else(|| self.responses.last())
            .cloned()
            .unwrap_or_default();
        Ok(LlmResponse {
            content,
            model: request.model,
            session_id: request.session_id,
            message_json: None,
            tokens_used: Some(4),
            finish_reason: Some("stop".to_string()),
            raw_response_json: None,
        })
    }
}

fn scripted_provider(responses: Vec<String>) -> Arc<dyn LlmProvider> {
    Arc::new(ScriptedStub {
        responses,
        calls: AtomicU32::new(0),
    })
}

/// The two-step script used by tool-loop tests: call `tool` once, then final.
fn call_then_final(tool: &str, input: Value) -> Vec<String> {
    vec![
        json!({"action": "call_tool", "tool": tool, "input": input}).to_string(),
        json!({"action": "final", "content": "after-tool"}).to_string(),
    ]
}

fn build_server(
    config: ServerRuntimeConfig,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
) -> RuntimeServer {
    RuntimeServer::new_with_providers(config, providers, tokio::runtime::Handle::current())
        .expect("build server runtime")
}

/// Bind the wire-protocol router to an ephemeral port and return its base URL.
async fn spawn_http_server(server: &RuntimeServer) -> String {
    let router = server.http_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("ephemeral local addr");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("wire http server");
    });
    format!("http://{addr}")
}

/// Connect to the SSE control channel as the wire client. Every received
/// envelope is appended to the returned log; when `on_event` returns Some the
/// reader POSTs the body back to
/// `POST /antikythera/v1/events/{correlation-id}/response`.
fn spawn_sse_client(
    base_url: &str,
    client_id: &str,
    session_id: Option<&str>,
    on_event: impl Fn(Value) -> Option<Value> + Send + Sync + 'static,
) -> (tokio::task::JoinHandle<()>, Arc<Mutex<Vec<Value>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_log = events.clone();
    let mut url = format!("{base_url}/antikythera/v1/events?client_id={client_id}");
    if let Some(sid) = session_id {
        url.push_str("&session_id=");
        url.push_str(&sid.replace('&', "%26"));
    }
    let base_url = base_url.to_string();
    let client = reqwest::Client::new();
    let handle = tokio::spawn(async move {
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(e) => {
                eprintln!("[sse-client] connect failed: {e}");
                return;
            }
        };
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else { break };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer.drain(..=pos);
                let line = line.trim();
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "keepalive" {
                    continue;
                }
                let envelope: Value = match serde_json::from_str(data) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                events_log
                    .lock()
                    .expect("sse events lock poisoned")
                    .push(envelope.clone());
                if let Some(body) = on_event(envelope.clone()) {
                    let corr = envelope["correlation_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if corr.is_empty() {
                        continue;
                    }
                    let post_url = format!("{base_url}/antikythera/v1/events/{corr}/response");
                    let _ = client.post(post_url).json(&body).send().await;
                }
            }
        }
    });
    (handle, events)
}

/// Wait until the SSE client has registered on the control channel (otherwise
/// remote requests fail closed with `requires a connected client`).
async fn wait_until_connected(server: &RuntimeServer, client_id: &str) {
    let control = server.control();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !control.is_client_connected(client_id) {
        assert!(
            std::time::Instant::now() < deadline,
            "SSE client '{client_id}' never connected"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Connect an MCP server from a non-async context (the router blocks on the
/// runtime handle internally).
fn connect_mcp(router: Arc<ToolRouter>, config: ServerConfig) -> Result<(), String> {
    std::thread::spawn(move || router.connect_mcp_server(config))
        .join()
        .map_err(|_| "mcp connect thread panicked".to_string())?
}

fn mcp_server_config(name: &str, url: &str) -> ServerConfig {
    ServerConfig {
        name: name.to_string(),
        transport: TransportType::Http,
        command: None,
        args: Vec::new(),
        env: HashMap::new(),
        workdir: None,
        url: Some(url.to_string()),
        headers: HashMap::new(),
        default_timezone: None,
        default_city: None,
    }
}

/// State of the mock MCP server (HTTP JSON-RPC).
#[derive(Clone, Default)]
struct MockMcpState {
    tool_call_count: Arc<AtomicU32>,
}

async fn mock_mcp_handle(
    State(state): State<MockMcpState>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let method = body["method"].as_str().unwrap_or_default().to_string();
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let result = match method.as_str() {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "serverInfo": {"name": "mock-mcp", "version": "1.0.0"},
            "instructions": null,
        }),
        "notifications/initialized" => json!({}),
        "tools/list" => json!({
            "tools": [{
                "name": "mcp_greet",
                "description": "mock mcp tool",
                "inputSchema": {"type": "object", "properties": {}, "required": []},
            }]
        }),
        "tools/call" => {
            state.tool_call_count.fetch_add(1, Ordering::SeqCst);
            json!({"content": [{"type": "text", "text": "mcp-done-42"}], "isError": false})
        }
        _ => json!({"error": {"code": -32601, "message": "method not found"}}),
    };
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

async fn spawn_mock_mcp() -> (String, MockMcpState) {
    let state = MockMcpState::default();
    let app = Router::new()
        .route("/", post(mock_mcp_handle))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock mcp port");
    let addr = listener.local_addr().expect("mock mcp local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock mcp server");
    });
    (format!("http://{addr}/"), state)
}

/// Golden-shape comparison helpers (acceptance invariant 5).
///
fn golden() -> Value {
    let path = golden_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden file {}: {e}", path.display()));
    serde_json::from_str(&text).expect("golden file is valid JSON")
}

/// Every field the implementation serialized must be declared in the golden
/// shape (the golden file is the source of truth; no extra fields allowed).
fn assert_live_fields_within_golden(label: &str, live: &Value, golden_shape: &Value) {
    let live_obj = live
        .as_object()
        .unwrap_or_else(|| panic!("{label}: live value is not an object: {live}"));
    let golden_obj = golden_shape
        .as_object()
        .unwrap_or_else(|| panic!("{label}: golden shape is not an object"));
    for key in live_obj.keys() {
        assert!(
            golden_obj.contains_key(key),
            "{label}: live field '{key}' is not declared in the golden shape"
        );
    }
}

/// The live object must carry exactly the golden field set.
fn assert_live_fields_exact_golden(label: &str, live: &Value, golden_shape: &Value) {
    assert_live_fields_within_golden(label, live, golden_shape);
    let live_obj = live.as_object().expect("live object");
    let golden_obj = golden_shape.as_object().expect("golden object");
    for key in golden_obj.keys() {
        assert!(
            live_obj.contains_key(key),
            "{label}: golden field '{key}' is missing from the live response"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// 1. core@server -> tool server (builtin in-band).
///
/// The local handler counts host executions. The composite must execute `echo` IN-BAND inside
/// commit (tool-registry builtin), so the loop must never route it to the host: reaching final
/// with counter == 0 proves the drain contained the in-band `tool_result` and there was no double
/// execution.
///
async fn builtin_echo_executes_in_band_without_host_execution() {
    if !composite_ready() {
        return;
    }
    let mut config = stub_config();
    config.policy.allow_tool(Destination::Local, "echo");
    let provider = scripted_provider(call_then_final("echo", json!({"hi": 1})));
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));

    let host_calls = Arc::new(AtomicU32::new(0));
    let counter = host_calls.clone();
    server
        .router()
        .register_local_tool(
            ToolDefinition::simple("echo", "reference builtin echo"),
            Arc::new(move |args| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"echoed": args}))
            }),
        )
        .expect("register echo");

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "in-band-echo".to_string(),
        prompts: vec!["use echo".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| run_tool_loop(core, &shared, loop_config));
    let outcome = handle
        .join()
        .expect("core thread panicked")
        .expect("tool loop failed");

    assert_eq!(outcome.action, "final", "commit: {}", outcome.commit_json);
    assert_eq!(outcome.content.as_deref(), Some("after-tool"));
    assert_eq!(
        host_calls.load(Ordering::SeqCst),
        0,
        "builtin echo must execute in-band inside the composite; a host \
         execution (counter > 0) proves the tool was routed to the host, \
         i.e. double execution"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// 2. core@server -> tool server (server-only local).
///
/// The handler's output was fed back: the runner recorded a tool_result.
///
async fn server_local_tool_executes_via_handler_through_the_loop() {
    if !composite_ready() {
        return;
    }
    let mut config = stub_config();
    config.policy.allow_tool(Destination::Local, "server_time");
    let provider = scripted_provider(call_then_final("server_time", json!({"tz": "UTC"})));
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));

    let handler_calls = Arc::new(AtomicU32::new(0));
    let counter = handler_calls.clone();
    server
        .router()
        .register_local_tool(
            ToolDefinition::simple("server_time", "server-only deterministic clock"),
            Arc::new(move |args| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"datetime": "2026-08-12T00:00:00Z", "args": args}))
            }),
        )
        .expect("register server_time");

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "local-tool-server".to_string(),
        prompts: vec!["what time is it".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| {
        let outcome = run_tool_loop(core, &shared, loop_config)?;
        let drain = core.drain_events(&outcome.session_id)?;
        Ok((outcome, drain))
    });
    let (outcome, drain) = handle
        .join()
        .expect("core thread panicked")
        .expect("tool loop failed");

    assert_eq!(outcome.action, "final");
    assert_eq!(outcome.content.as_deref(), Some("after-tool"));
    assert_eq!(
        handler_calls.load(Ordering::SeqCst),
        1,
        "the local handler must be invoked exactly once"
    );
    let drain: Value = serde_json::from_str(&drain).expect("drain is JSON");
    let tool_result = drain
        .as_array()
        .expect("drain is array")
        .iter()
        .find(|e| e["kind"] == "tool_result" && e["payload"]["tool"] == "server_time")
        .unwrap_or_else(|| panic!("no tool_result for server_time in drain: {drain}"));
    assert_eq!(
        tool_result["payload"]["success"], true,
        "server_time handler output must be consumed by process-tool-result"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// 3. core@server -> tool client (remote, SSE + POST-back).
///
/// The wire client: connect over SSE and answer the tool-execution-request with a canned
/// tool-execution-result POST-back.
///
/// Assertion 1: the server emitted a wire-shaped tool-execution-request on the SSE control
/// channel.
///
/// Envelope shape is the golden tool_execution_request_event shape.
///
/// Assertion 2: the POST-back result was consumed by the loop â€” the runner recorded a
/// tool_result for client_secret after process-tool-result.
///
async fn client_tool_routes_remote_via_sse_request_and_postback() {
    if !composite_ready() {
        return;
    }
    let mut config = stub_config();
    config
        .policy
        .allow_tool(Destination::Remote, "client_secret");
    let provider = scripted_provider(call_then_final("client_secret", json!({"ask": "secret"})));
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));
    server
        .router()
        .register_client_tools(vec![ToolDefinition::simple(
            "client_secret",
            "client-side secret tool",
        )])
        .expect("register client tool");

    let base_url = spawn_http_server(&server).await;
    let client_id = server.client_id().to_string();

    let (sse_task, events) =
        spawn_sse_client(&base_url, &client_id, None, move |envelope: Value| {
            if envelope["type"] == "tool-execution-request"
                && envelope["payload"]["tool-name"] == "client_secret"
            {
                let corr = envelope["correlation_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                Some(json!({
                    "correlation_id": corr,
                    "ok": true,
                    "payload": {
                        "tool-name": "client_secret",
                        "success": true,
                        "output-json": "{\"secret\":\"opensecret\"}",
                        "error-message": null,
                        "step-id": 0,
                    },
                    "error": null,
                }))
            } else {
                None
            }
        });
    wait_until_connected(&server, &client_id).await;

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "remote-tool-server".to_string(),
        prompts: vec!["read the secret".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| {
        let outcome = run_tool_loop(core, &shared, loop_config)?;
        let drain = core.drain_events(&outcome.session_id)?;
        Ok((outcome, drain))
    });
    let (outcome, drain) = handle
        .join()
        .expect("core thread panicked")
        .expect("tool loop failed");
    assert_eq!(outcome.action, "final");

    let events = events.lock().expect("sse events lock poisoned");
    let request = events
        .iter()
        .find(|e| {
            e["type"] == "tool-execution-request" && e["payload"]["tool-name"] == "client_secret"
        })
        .unwrap_or_else(|| panic!("no tool-execution-request received; envelopes: {events:?}"));
    assert_eq!(request["type"], "tool-execution-request");
    assert!(request["correlation_id"].as_str().is_some());
    assert_eq!(request["client_id"].as_str(), Some(client_id.as_str()));
    let golden_shape = &golden()["tool_execution_request_event"];
    assert_live_fields_within_golden("tool-execution-request envelope", request, golden_shape);

    let drain: Value = serde_json::from_str(&drain).expect("drain is JSON");
    let tool_result = drain
        .as_array()
        .expect("drain is array")
        .iter()
        .find(|e| e["kind"] == "tool_result" && e["payload"]["tool"] == "client_secret")
        .unwrap_or_else(|| panic!("no tool_result for client_secret in drain: {drain}"));
    assert_eq!(
        tool_result["payload"]["success"], true,
        "POST-back tool-execution-result must be processed by the loop"
    );
    sse_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// 4. hook-request roundtrip (runtime-hooks hosted on the server).
///
/// The wire client answers hook-requests: passthrough for prepare-turn, an action/content
/// override for decide-action.
///
/// The decide-action hook-request was sent over SSE and its POST-back decision was committed.
///
async fn hook_request_roundtrips_over_sse_and_override_is_committed() {
    if !composite_ready() {
        return;
    }
    let mut config = stub_config();
    config.policy.allow_hook(HookName::PrepareTurn);
    config.policy.allow_hook(HookName::DecideAction);
    let provider = scripted_provider(vec![
        json!({"action": "final", "content": "server-default"}).to_string(),
    ]);
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));

    let base_url = spawn_http_server(&server).await;
    let client_id = server.client_id().to_string();

    let (sse_task, events) =
        spawn_sse_client(&base_url, &client_id, None, move |envelope: Value| {
            if envelope["type"] == "hook-request" {
                let corr = envelope["correlation_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let hook = envelope["payload"]["hook"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let payload = if hook == "decide-action" {
                    json!({"action": "final", "content": "client-hook-decision"})
                } else {
                    json!({"passthrough": true})
                };
                Some(json!({"correlation_id": corr, "ok": true, "payload": payload, "error": null}))
            } else {
                None
            }
        });
    wait_until_connected(&server, &client_id).await;

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "hook-override-server".to_string(),
        prompts: vec!["decide for me".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        runtime_hooks_enabled: true,
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| run_tool_loop(core, &shared, loop_config));
    let outcome = handle
        .join()
        .expect("core thread panicked")
        .expect("tool loop failed");

    assert_eq!(outcome.action, "final");
    assert_eq!(
        outcome.content.as_deref(),
        Some("client-hook-decision"),
        "the client hook override must be committed as the final content"
    );
    let events = events.lock().expect("sse events lock poisoned");
    let hook_requests: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "hook-request")
        .collect();
    assert!(
        hook_requests
            .iter()
            .any(|e| e["payload"]["hook"] == "decide-action"),
        "a decide-action hook-request must have been sent; envelopes: {events:?}"
    );
    let golden_shape = &golden()["hook_request_event"];
    for event in &hook_requests {
        assert_live_fields_within_golden("hook-request envelope", event, golden_shape);
    }
    sse_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// The mock MCP server was reached through the MCP transport.
///
/// The MCP result entered the loop through process-tool-result.
///
async fn mcp_tool_routes_through_mcp_transport_into_the_loop() {
    if !composite_ready() {
        return;
    }
    let (mcp_url, mcp_state) = spawn_mock_mcp().await;
    let mut config = stub_config();
    config.policy.allow_tool(Destination::Mcp, "mcp_greet");
    let provider = scripted_provider(call_then_final("mcp_greet", json!({"name": "world"})));
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));

    let router = server.router();
    connect_mcp(router.clone(), mcp_server_config("mock-mcp", &mcp_url))
        .expect("connect mock mcp server");
    assert_eq!(
        router.owner_of("mcp_greet"),
        Some(ToolOwner::Mcp),
        "the MCP transport must register its tools as mcp-owned"
    );

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "mcp-loop-server".to_string(),
        prompts: vec!["greet via mcp".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| {
        let outcome = run_tool_loop(core, &shared, loop_config)?;
        let drain = core.drain_events(&outcome.session_id)?;
        Ok((outcome, drain))
    });
    let (outcome, drain) = handle
        .join()
        .expect("core thread panicked")
        .expect("tool loop failed");
    assert_eq!(outcome.action, "final");

    assert!(
        mcp_state.tool_call_count.load(Ordering::SeqCst) >= 1,
        "mock MCP tools/call must be invoked"
    );
    let drain: Value = serde_json::from_str(&drain).expect("drain is JSON");
    let tool_result = drain
        .as_array()
        .expect("drain is array")
        .iter()
        .find(|e| e["kind"] == "tool_result" && e["payload"]["tool"] == "mcp_greet")
        .unwrap_or_else(|| panic!("no tool_result for mcp_greet in drain: {drain}"));
    assert_eq!(
        tool_result["payload"]["success"], true,
        "MCP execution result must be consumed by the loop"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// Default-deny policy: `restricted_tool` is registered but NOT allowed.
///
async fn tool_outside_allowlist_denied_with_permission_error() {
    if !composite_ready() {
        return;
    }
    let config = stub_config();
    let provider = scripted_provider(vec![
        json!({"action": "call_tool", "tool": "restricted_tool", "input": {}}).to_string(),
        json!({"action": "final", "content": "unreachable"}).to_string(),
    ]);
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));
    server
        .router()
        .register_local_tool(
            ToolDefinition::simple("restricted_tool", "registered but not allowlisted"),
            Arc::new(|_| Ok(json!({"ran": true}))),
        )
        .expect("register restricted_tool");

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "denied-loop-server".to_string(),
        prompts: vec!["use restricted_tool".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| run_tool_loop(core, &shared, loop_config));
    let err = handle
        .join()
        .expect("core thread panicked")
        .expect_err("the loop must fail on a permission denial");
    assert!(
        err.contains("permission:"),
        "denial must surface as `permission:` error, got: {err}"
    );
}

#[test]
/// The original owner is untouched.
///
fn union_registry_rejects_cross_side_name_collision() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("test runtime");
    let server = RuntimeServer::new_with_providers(
        ServerRuntimeConfig::default(),
        HashMap::new(),
        rt.handle().clone(),
    )
    .expect("build server runtime");
    let router = server.router();

    router
        .register_local_tool(
            ToolDefinition::simple("dup_tool", "server side"),
            Arc::new(|_| Ok(json!({}))),
        )
        .expect("register server owner");
    let err = router
        .register_client_tools(vec![ToolDefinition::simple("dup_tool", "client side")])
        .expect_err("cross-side same-name registration must be rejected");
    assert!(
        err.contains("tool registry: name collision"),
        "expected canonical R5 collision message, got: {err}"
    );
    assert_eq!(router.owner_of("dup_tool"), Some(ToolOwner::Server));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// Two LLM calls are needed to reach final (call_tool echo, then final); the quota allows exactly
/// one, so the second check must fail closed.
///
/// Echo is builtin in-band; registration only makes the union validate.
///
async fn llm_quota_gate_denies_when_session_exceeds_quota() {
    if !composite_ready() {
        return;
    }
    let mut config = stub_config();
    config.policy.allow_tool(Destination::Local, "echo");
    config.policy.set_llm_quota(1);
    let provider = scripted_provider(call_then_final("echo", json!({})));
    let server = build_server(config, HashMap::from([("scripted".to_string(), provider)]));
    server
        .router()
        .register_local_tool(
            ToolDefinition::simple("echo", "echo"),
            Arc::new(|_| Ok(json!({"echoed": true}))),
        )
        .expect("register echo");

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "quota-loop-server".to_string(),
        prompts: vec!["use echo".to_string()],
        provider: "scripted".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| run_tool_loop(core, &shared, loop_config));
    let err = handle
        .join()
        .expect("core thread panicked")
        .expect_err("the second LLM call must be denied by the quota");
    assert!(
        err.contains("permission: llm quota exceeded"),
        "expected quota denial, got: {err}"
    );
}

/// Drive the drop-in logic core (`logic-core-host-example`) through a single
/// `process-tool-result-for-session` call; the core's custom hook asks the
/// host to execute `tool` via `host-imports.emit-tool-call`, which routes
/// through the server's tool router. Returns the committed summary content.
/// 9. emit-tool-call logic core -> 3 destinations (invariant 6).
///
fn drive_logic_core_tool(
    server: &RuntimeServer,
    session: &str,
    tool: &str,
) -> Result<String, String> {
    let shared = server.shared.clone();
    let path = logic_core_path();
    let session = session.to_string();
    let tool = tool.to_string();
    std::thread::spawn(move || {
        let mut core = CoreSession::new(&path, shared.clone())
            .map_err(|e| format!("logic core session: {e:#}"))?;
        let config_json = json!({
            "session_id": session,
            "max_steps": 5,
            "auto_execute_tools": false,
        })
        .to_string();
        let sid = core.init(&config_json)?;
        let tool_result_json = json!({
            "tool_name": tool,
            "success": true,
            "output_json": "{}",
            "error_message": null,
            "correlation_id": null,
        })
        .to_string();
        let result = core.process_tool_result_for_session(&sid, &tool_result_json)?;
        let value: Value = serde_json::from_str(&result)
            .map_err(|e| format!("logic core result is not JSON: {e}"))?;
        let content = value["content"].as_str().unwrap_or_default().to_string();
        Ok(content)
    })
    .join()
    .map_err(|_| "logic core thread panicked".to_string())?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// (a) local destination: allowlisted echo executes server-side.
///
/// (b) remote destination: client_secret goes out as an SSE tool-execution-request and the
/// POST-back result comes back.
///
/// (c) mcp destination: mcp_greet executes through the MCP transport.
///
async fn logic_core_emit_tool_call_routes_local_remote_mcp() {
    let path = logic_core_path();
    if !path.exists() {
        panic!(
            "Required artifact missing: logic-core-host-example artifact {} not built; run \
             `cargo component build -p logic-core-host-example --release \
             --target wasm32-wasip2 --no-default-features --features component`",
            path.display()
        );
    }

    let (mcp_url, _mcp_state) = spawn_mock_mcp().await;
    let mut config = stub_config();
    config.policy.allow_tool(Destination::Local, "echo");
    config
        .policy
        .allow_tool(Destination::Remote, "client_secret");
    config.policy.allow_tool(Destination::Mcp, "mcp_greet");
    let server = build_server(config, HashMap::new());
    server
        .router()
        .register_local_tool(
            ToolDefinition::simple("echo", "server echo"),
            Arc::new(|args| Ok(json!({"echoed": args}))),
        )
        .expect("register echo");
    server
        .router()
        .register_client_tools(vec![ToolDefinition::simple(
            "client_secret",
            "client-side secret",
        )])
        .expect("register client_secret");
    connect_mcp(server.router(), mcp_server_config("mock-mcp", &mcp_url))
        .expect("connect mock mcp");

    let content = drive_logic_core_tool(&server, "lc-local", "echo")
        .unwrap_or_else(|e| panic!("logic core local drive failed: {e}"));
    assert!(
        content.contains("tool echo -> success: true"),
        "local emit-tool-call must route to the server handler, got: {content}"
    );

    let base_url = spawn_http_server(&server).await;
    let client_id = server.client_id().to_string();
    let (sse_task, _events) =
        spawn_sse_client(&base_url, &client_id, None, move |envelope: Value| {
            if envelope["type"] == "tool-execution-request"
                && envelope["payload"]["tool-name"] == "client_secret"
            {
                let corr = envelope["correlation_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                Some(json!({
                    "correlation_id": corr,
                    "ok": true,
                    "payload": {
                        "tool-name": "client_secret",
                        "success": true,
                        "output-json": "{\"secret\":\"opensecret\"}",
                        "error-message": null,
                        "step-id": 0,
                    },
                    "error": null,
                }))
            } else {
                None
            }
        });
    wait_until_connected(&server, &client_id).await;
    let content = drive_logic_core_tool(&server, "lc-remote", "client_secret")
        .unwrap_or_else(|e| panic!("logic core remote drive failed: {e}"));
    assert!(
        content.contains("client_secret"),
        "remote emit-tool-call must round-trip through the client, got: {content}"
    );
    assert!(
        content.contains("opensecret"),
        "the POST-back output must reach the logic core, got: {content}"
    );
    sse_task.abort();

    let content = drive_logic_core_tool(&server, "lc-mcp", "mcp_greet")
        .unwrap_or_else(|e| panic!("logic core mcp drive failed: {e}"));
    assert!(
        content.contains("mcp-done-42"),
        "mcp emit-tool-call must route through the MCP transport, got: {content}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// 10. Wire-shape consistency vs the golden contract.
///
/// POST /llm/call -> llm-response shape must equal the golden field set.
///
/// POST /tools/execute -> tool-execution-result shape must equal the golden field set.
///
/// GET /tools -> array of ToolDefinition; every live field is declared in the golden tool shape
/// and the required fields are present.
///
async fn wire_shapes_match_golden_contract() {
    if !composite_ready() {
        return;
    }
    let mut config = stub_config();
    config
        .policy
        .allow_tool(Destination::Local, "get_current_time");
    let server = RuntimeServer::new(config, tokio::runtime::Handle::current())
        .expect("build server runtime");
    server
        .router()
        .register_local_tool(
            ToolDefinition::simple("get_current_time", "deterministic clock"),
            Arc::new(|_| Ok(json!({"datetime": "2026-08-12T00:00:00Z"}))),
        )
        .expect("register get_current_time");
    let base_url = spawn_http_server(&server).await;
    let client = reqwest::Client::new();
    let golden = golden();

    let llm_request = json!({
        "provider": "stub",
        "model": "gpt-oss:120b-cloud",
        "session_id": "session-123",
        "messages_json": "[{\"role\":\"user\",\"content\":\"hi\"}]",
        "force_json": false,
        "temperature": 0.7,
        "max_tokens": 512,
        "schema_name": null,
        "metadata_json": null,
    });
    let response = client
        .post(format!("{base_url}/antikythera/v1/llm/call"))
        .json(&llm_request)
        .send()
        .await
        .expect("POST /llm/call");
    assert!(
        response.status().is_success(),
        "llm/call status: {}",
        response.status()
    );
    let body: Value = response.json().await.expect("llm/call body");
    assert_live_fields_exact_golden("llm_call_response", &body, &golden["llm_call_response"]);

    let execute_request = json!({
        "tool-name": "get_current_time",
        "arguments-json": "{}",
        "session-id": "session-123",
        "step-id": 1,
    });
    let response = client
        .post(format!("{base_url}/antikythera/v1/tools/execute"))
        .json(&execute_request)
        .send()
        .await
        .expect("POST /tools/execute");
    assert!(
        response.status().is_success(),
        "tools/execute status: {}",
        response.status()
    );
    let body: Value = response.json().await.expect("tools/execute body");
    assert_live_fields_exact_golden(
        "tool_execute_response",
        &body,
        &golden["tool_execute_response"],
    );
    assert_eq!(
        body["output-json"],
        "{\"datetime\":\"2026-08-12T00:00:00Z\"}"
    );

    let response = client
        .get(format!("{base_url}/antikythera/v1/tools"))
        .send()
        .await
        .expect("GET /tools");
    assert!(response.status().is_success(), "GET /tools status");
    let tools: Value = response.json().await.expect("tools list body");
    let tools = tools.as_array().expect("tools list is an array");
    assert!(
        !tools.is_empty(),
        "peer tools list must expose server tools"
    );
    let golden_tool = &golden["tools_list_response"][0];
    for tool in tools {
        assert_live_fields_within_golden("tool_definition", tool, golden_tool);
        assert!(
            tool.get("name").is_some(),
            "tool definition missing name: {tool}"
        );
        assert!(
            tool.get("description").is_some(),
            "tool definition missing description: {tool}"
        );
    }
}
