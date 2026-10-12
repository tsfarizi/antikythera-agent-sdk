//! The wasmtime core session and the `RuntimeServer` facade.
//!
//! The core runs on a dedicated thread that owns the `Store`. Host functions
//! called from inside a store call block on the shared tokio runtime handle
//! (`Handle::block_on`) — safe because the core thread is never a tokio
//! worker. The HTTP/SSE layer runs on tokio workers against the same
//! `SharedState`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::config::ServerRuntimeConfig;
use crate::llm::LlmProvider;
use crate::routing::LocalToolHandler;
use crate::wire::ToolDefinition;
use crate::wit::{HostState, ServerRuntime, SharedState};

/// A live runner session over the loaded composite.
pub struct CoreSession {
    store: wasmtime::Store<HostState>,
    root: ServerRuntime,
    pub shared: Arc<SharedState>,
}

/// The generated runner export handle.
pub type Runner = crate::wit::exports::antikythera::agent_sdk::runner::Guest;

impl CoreSession {
    /// Load the component and instantiate it against the wired linker.
    pub fn new(component_path: &Path, shared: Arc<SharedState>) -> Result<Self> {
        let mut linker = wasmtime::component::Linker::new(&shared.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .context("register wasi imports into linker")?;
        ServerRuntime::add_to_linker::<_, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |state| state,
        )
        .context("register runtime-hooks + host-imports into linker")?;

        let component_bytes = std::fs::read(component_path).with_context(|| {
            format!(
                "read composite component path: {}",
                component_path.display()
            )
        })?;
        let component = wasmtime::component::Component::new(&shared.engine, &component_bytes)
            .context("compile composite component")?;

        let ctx = wasmtime_wasi::WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();
        let mut store = wasmtime::Store::new(
            &shared.engine,
            HostState {
                ctx,
                table: wasmtime_wasi::ResourceTable::new(),
                shared: shared.clone(),
            },
        );
        let root = ServerRuntime::instantiate(&mut store, &component, &linker)
            .context("instantiate composite runner export")?;
        Ok(Self {
            store,
            root,
            shared,
        })
    }

    fn with_runner<T>(
        &mut self,
        f: impl FnOnce(&Runner, &mut wasmtime::Store<HostState>) -> Result<T, wasmtime::Error>,
    ) -> Result<T, String> {
        let runner = self.root.antikythera_agent_sdk_runner();
        f(runner, &mut self.store).map_err(|e| format!("wasmtime trap: {e}"))
    }

    pub fn init(&mut self, config_json: &str) -> Result<String, String> {
        let result = self.with_runner(|runner, store| runner.call_init(store, config_json))?;
        result.map_err(|e| format!("runner init failed: {e}"))
    }

    pub fn prepare_user_turn(&mut self, request_json: &str) -> Result<String, String> {
        let result =
            self.with_runner(|runner, store| runner.call_prepare_user_turn(store, request_json))?;
        result.map_err(|e| format!("prepare-user-turn failed: {e}"))
    }

    pub fn commit_llm_response(
        &mut self,
        prepared_turn_json: &str,
        llm_response_json: &str,
    ) -> Result<String, String> {
        let result = self.with_runner(|runner, store| {
            runner.call_commit_llm_response(store, prepared_turn_json, llm_response_json)
        })?;
        result.map_err(|e| format!("commit-llm-response failed: {e}"))
    }

    pub fn commit_llm_stream(&mut self, prepared_turn_json: &str) -> Result<String, String> {
        let result = self.with_runner(|runner, store| {
            runner.call_commit_llm_stream(store, prepared_turn_json)
        })?;
        result.map_err(|e| format!("commit-llm-stream failed: {e}"))
    }

    pub fn drain_events(&mut self, session_id: &str) -> Result<String, String> {
        let result =
            self.with_runner(|runner, store| runner.call_drain_events(store, session_id))?;
        result.map_err(|e| format!("drain-events failed: {e}"))
    }

    pub fn process_tool_result_for_session(
        &mut self,
        session_id: &str,
        tool_result_json: &str,
    ) -> Result<String, String> {
        let result = self.with_runner(|runner, store| {
            runner.call_process_tool_result_for_session(store, session_id, tool_result_json)
        })?;
        result.map_err(|e| format!("process-tool-result-for-session failed: {e}"))
    }

    pub fn append_llm_chunk(
        &mut self,
        session_id: &str,
        chunk: &str,
        correlation_id: Option<&str>,
    ) -> Result<bool, String> {
        let result = self.with_runner(|runner, store| {
            runner.call_append_llm_chunk(store, session_id, chunk, correlation_id)
        })?;
        result.map_err(|e| format!("append-llm-chunk failed: {e}"))
    }

    pub fn register_tools(&mut self, tools_json: &str) -> Result<u32, String> {
        let result =
            self.with_runner(|runner, store| runner.call_register_tools(store, tools_json))?;
        result.map_err(|e| format!("register-tools failed: {e}"))
    }

    pub fn reset_session(&mut self, session_id: &str) -> Result<bool, String> {
        let result =
            self.with_runner(|runner, store| runner.call_reset_session(store, session_id))?;
        result.map_err(|e| format!("reset-session failed: {e}"))
    }

    pub fn get_state(&mut self, session_id: &str) -> Result<String, String> {
        let result = self.with_runner(|runner, store| runner.call_get_state(store, session_id))?;
        result.map_err(|e| format!("get-state failed: {e}"))
    }
}

/// The server runtime facade: owns the shared state, spawns core sessions,
/// and exposes the HTTP router.
pub struct RuntimeServer {
    pub shared: Arc<SharedState>,
    config: ServerRuntimeConfig,
}

impl RuntimeServer {
    /// Build the server with providers resolved from the config specs.
    ///
    /// The caller supplies the tokio `Handle` that host functions block on
    /// (the handle must belong to a multi-thread runtime whose workers drive
    /// the HTTP server). Does not block the calling thread.
    pub fn new(config: ServerRuntimeConfig, handle: tokio::runtime::Handle) -> Result<Self> {
        let providers = crate::llm::build_providers(config.providers.clone())?;
        Self::new_with_providers(config, providers, handle)
    }

    /// Build the server with an explicit provider map (pluggable providers).
    /// Registers `--server-tool` entries: a deterministic local handler that always returns the
    /// configured response JSON. Registration already granted the local destination in the policy
    /// (main.rs), so the default-deny gate accepts these tools without `--allow-tool`.
    ///
    pub fn new_with_providers(
        config: ServerRuntimeConfig,
        providers: HashMap<String, Arc<dyn LlmProvider>>,
        handle: tokio::runtime::Handle,
    ) -> Result<Self> {
        anyhow::ensure!(
            config
                .auth_token
                .as_ref()
                .is_none_or(|token| !token.trim().is_empty()),
            "auth token must not be empty"
        );
        anyhow::ensure!(
            config.bind_addr.ip().is_loopback() || config.auth_token.is_some(),
            "non-loopback bind requires an auth token"
        );
        let engine = Arc::new(wasmtime::Engine::default());
        let policy = Arc::new(config.policy.clone());
        let control = Arc::new(crate::control::ControlChannel::new());
        let router = Arc::new(crate::routing::ToolRouter::new(
            control.clone(),
            policy.clone(),
            handle.clone(),
            config.client_id.clone(),
            config.pending_ttl,
        ));
        for spec in &config.server_tools {
            let definition = ToolDefinition::simple(
                spec.name.clone(),
                "Server tool registered via --server-tool".to_string(),
            );
            let response = spec.response_json.clone();
            let handler: Arc<LocalToolHandler> = Arc::new(move |_args| Ok(response.clone()));
            router
                .register_local_tool(definition, handler)
                .map_err(|e| anyhow::anyhow!("register --server-tool '{}': {e}", spec.name))?;
        }
        let shared = Arc::new(SharedState {
            auth_token: config.auth_token.clone(),
            allowed_origins: config.allowed_origins.clone(),
            engine,
            policy,
            control,
            providers,
            default_provider: config.default_provider.clone(),
            router,
            storage: std::sync::Mutex::new(HashMap::new()),
            storage_capacity_bytes: config.storage_capacity_bytes,
            llm_calls: std::sync::Mutex::new(HashMap::new()),
            runtime: handle,
            client_id: config.client_id.clone(),
            session_id: config.session_id.clone(),
            pending_ttl: config.pending_ttl,
        });
        Ok(Self { shared, config })
    }

    /// Connect every configured MCP server and register its tools as
    /// `mcp`-owned. Blocking: must be called from a non-async context.
    pub fn connect_mcp_servers(&self) -> Result<(), String> {
        for server in &self.config.mcp_servers {
            self.shared.router.connect_mcp_server(server.clone())?;
        }
        Ok(())
    }

    pub fn router(&self) -> Arc<crate::routing::ToolRouter> {
        self.shared.router.clone()
    }

    pub fn control(&self) -> Arc<crate::control::ControlChannel> {
        self.shared.control.clone()
    }

    pub fn client_id(&self) -> &str {
        &self.shared.client_id
    }

    pub fn component_path(&self) -> &Path {
        &self.config.component_path
    }

    pub fn config(&self) -> &ServerRuntimeConfig {
        &self.config
    }

    /// Run `f` on a dedicated core thread with a fresh `CoreSession`.
    /// The thread is never a tokio worker, so blocking host functions may
    /// drive the runtime via `Handle::block_on`.
    pub fn with_core<F, R>(&self, f: F) -> std::thread::JoinHandle<Result<R, String>>
    where
        F: FnOnce(&mut CoreSession) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let shared = self.shared.clone();
        let component_path = self.config.component_path.clone();
        std::thread::spawn(move || {
            let mut core = CoreSession::new(&component_path, shared.clone())
                .map_err(|e| format!("core session: {e:#}"))?;
            f(&mut core)
        })
    }

    /// The axum router implementing the wire protocol.
    pub fn http_router(&self) -> axum::Router {
        crate::http::router(self.shared.clone())
    }
}
