//! Integration smoke: load the composite `dist/antikythera-sdk.wasm` on a
//! core thread, run init â†’ prepare â†’ commit with a stub LLM (no external
//! provider) and assert the committed action is `final`.
//!
//! Also covers the runtime-hooks round trip: with `runtime_hooks_enabled`
//! and an SSE-side fake client that answers `hook-request` POST-backs with
//! `{"passthrough": true}`, prepare succeeds without an external peer, and
//! the default-deny policy fails closed when no hook is allowed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use antikythera_server_runtime::RuntimeServer;
use antikythera_server_runtime::config::{HookName, LlmProviderSpec, ServerRuntimeConfig};
use antikythera_server_runtime::llm::{LlmError, LlmProvider};
use antikythera_server_runtime::loop_owner::{ToolLoopConfig, run_tool_loop};
use antikythera_server_runtime::registry::Destination;
use antikythera_server_runtime::wire::{LlmRequest, LlmResponse, PostbackBody, ToolDefinition};
use serde_json::{Value, json};

fn component_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/antikythera-sdk.wasm")
}

fn stub_config() -> ServerRuntimeConfig {
    ServerRuntimeConfig {
        component_path: component_path(),
        providers: HashMap::from([(
            "stub".to_string(),
            LlmProviderSpec::Stub {
                response: "{\"action\":\"final\",\"content\":\"smoke-complete\"}".to_string(),
            },
        )]),
        default_provider: "stub".to_string(),
        ..ServerRuntimeConfig::default()
    }
}

/// A stub LLM that answers call #0 with `call_tool` and every later call
/// with `final`, proving the loop executes a host-routed tool and feeds the
/// result back through `process-tool-result-for-session`.
struct TwoStepStub {
    calls: AtomicU32,
}

#[async_trait::async_trait]
impl LlmProvider for TwoStepStub {
    fn name(&self) -> &str {
        "two-step"
    }

    async fn call(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let content = if call == 0 {
            json!({"action": "call_tool", "tool": "echo_local", "input": {"hi": 1}}).to_string()
        } else {
            json!({"action": "final", "content": "after-tool"}).to_string()
        };
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_composite_loads_and_reaches_final_with_stub_llm() {
    if !component_path().exists() {
        panic!(
            "Required artifact missing: composite {} not built; run `task build` first",
            component_path().display()
        );
    }
    let server = RuntimeServer::new(stub_config(), tokio::runtime::Handle::current())
        .expect("build server runtime");

    let handle = server.with_core(|core| {
        let config_json = json!({
            "session_id": "smoke-final",
            "max_steps": 5,
            "auto_execute_tools": false,
            "runtime_hooks_enabled": false,
        })
        .to_string();
        let session_id = core.init(&config_json)?;
        let request_json = json!({
            "prompt": "smoke test",
            "session_id": session_id,
            "correlation_id": "smoke-1",
        })
        .to_string();
        let prepared = core.prepare_user_turn(&request_json)?;
        let llm_response = json!({
            "action": "final",
            "content": "smoke-complete",
        })
        .to_string();
        let commit = core.commit_llm_response(&prepared, &llm_response)?;
        let commit: Value = serde_json::from_str(&commit).expect("commit is JSON");
        assert_eq!(commit["action"], "final", "commit: {commit}");
        assert_eq!(commit["content"], "smoke-complete");
        let _ = core.drain_events(&session_id)?;
        Ok(session_id)
    });

    let session_id = handle
        .join()
        .expect("core thread panicked")
        .expect("core flow failed");
    assert!(session_id.starts_with("smoke-final"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_tool_loop_reaches_final_with_stub_llm() {
    if !component_path().exists() {
        panic!(
            "Required artifact missing: composite {} not built; run `task build` first",
            component_path().display()
        );
    }
    let server = RuntimeServer::new(stub_config(), tokio::runtime::Handle::current())
        .expect("build server runtime");

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "smoke-loop".to_string(),
        prompts: vec!["smoke test".to_string()],
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| run_tool_loop(core, &shared, loop_config));
    let outcome = handle
        .join()
        .expect("core thread panicked")
        .expect("tool loop failed");
    assert_eq!(outcome.action, "final");
    assert_eq!(outcome.content.as_deref(), Some("smoke-complete"));
    assert!(outcome.steps >= 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
/// Fake peer: register as the SSE client, then answer every hook-request with `{"passthrough":
/// true}` via the POST-back mechanism.
///
/// Prepare-user-turn consults runtime-hooks.prepare-turn; the fake client answers passthrough so
/// the SDK default survives.
///
async fn runtime_hook_decision_roundtrips_via_control_channel() {
    if !component_path().exists() {
        panic!(
            "Required artifact missing: composite {} not built; run `task build` first",
            component_path().display()
        );
    }
    let mut config = stub_config();
    config.policy.allow_hook(HookName::PrepareTurn);
    config.policy.allow_hook(HookName::DecideAction);
    let server = RuntimeServer::new(config, tokio::runtime::Handle::current())
        .expect("build server runtime");

    let client_id = server.client_id().to_string();
    let control = server.control();
    let mut rx = control.register_client(&client_id);
    let responder_control = control.clone();
    let responder = tokio::spawn(async move {
        while let Ok(json) = rx.recv().await {
            let value: Value = serde_json::from_str(&json).unwrap();
            if value["type"] == "hook-request" {
                let correlation_id = value["correlation_id"].as_str().unwrap().to_string();
                responder_control.complete_postback(PostbackBody {
                    correlation_id,
                    ok: true,
                    payload: json!({"passthrough": true}),
                    error: None,
                });
            }
        }
    });

    let handle = server.with_core(|core| {
        let config_json = json!({
            "session_id": "hook-roundtrip",
            "max_steps": 5,
            "auto_execute_tools": false,
            "runtime_hooks_enabled": true,
        })
        .to_string();
        let session_id = core.init(&config_json)?;
        let request_json = json!({
            "prompt": "hook probe",
            "session_id": session_id,
            "correlation_id": "hook-1",
        })
        .to_string();
        let prepared = core.prepare_user_turn(&request_json)?;
        let prepared: Value = serde_json::from_str(&prepared).expect("prepared is JSON");
        assert_eq!(prepared["prompt"], "hook probe");
        Ok(session_id)
    });

    let session_id = handle
        .join()
        .expect("core thread panicked")
        .expect("hook roundtrip failed");
    assert_eq!(session_id, "hook-roundtrip");
    responder.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
/// Default-deny policy: no hooks allowed.
///
/// The runner wraps the gate denial in its fail-closed envelope; the `permission:` marker is
/// preserved inside the surfaced error.
///
async fn runtime_hook_fails_closed_when_hook_denied_by_policy() {
    if !component_path().exists() {
        panic!(
            "Required artifact missing: composite {} not built; run `task build` first",
            component_path().display()
        );
    }
    let server = RuntimeServer::new(stub_config(), tokio::runtime::Handle::current())
        .expect("build server runtime");

    let handle = server.with_core(|core| {
        let config_json = json!({
            "session_id": "hook-denied",
            "max_steps": 5,
            "auto_execute_tools": false,
            "runtime_hooks_enabled": true,
        })
        .to_string();
        core.init(&config_json)?;
        let request_json = json!({
            "prompt": "deny probe",
            "session_id": "hook-denied",
            "correlation_id": "deny-1",
        })
        .to_string();
        let err = core.prepare_user_turn(&request_json).unwrap_err();
        assert!(
            err.contains("permission:") && err.contains("hook 'prepare-turn'"),
            "expected permission denial, got: {err}"
        );
        Ok(())
    });

    handle
        .join()
        .expect("core thread panicked")
        .expect("denial flow");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_tool_executes_through_the_loop() {
    if !component_path().exists() {
        panic!(
            "Required artifact missing: composite {} not built; run `task build` first",
            component_path().display()
        );
    }
    let mut config = stub_config();
    config.policy.allow_tool(Destination::Local, "echo_local");
    let server = RuntimeServer::new_with_providers(
        config,
        HashMap::from([(
            "two-step".to_string(),
            Arc::new(TwoStepStub {
                calls: AtomicU32::new(0),
            }) as Arc<dyn LlmProvider>,
        )]),
        tokio::runtime::Handle::current(),
    )
    .expect("build server runtime");

    let router = server.router();
    router
        .register_local_tool(
            ToolDefinition::simple("echo_local", "server-side echo"),
            Arc::new(|args| Ok(json!({"echoed": args}))),
        )
        .expect("register local tool");

    let shared = server.shared.clone();
    let loop_config = ToolLoopConfig {
        session_id: "local-tool-loop".to_string(),
        prompts: vec!["use echo_local".to_string()],
        provider: "two-step".to_string(),
        model: "stub".to_string(),
        ..ToolLoopConfig::default()
    };
    let handle = server.with_core(move |core| run_tool_loop(core, &shared, loop_config));
    let outcome = handle
        .join()
        .expect("core thread panicked")
        .expect("local tool loop failed");
    assert_eq!(outcome.action, "final");
    assert_eq!(outcome.content.as_deref(), Some("after-tool"));
}
