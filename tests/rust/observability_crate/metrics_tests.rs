//! Observability crate integration tests — metrics subsystem.

use antikythera_observability::{
    InMemoryMetricsExporter, LatencyTracker, MetricKind, MetricsExporter, percentile,
};

include!("metrics_tests/counter_gauge_histogram.rs");
include!("metrics_tests/latency_percentiles.rs");
include!("metrics_tests/clear_snapshot.rs");
include!("metrics_tests/port_trait.rs");
