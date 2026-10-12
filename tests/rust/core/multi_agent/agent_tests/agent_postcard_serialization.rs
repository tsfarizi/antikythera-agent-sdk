use antikythera_core::application::agent::AgentStateSnapshot;

#[test]
fn test_json_serialization() {
    let state = AgentStateSnapshot::new("test".into(), "agent".into());

    let json = state.to_json().unwrap();

    let loaded = AgentStateSnapshot::from_json(&json).unwrap();

    assert_eq!(loaded.context_id, state.context_id);
    assert_eq!(loaded.agent_id, state.agent_id);
    assert_eq!(loaded.schema_version, state.schema_version);
}
