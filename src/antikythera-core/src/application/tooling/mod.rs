pub use antikythera_tooling::{
    EnvelopeError, McpTransport, PROTOCOL_VERSION, ServerInstance, ServerManager, ServerToolInfo,
    TaskSupport, ToolAnnotations, ToolCallEnvelope, ToolExecution, ToolIcon, ToolInvokeError,
    ToolResultEnvelope, ToolServerInterface, TransportFactory, validate_tool_call_envelope,
    validate_tool_result_envelope,
};

/// Submodule re-exports preserved for backward compatibility.
///
pub use antikythera_tooling::error;
pub use antikythera_tooling::interface;
pub use antikythera_tooling::manager;
pub use antikythera_tooling::transport;
