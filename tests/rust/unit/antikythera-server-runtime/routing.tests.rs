use super::*;
use crate::config::GatePolicy;
use crate::control::ControlChannel;
use crate::wire::PostbackBody;

fn test_router(policy: GatePolicy) -> (Arc<ToolRouter>, Arc<ControlChannel>) {
    let control = Arc::new(ControlChannel::new());
    let router = Arc::new(ToolRouter::new(
        control.clone(),
        Arc::new(policy),
        tokio::runtime::Handle::current(),
        "client-a".to_string(),
        Duration::from_secs(5),
    ));
    (router, control)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routing_decision_resolves_owner_to_destination() {
    let (router, _) = test_router(GatePolicy::deny_all());
    router
        .register_local_tool(
            ToolDefinition::simple("local_tool", "server tool"),
            Arc::new(Ok),
        )
        .unwrap();
    router
        .register_client_tools(vec![ToolDefinition::simple("client_tool", "client tool")])
        .unwrap();

    assert_eq!(
        router.resolve_destination("local_tool").unwrap(),
        Destination::Local
    );
    assert_eq!(
        router.resolve_destination("client_tool").unwrap(),
        Destination::Remote
    );
    assert!(router.resolve_destination("unknown_tool").is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
/// The tool is registered but the default-deny policy blocks it.
///
async fn gate_denial_blocks_execution_before_dispatch() {
    let (router, _) = test_router(GatePolicy::deny_all());
    router
        .register_local_tool(
            ToolDefinition::simple("echo", "server tool"),
            Arc::new(|_| Ok(serde_json::json!({"echoed": true}))),
        )
        .unwrap();
    let event = ToolCallEvent {
        tool_name: "echo".to_string(),
        arguments_json: "{}".to_string(),
        session_id: None,
        step_id: 1,
    };
    let err = router.execute(&event).await.unwrap_err();
    assert!(err.starts_with("permission: "), "got: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_execution_runs_registered_handler() {
    let mut policy = GatePolicy::deny_all();
    policy.allow_tool(Destination::Local, "echo");
    let (router, _) = test_router(policy);
    router
        .register_local_tool(
            ToolDefinition::simple("echo", "server tool"),
            Arc::new(|args| Ok(serde_json::json!({"echoed": args}))),
        )
        .unwrap();
    let event = ToolCallEvent {
        tool_name: "echo".to_string(),
        arguments_json: "{\"x\": 1}".to_string(),
        session_id: None,
        step_id: 2,
    };
    let result = router.execute(&event).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output_json, "{\"echoed\":{\"x\":1}}");
    assert_eq!(result.step_id, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_handler_failure_is_a_result_not_an_error() {
    let mut policy = GatePolicy::deny_all();
    policy.allow_tool(Destination::Local, "boom");
    let (router, _) = test_router(policy);
    router
        .register_local_tool(
            ToolDefinition::simple("boom", "fails"),
            Arc::new(|_| Err("handler exploded".to_string())),
        )
        .unwrap();
    let event = ToolCallEvent {
        tool_name: "boom".to_string(),
        arguments_json: "{}".to_string(),
        session_id: None,
        step_id: 1,
    };
    let result = router.execute(&event).await.unwrap();
    assert!(!result.success);
    assert_eq!(result.error_message.as_deref(), Some("handler exploded"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
/// Read the SSE envelope, answer the tool-execution-request.
///
async fn remote_execution_roundtrips_through_postback() {
    let mut policy = GatePolicy::deny_all();
    policy.allow_tool(Destination::Remote, "client_tool");
    let (router, control) = test_router(policy);
    router
        .register_client_tools(vec![ToolDefinition::simple("client_tool", "client tool")])
        .unwrap();

    let mut rx = control.register_client("client-a");
    let client = control.clone();
    let responder = tokio::spawn(async move {
        while let Ok(json) = rx.recv().await {
            let value: Value = serde_json::from_str(&json).unwrap();
            if value["type"] == "tool-execution-request" {
                let correlation_id = value["correlation_id"].as_str().unwrap().to_string();
                client.complete_postback(PostbackBody {
                    correlation_id,
                    ok: true,
                    payload: serde_json::json!({
                        "tool-name": "client_tool",
                        "success": true,
                        "output-json": "{\"done\":true}",
                        "error-message": null,
                        "step-id": 1,
                    }),
                    error: None,
                });
                break;
            }
        }
    });

    let event = ToolCallEvent {
        tool_name: "client_tool".to_string(),
        arguments_json: "{}".to_string(),
        session_id: None,
        step_id: 1,
    };
    let result = router.execute(&event).await.unwrap();
    responder.await.unwrap();
    assert!(result.success);
    assert_eq!(result.output_json, "{\"done\":true}");
    assert_eq!(result.tool_name, "client_tool");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_execution_fails_closed_without_client() {
    let mut policy = GatePolicy::deny_all();
    policy.allow_tool(Destination::Remote, "client_tool");
    let (router, _) = test_router(policy);
    router
        .register_client_tools(vec![ToolDefinition::simple("client_tool", "client tool")])
        .unwrap();
    let event = ToolCallEvent {
        tool_name: "client_tool".to_string(),
        arguments_json: "{}".to_string(),
        session_id: None,
        step_id: 1,
    };
    let err = router.execute(&event).await.unwrap_err();
    assert!(err.starts_with("permission: "), "got: {err}");
    assert_eq!(router.control_pending_len_for_test(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_owned_execution_denies_client_tools() {
    let mut policy = GatePolicy::deny_all();
    policy.allow_tool(Destination::Remote, "client_tool");
    let (router, _) = test_router(policy);
    router
        .register_client_tools(vec![ToolDefinition::simple("client_tool", "client tool")])
        .unwrap();
    let event = ToolCallEvent {
        tool_name: "client_tool".to_string(),
        arguments_json: "{}".to_string(),
        session_id: None,
        step_id: 1,
    };
    let err = router.execute_server_owned(&event).await.unwrap_err();
    assert!(err.starts_with("permission: "), "got: {err}");
    assert!(err.contains("owned by the client"));
}
