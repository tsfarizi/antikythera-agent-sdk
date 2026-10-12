use std::collections::HashMap;

use antikythera_core::application::hooks::InMemoryTelemetryHook;
use antikythera_core::application::observability::metrics::percentile;
use antikythera_core::application::observability::{
    AuditCategory, AuditRecord, AuditTrail, CallerContext, InMemoryMetricsExporter,
    InMemoryTracingHook, LatencyTracker, MetricKind, MetricsExporter, ObservabilityHook,
    TelemetryEvent, TraceSpanContext, TraceStatus, TracingHook,
};

include!("observability_tests/latency_tracker_sla.rs");
include!("observability_tests/metrics_exporter_histogram.rs");
include!("observability_tests/audit_trail_events.rs");
include!("observability_tests/tracing_hook_lifecycle.rs");
include!("observability_tests/audit_filter_detail.rs");
include!("observability_tests/metrics_latency_percentiles.rs");
include!("observability_tests/tracing_telemetry.rs");
include!("observability_tests/caller_context_telemetry.rs");
