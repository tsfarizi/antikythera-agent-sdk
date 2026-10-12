//! Unit tests for WASM AgentFsmState transitions.
//!
//! Contracts under test (traced to antikythera-sdk/src/wasm_agent/types.rs):
//! 1. `AgentFsmState::initial()` returns `Idle` — pre-condition for any agent loop.
//! 2. `transition_to(next)` mutates self to `next` and returns `Ok(())` iff the
//!    pair (self, next) is in the valid transition matrix.
//! 3. `transition_to(next)` returns `Err(FsmTransitionError)` and leaves self
//!    unchanged iff the pair is invalid.
//! 4. `can_transition_to(next)` is a pure predicate: returns the same answer as
//!    `transition_to` without mutating self.
//! 5. `Display` renders each variant as lowercase snake_case.
//! 6. `FsmTransitionError::Display` includes both `from` and `to` state names.
//! 7. Serialization round-trips through serde JSON using snake_case strings.
//!
//! All 7 states × 7 states = 49 transitions are covered exhaustively.
//! The 11 valid transitions and 38 invalid transitions are both asserted.

use antikythera_sdk::wasm_agent::types::{AgentFsmState, FsmTransitionError};

const ALL_STATES: &[AgentFsmState] = &[
    AgentFsmState::Idle,
    AgentFsmState::UserTurnPrepared,
    AgentFsmState::LlmStreaming,
    AgentFsmState::LlmCommitted,
    AgentFsmState::ToolRequested,
    AgentFsmState::ToolResultProcessed,
    AgentFsmState::Final,
];

/// The 11 valid transitions as declared in the source of truth.
/// Kept as a const slice of `(from, to)` tuples for exhaustive iteration.
const VALID_TRANSITIONS: &[(AgentFsmState, AgentFsmState)] = &[
    (AgentFsmState::Idle, AgentFsmState::UserTurnPrepared),
    (AgentFsmState::UserTurnPrepared, AgentFsmState::LlmStreaming),
    (AgentFsmState::LlmStreaming, AgentFsmState::LlmCommitted),
    (AgentFsmState::LlmCommitted, AgentFsmState::ToolRequested),
    (AgentFsmState::LlmCommitted, AgentFsmState::Final),
    (AgentFsmState::LlmCommitted, AgentFsmState::Idle),
    (
        AgentFsmState::ToolRequested,
        AgentFsmState::ToolResultProcessed,
    ),
    (
        AgentFsmState::ToolResultProcessed,
        AgentFsmState::LlmStreaming,
    ),
    (AgentFsmState::ToolResultProcessed, AgentFsmState::Final),
    (AgentFsmState::ToolResultProcessed, AgentFsmState::Idle),
    (AgentFsmState::Final, AgentFsmState::Idle),
];

fn is_valid_transition(from: AgentFsmState, to: AgentFsmState) -> bool {
    VALID_TRANSITIONS.iter().any(|&(f, t)| f == from && t == to)
}

#[test]
fn initial_state_is_idle() {
    assert_eq!(AgentFsmState::initial(), AgentFsmState::Idle);
}

#[test]
/// WASM variant derives Default; Idle is #`default`.
///
fn default_state_is_idle() {
    assert_eq!(AgentFsmState::default(), AgentFsmState::Idle);
}

#[test]
/// 2. All valid transitions succeed and mutate state.
///
fn all_valid_transitions_succeed() {
    for &(from, to) in VALID_TRANSITIONS {
        let mut state = from;
        let result = state.transition_to(to);
        assert!(
            result.is_ok(),
            "transition_to({:?} -> {:?}) should return Ok, got Err",
            from,
            to
        );
        assert_eq!(state, to, "state must be mutated to {:?}", to);
    }
}

#[test]
fn each_valid_transition_is_also_detected_by_can_transition_to() {
    for &(from, to) in VALID_TRANSITIONS {
        assert!(
            from.can_transition_to(&to),
            "can_transition_to({:?} -> {:?}) should be true",
            from,
            to
        );
    }
}

#[test]
/// 3. All invalid transitions are rejected and state is unchanged.
///
fn all_invalid_transitions_are_rejected() {
    let invalid_count = count_invalid_transitions();
    let mut tested = 0;

    for &from in ALL_STATES {
        for &to in ALL_STATES {
            if is_valid_transition(from, to) {
                continue;
            }
            tested += 1;
            let mut state = from;
            let result = state.transition_to(to);
            assert!(
                result.is_err(),
                "transition_to({:?} -> {:?}) should return Err, got Ok",
                from,
                to
            );
            assert_eq!(
                state, from,
                "state must remain {:?} after rejected transition",
                from
            );
        }
    }
    assert_eq!(tested, invalid_count, "exhausted all invalid transitions");
}

/// Count invalid transitions programmatically for assertion.
/// 49 - 11 = 38.
///
fn count_invalid_transitions() -> usize {
    let total = ALL_STATES.len() * ALL_STATES.len();
    total - VALID_TRANSITIONS.len()
}

#[test]
fn invalid_transition_count_is_38() {
    assert_eq!(count_invalid_transitions(), 38);
}

#[test]
fn valid_transition_count_is_11() {
    assert_eq!(VALID_TRANSITIONS.len(), 11);
}

#[test]
/// 4. Self-transitions are rejected (no state stays in place via transition_to).
///
fn all_self_transitions_are_rejected() {
    for &state in ALL_STATES {
        let mut s = state;
        let result = s.transition_to(state);
        assert!(
            result.is_err(),
            "self-transition {:?} -> {:?} should be rejected",
            state,
            state
        );
        assert_eq!(s, state);
    }
}

#[test]
fn can_transition_to_self_returns_false() {
    for &state in ALL_STATES {
        assert!(
            !state.can_transition_to(&state),
            "can_transition_to for self {:?} should be false",
            state
        );
    }
}

#[test]
fn transition_error_contains_from_and_to() {
    for &from in ALL_STATES {
        for &to in ALL_STATES {
            if is_valid_transition(from, to) {
                continue;
            }
            let mut state = from;
            let err = state.transition_to(to).unwrap_err();
            assert_eq!(err.from, from, "error.from must be {:?}", from);
            assert_eq!(err.to, to, "error.to must be {:?}", to);
        }
    }
}

#[test]
/// Actual format: "invalid FSM transition: idle -> llm_committed".
///
fn transition_error_display_uses_arrow_format() {
    let err = FsmTransitionError {
        from: AgentFsmState::Idle,
        to: AgentFsmState::LlmCommitted,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("idle"),
        "error display should contain 'idle', got: {}",
        msg
    );
    assert!(
        msg.contains("llm_committed"),
        "error display should contain 'llm_committed', got: {}",
        msg
    );
    assert!(
        msg.contains("invalid FSM transition"),
        "error display should contain prefix, got: {}",
        msg
    );
}

#[test]
fn transition_error_is_clone_and_copy() {
    let err = FsmTransitionError {
        from: AgentFsmState::ToolRequested,
        to: AgentFsmState::Final,
    };
    let err2 = err;
    assert_eq!(err, err2);
    let err3 = err;
    assert_eq!(err, err3);
}

#[test]
fn transition_error_is_debug() {
    let err = FsmTransitionError {
        from: AgentFsmState::LlmCommitted,
        to: AgentFsmState::Idle,
    };
    let dbg = format!("{:?}", err);
    assert!(dbg.contains("FsmTransitionError"));
    assert!(dbg.contains("LlmCommitted"));
    assert!(dbg.contains("Idle"));
}

#[test]
/// Verify it satisfies std::error::Error via downcast.
///
fn transition_error_implements_std_error() {
    let err = FsmTransitionError {
        from: AgentFsmState::Idle,
        to: AgentFsmState::Final,
    };
    let _dyn_err: &dyn std::error::Error = &err;
}

#[test]
/// 6. can_transition_to is a pure function (no mutation).
///
fn can_transition_to_is_pure_function() {
    for &from in ALL_STATES {
        for &to in ALL_STATES {
            let state_before = from;
            let _result = from.can_transition_to(&to);
            assert_eq!(from, state_before, "can_transition_to must not mutate self");
        }
    }
}

#[test]
fn can_transition_to_matches_transition_to_for_valid() {
    for &(from, to) in VALID_TRANSITIONS {
        assert!(
            from.can_transition_to(&to),
            "can_transition_to disagrees with transition_to for valid pair"
        );
    }
}

#[test]
fn can_transition_to_matches_transition_to_for_invalid() {
    for &from in ALL_STATES {
        for &to in ALL_STATES {
            if is_valid_transition(from, to) {
                continue;
            }
            assert!(
                !from.can_transition_to(&to),
                "can_transition_to should return false for invalid pair {:?} -> {:?}",
                from,
                to
            );
        }
    }
}

#[test]
/// 7. Display trait: lowercase snake_case.
///
fn display_idle() {
    assert_eq!(AgentFsmState::Idle.to_string(), "idle");
}

#[test]
fn display_user_turn_prepared() {
    assert_eq!(
        AgentFsmState::UserTurnPrepared.to_string(),
        "user_turn_prepared"
    );
}

#[test]
fn display_llm_streaming() {
    assert_eq!(AgentFsmState::LlmStreaming.to_string(), "llm_streaming");
}

#[test]
fn display_llm_committed() {
    assert_eq!(AgentFsmState::LlmCommitted.to_string(), "llm_committed");
}

#[test]
fn display_tool_requested() {
    assert_eq!(AgentFsmState::ToolRequested.to_string(), "tool_requested");
}

#[test]
fn display_tool_result_processed() {
    assert_eq!(
        AgentFsmState::ToolResultProcessed.to_string(),
        "tool_result_processed"
    );
}

#[test]
fn display_final() {
    assert_eq!(AgentFsmState::Final.to_string(), "final");
}

#[test]
fn serialization_roundtrip_for_all_states() {
    for &state in ALL_STATES {
        let json = serde_json::to_string(&state).expect("serialize should succeed");
        let deserialized: AgentFsmState =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(state, deserialized, "round-trip failed for {:?}", state);
    }
}

#[test]
fn serialization_uses_snake_case_strings() {
    let cases = &[
        (AgentFsmState::Idle, "\"idle\""),
        (AgentFsmState::UserTurnPrepared, "\"user_turn_prepared\""),
        (AgentFsmState::LlmStreaming, "\"llm_streaming\""),
        (AgentFsmState::LlmCommitted, "\"llm_committed\""),
        (AgentFsmState::ToolRequested, "\"tool_requested\""),
        (
            AgentFsmState::ToolResultProcessed,
            "\"tool_result_processed\"",
        ),
        (AgentFsmState::Final, "\"final\""),
    ];
    for &(state, expected_json) in cases {
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json, expected_json,
            "JSON for {:?} should be {}",
            state, expected_json
        );
    }
}

#[test]
fn deserialization_rejects_unknown_variant() {
    let bad_variants = &[
        "\"unknown\"",
        "\"IDLE\"",
        "\"Idle\"",
        "\"idle_state\"",
        "\"running\"",
        "\"\"",
    ];
    for variant in bad_variants {
        let result = serde_json::from_str::<AgentFsmState>(variant);
        assert!(
            result.is_err(),
            "deserialization of {} should fail",
            variant
        );
    }
}

#[test]
fn deserialization_rejects_non_string() {
    let bad_inputs = &["null", "42", "true", "{}", "[]"];
    for input in bad_inputs {
        let result = serde_json::from_str::<AgentFsmState>(input);
        assert!(result.is_err(), "deserialization of {} should fail", input);
    }
}

#[test]
/// 9. AgentFsmState traits: Debug, Clone, Copy, PartialEq, Eq, Hash.
///
/// Should compile and produce non-empty output.
///
fn state_is_debug() {
    let dbg = format!("{:?}", AgentFsmState::Idle);
    assert!(!dbg.is_empty());
}

#[test]
fn state_is_clone_and_copy() {
    let a = AgentFsmState::LlmStreaming;
    let b = a;
    let c = a;
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn state_equality() {
    assert_eq!(AgentFsmState::Idle, AgentFsmState::Idle);
    assert_ne!(AgentFsmState::Idle, AgentFsmState::Final);
    assert_ne!(
        AgentFsmState::ToolRequested,
        AgentFsmState::ToolResultProcessed
    );
}

#[test]
fn state_is_hash() {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    map.insert(AgentFsmState::Idle, "idle_value");
    assert_eq!(map.get(&AgentFsmState::Idle), Some(&"idle_value"));
    assert_eq!(map.get(&AgentFsmState::Final), None);
}

#[test]
/// 10. Full agent loop cycle: tool call path.
///
/// LLM responds with tool call.
///
/// Loop back for next LLM call.
///
fn full_tool_call_loop_cycle() {
    let mut state = AgentFsmState::initial();
    assert_eq!(state, AgentFsmState::Idle);

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();

    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::ToolRequested).unwrap();

    state
        .transition_to(AgentFsmState::ToolResultProcessed)
        .unwrap();

    state.transition_to(AgentFsmState::LlmStreaming).unwrap();

    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::Final).unwrap();

    state.transition_to(AgentFsmState::Idle).unwrap();
    assert_eq!(state, AgentFsmState::Idle);
}

#[test]
/// 11. Full agent loop: retry from LlmCommitted.
///
/// LLM responds but wants retry.
///
fn full_tool_call_loop_with_retry_from_llm_committed() {
    let mut state = AgentFsmState::initial();

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();

    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::Idle).unwrap();

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();
    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::Final).unwrap();
    state.transition_to(AgentFsmState::Idle).unwrap();

    assert_eq!(state, AgentFsmState::Idle);
}

#[test]
/// 12. Full agent loop: retry from ToolResultProcessed.
///
/// Tool call + tool result, but retry.
///
fn full_tool_call_loop_with_retry_from_tool_result_processed() {
    let mut state = AgentFsmState::initial();

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();

    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::ToolRequested).unwrap();
    state
        .transition_to(AgentFsmState::ToolResultProcessed)
        .unwrap();
    state.transition_to(AgentFsmState::Idle).unwrap();

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();
    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::Final).unwrap();
    state.transition_to(AgentFsmState::Idle).unwrap();

    assert_eq!(state, AgentFsmState::Idle);
}

#[test]
/// 13. Multiple tool call rounds in sequence.
///
fn full_tool_call_loop_with_two_rounds() {
    let mut state = AgentFsmState::initial();

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();

    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::ToolRequested).unwrap();
    state
        .transition_to(AgentFsmState::ToolResultProcessed)
        .unwrap();

    state.transition_to(AgentFsmState::LlmStreaming).unwrap();
    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::ToolRequested).unwrap();
    state
        .transition_to(AgentFsmState::ToolResultProcessed)
        .unwrap();

    state.transition_to(AgentFsmState::LlmStreaming).unwrap();
    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::Final).unwrap();
    state.transition_to(AgentFsmState::Idle).unwrap();

    assert_eq!(state, AgentFsmState::Idle);
}

#[test]
/// 14. Direct-to-final path (LLM commits without tool call).
///
fn direct_final_from_llm_committed() {
    let mut state = AgentFsmState::initial();

    state
        .transition_to(AgentFsmState::UserTurnPrepared)
        .unwrap();
    state.transition_to(AgentFsmState::LlmStreaming).unwrap();
    state.transition_to(AgentFsmState::LlmCommitted).unwrap();
    state.transition_to(AgentFsmState::Final).unwrap();
    state.transition_to(AgentFsmState::Idle).unwrap();

    assert_eq!(state, AgentFsmState::Idle);
}

#[test]
/// 15. Rejected transitions from each state (spot checks).
///
fn idle_rejects_all_except_user_turn_prepared() {
    let mut state = AgentFsmState::Idle;
    for &target in ALL_STATES {
        if target == AgentFsmState::UserTurnPrepared {
            assert!(state.transition_to(target).is_ok());
            state = AgentFsmState::Idle;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(result.is_err(), "Idle -> {:?} should be rejected", target);
            assert_eq!(state, before, "state must not change on rejection");
        }
    }
}

#[test]
fn final_only_allows_transition_to_idle() {
    let mut state = AgentFsmState::Final;
    for &target in ALL_STATES {
        if target == AgentFsmState::Idle {
            assert!(state.transition_to(target).is_ok());
            state = AgentFsmState::Final;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(result.is_err(), "Final -> {:?} should be rejected", target);
            assert_eq!(state, before);
        }
    }
}

#[test]
fn tool_requested_only_allows_tool_result_processed() {
    let mut state = AgentFsmState::ToolRequested;
    for &target in ALL_STATES {
        if target == AgentFsmState::ToolResultProcessed {
            assert!(state.transition_to(target).is_ok());
            state = AgentFsmState::ToolRequested;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(
                result.is_err(),
                "ToolRequested -> {:?} should be rejected",
                target
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn llm_streaming_only_allows_llm_committed() {
    let mut state = AgentFsmState::LlmStreaming;
    for &target in ALL_STATES {
        if target == AgentFsmState::LlmCommitted {
            assert!(state.transition_to(target).is_ok());
            state = AgentFsmState::LlmStreaming;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(
                result.is_err(),
                "LlmStreaming -> {:?} should be rejected",
                target
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn user_turn_prepared_only_allows_llm_streaming() {
    let mut state = AgentFsmState::UserTurnPrepared;
    for &target in ALL_STATES {
        if target == AgentFsmState::LlmStreaming {
            assert!(state.transition_to(target).is_ok());
            state = AgentFsmState::UserTurnPrepared;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(
                result.is_err(),
                "UserTurnPrepared -> {:?} should be rejected",
                target
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn llm_committed_allows_three_targets() {
    let valid_targets = &[
        AgentFsmState::ToolRequested,
        AgentFsmState::Final,
        AgentFsmState::Idle,
    ];
    let mut state = AgentFsmState::LlmCommitted;
    for &target in ALL_STATES {
        if valid_targets.contains(&target) {
            let result = state.transition_to(target);
            assert!(
                result.is_ok(),
                "LlmCommitted -> {:?} should be accepted",
                target
            );
            state = AgentFsmState::LlmCommitted;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(
                result.is_err(),
                "LlmCommitted -> {:?} should be rejected",
                target
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn tool_result_processed_allows_three_targets() {
    let valid_targets = &[
        AgentFsmState::LlmStreaming,
        AgentFsmState::Final,
        AgentFsmState::Idle,
    ];
    let mut state = AgentFsmState::ToolResultProcessed;
    for &target in ALL_STATES {
        if valid_targets.contains(&target) {
            let result = state.transition_to(target);
            assert!(
                result.is_ok(),
                "ToolResultProcessed -> {:?} should be accepted",
                target
            );
            state = AgentFsmState::ToolResultProcessed;
        } else {
            let before = state;
            let result = state.transition_to(target);
            assert!(
                result.is_err(),
                "ToolResultProcessed -> {:?} should be rejected",
                target
            );
            assert_eq!(state, before);
        }
    }
}
