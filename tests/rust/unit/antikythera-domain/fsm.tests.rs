use super::*;

#[test]
fn initial_state_is_idle() {
    assert_eq!(AgentFsmState::initial(), AgentFsmState::Idle);
}

#[test]
fn valid_happy_path_transitions() {
    let mut s = AgentFsmState::Idle;
    s.transition_to(AgentFsmState::UserTurnPrepared).unwrap();
    s.transition_to(AgentFsmState::LlmStreaming).unwrap();
    s.transition_to(AgentFsmState::LlmCommitted).unwrap();
    s.transition_to(AgentFsmState::ToolRequested).unwrap();
    s.transition_to(AgentFsmState::ToolResultProcessed).unwrap();
    s.transition_to(AgentFsmState::LlmStreaming).unwrap();
    s.transition_to(AgentFsmState::LlmCommitted).unwrap();
    s.transition_to(AgentFsmState::Final).unwrap();
    s.transition_to(AgentFsmState::Idle).unwrap();
}

#[test]
fn invalid_transition_returns_error() {
    let mut s = AgentFsmState::Idle;
    let result = s.transition_to(AgentFsmState::LlmStreaming);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.from, AgentFsmState::Idle);
    assert_eq!(err.to, AgentFsmState::LlmStreaming);
}

#[test]
fn can_transition_to_pure_check() {
    assert!(AgentFsmState::Idle.can_transition_to(&AgentFsmState::UserTurnPrepared));
    assert!(!AgentFsmState::Idle.can_transition_to(&AgentFsmState::Final));
}

#[test]
fn committed_can_go_to_idle_or_final() {
    assert!(AgentFsmState::LlmCommitted.can_transition_to(&AgentFsmState::Idle));
    assert!(AgentFsmState::LlmCommitted.can_transition_to(&AgentFsmState::Final));
    assert!(AgentFsmState::LlmCommitted.can_transition_to(&AgentFsmState::ToolRequested));
}

#[test]
fn display_produces_snake_case() {
    assert_eq!(AgentFsmState::Idle.to_string(), "idle");
    assert_eq!(
        AgentFsmState::ToolResultProcessed.to_string(),
        "tool_result_processed"
    );
}

#[test]
fn transition_error_display() {
    let err = FsmTransitionError {
        from: AgentFsmState::Idle,
        to: AgentFsmState::Final,
    };
    let msg = err.to_string();
    assert!(msg.contains("idle"));
    assert!(msg.contains("final"));
}
