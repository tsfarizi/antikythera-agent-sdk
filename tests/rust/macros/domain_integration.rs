//! Integration tests verifying `antikythera-macros` derives work across crate boundaries
//! when used within the `antikythera-domain` crate's test context.

#![allow(dead_code)]

use antikythera_macros::FsmComplete;

/// Mirror of `antikythera_domain::fsm::AgentFsmState` transition matrix.
/// Used to validate that `FsmComplete` compiles and passes across crate boundary.
#[derive(FsmComplete)]
#[fsm_transitions(
    Idle => [UserTurnPrepared],
    UserTurnPrepared => [LlmStreaming],
    LlmStreaming => [LlmCommitted],
    LlmCommitted => [ToolRequested, Final, Idle],
    ToolRequested => [ToolResultProcessed],
    ToolResultProcessed => [LlmStreaming, Final, Idle],
    Final => [Idle]
)]
enum DomainAgentFsmState {
    Idle,
    UserTurnPrepared,
    LlmStreaming,
    LlmCommitted,
    ToolRequested,
    ToolResultProcessed,
    Final,
}

#[test]
/// If this compiles, the macro validated the transition matrix at compile time.
///
fn fsm_complete_compiles_with_domain_types() {
    let state = DomainAgentFsmState::Idle;
    assert!(matches!(state, DomainAgentFsmState::Idle));
}

#[test]
/// Verify the hand-coded domain FSM accepts the same transitions that FsmComplete validated at
/// compile time for our mirror enum.
///
fn fsm_mirror_matches_domain_transition_rules() {
    use antikythera_domain::fsm::AgentFsmState;

    let mut s = AgentFsmState::Idle;
    s.transition_to(AgentFsmState::UserTurnPrepared).unwrap();
    s.transition_to(AgentFsmState::LlmStreaming).unwrap();
    s.transition_to(AgentFsmState::LlmCommitted).unwrap();
    s.transition_to(AgentFsmState::ToolRequested).unwrap();
    s.transition_to(AgentFsmState::ToolResultProcessed).unwrap();
    s.transition_to(AgentFsmState::Final).unwrap();
    s.transition_to(AgentFsmState::Idle).unwrap();
    assert_eq!(s, AgentFsmState::Idle);
}

/// Simple two-state FSM to test the macro with minimal complexity.
#[derive(FsmComplete)]
#[fsm_transitions(
    Active => [Inactive],
    Inactive => [Active]
)]
enum SimpleFsm {
    Active,
    Inactive,
}

#[test]
fn simple_two_state_fsm_compiles() {
    let state = SimpleFsm::Active;
    assert!(matches!(state, SimpleFsm::Active));
}
