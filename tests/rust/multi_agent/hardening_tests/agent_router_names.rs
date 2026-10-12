

#[test]
/// AgentRouter name() and routing_reason() introspection.
///
fn router_name_returns_correct_strings() {
    use antikythera_core::application::agent::multi_agent::AgentRouter;
    use antikythera_core::application::agent::multi_agent::router::{
        DirectRouter, FirstAvailableRouter, RoleRouter, RoundRobinRouter,
    };

    assert_eq!(DirectRouter.name(), "direct");
    assert_eq!(RoundRobinRouter::new().name(), "round-robin");
    assert_eq!(FirstAvailableRouter.name(), "first-available");
    assert_eq!(RoleRouter::new("executor").name(), "role");
}
