use super::context::SessionContext;
use super::registry::get_logger;
use antikythera_log::{LogLevel, Logger};
use std::sync::Arc;

macro_rules! define_module_logger {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident => $source:literal
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug)]
        $vis struct $name {
            logger: Arc<Logger>,
        }

        impl $name {
            pub fn new(session_id: &str) -> Self {
                Self {
                    logger: get_logger(session_id),
                }
            }

            pub fn from_context(ctx: &SessionContext) -> Self {
                Self::new(ctx.session_id())
            }

            pub fn debug(&self, message: impl Into<String>) {
                self.logger.log_with_source(LogLevel::Debug, $source, message);
            }

            pub fn info(&self, message: impl Into<String>) {
                self.logger.log_with_source(LogLevel::Info, $source, message);
            }

            pub fn warn(&self, message: impl Into<String>) {
                self.logger.log_with_source(LogLevel::Warn, $source, message);
            }

            pub fn error(&self, message: impl Into<String>) {
                self.logger.log_with_source(LogLevel::Error, $source, message);
            }
        }
    };
}

define_module_logger! {
    /// Config module logger
    pub struct ConfigLogger => "config"
}

define_module_logger! {
    /// Agent module logger — covers FSM runner, agent runner, parser, context
    pub struct AgentLogger => "agent"
}

impl AgentLogger {
    /// Log a tool call with structured context
    pub fn tool_call(&self, tool: &str, args: &serde_json::Value) {
        let context = format!("{{\"tool\": \"{}\", \"args\": {}}}", tool, args);
        self.logger
            .log_with_context(LogLevel::Info, format!("Tool call: {}", tool), context)
    }

    /// Log a tool execution result
    pub fn tool_result(&self, tool: &str, success: bool, step: u32) {
        let level = if success {
            LogLevel::Info
        } else {
            LogLevel::Error
        };
        let context = format!(
            "{{\"tool\": \"{}\", \"success\": {}, \"step\": {}}}",
            tool, success, step
        );
        self.logger.log_with_context(
            level,
            format!("Tool result: {} (step {})", tool, step),
            context,
        )
    }

    /// Log an agent step (debug level)
    pub fn agent_step(&self, step: u32, max_steps: u32) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "agent",
            format!("Agent step {}/{}", step, max_steps),
        )
    }

    /// Log agent completion
    pub fn agent_complete(&self, steps: u32) {
        self.logger.log_with_source(
            LogLevel::Info,
            "agent",
            format!("Agent completed in {} steps", steps),
        )
    }
}

define_module_logger! {
    /// Transport module logger — covers HTTP, SSE, RPC, process management
    pub struct TransportLogger => "transport"
}

impl TransportLogger {
    pub fn connect(&self, server: &str) {
        self.logger.log_with_source(
            LogLevel::Info,
            "transport",
            format!("Connecting to: {}", server),
        );
    }

    pub fn disconnect(&self, server: &str) {
        self.logger.log_with_source(
            LogLevel::Info,
            "transport",
            format!("Disconnected from: {}", server),
        );
    }

    pub fn tool_request(&self, server: &str, tool: &str) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "transport",
            format!("Tool request: {}.{} ", server, tool),
        );
    }

    pub fn tool_response(&self, server: &str, tool: &str, success: bool) {
        let level = if success {
            LogLevel::Debug
        } else {
            LogLevel::Error
        };
        self.logger.log_with_source(
            level,
            "transport",
            format!("Tool response: {}.{} (success: {})", server, tool, success),
        );
    }
}

define_module_logger! {
    /// Provider module logger — covers model provider API calls
    pub struct ProviderLogger => "provider"
}

impl ProviderLogger {
    pub fn api_call(&self, provider: &str, model: &str) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "provider",
            format!("API call: {} ({})", provider, model),
        );
    }

    pub fn api_response(&self, provider: &str, model: &str, tokens: Option<u32>) {
        let token_info = tokens
            .map(|t| format!(", {} tokens", t))
            .unwrap_or_default();
        self.logger.log_with_source(
            LogLevel::Debug,
            "provider",
            format!("API response: {} ({}){}", provider, model, token_info),
        );
    }

    pub fn api_error(&self, provider: &str, error: &str) {
        self.logger.log_with_source(
            LogLevel::Error,
            "provider",
            format!("API error: {} ({})", provider, error),
        );
    }
}

define_module_logger! {
    /// Discovery module logger — covers server discovery, scanning, loading
    pub struct DiscoveryLogger => "discovery"
}

define_module_logger! {
    /// STDIO module logger — covers STDIO command processing
    pub struct StdioLogger => "stdio"
}

define_module_logger! {
    /// Chat service module logger
    pub struct ChatLogger => "chat"
}

define_module_logger! {
    /// WASM runtime module logger
    pub struct WasmLogger => "wasm"
}

define_module_logger! {
    /// Resilience module logger — covers retry, circuit breaker, etc.
    pub struct ResilienceLogger => "resilience"
}

define_module_logger! {
    /// Orchestrator module logger — covers multi-agent orchestration
    pub struct OrchestratorLogger => "orchestrator"
}

define_module_logger! {
    /// Streaming module logger — covers LLM streaming
    pub struct StreamingLogger => "streaming"
}

impl StreamingLogger {
    /// Log a streaming session start
    pub fn session_start(&self, mode: &str, session_id: &str) {
        self.logger.log_with_source(
            LogLevel::Info,
            "streaming",
            format!(
                "Streaming session started | mode={} session_id={}",
                mode, session_id
            ),
        );
    }

    /// Log a streaming session end
    pub fn session_end(&self, session_id: &str, total_events: usize) {
        self.logger.log_with_source(
            LogLevel::Info,
            "streaming",
            format!(
                "Streaming session ended | session_id={} total_events={}",
                session_id, total_events
            ),
        );
    }

    /// Log a token stream
    pub fn token_emitted(&self, session_id: &str, content_len: usize) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "streaming",
            format!(
                "Token emitted | session_id={} content_len={}",
                session_id, content_len
            ),
        );
    }

    /// Log buffer flush
    pub fn buffer_flushed(&self, session_id: &str, event_count: usize) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "streaming",
            format!(
                "Buffer flushed | session_id={} event_count={}",
                session_id, event_count
            ),
        );
    }

    /// Log buffer overflow
    pub fn buffer_overflow(&self, session_id: &str, dropped: usize) {
        self.logger.log_with_source(
            LogLevel::Warn,
            "streaming",
            format!(
                "Buffer overflow | session_id={} dropped={}",
                session_id, dropped
            ),
        );
    }

    /// Log a tool event
    pub fn tool_event(&self, session_id: &str, tool_name: &str, phase: &str) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "streaming",
            format!(
                "Tool event | session_id={} tool={} phase={}",
                session_id, tool_name, phase
            ),
        );
    }

    /// Log a streaming error
    pub fn stream_error(&self, session_id: &str, error: &str) {
        self.logger.log_with_source(
            LogLevel::Error,
            "streaming",
            format!("Stream error | session_id={} error={}", session_id, error),
        );
    }
}

define_module_logger! {
    /// Observability module logger — covers telemetry, metrics, audit, tracing
    pub struct ObservabilityLogger => "observability"
}

define_module_logger! {
    /// Security module logger — covers rate limiting, secrets, validation
    pub struct SecurityLogger => "security"
}

impl SecurityLogger {
    pub fn rate_limit_check(&self, session_id: &str, allowed: bool) {
        let level = if allowed {
            LogLevel::Debug
        } else {
            LogLevel::Warn
        };
        self.logger.log_with_source(
            level,
            "security",
            format!(
                "Rate limit check | session={} allowed={}",
                session_id, allowed
            ),
        );
    }

    pub fn rate_limit_exceeded(&self, session_id: &str, reason: &str) {
        self.logger.log_with_source(
            LogLevel::Warn,
            "security",
            format!(
                "Rate limit exceeded | session={} reason={}",
                session_id, reason
            ),
        );
    }

    pub fn secret_stored(&self, id: &str) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "security",
            format!("Secret stored | id={}", id),
        );
    }

    pub fn secret_retrieved(&self, id: &str) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "security",
            format!("Secret retrieved | id={}", id),
        );
    }

    pub fn secret_rotated(&self, id: &str) {
        self.logger.log_with_source(
            LogLevel::Info,
            "security",
            format!("Secret rotated | id={}", id),
        );
    }

    pub fn secret_deleted(&self, id: &str) {
        self.logger.log_with_source(
            LogLevel::Info,
            "security",
            format!("Secret deleted | id={}", id),
        );
    }

    pub fn secret_error(&self, id: &str, error: &str) {
        self.logger.log_with_source(
            LogLevel::Error,
            "security",
            format!("Secret error | id={} error={}", id, error),
        );
    }

    pub fn cleanup_task(&self, action: &str) {
        self.logger.log_with_source(
            LogLevel::Debug,
            "security",
            format!("Cleanup task | action={}", action),
        );
    }
}

macro_rules! impl_app_logger {
    ($ty:ty) => {
        impl crate::application::ports::logging::AppLogger for $ty {
            fn log_info(&self, message: String) {
                self.info(message);
            }
            fn log_warn(&self, message: String) {
                self.warn(message);
            }
            fn log_error(&self, message: String) {
                self.error(message);
            }
            fn log_debug(&self, message: String) {
                self.debug(message);
            }
        }
    };
}

impl_app_logger!(ConfigLogger);
impl_app_logger!(AgentLogger);
impl_app_logger!(TransportLogger);
impl_app_logger!(ProviderLogger);
impl_app_logger!(DiscoveryLogger);
impl_app_logger!(StdioLogger);
impl_app_logger!(ChatLogger);
impl_app_logger!(WasmLogger);
impl_app_logger!(ResilienceLogger);
impl_app_logger!(OrchestratorLogger);
impl_app_logger!(StreamingLogger);
impl_app_logger!(ObservabilityLogger);
impl_app_logger!(SecurityLogger);
use super::registry::SessionLogger;

/// Newtype adapter wrapping [`antikythera_log::SessionLogger`] to implement
/// the [`crate::application::ports::logging::AppLogger`] port trait. Required because both the trait (from
/// `antikythera_ports`) and the wrapped type (from `antikythera_log`) are
/// external to this crate, so the orphan rule prevents a direct impl.
pub struct SessionLoggerAdapter(pub SessionLogger);

impl crate::application::ports::logging::AppLogger for SessionLoggerAdapter {
    fn log_info(&self, message: String) {
        self.0.info(message);
    }
    fn log_warn(&self, message: String) {
        self.0.warn(message);
    }
    fn log_error(&self, message: String) {
        self.0.error(message);
    }
    fn log_debug(&self, message: String) {
        self.0.debug(message);
    }
}
