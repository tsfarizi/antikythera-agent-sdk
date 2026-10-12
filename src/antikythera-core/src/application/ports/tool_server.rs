//! Port: Tool Server
//!
//! Application defines this trait. Infrastructure (transport adapters) implements it.

/// Re-export the existing trait — it's already correctly placed in application.
///
pub use crate::application::tooling::{
    PROTOCOL_VERSION, ServerToolInfo, TaskSupport, ToolAnnotations, ToolExecution, ToolIcon,
    ToolServerInterface,
};
