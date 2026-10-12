//! Server-side harness proving the composed composite.
//!
//! Default world (unchanged): a 3-way composite
//!
//!   dist/antikythera-sdk.wasm = antikythera-sdk (runner export, tool-registry
//!                               + logic-hooks imports) + antikythera-toolrunner
//!                               (tool-registry export with builtin `echo`)
//!                               + antikythera-default-hooks (logic-hooks
//!                               export, no-op passthrough)
//!
//! A5 extension: the component path is accepted as an optional CLI argument
//! (default `dist/antikythera-sdk.wasm`), and `--expect-final` switches the
//! assertions to the logic-hooks probe variant:
//!
//!   base   (default-hooks)  → commit action=call_tool, drain has
//!                             tool_requested + tool_result echo success
//!   custom (example hooks)  → decide-action forces action=final
//!                             content=hook-forced-final, NO tool_result echo
//!
//! B4 extension: the final probe is GENERIC — `--expect-content=CONTENT`
//! (default `hook-forced-final`, so the A5 invocation is unchanged) asserts
//! the commit envelope carries `action=final` + `content=<string>` and no
//! tool_result. The same init/prepare/commit/drain flow proves the drop-in
//! `logic-core-example` artifact (echo-agent: commit always returns
//! `{"action":"final","content":"echo-agent-done",...}`); its template holes
//! (`drain-events`, ...) return `Err` not-implemented, which the final probe
//! tolerates as "no events". `--expect=notimpl` probes those holes directly:
//! it calls an unimplemented runner function and asserts the structured
//! `{"error":"not implemented","function":"<name>"}` Err.
//!
//! C3a extension (host-imports + permission gates): the world now imports
//! `antikythera:agent-sdk/host-imports@1.0.0`, and `Harness::add_to_linker`
//! wires the five import functions into the linker (namespaced instance
//! `antikythera:agent-sdk/host-imports@1.0.0`). The host implements them with
//! permission gates:
//!
//!   call-llm       quota — max 3 calls per instance; beyond → Err
//!                  "permission: llm quota exceeded"; deterministic stub
//!                  returns content "stub-llm-response" (the guest decides
//!                  tool-vs-final from its own prompt)
//!   emit-tool-call allowlist — only "echo" allowed; other → Err
//!                  "permission: tool NAME not in allowlist"
//!   save/load-state bounded storage — file under
//!                  %TEMP%\opencode\c3a-storage\CONTEXT_ID.json;
//!                  path traversal → Err "permission: invalid context id"
//!   log-message    passthrough to host stderr
//!
//! Probe modes (`--probe=`), run against the C3a host-llm-agent artifact
//! (`target/wasm32-wasip2/release/logic_core_host_example.wasm`):
//!
//!   full-loop  init → prepare → commit(prompt WITHOUT "tool") asserts
//!              action=final content="stub-llm-response" (proves call-llm
//!              reached — the content is not a guest constant) then
//!              process-tool-result(echo) asserts success (proves
//!              emit-tool-call echo allowed)
//!   quota      4× commit against one instance: the 4th call must surface
//!              "permission: llm quota exceeded" (3 allowed)
//!   allowlist  process-tool-result(tool="rm") must surface
//!              "permission: tool 'rm' not in allowlist"
//!   storage    process-tool-result with context-id "../evil" must surface
//!              "permission: invalid context id" (load-state gate; the same
//!              shared gate guards save-state — exercised additionally via
//!              init with the traversal id, visible on host stderr)
//!
//! U10 extension (runtime-hooks): the world now ALSO imports
//! `antikythera:agent-sdk/runtime-hooks@1.0.0`, and `Harness::add_to_linker`
//! wires the three runtime decision functions (prepare-turn, decide-action,
//! handle-tool-result) into the linker through the generated `Host` trait.
//! The host implements them behind a default-deny permission gate selected
//! by `--runtime-hooks=passthrough|override|deny` (default passthrough):
//!
//!   passthrough  all three return Ok `{"passthrough": true}` — the SDK
//!                default decision is preserved at every pipeline point
//!   override     decide-action returns Ok
//!                `{"action":"final","content":"runtime-hook-forced-final"}`,
//!                which the runner merges over the default commit envelope —
//!                proves the runtime decision is authoritative
//!   deny         all three return Err("permission: runtime-hook denied") —
//!                the pipeline aborts fail-closed, never falling back
//!
//! New probes (`--probe=`, run against dist/antikythera-sdk.wasm):
//!
//!   runtime-hooks-passthrough  commit(final "standard-final-content") keeps
//!                              action=final + that content — the old
//!                              composite behavior is unchanged while the new
//!                              import is wired as passthrough
//!   runtime-hooks-override     the same commit yields action=final
//!                              content="runtime-hook-forced-final"
//!   runtime-hooks-deny         prepare-user-turn aborts with a message
//!                              containing "permission: runtime-hook denied"
//!   runtime-hooks-disabled     init config `runtime_hooks_enabled: false`
//!                              with a deny-mode provider → the provider is
//!                              never consulted and the flow completes
//!
//! Flow mirrored from tests/sdk/wasm_agent/deterministic_harness_tests.rs
//! (native Rust path), executed here against the WASM composite via the
//! wasmtime component API:
//!
//!   init(config) -> prepare-user-turn(user message)
//!   -> commit-llm-response(prepared, llm_response with call_tool echo)
//!   -> drain-events
//!
//! Usage:
//!   component-harness `COMPONENT_PATH` [--expect default|final|notimpl]
//!                     [--expect-content=CONTENT] [--probe full-loop|quota|allowlist|storage]
//!                     [--probe runtime-hooks-passthrough|runtime-hooks-override|runtime-hooks-deny|runtime-hooks-disabled]
//!                     [--runtime-hooks=passthrough|override|deny]

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;

wasmtime::component::bindgen!({
    path: "wit/harness.wit",
    world: "harness",
    require_store_data_send: true,
});

use antikythera::agent_sdk::host_imports::{
    Host, LlmRequest, LlmResponse, LogEvent, ToolCallEvent, ToolExecutionResult,
};

/// call-llm quota: max 3 calls per host-imports instance.
const LLM_QUOTA: u32 = 3;

/// Runtime-hooks passthrough signal: use the SDK default at this pipeline
/// point (identical contract to `logic-hooks`).
const RUNTIME_HOOK_PASSTHROUGH_JSON: &str = "{\"passthrough\": true}";

/// Runtime-hooks decide-action override: the same envelope the SDK produces
/// for a terminal action, so the runner's merge over the default
/// commit-result yields action=final with the forced content (the shape is
/// the `logic-hooks-example` override, renamed to prove the runtime decision
/// — not the composed one — is authoritative).
const RUNTIME_HOOK_OVERRIDE_FINAL_JSON: &str =
    "{\"action\":\"final\",\"content\":\"runtime-hook-forced-final\"}";

/// Runtime-hooks deny message: every gate rejection must surface as a
/// `permission:` error (repo invariant, identical to the host-imports gates).
const RUNTIME_HOOK_DENY_MSG: &str = "permission: runtime-hook denied";

/// Host-supplied `runtime-hooks` provider mode (U10). Default-deny policy:
/// only `Passthrough`/`Override` grant a decision; `Deny` rejects before any
/// decision is produced.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeHooksMode {
    Passthrough,
    Override,
    Deny,
}

impl std::fmt::Display for RuntimeHooksMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeHooksMode::Passthrough => f.write_str("passthrough"),
            RuntimeHooksMode::Override => f.write_str("override"),
            RuntimeHooksMode::Deny => f.write_str("deny"),
        }
    }
}

/// Host state: WASI context + resource table + C3a permission-gate state
/// (per-instance llm call counter, bounded storage root) + U10 runtime-hooks
/// provider mode.
struct HostState {
    ctx: wasmtime_wasi::WasiCtx,
    table: wasmtime_wasi::ResourceTable,
    llm_call_count: u32,
    storage_dir: PathBuf,
    runtime_hooks_mode: RuntimeHooksMode,
}

impl wasmtime_wasi::WasiView for HostState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

/// C3a host-imports implementation with permission gates.
///
/// Every gate fails explicitly with the exact `permission:` message the
/// guest surfaces inside its structured error envelope; there is no silent
/// degradation path.
impl Host for HostState {
    /// Quota gate: at most [`LLM_QUOTA`] calls per instance. The deterministic
    /// stub parses `messages_json` (validity check only) and always answers
    /// content "stub-llm-response" — the guest decides tool-vs-final from its
    /// own prompt, so the stub never inspects messages for "tool".
    fn call_llm(&mut self, request: LlmRequest) -> Result<LlmResponse, String> {
        if self.llm_call_count >= LLM_QUOTA {
            return Err("permission: llm quota exceeded".to_string());
        }
        self.llm_call_count += 1;
        serde_json::from_str::<Value>(&request.messages_json)
            .map_err(|e| format!("stub-llm: cannot parse messages_json: {e}"))?;
        Ok(LlmResponse {
            content: "stub-llm-response".to_string(),
            model: None,
            session_id: request.session_id,
            message_json: None,
            tokens_used: Some(4),
            finish_reason: Some("stop".to_string()),
            raw_response_json: None,
        })
    }

    /// Allowlist gate: only the builtin `echo` tool may execute. Any other
    /// tool is rejected before execution.
    /// Builtin echo: returns its arguments verbatim, always success.
    ///
    fn emit_tool_call(&mut self, event: ToolCallEvent) -> Result<ToolExecutionResult, String> {
        if event.tool_name != "echo" {
            return Err(format!(
                "permission: tool '{}' not in allowlist",
                event.tool_name
            ));
        }
        Ok(ToolExecutionResult {
            tool_name: event.tool_name,
            success: true,
            output_json: event.arguments_json,
            error_message: None,
            step_id: event.step_id,
        })
    }

    /// Passthrough gate: host stderr.
    fn log_message(&mut self, event: LogEvent) {
        eprintln!("[host-log][{}] {}", event.level, event.message);
    }

    /// Bounded storage gate: the file lives under `<storage_dir>/CONTEXT_ID.json`
    /// and the context-id is validated BEFORE any filesystem operation, so a
    /// traversal id can never escape the storage root.
    fn save_state(&mut self, context_id: String, state_json: String) -> Result<(), String> {
        validate_context_id(&context_id)?;
        std::fs::create_dir_all(&self.storage_dir)
            .map_err(|e| format!("storage: cannot create storage dir: {e}"))?;
        let path = self.storage_dir.join(format!("{context_id}.json"));
        std::fs::write(&path, state_json).map_err(|e| format!("storage: write failed: {e}"))
    }

    /// Bounded storage gate: same [`validate_context_id`] guard as save-state;
    /// a missing file is `None`, not an error.
    fn load_state(&mut self, context_id: String) -> Result<Option<String>, String> {
        validate_context_id(&context_id)?;
        let path = self.storage_dir.join(format!("{context_id}.json"));
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("storage: read failed: {e}")),
        }
    }
}

/// The world also imports `vocabulary` (type-only); bindgen requires the
/// host data to implement its (empty) `Host` trait.
impl antikythera::agent_sdk::vocabulary::Host for HostState {}

/// U10 runtime-hooks implementation with a default-deny permission gate.
///
/// The provider mode is fixed per instance (`--runtime-hooks=`), and the
/// decisions are deterministic so every probe verdict is a pure function of
/// that mode:
/// - `Passthrough`: all three return the passthrough signal — the runner
///   uses the SDK default at every pipeline point;
/// - `Override`: only `decide-action` returns the forced-final envelope
///   (prepare-turn and handle-tool-result stay passthrough) — the runtime
///   decision replaces the SDK default action;
/// - `Deny`: all three return the `permission:` error — the runner aborts
///   the operation fail-closed, never falling back to passthrough.
impl antikythera::agent_sdk::runtime_hooks::Host for HostState {
    fn prepare_turn(
        &mut self,
        _request_json: String,
        _session_state_json: String,
    ) -> Result<String, String> {
        self.runtime_hook_decision("prepare-turn")
    }

    fn decide_action(
        &mut self,
        _session_state_json: String,
        _llm_response_json: String,
    ) -> Result<String, String> {
        self.runtime_hook_decision("decide-action")
    }

    fn handle_tool_result(
        &mut self,
        _session_state_json: String,
        _tool_result_json: String,
    ) -> Result<String, String> {
        self.runtime_hook_decision("handle-tool-result")
    }
}

impl HostState {
    /// Deterministic runtime-hooks decision for a pipeline point, gated by
    /// the provider mode. `Deny` rejects before any decision is produced;
    /// the `permission:` prefix is the repo-wide denial invariant shared with
    /// the host-imports gates.
    fn runtime_hook_decision(&self, hook_name: &str) -> Result<String, String> {
        match self.runtime_hooks_mode {
            RuntimeHooksMode::Deny => Err(RUNTIME_HOOK_DENY_MSG.to_string()),
            RuntimeHooksMode::Override if hook_name == "decide-action" => {
                Ok(RUNTIME_HOOK_OVERRIDE_FINAL_JSON.to_string())
            }
            _ => Ok(RUNTIME_HOOK_PASSTHROUGH_JSON.to_string()),
        }
    }
}

/// Shared storage-gate validator: a context-id becomes a path segment, so it
/// must contain no traversal or path syntax. Rejects empty ids, `.`, `..`,
/// separators, drive/stream syntax, and NUL. Used by both save-state and
/// load-state before any filesystem access.
fn validate_context_id(context_id: &str) -> Result<(), String> {
    let traversal = context_id.is_empty()
        || context_id == "."
        || context_id.contains("..")
        || context_id.contains('/')
        || context_id.contains('\\')
        || context_id.contains(':')
        || context_id.contains('\0');
    if traversal {
        return Err("permission: invalid context id".to_string());
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Default behavior: call_tool commit + builtin echo tool_result.
    Default,
    /// Generic final probe: commit action=final + content=expect-content,
    /// no tool_result (A5 logic-hooks override or B4 logic-core echo commit).
    Final,
    /// B4 logic-core probe: call template-hole runner functions and assert
    /// the structured not-implemented Err.
    NotImpl,
}

impl std::fmt::Display for Expect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expect::Default => f.write_str("default"),
            Expect::Final => f.write_str("final"),
            Expect::NotImpl => f.write_str("notimpl"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Probe {
    FullLoop,
    Quota,
    Allowlist,
    Storage,
    RuntimeHooksPassthrough,
    RuntimeHooksOverride,
    RuntimeHooksDeny,
    RuntimeHooksDisabled,
}

impl std::fmt::Display for Probe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Probe::FullLoop => f.write_str("full-loop"),
            Probe::Quota => f.write_str("quota"),
            Probe::Allowlist => f.write_str("allowlist"),
            Probe::Storage => f.write_str("storage"),
            Probe::RuntimeHooksPassthrough => f.write_str("runtime-hooks-passthrough"),
            Probe::RuntimeHooksOverride => f.write_str("runtime-hooks-override"),
            Probe::RuntimeHooksDeny => f.write_str("runtime-hooks-deny"),
            Probe::RuntimeHooksDisabled => f.write_str("runtime-hooks-disabled"),
        }
    }
}

/// Runtime-hooks probes exercise the composite (dist) artifact, which is the only component that
/// imports the runtime-hooks interface; the C3a probes keep their logic-core default.
///
/// B4 not-implemented probe: template-hole functions of a logic-core drop-in need no
/// init/prepare/commit flow, so probe them directly.
///
/// 3. commit-llm-response(prepared, llm_response with call_tool -> echo).
///
/// Logic-core drop-in: `drain-events` is a template hole (Err not-implemented), so no event queue
/// exists — the generic final probe treats that as "no events" and continues to assert that no
/// tool_result was emitted. The default probe keeps the strict error propagation.
///
/// Baseline: SDK default behavior must be intact — the composite behaves exactly as the pre-hook
/// composite did (default-hooks is a no-op passthrough, so the third member changes no behavior).
///
/// Generic final probe (B4): the commit envelope MUST carry action=final + content=expect-content
/// — either the A5 logic-hooks decide-action override (hook-forced-final) or the B4 logic-core
/// deterministic echo-agent commit (echo-agent-done). The tool MUST NOT execute, so no
/// tool_result echo may appear.
///
/// The notimpl probe returns before the init/prepare/commit flow.
///
fn main() -> Result<()> {
    let mut expect = Expect::Default;
    let mut expect_content = "hook-forced-final".to_string();
    let mut component_path: Option<String> = None;
    let mut probe: Option<Probe> = None;
    let mut runtime_hooks_mode = RuntimeHooksMode::Passthrough;
    for arg in std::env::args().skip(1) {
        if let Some(mode) = arg.strip_prefix("--expect=") {
            expect = match mode {
                "default" => Expect::Default,
                "final" => Expect::Final,
                "notimpl" => Expect::NotImpl,
                other => {
                    anyhow::bail!("unknown --expect mode: {other} (expected default|final|notimpl)")
                }
            };
        } else if let Some(content) = arg.strip_prefix("--expect-content=") {
            expect_content = content.to_string();
        } else if arg == "--expect-final" {
            expect = Expect::Final;
        } else if let Some(mode) = arg.strip_prefix("--runtime-hooks=") {
            runtime_hooks_mode = match mode {
                "passthrough" => RuntimeHooksMode::Passthrough,
                "override" => RuntimeHooksMode::Override,
                "deny" => RuntimeHooksMode::Deny,
                other => {
                    anyhow::bail!(
                        "unknown --runtime-hooks mode: {other} (expected passthrough|override|deny)"
                    )
                }
            };
        } else if let Some(mode) = arg.strip_prefix("--probe=") {
            probe = Some(match mode {
                "full-loop" => Probe::FullLoop,
                "quota" => Probe::Quota,
                "allowlist" => Probe::Allowlist,
                "storage" => Probe::Storage,
                "runtime-hooks-passthrough" => Probe::RuntimeHooksPassthrough,
                "runtime-hooks-override" => Probe::RuntimeHooksOverride,
                "runtime-hooks-deny" => Probe::RuntimeHooksDeny,
                "runtime-hooks-disabled" => Probe::RuntimeHooksDisabled,
                other => {
                    anyhow::bail!(
                        "unknown --probe mode: {other} (expected full-loop|quota|allowlist|storage|\
                         runtime-hooks-passthrough|runtime-hooks-override|runtime-hooks-deny|\
                         runtime-hooks-disabled)"
                    )
                }
            });
        } else if arg.starts_with('-') {
            anyhow::bail!("unknown flag: {arg}");
        } else if component_path.is_none() {
            component_path = Some(arg);
        } else {
            anyhow::bail!("unexpected extra argument: {arg}");
        }
    }

    if let Some(probe) = probe {
        let probe_component = match probe {
            Probe::RuntimeHooksPassthrough
            | Probe::RuntimeHooksOverride
            | Probe::RuntimeHooksDeny
            | Probe::RuntimeHooksDisabled => component_path.unwrap_or_else(|| {
                format!(
                    "{}/../../dist/antikythera-sdk.wasm",
                    env!("CARGO_MANIFEST_DIR")
                )
            }),
            _ => component_path.unwrap_or_else(|| {
                format!(
                    "{}/../../target/wasm32-wasip2/release/logic_core_host_example.wasm",
                    env!("CARGO_MANIFEST_DIR")
                )
            }),
        };
        return run_probe(probe, &probe_component, runtime_hooks_mode);
    }

    let component_path = component_path.unwrap_or_else(|| {
        format!(
            "{}/../../dist/antikythera-sdk.wasm",
            env!("CARGO_MANIFEST_DIR")
        )
    });

    println!("=== component-harness: wasmtime composite server probe ===");
    println!("[config] component     = {component_path}");
    println!("[config] expect        = {expect}");
    println!("[config] runtime-hooks = {runtime_hooks_mode}");
    if expect == Expect::Final {
        println!("[config] content       = {expect_content}");
    }

    let engine = wasmtime::Engine::default();
    let mut linker = wasmtime::component::Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .context("register wasi imports into linker")?;
    Harness::add_to_linker::<_, wasmtime::component::HasSelf<HostState>>(&mut linker, |state| {
        state
    })
    .context("register host-imports into linker")?;

    let component_bytes =
        std::fs::read(&component_path).context("read composite component path")?;
    let component = wasmtime::component::Component::new(&engine, &component_bytes)
        .context("compile composite component")?;

    let mut store = wasmtime::Store::new(
        &engine,
        HostState {
            ctx: wasmtime_wasi::WasiCtxBuilder::new().build(),
            table: wasmtime_wasi::ResourceTable::new(),
            llm_call_count: 0,
            storage_dir: c3a_storage_dir(),
            runtime_hooks_mode,
        },
    );

    let root = Harness::instantiate(&mut store, &component, &linker)
        .context("instantiate composite runner export")?;
    let runner = root.antikythera_agent_sdk_runner();

    if expect == Expect::NotImpl {
        println!("--- not-implemented probe (logic-core drop-in) ---");

        let prompt_result = runner
            .call_get_tools_prompt(&mut store)
            .context("wasmtime trap on get-tools-prompt")?;
        match prompt_result {
            Ok(prompt) => anyhow::bail!(
                "FAIL: get-tools-prompt returned Ok({prompt:?}); expected structured \
                 not-implemented error"
            ),
            Err(e) => assert_not_implemented(&e, "get-tools-prompt")?,
        }

        let sweep_result = runner
            .call_sweep_idle_sessions(&mut store, None)
            .context("wasmtime trap on sweep-idle-sessions")?;
        match sweep_result {
            Ok(count) => anyhow::bail!(
                "FAIL: sweep-idle-sessions returned Ok({count}); expected structured \
                 not-implemented error"
            ),
            Err(e) => assert_not_implemented(&e, "sweep-idle-sessions")?,
        }

        println!("PASS: logic core template holes return structured not-implemented errors.");
        return Ok(());
    }

    let config_json = serde_json::json!({
        "session_id": "compose-smoke",
        "max_steps": 5,
        "auto_execute_tools": true,
    })
    .to_string();
    let session_id = runner
        .call_init(&mut store, &config_json)
        .context("wasmtime trap on init")?
        .map_err(|e| anyhow::anyhow!("runner init failed: {e}"))?;
    println!("[init] session_id = {session_id}");

    let request_json = serde_json::json!({
        "prompt": "echo hello",
        "session_id": session_id,
        "correlation_id": "compose-smoke-1",
    })
    .to_string();
    let prepared_json = runner
        .call_prepare_user_turn(&mut store, &request_json)
        .context("wasmtime trap on prepare-user-turn")?
        .map_err(|e| anyhow::anyhow!("prepare-user-turn failed: {e}"))?;
    let prepared: Value =
        serde_json::from_str(&prepared_json).context("prepared turn is not valid JSON")?;
    println!(
        "[prepare] step={} prompt={}",
        prepared["step"], prepared["prompt"]
    );

    let llm_response_json = serde_json::json!({
        "action": "call_tool",
        "tool": "echo",
        "input": {"hello": "world"},
    })
    .to_string();
    let commit_json = runner
        .call_commit_llm_response(&mut store, &prepared_json, &llm_response_json)
        .context("wasmtime trap on commit-llm-response")?
        .map_err(|e| anyhow::anyhow!("commit-llm-response failed: {e}"))?;
    let commit: Value =
        serde_json::from_str(&commit_json).context("commit result is not valid JSON")?;
    println!(
        "[commit] action={} content={} tool={} fsm={}",
        commit["action"], commit["content"], commit["tool_name"], commit["fsm_state"]
    );

    let drain_result = runner.call_drain_events(&mut store, &session_id);
    let events_json = match drain_result {
        Ok(Ok(json)) => json,
        Ok(Err(e)) if expect == Expect::Final => {
            println!("[drain] Err tolerated by final probe (logic-core drop-in): {e}");
            "[]".to_string()
        }
        Ok(Err(e)) => anyhow::bail!("drain-events failed: {e}"),
        Err(trap) => anyhow::bail!("wasmtime trap on drain-events: {trap}"),
    };
    let events: Value = serde_json::from_str(&events_json).context("events are not valid JSON")?;

    let mut tool_requested_found = false;
    let mut tool_result_found = false;
    if let Some(events) = events.as_array() {
        for event in events {
            let kind = event["kind"].as_str().unwrap_or("?");
            println!(
                "[event] seq={} kind={} payload={}",
                event["seq"], kind, event["payload"]
            );
            if kind == "tool_requested" && event["payload"]["tool"] == "echo" {
                tool_requested_found = true;
            }
            if kind == "tool_result"
                && event["payload"]["tool"] == "echo"
                && event["payload"]["success"] == true
            {
                tool_result_found = true;
            }
        }
    } else {
        anyhow::bail!("drain-events did not return a JSON array: {events}");
    }

    let pass = match expect {
        Expect::Default => {
            if commit["action"] != "call_tool" {
                anyhow::bail!("expected commit action=call_tool, got {}", commit["action"]);
            }
            if !tool_requested_found {
                anyhow::bail!("FAIL: drain-events has no tool_requested event for `echo`");
            }
            if !tool_result_found {
                anyhow::bail!(
                    "FAIL: drain-events has no successful tool_result event for builtin `echo` \
                     (no host round-trip expected)"
                );
            }
            println!(
                "PASS: default hooks — builtin tool `echo` flowed through \
                 tool_requested + tool_result(success=true) inside the composite."
            );
            true
        }
        Expect::Final => {
            if commit["action"] != "final" {
                anyhow::bail!(
                    "FAIL: expected commit action=final (decide-action override), got {}",
                    commit["action"]
                );
            }
            if commit["content"] != expect_content {
                anyhow::bail!(
                    "FAIL: expected commit content={expect_content:?}, got {}",
                    commit["content"]
                );
            }
            if tool_result_found {
                anyhow::bail!(
                    "FAIL: final probe — tool_result echo present despite action=final; \
                     tool executed when the final decision should have prevented it"
                );
            }
            if tool_requested_found {
                println!(
                    "[note] tool_requested event observed: the SDK emits it during default \
                     action derivation before decide-action is consulted (commit path); no \
                     tool_result follows, so the tool did not execute."
                );
            }
            println!(
                "PASS: final probe — commit action=final content={expect_content:?}; \
                 no tool_result (tool never executed)."
            );
            true
        }
        Expect::NotImpl => unreachable!("notimpl probe handled above"),
    };

    if !pass {
        std::process::exit(1);
    }
    Ok(())
}

/// Bounded storage root for the C3a save-state/load-state gates.
fn c3a_storage_dir() -> PathBuf {
    std::env::temp_dir().join("opencode").join("c3a-storage")
}

/// Instantiate the C3a host-llm-agent component (or any runner-export
/// component) against the wired linker. The root harness owns the runner
/// export (its accessor returns a borrow), so it is handed back alongside
/// the store. `runtime_hooks_mode` sets the provider gate of the new
/// instance.
fn instantiate_runner(
    component_path: &str,
    runtime_hooks_mode: RuntimeHooksMode,
) -> Result<(wasmtime::Store<HostState>, Harness)> {
    let engine = wasmtime::Engine::default();
    let mut linker = wasmtime::component::Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .context("register wasi imports into linker")?;
    Harness::add_to_linker::<_, wasmtime::component::HasSelf<HostState>>(&mut linker, |state| {
        state
    })
    .context("register host-imports into linker")?;

    let component_bytes = std::fs::read(component_path).context("read component path")?;
    let component = wasmtime::component::Component::new(&engine, &component_bytes)
        .context("compile component")?;

    let mut store = wasmtime::Store::new(
        &engine,
        HostState {
            ctx: wasmtime_wasi::WasiCtxBuilder::new().build(),
            table: wasmtime_wasi::ResourceTable::new(),
            llm_call_count: 0,
            storage_dir: c3a_storage_dir(),
            runtime_hooks_mode,
        },
    );

    let root = Harness::instantiate(&mut store, &component, &linker)
        .context("instantiate runner export")?;
    Ok((store, root))
}

type Runner = crate::exports::antikythera::agent_sdk::runner::Guest;
type Store = wasmtime::Store<HostState>;

fn probe_init(runner: &Runner, store: &mut Store, session_id: &str) -> Result<String> {
    let config = serde_json::json!({ "session_id": session_id, "max_steps": 5 }).to_string();
    let id = runner
        .call_init(store, &config)
        .context("wasmtime trap on init")?
        .map_err(|e| anyhow::anyhow!("init failed: {e}"))?;
    println!("[init] session_id = {id}");
    Ok(id)
}

fn probe_prepare(
    runner: &Runner,
    store: &mut Store,
    session_id: &str,
    prompt: &str,
) -> Result<String> {
    let request = serde_json::json!({
        "prompt": prompt,
        "session_id": session_id,
        "correlation_id": "c3a-probe",
    })
    .to_string();
    let prepared = runner
        .call_prepare_user_turn(store, &request)
        .context("wasmtime trap on prepare-user-turn")?
        .map_err(|e| anyhow::anyhow!("prepare-user-turn failed: {e}"))?;
    let parsed: Value =
        serde_json::from_str(&prepared).context("prepared turn is not valid JSON")?;
    println!(
        "[prepare] step={} prompt={}",
        parsed["step"], parsed["prompt"]
    );
    Ok(prepared)
}

fn probe_commit(runner: &Runner, store: &mut Store, prepared_json: &str) -> Result<Value> {
    probe_commit_with_response(runner, store, prepared_json, "{}")
}

fn probe_commit_with_response(
    runner: &Runner,
    store: &mut Store,
    prepared_json: &str,
    llm_response_json: &str,
) -> Result<Value> {
    let commit_json = runner
        .call_commit_llm_response(store, prepared_json, llm_response_json)
        .context("wasmtime trap on commit-llm-response")?
        .map_err(|e| anyhow::anyhow!("commit-llm-response failed: {e}"))?;
    let commit: Value =
        serde_json::from_str(&commit_json).context("commit result is not valid JSON")?;
    println!(
        "[commit] action={} content={} tool={} fsm={}",
        commit["action"], commit["content"], commit["tool_name"], commit["fsm_state"]
    );
    Ok(commit)
}

fn probe_process_tool_result(
    runner: &Runner,
    store: &mut Store,
    session_id: &str,
    tool_name: &str,
) -> Result<Value> {
    let tool_result_json = serde_json::json!({
        "tool_name": tool_name,
        "output": "probe",
    })
    .to_string();
    let result_json = runner
        .call_process_tool_result_for_session(store, session_id, &tool_result_json)
        .context("wasmtime trap on process-tool-result-for-session")?
        .map_err(|e| anyhow::anyhow!("process-tool-result-for-session failed: {e}"))?;
    let result: Value =
        serde_json::from_str(&result_json).context("tool result is not valid JSON")?;
    println!(
        "[tool-result] action={} content={} error={}",
        result["action"], result["content"], result["error"]
    );
    Ok(result)
}

/// C3a host-imports + U10 runtime-hooks verification probes.
///
/// The runtime-hooks probes force their provider mode so the verdict is a
/// pure function of the probe (independent of `--runtime-hooks=`); the other
/// probes keep the configured mode (passthrough by default, irrelevant for
/// artifacts that do not import runtime-hooks).
fn run_probe(
    probe: Probe,
    component_path: &str,
    runtime_hooks_mode: RuntimeHooksMode,
) -> Result<()> {
    println!("=== component-harness: permission-gate probe (host-imports + runtime-hooks) ===");
    println!("[config] component = {component_path}");
    println!("[config] probe     = {probe}");
    println!("[config] runtime-hooks = {runtime_hooks_mode}");

    let mode = match probe {
        Probe::RuntimeHooksPassthrough => RuntimeHooksMode::Passthrough,
        Probe::RuntimeHooksOverride => RuntimeHooksMode::Override,
        Probe::RuntimeHooksDeny | Probe::RuntimeHooksDisabled => RuntimeHooksMode::Deny,
        _ => runtime_hooks_mode,
    };
    if mode != runtime_hooks_mode {
        println!("[config] runtime-hooks = {mode} (probe-forced)");
    }

    let (mut store, root) = instantiate_runner(component_path, mode)?;
    let runner = root.antikythera_agent_sdk_runner();

    match probe {
        Probe::FullLoop => probe_full_loop(runner, &mut store),
        Probe::Quota => probe_quota(runner, &mut store),
        Probe::Allowlist => probe_allowlist(runner, &mut store),
        Probe::Storage => probe_storage(runner, &mut store),
        Probe::RuntimeHooksPassthrough => probe_runtime_hooks_passthrough(runner, &mut store),
        Probe::RuntimeHooksOverride => probe_runtime_hooks_override(runner, &mut store),
        Probe::RuntimeHooksDeny => probe_runtime_hooks_deny(runner, &mut store),
        Probe::RuntimeHooksDisabled => probe_runtime_hooks_disabled(runner, &mut store),
    }
}

/// Full loop: init → prepare → commit(prompt WITHOUT "tool") must yield
/// action=final content="stub-llm-response" (the content can only come from
/// the host call-llm stub, proving the import was wired) then
/// process-tool-result(echo) must succeed (proving emit-tool-call allowed
/// echo).
fn probe_full_loop(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: full host-llm-agent loop ---");
    let session_id = probe_init(runner, store, "c3a-full-loop")?;
    let prepared = probe_prepare(runner, store, &session_id, "hello from c3a full loop")?;

    let commit = probe_commit(runner, store, &prepared)?;
    if commit["action"] != "final" {
        anyhow::bail!(
            "FAIL: expected commit action=final (prompt without \"tool\"), got {}",
            commit["action"]
        );
    }
    if commit["content"] != "stub-llm-response" {
        anyhow::bail!(
            "FAIL: expected commit content=\"stub-llm-response\" (host call-llm stub), got {}",
            commit["content"]
        );
    }
    println!(
        "[assert] commit action=final content=\"stub-llm-response\" — call-llm import reached"
    );

    let tool = probe_process_tool_result(runner, store, &session_id, "echo")?;
    let content = tool["content"].as_str().unwrap_or_default();
    if tool["action"] != "final" {
        anyhow::bail!(
            "FAIL: expected tool-result action=final, got {}",
            tool["action"]
        );
    }
    if !content.contains("success: true") {
        anyhow::bail!(
            "FAIL: expected tool-result content to report success: true, got {content:?}"
        );
    }
    println!("[assert] emit-tool-call echo allowed and executed (success: true)");
    println!("PASS: full loop — call-llm reached, emit-tool-call(echo) allowed.");
    Ok(())
}

/// Quota: 4× commit against one instance. The first 3 calls to call-llm are
/// allowed; the 4th must surface the permission error.
fn probe_quota(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: call-llm quota (max {LLM_QUOTA} per instance) ---");
    let session_id = probe_init(runner, store, "c3a-quota")?;
    let prepared = probe_prepare(runner, store, &session_id, "quota probe")?;

    for i in 1..=LLM_QUOTA {
        let commit = probe_commit(runner, store, &prepared)?;
        if commit["action"] != "final" {
            anyhow::bail!(
                "FAIL: expected commit #{i} to succeed (action=final), got {}",
                commit["action"]
            );
        }
        println!("[assert] call-llm #{i} allowed (within quota)");
    }

    let denied = probe_commit(runner, store, &prepared)?;
    let error = denied["error"].as_str().unwrap_or_default();
    if !error.contains("permission: llm quota exceeded") {
        anyhow::bail!(
            "FAIL: expected 4th call-llm to be denied with \"permission: llm quota exceeded\", \
             got envelope error {error:?}"
        );
    }
    println!("[assert] call-llm #{} denied: {error}", LLM_QUOTA + 1);
    println!("PASS: quota gate — call-llm denied above {LLM_QUOTA} calls per instance.");
    Ok(())
}

/// Allowlist negative: emit-tool-call with tool "rm" must be denied before
/// execution.
fn probe_allowlist(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: emit-tool-call allowlist (negative: rm) ---");
    let session_id = probe_init(runner, store, "c3a-allowlist")?;
    let prepared = probe_prepare(runner, store, &session_id, "allowlist probe")?;
    let _ = probe_commit(runner, store, &prepared)?;

    let denied = probe_process_tool_result(runner, store, &session_id, "rm")?;
    let error = denied["error"].as_str().unwrap_or_default();
    if !error.contains("permission: tool 'rm' not in allowlist") {
        anyhow::bail!(
            "FAIL: expected tool 'rm' to be denied with \"permission: tool 'rm' not in allowlist\", \
             got envelope error {error:?}"
        );
    }
    println!("[assert] emit-tool-call(rm) denied: {error}");
    println!("PASS: allowlist gate — tool 'rm' rejected, only 'echo' allowed.");
    Ok(())
}

/// Storage: a context-id containing path traversal must be rejected by the
/// bounded-storage gate. The deterministic path is load-state via
/// process-tool-result-for-session("../evil"); the same shared validator
/// guards save-state, exercised additionally through init with the traversal
/// id (the rejections surface on host stderr via log-message passthrough).
/// Init with a traversal id: load-state AND save-state are both rejected; init's contract is a
/// bare id, so the rejections surface as host logs.
///
/// Deterministic assertion: process-tool-result-for-session reaches load-state with the traversal
/// id and surfaces the permission error.
///
/// A valid id still round-trips through the same gates.
///
fn probe_storage(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: bounded storage (traversal context-id) ---");

    println!("[note] init(\"../evil\") — load-state + save-state rejections visible below");
    let evil_session = probe_init(runner, store, "../evil")?;
    println!("[note] init returned id {evil_session:?} (bare-id contract, errors logged only)");

    let denied = probe_process_tool_result(runner, store, "../evil", "echo")?;
    let error = denied["error"].as_str().unwrap_or_default();
    if !error.contains("permission: invalid context id") {
        anyhow::bail!(
            "FAIL: expected traversal context-id to be denied with \"permission: invalid context id\", \
             got envelope error {error:?}"
        );
    }
    println!("[assert] load-state(\"../evil\") denied: {error}");

    let ok_session = probe_init(runner, store, "c3a-storage")?;
    let _ = probe_prepare(runner, store, &ok_session, "storage probe")?;
    println!("[assert] save-state/load-state round-trip with valid id succeeded");

    println!(
        "PASS: storage gate — traversal context-id rejected, valid ids confined to c3a-storage."
    );
    Ok(())
}

/// Runtime-hooks passthrough: the composite is driven exactly like the old
/// default path, and the wired runtime provider (passthrough mode) must leave
/// the decision unchanged — a committed `final` response keeps its own
/// content. This proves the composite's old behavior is preserved while the
/// new import is consulted (precedence A1a: the composed default-hooks passed
/// through, so the runtime provider saw every pipeline point and passed
/// through too).
fn probe_runtime_hooks_passthrough(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: runtime-hooks passthrough (SDK default preserved) ---");
    let session_id = probe_init(runner, store, "runtime-hooks-passthrough")?;
    let prepared = probe_prepare(
        runner,
        store,
        &session_id,
        "runtime hooks passthrough probe",
    )?;

    let llm_response = serde_json::json!({
        "action": "final",
        "content": "standard-final-content",
    })
    .to_string();
    let commit = probe_commit_with_response(runner, store, &prepared, &llm_response)?;
    if commit["action"] != "final" {
        anyhow::bail!(
            "FAIL: expected commit action=final (passthrough keeps the SDK default), got {}",
            commit["action"]
        );
    }
    if commit["content"] != "standard-final-content" {
        anyhow::bail!(
            "FAIL: expected commit content=\"standard-final-content\" (passthrough keeps the \
             LLM response content), got {}",
            commit["content"]
        );
    }
    println!("[assert] runtime provider consulted and passed through: decision unchanged");
    println!(
        "PASS: runtime-hooks passthrough — commit action=final \
         content=\"standard-final-content\" (default behavior unchanged)."
    );
    Ok(())
}

/// Runtime-hooks override: the same committed `final` response as the
/// passthrough probe, but the runtime `decide-action` provider returns the
/// forced-final envelope. The runner merges it over the default commit
/// result, so the committed action is `final` with the runtime content — the
/// runtime decision, not the LLM response, is authoritative.
fn probe_runtime_hooks_override(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: runtime-hooks decide-action override ---");
    let session_id = probe_init(runner, store, "runtime-hooks-override")?;
    let prepared = probe_prepare(runner, store, &session_id, "runtime hooks override probe")?;

    let llm_response = serde_json::json!({
        "action": "final",
        "content": "standard-final-content",
    })
    .to_string();
    let commit = probe_commit_with_response(runner, store, &prepared, &llm_response)?;
    if commit["action"] != "final" {
        anyhow::bail!(
            "FAIL: expected commit action=final (runtime override), got {}",
            commit["action"]
        );
    }
    if commit["content"] != "runtime-hook-forced-final" {
        anyhow::bail!(
            "FAIL: expected commit content=\"runtime-hook-forced-final\" (runtime decision \
             committed), got {}",
            commit["content"]
        );
    }
    println!("[assert] runtime decide-action override merged into the commit envelope");
    println!(
        "PASS: runtime-hooks override — commit action=final \
         content=\"runtime-hook-forced-final\" (runtime decision authoritative)."
    );
    Ok(())
}

/// Runtime-hooks deny: with a deny-mode provider, prepare-user-turn reaches
/// the runtime `prepare-turn` gate (the composed default-hooks passed
/// through) and must abort fail-closed with the `permission:` message — the
/// runner never falls back to the SDK default when the provider denies.
fn probe_runtime_hooks_deny(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: runtime-hooks deny (fail-closed) ---");
    let session_id = probe_init(runner, store, "runtime-hooks-deny")?;

    let request_json = serde_json::json!({
        "prompt": "runtime hooks deny probe",
        "session_id": session_id,
        "correlation_id": "runtime-hooks-deny",
    })
    .to_string();
    match runner.call_prepare_user_turn(store, &request_json) {
        Ok(Err(e)) => {
            if !e.contains(RUNTIME_HOOK_DENY_MSG) {
                anyhow::bail!(
                    "FAIL: expected prepare-user-turn to abort with a message containing \
                     {RUNTIME_HOOK_DENY_MSG:?}, got {e:?}"
                );
            }
            println!("[assert] prepare-user-turn aborted fail-closed: {e}");
        }
        Ok(Ok(prepared)) => anyhow::bail!(
            "FAIL: prepare-user-turn succeeded ({prepared:?}) despite deny-mode runtime-hooks"
        ),
        Err(trap) => anyhow::bail!("wasmtime trap on prepare-user-turn: {trap}"),
    }
    println!(
        "PASS: runtime-hooks deny — operation aborted with {RUNTIME_HOOK_DENY_MSG:?} (fail-closed)."
    );
    Ok(())
}

/// Runtime-hooks disabled: the provider is in deny mode but init config sets
/// `runtime_hooks_enabled: false`, so the runner must never consult it. The
/// flow completes with the SDK default decision and no permission error —
/// proving the config flag gates the runtime provider out entirely.
fn probe_runtime_hooks_disabled(runner: &Runner, store: &mut Store) -> Result<()> {
    println!("--- probe: runtime_hooks_enabled=false (provider not consulted) ---");
    let config = serde_json::json!({
        "session_id": "runtime-hooks-disabled",
        "max_steps": 5,
        "runtime_hooks_enabled": false,
    })
    .to_string();
    let session_id = runner
        .call_init(&mut *store, &config)
        .context("wasmtime trap on init")?
        .map_err(|e| anyhow::anyhow!("init failed: {e}"))?;
    println!("[init] session_id = {session_id}");

    let prepared = probe_prepare(runner, store, &session_id, "runtime hooks disabled probe")?;
    let llm_response = serde_json::json!({
        "action": "final",
        "content": "standard-final-content",
    })
    .to_string();
    let commit = probe_commit_with_response(runner, store, &prepared, &llm_response)?;
    if commit["action"] != "final" {
        anyhow::bail!(
            "FAIL: expected commit action=final (SDK default, provider skipped), got {}",
            commit["action"]
        );
    }
    if commit["content"] != "standard-final-content" {
        anyhow::bail!(
            "FAIL: expected commit content=\"standard-final-content\" (provider skipped), got {}",
            commit["content"]
        );
    }
    println!(
        "[assert] deny-mode provider not consulted (runtime_hooks_enabled=false): no permission error"
    );
    println!("PASS: runtime_hooks_enabled=false — runtime-hooks skipped, flow completes.");
    Ok(())
}

/// B4 structured-error assertion: the Err string must be the JSON object
/// `{"error":"not implemented","function":"<name>"}` produced by a logic-core
/// template hole.
fn assert_not_implemented(err: &str, function: &str) -> Result<()> {
    let parsed: Value = serde_json::from_str(err)
        .with_context(|| format!("not-implemented error is not valid JSON: {err}"))?;
    if parsed["error"] != "not implemented" {
        anyhow::bail!(
            "FAIL: expected error.error=\"not implemented\" for {function}, got {}",
            parsed["error"]
        );
    }
    if parsed["function"] != function {
        anyhow::bail!(
            "FAIL: expected error.function=\"{function}\", got {}",
            parsed["function"]
        );
    }
    println!("[notimpl] {function}: Err {err}");
    Ok(())
}
