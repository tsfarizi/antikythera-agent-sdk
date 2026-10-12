use super::*;
use crate::application::agent::models::AgentStep;

#[test]
fn test_domain_event_serialization_roundtrip_agent_run_started() {
    let event = DomainEvent::AgentRunStarted {
        session_id: Some("sess-123".into()),
        prompt: "hello".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
    match deserialized {
        DomainEvent::AgentRunStarted { session_id, prompt } => {
            assert_eq!(session_id.as_deref(), Some("sess-123"));
            assert_eq!(prompt, "hello");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_domain_event_serialization_roundtrip_step_completed() {
    let step = AgentStep {
        tool: "read_file".into(),
        input: serde_json::json!({"path": "/tmp"}),
        success: true,
        output: serde_json::json!("contents"),
        message: None,
    };
    let event = DomainEvent::AgentStepCompleted {
        step,
        remaining_steps: 3,
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
    match deserialized {
        DomainEvent::AgentStepCompleted {
            step,
            remaining_steps,
        } => {
            assert_eq!(step.tool, "read_file");
            assert_eq!(remaining_steps, 3);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_domain_event_serialization_roundtrip_tool_invoked() {
    let event = DomainEvent::ToolInvoked {
        tool: "bash".into(),
        input: serde_json::json!({"command": "ls"}),
        success: true,
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
    match deserialized {
        DomainEvent::ToolInvoked {
            tool,
            input,
            success,
        } => {
            assert_eq!(tool, "bash");
            assert_eq!(input, serde_json::json!({"command": "ls"}));
            assert!(success);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_domain_event_serialization_roundtrip_run_completed() {
    let event = DomainEvent::AgentRunCompleted {
        session_id: "sess-abc".into(),
        response: "done".into(),
        total_steps: 5,
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
    match deserialized {
        DomainEvent::AgentRunCompleted {
            session_id,
            response,
            total_steps,
        } => {
            assert_eq!(session_id, "sess-abc");
            assert_eq!(response, "done");
            assert_eq!(total_steps, 5);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_domain_event_serialization_roundtrip_session_updated() {
    let event = DomainEvent::SessionUpdated {
        session_id: "sess-xyz".into(),
        message_count: 12,
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
    match deserialized {
        DomainEvent::SessionUpdated {
            session_id,
            message_count,
        } => {
            assert_eq!(session_id, "sess-xyz");
            assert_eq!(message_count, 12);
        }
        _ => panic!("wrong variant"),
    }
}
