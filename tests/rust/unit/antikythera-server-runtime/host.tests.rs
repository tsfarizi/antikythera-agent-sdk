use super::*;

#[test]
fn context_id_rejects_traversal() {
    for evil in ["../evil", ".", "a/../b", "C:\\evil", "a:b", "", "x\0y"] {
        let err = validate_context_id(evil).unwrap_err();
        assert!(err.starts_with("permission: "), "id {evil:?} -> {err}");
        assert_eq!(err, "permission: invalid context id");
    }
    for ok in ["session-1", "context_123", "a.b"] {
        assert!(validate_context_id(ok).is_ok(), "id {ok:?} should be valid");
    }
}
