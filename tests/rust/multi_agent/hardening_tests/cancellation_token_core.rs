

#[test]
fn cancellation_token_new_is_not_cancelled() {
    use antikythera_core::application::agent::multi_agent::cancellation::CancellationToken;
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn cancellation_token_cancel_sets_flag() {
    use antikythera_core::application::agent::multi_agent::cancellation::CancellationToken;
    let token = CancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
/// Cancelling parent is visible through child.
///
fn cancellation_token_child_shares_flag_with_parent() {
    use antikythera_core::application::agent::multi_agent::cancellation::CancellationToken;
    let parent = CancellationToken::new();
    let child = parent.child_token();
    parent.cancel();
    assert!(
        child.is_cancelled(),
        "child must share the cancellation flag"
    );
}

#[test]
/// Cancelling via child is visible on parent.
///
fn cancellation_token_child_can_cancel_parent_flag() {
    use antikythera_core::application::agent::multi_agent::cancellation::CancellationToken;
    let parent = CancellationToken::new();
    let child = parent.child_token();
    child.cancel();
    assert!(
        parent.is_cancelled(),
        "cancelling child must cancel the shared flag"
    );
}

#[test]
fn cancellation_snapshot_serde_roundtrip() {
    use antikythera_core::application::agent::multi_agent::cancellation::{
        CancellationSnapshot, CancellationToken,
    };
    let token = CancellationToken::new();
    token.cancel();
    let snap = CancellationSnapshot::from(&token);
    let json = serde_json::to_string(&snap).expect("serialize");
    let restored: CancellationSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert!(restored.was_cancelled);
}
