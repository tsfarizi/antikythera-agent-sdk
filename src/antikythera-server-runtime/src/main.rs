//! Binary entrypoint for the server runtime.
//!
//! Starts the wire-protocol HTTP server and, with `--smoke`, runs the K1
//! tool loop against the composite with a stub LLM (no external provider) to
//! prove the hosting path end-to-end.

use std::path::PathBuf;

use antikythera_server_runtime::config::{LlmProviderSpec, ServerRuntimeConfig, ServerToolSpec};
use antikythera_server_runtime::loop_owner::{ToolLoopConfig, run_tool_loop};
use antikythera_server_runtime::registry::Destination;
use antikythera_server_runtime::{CoreSession, RuntimeServer};

/// Registration is a grant: the local destination is allowlisted so the default-deny gate does
/// not reject the registered tool.
///
fn parse_args() -> (ServerRuntimeConfig, bool) {
    let mut config = ServerRuntimeConfig {
        auth_token: std::env::var("ANTIKYTHERA_AUTH_TOKEN").ok(),
        allowed_origins: std::env::var("ANTIKYTHERA_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .collect(),
        ..Default::default()
    };
    let mut smoke = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--component" => {
                i += 1;
                config.component_path =
                    PathBuf::from(args.get(i).expect("--component needs a path"));
            }
            "--bind" => {
                i += 1;
                config.bind_addr = args
                    .get(i)
                    .expect("--bind needs an address")
                    .parse()
                    .expect("invalid bind address");
            }
            "--client-id" => {
                i += 1;
                config.client_id = args.get(i).expect("--client-id needs a value").clone();
            }
            "--allow-tool" => {
                i += 1;
                let tool = args.get(i).expect("--allow-tool needs a name");
                config.policy.allow_tool(Destination::Local, tool);
                config.policy.allow_tool(Destination::Remote, tool);
                config.policy.allow_tool(Destination::Mcp, tool);
            }
            "--server-tool" => {
                i += 1;
                let spec = args
                    .get(i)
                    .expect("--server-tool needs <name>:<response-json>");
                let tool = ServerToolSpec::parse(spec).unwrap_or_else(|e| panic!("{e}"));
                config.policy.allow_tool(Destination::Local, &tool.name);
                config.server_tools.push(tool);
            }
            "--provider-stub" => {
                i += 1;
                let response = args.get(i).expect("--provider-stub needs a response JSON");
                config.providers.insert(
                    "stub".to_string(),
                    LlmProviderSpec::Stub {
                        response: response.clone(),
                    },
                );
                config.default_provider = "stub".to_string();
            }
            "--smoke" => smoke = true,
            other => panic!("unknown argument: {other}"),
        }
        i += 1;
    }
    (config, smoke)
}

/// Deterministic stub: commit consumes `{"action":"final",...}`.
///
/// Keeps the process alive serving HTTP.
///
fn main() {
    let (mut config, smoke) = parse_args();

    if smoke {
        config.providers.insert(
            "stub".to_string(),
            LlmProviderSpec::Stub {
                response: "{\"action\":\"final\",\"content\":\"smoke-complete\"}".to_string(),
            },
        );
        config.default_provider = "stub".to_string();
    }

    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let server =
        RuntimeServer::new(config, runtime.handle().clone()).expect("build server runtime");
    let bind_addr = server.config().bind_addr;

    let http_router = server.http_router();
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind(bind_addr))
        .expect("bind HTTP listener");
    runtime.spawn(async move {
        axum::serve(listener, http_router)
            .await
            .expect("serve HTTP");
    });
    println!("[server-runtime] HTTP wire bridge listening on {bind_addr}");

    if smoke {
        let shared = server.shared.clone();
        let component_path = server.component_path().to_path_buf();
        let loop_config = ToolLoopConfig {
            session_id: "smoke".to_string(),
            prompts: vec!["smoke test".to_string()],
            ..ToolLoopConfig::default()
        };
        let handle = std::thread::spawn(move || {
            let mut core = CoreSession::new(&component_path, shared.clone())
                .map_err(|e| format!("core session: {e:#}"))?;
            run_tool_loop(&mut core, &shared, loop_config)
        });
        match handle.join().expect("smoke core thread panicked") {
            Ok(outcome) => {
                if outcome.action != "final" {
                    eprintln!("FAIL: expected action=final, got {}", outcome.action);
                    std::process::exit(1);
                }
                println!(
                    "PASS: smoke — init+prepare+commit with stub LLM reached action=final \
                     (content={:?}, steps={})",
                    outcome.content, outcome.steps
                );
            }
            Err(e) => {
                eprintln!("FAIL: smoke loop error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    std::thread::park();
}
