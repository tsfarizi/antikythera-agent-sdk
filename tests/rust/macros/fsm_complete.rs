use antikythera_macros::FsmComplete;

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
pub enum AgentFsmState {
    Idle,
    UserTurnPrepared,
    LlmStreaming,
    LlmCommitted,
    ToolRequested,
    ToolResultProcessed,
    Final,
}

#[test]
/// The fact that this file compiles at all proves the macro works for a valid FSM with complete
/// transitions.
///
fn test_valid_fsm_compiles() {
    let _state = AgentFsmState::Idle;
}

#[derive(FsmComplete)]
#[fsm_transitions(
    On => [Off],
    Off => [On]
)]
pub enum ToggleState {
    On,
    Off,
}

#[test]
fn test_simple_two_state_fsm() {
    let _state = ToggleState::On;
}

#[derive(FsmComplete)]
#[fsm_transitions(
    Running => [Running, Stopped],
    Stopped => [Running]
)]
pub enum ProcessState {
    Running,
    Stopped,
}

#[test]
fn test_self_loop_fsm() {
    let _state = ProcessState::Running;
}
