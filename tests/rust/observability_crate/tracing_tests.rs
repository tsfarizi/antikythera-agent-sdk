//! Observability crate integration tests — tracing subsystem.

use antikythera_observability::{InMemoryTracingHook, TraceSpanContext, TraceStatus, TracingHook};

include!("tracing_tests/span_lifecycle.rs");
include!("tracing_tests/port_trait.rs");
