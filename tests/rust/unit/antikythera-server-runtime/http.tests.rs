use super::*;

#[test]
/// `client_id` has no default: missing it is a deserialization error.
///
fn events_query_requires_client_id() {
    let query: Result<EventsQuery, _> = serde_urlencoded::from_str("session_id=s1");
    assert!(query.is_err());
    let query: EventsQuery = serde_urlencoded::from_str("client_id=c1&session_id=s1").unwrap();
    assert_eq!(query.client_id, "c1");
    assert_eq!(query.session_id.as_deref(), Some("s1"));
}

#[test]
fn llm_call_query_stream_defaults_false() {
    let query: LlmCallQuery = serde_urlencoded::from_str("").unwrap();
    assert!(!query.stream);
    let query: LlmCallQuery = serde_urlencoded::from_str("stream=true").unwrap();
    assert!(query.stream);
}
