use super::*;

#[test]
fn postback_unknown_correlation_is_ignored() {
    let control = ControlChannel::new();
    let body = PostbackBody {
        correlation_id: "unknown".to_string(),
        ok: true,
        payload: Value::Null,
        error: None,
    };
    assert!(!control.complete_postback(body));
    assert_eq!(control.pending_len(), 0);
}

#[test]
fn postback_completes_registered_pending() {
    let control = ControlChannel::new();
    let mut rx = control.register_pending(
        "corr-1".to_string(),
        PendingKind::Hook,
        Duration::from_secs(60),
    );
    assert_eq!(control.pending_len(), 1);
    let completed = control.complete_postback(PostbackBody {
        correlation_id: "corr-1".to_string(),
        ok: true,
        payload: json!({"passthrough": true}),
        error: None,
    });
    assert!(completed);
    assert_eq!(control.pending_len(), 0);
    let body = rx.try_recv().unwrap();
    assert!(body.ok);
    assert_eq!(body.payload, json!({"passthrough": true}));
}

#[test]
fn cancel_pending_removes_entry() {
    let control = ControlChannel::new();
    let _rx = control.register_pending(
        "corr-2".to_string(),
        PendingKind::Tool,
        Duration::from_secs(60),
    );
    control.cancel_pending("corr-2");
    assert_eq!(control.pending_len(), 0);
}

#[test]
fn push_delivers_only_to_registered_client() {
    let control = ControlChannel::new();
    assert!(!control.is_client_connected("client-a"));
    let _rx = control.register_client("client-a");
    assert!(control.is_client_connected("client-a"));
    assert!(!control.is_client_connected("client-b"));
}
