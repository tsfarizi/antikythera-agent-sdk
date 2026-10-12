//! Parity tests ensuring WASM and core FSM implementations stay in sync.
//!
//! Contract: `antikythera_sdk::wasm_agent::types::AgentFsmState` MUST have
//! identical transition semantics to `antikythera_core::domain::fsm::AgentFsmState`.
//! These tests hardcode the expected transition matrix and variant count so that
//! a drift in one copy without the other causes a test failure.
//!
//! NOTE: These tests import both crates independently and exercise their
//! `transition_to` / `can_transition_to` methods against the same matrix.
//! If either crate changes its transition logic, the mismatch surfaces here.

use antikythera_core::domain::fsm::AgentFsmState as CoreFsmState;
use antikythera_domain::fsm::AgentFsmState as DomainFsmState;
use antikythera_sdk::wasm_agent::types::AgentFsmState as WasmFsmState;

/// Shared expected matrix (source of truth lives in the doc-comment above).
///
const ALL_STATES_WASM: &[WasmFsmState] = &[
    WasmFsmState::Idle,
    WasmFsmState::UserTurnPrepared,
    WasmFsmState::LlmStreaming,
    WasmFsmState::LlmCommitted,
    WasmFsmState::ToolRequested,
    WasmFsmState::ToolResultProcessed,
    WasmFsmState::Final,
];

const ALL_STATES_CORE: &[CoreFsmState] = &[
    CoreFsmState::Idle,
    CoreFsmState::UserTurnPrepared,
    CoreFsmState::LlmStreaming,
    CoreFsmState::LlmCommitted,
    CoreFsmState::ToolRequested,
    CoreFsmState::ToolResultProcessed,
    CoreFsmState::Final,
];

/// The 11 valid transitions — identical for both implementations.
/// Idle -> UserTurnPrepared UserTurnPrepared -> LlmStreaming LlmStreaming -> LlmCommitted
/// LlmCommitted -> ToolRequested LlmCommitted -> Final LlmCommitted -> Idle ToolRequested ->
/// ToolResultProcessed ToolResultProcessed -> LlmStreaming ToolResultProcessed -> Final
/// ToolResultProcessed -> Idle Final -> Idle.
///
const EXPECTED_VALID: &[(usize, usize)] = &[
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (3, 6),
    (3, 0),
    (4, 5),
    (5, 2),
    (5, 6),
    (5, 0),
    (6, 0),
];

fn state_at_wasm(idx: usize) -> WasmFsmState {
    ALL_STATES_WASM[idx]
}

fn state_at_core(idx: usize) -> CoreFsmState {
    ALL_STATES_CORE[idx]
}

/// Collect all (source, target) pairs from an FsmComplete-generated matrix.
fn matrix_pairs(
    matrix: &'static [(&'static str, &'static [&'static str])],
) -> std::collections::HashSet<(&'static str, &'static str)> {
    let mut pairs = std::collections::HashSet::new();
    for (source, targets) in matrix {
        for target in *targets {
            pairs.insert((*source, *target));
        }
    }
    pairs
}

/// Map a domain FSM state to its variant name as emitted in TRANSITION_MATRIX.
fn domain_state_name(state: DomainFsmState) -> &'static str {
    match state {
        DomainFsmState::Idle => "Idle",
        DomainFsmState::UserTurnPrepared => "UserTurnPrepared",
        DomainFsmState::LlmStreaming => "LlmStreaming",
        DomainFsmState::LlmCommitted => "LlmCommitted",
        DomainFsmState::ToolRequested => "ToolRequested",
        DomainFsmState::ToolResultProcessed => "ToolResultProcessed",
        DomainFsmState::Final => "Final",
    }
}

fn is_valid_wasm(from: WasmFsmState, to: WasmFsmState) -> bool {
    EXPECTED_VALID
        .iter()
        .any(|&(f, t)| state_at_wasm(f) == from && state_at_wasm(t) == to)
}

fn is_valid_core(from: CoreFsmState, to: CoreFsmState) -> bool {
    EXPECTED_VALID
        .iter()
        .any(|&(f, t)| state_at_core(f) == from && state_at_core(t) == to)
}

#[test]
fn wasm_fsm_variant_count_matches_core() {
    assert_eq!(
        ALL_STATES_WASM.len(),
        ALL_STATES_CORE.len(),
        "WASM and core must have the same number of FSM variants"
    );
    assert_eq!(ALL_STATES_WASM.len(), 7, "FSM must have exactly 7 states");
}

#[test]
/// 2. WASM implementation matches expected transition matrix.
///
fn wasm_fsm_matches_expected_transition_matrix() {
    for &from in ALL_STATES_WASM {
        for &to in ALL_STATES_WASM {
            let expected = is_valid_wasm(from, to);
            let mut state = from;
            let result = state.transition_to(to);
            let also_pure = from.can_transition_to(&to);

            if expected {
                assert!(
                    result.is_ok(),
                    "WASM: {:?} -> {:?} should be valid",
                    from,
                    to
                );
                assert_eq!(state, to, "WASM: state must mutate to {:?}", to);
                assert!(
                    also_pure,
                    "WASM: can_transition_to should agree for {:?} -> {:?}",
                    from, to
                );
            } else {
                assert!(
                    result.is_err(),
                    "WASM: {:?} -> {:?} should be invalid",
                    from,
                    to
                );
                assert_eq!(state, from, "WASM: state must not change on rejection");
                assert!(
                    !also_pure,
                    "WASM: can_transition_to should be false for {:?} -> {:?}",
                    from, to
                );
            }
        }
    }
}

#[test]
/// 3. Core implementation matches expected transition matrix.
///
fn core_fsm_matches_expected_transition_matrix() {
    for &from in ALL_STATES_CORE {
        for &to in ALL_STATES_CORE {
            let expected = is_valid_core(from, to);
            let mut state = from;
            let result = state.transition_to(to);
            let also_pure = from.can_transition_to(&to);

            if expected {
                assert!(
                    result.is_ok(),
                    "CORE: {:?} -> {:?} should be valid",
                    from,
                    to
                );
                assert_eq!(state, to, "CORE: state must mutate to {:?}", to);
                assert!(
                    also_pure,
                    "CORE: can_transition_to should agree for {:?} -> {:?}",
                    from, to
                );
            } else {
                assert!(
                    result.is_err(),
                    "CORE: {:?} -> {:?} should be invalid",
                    from,
                    to
                );
                assert_eq!(state, from, "CORE: state must not change on rejection");
                assert!(
                    !also_pure,
                    "CORE: can_transition_to should be false for {:?} -> {:?}",
                    from, to
                );
            }
        }
    }
}

#[test]
/// 4. Cross-crate behavioral parity: both accept/reject the same pairs.
///
/// Map WASM state to core state by variant name (positional).
///
fn wasm_and_core_agree_on_validity_for_every_pair() {
    for &from in ALL_STATES_WASM {
        for &to in ALL_STATES_WASM {
            let core_from = state_at_core(ALL_STATES_WASM.iter().position(|&s| s == from).unwrap());
            let core_to = state_at_core(ALL_STATES_WASM.iter().position(|&s| s == to).unwrap());

            let wasm_valid = from.can_transition_to(&to);
            let core_valid = core_from.can_transition_to(&core_to);

            assert_eq!(
                wasm_valid, core_valid,
                "Parity broken: WASM({:?} -> {:?}) = {}, CORE({:?} -> {:?}) = {}",
                from, to, wasm_valid, core_from, core_to, core_valid
            );
        }
    }
}

#[test]
/// 5. Cross-crate: identical transition result (Ok vs Err).
///
fn wasm_and_core_return_same_result_for_every_pair() {
    for &from in ALL_STATES_WASM {
        for &to in ALL_STATES_WASM {
            let core_from = state_at_core(ALL_STATES_WASM.iter().position(|&s| s == from).unwrap());
            let core_to = state_at_core(ALL_STATES_WASM.iter().position(|&s| s == to).unwrap());

            let mut wasm_state = from;
            let wasm_result = wasm_state.transition_to(to);

            let mut core_state = core_from;
            let core_result = core_state.transition_to(core_to);

            assert_eq!(
                wasm_result.is_ok(),
                core_result.is_ok(),
                "Result parity broken: WASM({:?} -> {:?}) ok={}, CORE({:?} -> {:?}) ok={}",
                from,
                to,
                wasm_result.is_ok(),
                core_from,
                core_to,
                core_result.is_ok()
            );
        }
    }
}

#[test]
/// 6. Parity: initial() returns Idle in both.
///
fn initial_state_is_idle_in_both_implementations() {
    assert_eq!(
        WasmFsmState::initial(),
        WasmFsmState::Idle,
        "WASM initial must be Idle"
    );
    assert_eq!(
        CoreFsmState::initial(),
        CoreFsmState::Idle,
        "CORE initial must be Idle"
    );
}

#[test]
/// 7. Parity: Display outputs match.
///
fn display_outputs_match_for_all_states() {
    for &wasm_state in ALL_STATES_WASM {
        let core_state = state_at_core(
            ALL_STATES_WASM
                .iter()
                .position(|&s| s == wasm_state)
                .unwrap(),
        );
        assert_eq!(
            wasm_state.to_string(),
            core_state.to_string(),
            "Display mismatch for {:?}",
            wasm_state
        );
    }
}

#[test]
/// 8. Parity: serialization uses same snake_case format.
///
fn serialization_format_matches() {
    for &wasm_state in ALL_STATES_WASM {
        let core_state = state_at_core(
            ALL_STATES_WASM
                .iter()
                .position(|&s| s == wasm_state)
                .unwrap(),
        );
        let wasm_json = serde_json::to_string(&wasm_state).unwrap();
        let core_json = serde_json::to_string(&core_state).unwrap();
        assert_eq!(
            wasm_json, core_json,
            "JSON format mismatch for {:?}",
            wasm_state
        );
    }
}

#[test]
/// 9. Parity: FsmTransitionError has same structure.
///
/// Both error types should contain the same from/to states.
///
fn transition_error_fields_match() {
    let wasm_err = WasmFsmState::Idle
        .transition_to(WasmFsmState::Final)
        .unwrap_err();
    let core_err = CoreFsmState::Idle
        .transition_to(CoreFsmState::Final)
        .unwrap_err();

    assert_eq!(wasm_err.from, WasmFsmState::Idle);
    assert_eq!(wasm_err.to, WasmFsmState::Final);
    assert_eq!(core_err.from, CoreFsmState::Idle);
    assert_eq!(core_err.to, CoreFsmState::Final);
}

#[test]
fn transition_error_display_format_matches() {
    let wasm_err = WasmFsmState::Idle
        .transition_to(WasmFsmState::Final)
        .unwrap_err();
    let core_err = CoreFsmState::Idle
        .transition_to(CoreFsmState::Final)
        .unwrap_err();

    assert_eq!(
        wasm_err.to_string(),
        core_err.to_string(),
        "Error display format must match"
    );
}

#[test]
/// 10. Generated TRANSITION_MATRIX: cross-crate parity.
///
fn generated_transition_matrices_match() {
    let domain_matrix = DomainFsmState::TRANSITION_MATRIX;
    let wasm_matrix = WasmFsmState::TRANSITION_MATRIX;

    assert_eq!(
        domain_matrix, wasm_matrix,
        "FsmComplete-generated TRANSITION_MATRIX must be identical across domain and WASM mirror"
    );
}

#[test]
/// 11. Generated TRANSITION_MATRIX: consistency with hand-written can_transition_to.
///
/// The matrix must contain exactly the 11 valid transitions that the hand-written implementation
/// (and the hardcoded EXPECTED_VALID list) accept.
///
/// For every (source, target) pair: membership in the matrix must agree with the hand-written
/// can_transition_to — in both directions.
///
fn generated_transition_matrix_matches_domain_can_transition_to() {
    let matrix = DomainFsmState::TRANSITION_MATRIX;
    let pairs = matrix_pairs(matrix);

    let total_transitions: usize = matrix.iter().map(|(_, targets)| targets.len()).sum();
    assert_eq!(
        total_transitions,
        EXPECTED_VALID.len(),
        "TRANSITION_MATRIX must contain exactly the same transition count as the manual matrix"
    );

    for &from in ALL_STATES_CORE {
        for &to in ALL_STATES_CORE {
            let in_matrix = pairs.contains(&(domain_state_name(from), domain_state_name(to)));
            let can_transition = from.can_transition_to(&to);
            assert_eq!(
                in_matrix, can_transition,
                "TRANSITION_MATRIX and can_transition_to disagree for {:?} -> {:?}: matrix={}, can_transition_to={}",
                from, to, in_matrix, can_transition
            );
        }
    }
}
