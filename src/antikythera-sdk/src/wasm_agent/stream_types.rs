use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEventKind {
    UserTurnPrepared,
    LlmChunk,
    LlmCommitted,
    ToolRequested,
    ToolResult,
    FinalResponse,
    SummaryUpdated,
    SessionArchived,
    SessionRestoreRequested,
    SessionRestoreProgress,
    SessionRestored,
    Telemetry,
    FsmStateChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub seq: u64,
    pub session_id: String,
    pub step: u32,
    pub correlation_id: Option<String>,
    pub kind: StreamEventKind,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelemetryCounters {
    pub turns_prepared: u64,
    pub llm_chunks: u64,
    pub llm_commits: u64,
    pub llm_retries: u64,
    pub tool_requests: u64,
    pub tool_results: u64,
    pub tool_errors: u64,
    pub final_responses: u64,
    pub context_summaries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelemetrySnapshot {
    pub session_id: String,
    pub correlation_id: Option<String>,
    pub counters: TelemetryCounters,
    pub total_prepare_latency_ms: u64,
    pub total_commit_latency_ms: u64,
    pub fsm_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SloSnapshot {
    pub session_id: String,
    pub correlation_id: Option<String>,
    pub success_rate: f64,
    pub tool_error_rate: f64,
    pub retry_ratio: f64,
    pub p95_prepare_latency_ms: u64,
    pub p95_commit_latency_ms: u64,
}
