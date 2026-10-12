use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use antikythera_core::application::agent::multi_agent::{
    AgentProfile, AgentTask, CancellationGuardrail, ErrorKind, ExecutionMode, GuardrailChain,
    MultiAgentOrchestrator, RateLimitGuardrail, TaskGuardrail, TimeoutGuardrail,
};
use antikythera_core::application::client::{ClientConfig, McpClient};
use antikythera_core::infrastructure::model::{
    ModelError, ModelProvider, ModelRequest, ModelResponse,
};
use async_trait::async_trait;

#[derive(Debug)]
struct CountingProvider {
    call_count: Arc<AtomicUsize>,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ModelProvider for CountingProvider {
    async fn chat(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(ModelResponse::new(
            r#"{"final_answer":"ok"}"#.to_string(),
            None,
        ))
    }
}

#[derive(Debug, Default)]
struct AlwaysRejectGuardrail;

impl TaskGuardrail for AlwaysRejectGuardrail {
    fn name(&self) -> &'static str {
        "always_reject"
    }

    fn pre_check(
        &self,
        _task: &AgentTask,
        _profile: &AgentProfile,
        _context: &antikythera_core::application::agent::multi_agent::GuardrailContext,
    ) -> Result<(), antikythera_core::application::agent::multi_agent::GuardrailRejection> {
        Err(
            antikythera_core::application::agent::multi_agent::GuardrailRejection::new(
                self.name(),
                antikythera_core::application::agent::multi_agent::GuardrailStage::PreCheck,
                ErrorKind::Permanent,
                "forced rejection",
            ),
        )
    }
}

fn build_orchestrator(call_count: Arc<AtomicUsize>) -> MultiAgentOrchestrator<CountingProvider> {
    let client = Arc::new(McpClient::new(
        CountingProvider { call_count },
        ClientConfig::new("mock", "mock-model"),
        None,
    ));

    MultiAgentOrchestrator::new(client, ExecutionMode::Sequential).register_agent(AgentProfile {
        id: "guarded-agent".to_string(),
        name: "Guarded Agent".to_string(),
        role: "general".to_string(),
        system_prompt: Some("You are a guarded test agent".to_string()),
        max_steps: Some(4),
    })
}

#[tokio::test]
async fn timeout_guardrail_blocks_dispatch_before_provider_is_called() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let orchestrator = build_orchestrator(call_count.clone())
        .with_guardrail(Arc::new(TimeoutGuardrail::new(1_000).require_timeout()));

    let result = orchestrator
        .dispatch(AgentTask::new("review this code"))
        .await;

    assert!(!result.success);
    assert_eq!(result.error_kind, Some(ErrorKind::Permanent));
    assert_eq!(result.metadata.guardrail_name.as_deref(), Some("timeout"));
    assert_eq!(
        result.metadata.guardrail_stage.as_deref(),
        Some("pre_check")
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn guardrail_chain_preserves_order_and_rate_limit_rejects_second_dispatch() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let guardrails = GuardrailChain::new()
        .with_guardrail(Arc::new(RateLimitGuardrail::new(1, 60_000)))
        .with_guardrail(Arc::new(AlwaysRejectGuardrail));
    let orchestrator = build_orchestrator(call_count.clone()).with_guardrails(guardrails);

    let first = orchestrator.dispatch(AgentTask::new("task one")).await;
    let second = orchestrator.dispatch(AgentTask::new("task two")).await;

    assert!(!first.success);
    assert_eq!(
        first.metadata.guardrail_name.as_deref(),
        Some("always_reject")
    );

    assert!(!second.success);
    assert_eq!(second.error_kind, Some(ErrorKind::Transient));
    assert_eq!(
        second.metadata.guardrail_name.as_deref(),
        Some("rate_limit")
    );
    assert_eq!(
        second.metadata.guardrail_stage.as_deref(),
        Some("pre_check")
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_guardrail_blocks_dispatch_after_orchestrator_cancel() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let orchestrator = build_orchestrator(call_count.clone())
        .with_guardrail(Arc::new(CancellationGuardrail::new()));

    orchestrator.cancel();
    let result = orchestrator
        .dispatch(AgentTask::new("summarize logs"))
        .await;

    assert!(!result.success);
    assert_eq!(result.error_kind, Some(ErrorKind::Cancelled));
    assert!(result.metadata.cancelled);
    assert_eq!(
        result.metadata.guardrail_name.as_deref(),
        Some("cancellation")
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 0);
}
