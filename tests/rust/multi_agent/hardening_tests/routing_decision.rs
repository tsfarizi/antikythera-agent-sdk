

#[test]
fn routing_decision_embedded_in_metadata() {
    let decision = RoutingDecision {
        router_name: "round-robin".to_string(),
        selected_agent_id: "agent-42".to_string(),
        candidates_considered: 3,
        reason: Some("round-robin selected agent-42".to_string()),
    };
    let meta = TaskExecutionMetadata {
        routing_decision: Some(decision.clone()),
        ..TaskExecutionMetadata::default()
    };
    let rd = meta.routing_decision.as_ref().unwrap();
    assert_eq!(rd.router_name, "round-robin");
    assert_eq!(rd.selected_agent_id, "agent-42");
    assert_eq!(rd.candidates_considered, 3);
    assert!(rd.reason.as_deref().unwrap().contains("round-robin"));
}

#[test]
fn routing_decision_serde_roundtrip() {
    let decision = RoutingDecision {
        router_name: "role".to_string(),
        selected_agent_id: "planner".to_string(),
        candidates_considered: 5,
        reason: Some("role='planner' matched agent_id='planner'".to_string()),
    };
    let json = serde_json::to_string(&decision).expect("serialize");
    let restored: RoutingDecision = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.router_name, "role");
    assert_eq!(restored.selected_agent_id, "planner");
    assert_eq!(restored.candidates_considered, 5);
}
