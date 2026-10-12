use antikythera_core::application::agent::multi_agent::{CancellationSnapshot, CancellationToken};

#[test]
fn new_token_is_not_cancelled() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn cancel_sets_flag() {
    let token = CancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn child_token_shares_flag() {
    let root = CancellationToken::new();
    let child = root.child_token();
    assert!(!child.is_cancelled());
    root.cancel();
    assert!(
        child.is_cancelled(),
        "child must observe parent cancellation"
    );
}

#[test]
fn clone_shares_flag() {
    let token = CancellationToken::new();
    let cloned = token.clone();
    token.cancel();
    assert!(cloned.is_cancelled());
}

#[test]
fn cancellation_snapshot_reflects_state() {
    let token = CancellationToken::new();
    let snap = CancellationSnapshot::from(&token);
    assert!(!snap.was_cancelled);
    token.cancel();
    let snap2 = CancellationSnapshot::from(&token);
    assert!(snap2.was_cancelled);
}
