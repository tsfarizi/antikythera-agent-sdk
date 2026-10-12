#![allow(clippy::module_inception)]

use crate::wasm_agent::runner::AgentRunnerRuntime;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Verify that AgentRunnerRuntime can be default-constructed.
    ///
    fn test_runner_facade_has_all_methods() {
        let rt = AgentRunnerRuntime::default();
        assert!(rt.sessions.is_empty());
        assert!(rt.archived_sessions.is_empty());
        assert!(rt.pending_events.is_empty());
    }

    #[test]
    fn test_runner_default_config() {
        let rt = AgentRunnerRuntime::default();
        assert_eq!(rt.max_in_memory_sessions, 128);
        assert!(rt.known_tools.is_empty());
    }

    #[test]
    /// Verify session_lifecycle module is accessible (compilation check) If this compiles, the
    /// module and its impl blocks are linked.
    ///
    fn test_session_lifecycle_module_accessible() {
        let _rt = AgentRunnerRuntime::default();
    }

    #[test]
    /// Verify llm_stream module is accessible (compilation check).
    ///
    fn test_llm_stream_module_accessible() {
        let _rt = AgentRunnerRuntime::default();
    }

    #[test]
    /// Verify tool_pipeline module is accessible (compilation check).
    ///
    fn test_tool_pipeline_module_accessible() {
        let _rt = AgentRunnerRuntime::default();
    }

    #[test]
    fn test_new_session_id_is_unique() {
        use crate::wasm_agent::runner::new_session_id;
        let id1 = new_session_id();
        let id2 = new_session_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("session-"));
        assert!(id2.starts_with("session-"));
    }

    #[test]
    fn test_now_unix_ms_returns_positive() {
        use crate::wasm_agent::runner::now_unix_ms;
        let ts = now_unix_ms();
        assert!(ts > 0);
    }

    #[test]
    fn test_agent_runner_error_display() {
        use crate::wasm_agent::runner::AgentRunnerError;
        let err = AgentRunnerError::SessionNotFound("test-123".to_string());
        assert_eq!(err.to_string(), "Session not found: test-123");

        let err = AgentRunnerError::ToolFailed("bad input".to_string());
        assert_eq!(err.to_string(), "Tool failed: bad input");

        let err = AgentRunnerError::Internal("oops".to_string());
        assert_eq!(err.to_string(), "Internal error: oops");
    }

    #[test]
    fn test_agent_runner_error_from_string() {
        use crate::wasm_agent::runner::AgentRunnerError;
        let err: AgentRunnerError = "some error".to_string().into();
        assert!(matches!(err, AgentRunnerError::Internal(_)));
    }

    /// Â”€â”€ runtime-hooks: pure classification/precedence/merge contract â”€â”€â”€â”€â”€â”€â”€
    /// Only the pure functions of `runner::runtime_hooks` are exercised here; the wit-bindgen
    /// shim callers are cfg-gated off native builds.
    ///
    use crate::wasm_agent::runner::runtime_hooks::{
        HookDecision, classify_decision, merge_json_object, resolve_precedence,
    };

    #[test]
    fn test_runtime_hooks_classify_passthrough() {
        let decision =
            classify_decision("prepare-turn", Ok(r#"{"passthrough": true}"#.to_string()));
        assert_eq!(decision, HookDecision::Passthrough);
    }

    #[test]
    fn test_runtime_hooks_classify_passthrough_with_extra_key_is_override() {
        let decision = classify_decision(
            "prepare-turn",
            Ok(r#"{"passthrough": true, "prompt": "x"}"#.to_string()),
        );
        assert!(matches!(decision, HookDecision::Override(_)));
    }

    #[test]
    fn test_runtime_hooks_classify_override() {
        let decision = classify_decision("prepare-turn", Ok(r#"{"prompt": "custom"}"#.to_string()));
        match decision {
            HookDecision::Override(value) => {
                assert_eq!(value, serde_json::json!({"prompt": "custom"}));
            }
            other => panic!("expected override, got {other:?}"),
        }
    }

    #[test]
    fn test_runtime_hooks_classify_unparseable_is_error() {
        let decision = classify_decision("prepare-turn", Ok("not json".to_string()));
        assert!(matches!(decision, HookDecision::Failed(_)));
    }

    #[test]
    fn test_runtime_hooks_classify_non_object_is_error() {
        let decision = classify_decision("prepare-turn", Ok(r#"[1, 2, 3]"#.to_string()));
        assert!(matches!(decision, HookDecision::Failed(_)));
    }

    #[test]
    fn test_runtime_hooks_classify_err_is_error() {
        let decision = classify_decision("prepare-turn", Err("boom".to_string()));
        assert!(matches!(decision, HookDecision::Failed(_)));
    }

    #[test]
    fn test_runtime_hooks_precedence_composed_passthrough_calls_runtime() {
        let mut runtime_called = false;
        let decision = resolve_precedence(true, HookDecision::Passthrough, || {
            runtime_called = true;
            HookDecision::Override(serde_json::json!({"prompt": "from runtime"}))
        })
        .expect("resolve should not fail");
        assert!(runtime_called);
        assert_eq!(
            decision,
            Some(serde_json::json!({"prompt": "from runtime"}))
        );
    }

    #[test]
    fn test_runtime_hooks_precedence_composed_override_skips_runtime() {
        let mut runtime_called = false;
        let decision = resolve_precedence(
            true,
            HookDecision::Override(serde_json::json!({"prompt": "from composed"})),
            || {
                runtime_called = true;
                HookDecision::Override(serde_json::json!({"prompt": "from runtime"}))
            },
        )
        .expect("resolve should not fail");
        assert!(
            !runtime_called,
            "runtime must not be consulted on composed override"
        );
        assert_eq!(
            decision,
            Some(serde_json::json!({"prompt": "from composed"}))
        );
    }

    #[test]
    fn test_runtime_hooks_precedence_composed_error_aborts() {
        let mut runtime_called = false;
        let err = resolve_precedence(
            true,
            HookDecision::Failed("logic-hook prepare-turn failed: boom".to_string()),
            || {
                runtime_called = true;
                HookDecision::Passthrough
            },
        )
        .expect_err("composed failure must abort");
        assert!(
            !runtime_called,
            "runtime must not be consulted on composed failure"
        );
        assert!(err.to_string().contains("logic-hook prepare-turn failed"));
    }

    #[test]
    fn test_runtime_hooks_disabled_skips_runtime() {
        let mut runtime_called = false;
        let decision = resolve_precedence(false, HookDecision::Passthrough, || {
            runtime_called = true;
            HookDecision::Override(serde_json::json!({"prompt": "from runtime"}))
        })
        .expect("resolve should not fail");
        assert!(!runtime_called, "runtime must not be called when disabled");
        assert_eq!(decision, None);
    }

    #[test]
    fn test_runtime_hooks_runtime_passthrough_keeps_default() {
        let decision = resolve_precedence(true, HookDecision::Passthrough, || {
            HookDecision::Passthrough
        })
        .expect("resolve should not fail");
        assert_eq!(decision, None);
    }

    #[test]
    fn test_runtime_hooks_runtime_error_aborts() {
        let err = resolve_precedence(true, HookDecision::Passthrough, || {
            HookDecision::Failed("runtime-hook prepare-turn failed: boom".to_string())
        })
        .expect_err("runtime failure must abort");
        assert!(err.to_string().contains("runtime-hook prepare-turn failed"));
    }

    #[test]
    fn test_runtime_hooks_merge_json_object() {
        let merged = merge_json_object(
            "prepare-turn",
            &serde_json::json!({"a": 1, "b": 2}),
            &serde_json::json!({"b": 3, "c": 4}),
        )
        .expect("merge should succeed");
        assert_eq!(merged, serde_json::json!({"a": 1, "b": 3, "c": 4}));
    }
}
