use super::*;

fn def(name: &str) -> ToolDefinition {
    ToolDefinition::simple(name, "test tool")
}

#[test]
/// The original owner is untouched.
///
fn cross_side_collision_is_rejected() {
    let mut registry = UnionRegistry::new();
    registry
        .register(ToolOwner::Server, def("get_current_time"))
        .unwrap();
    let err = registry
        .register(ToolOwner::Client, def("get_current_time"))
        .unwrap_err();
    assert_eq!(
        err,
        "tool registry: name collision for tool 'get_current_time' (owners server, client)"
    );
    let err = registry
        .register(ToolOwner::Mcp, def("get_current_time"))
        .unwrap_err();
    assert_eq!(
        err,
        "tool registry: name collision for tool 'get_current_time' (owners server, mcp)"
    );
    assert_eq!(
        registry.owner_of("get_current_time"),
        Some(ToolOwner::Server)
    );
}

#[test]
fn same_owner_reregistration_replaces() {
    let mut registry = UnionRegistry::new();
    registry.register(ToolOwner::Server, def("echo")).unwrap();
    let updated = ToolDefinition {
        name: "echo".to_string(),
        title: Some("New Echo".to_string()),
        description: "updated".to_string(),
        parameters: Vec::new(),
        input_schema: None,
        output_schema: None,
    };
    registry.register(ToolOwner::Server, updated).unwrap();
    let defs = registry.definitions_for(ToolOwner::Server);
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].title.as_deref(), Some("New Echo"));
}

#[test]
fn owner_resolution_and_union() {
    let mut registry = UnionRegistry::new();
    registry.register(ToolOwner::Server, def("a")).unwrap();
    registry.register(ToolOwner::Client, def("b")).unwrap();
    registry.register(ToolOwner::Mcp, def("c")).unwrap();

    assert_eq!(registry.owner_of("a"), Some(ToolOwner::Server));
    assert_eq!(registry.owner_of("b"), Some(ToolOwner::Client));
    assert_eq!(registry.owner_of("c"), Some(ToolOwner::Mcp));
    assert_eq!(registry.owner_of("nope"), None);

    let union = registry.definitions();
    assert_eq!(
        union.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(registry.definitions_for(ToolOwner::Server).len(), 1);
    assert_eq!(registry.definitions_for(ToolOwner::Client).len(), 1);
    assert_eq!(registry.definitions_for(ToolOwner::Mcp).len(), 1);
}

#[test]
fn destination_from_owner() {
    assert_eq!(Destination::from(ToolOwner::Server), Destination::Local);
    assert_eq!(Destination::from(ToolOwner::Client), Destination::Remote);
    assert_eq!(Destination::from(ToolOwner::Mcp), Destination::Mcp);
}
