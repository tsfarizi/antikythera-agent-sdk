pub mod types;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use types::{MetricKind, MetricRecord};

/// Local sync metrics export hook — backward-compatible with core's internal usage.
pub trait MetricsExporter: Send + Sync {
    fn export_metric(&self, metric: MetricRecord);

    fn export_counter(&self, name: &str, value: f64, attributes: HashMap<String, String>) {
        self.export_metric(MetricRecord::new(
            name,
            MetricKind::Counter,
            value,
            attributes,
        ));
    }

    fn export_gauge(&self, name: &str, value: f64, attributes: HashMap<String, String>) {
        self.export_metric(MetricRecord::new(
            name,
            MetricKind::Gauge,
            value,
            attributes,
        ));
    }

    fn export_histogram(&self, name: &str, value: f64, attributes: HashMap<String, String>) {
        self.export_metric(MetricRecord::new(
            name,
            MetricKind::Histogram,
            value,
            attributes,
        ));
    }
}

/// In-memory metric exporter used by tests and embedded hosts.
#[derive(Debug, Clone, Default)]
pub struct InMemoryMetricsExporter {
    metrics: Arc<Mutex<Vec<MetricRecord>>>,
}

impl InMemoryMetricsExporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<MetricRecord> {
        self.metrics
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "InMemoryMetricsExporter metrics lock poisoned in snapshot: {}",
                    e
                );
                Vec::new()
            })
    }

    pub fn clear(&self) {
        match self.metrics.lock() {
            Ok(mut guard) => guard.clear(),
            Err(e) => {
                tracing::warn!(
                    "InMemoryMetricsExporter metrics lock poisoned in clear: {}",
                    e
                );
            }
        }
    }
}

impl MetricsExporter for InMemoryMetricsExporter {
    fn export_metric(&self, metric: MetricRecord) {
        match self.metrics.lock() {
            Ok(mut guard) => guard.push(metric),
            Err(e) => {
                tracing::warn!(
                    "InMemoryMetricsExporter metrics lock poisoned in export_metric: {}",
                    e
                );
            }
        }
    }
}

#[async_trait]
impl antikythera_ports::observability::MetricsExporter for InMemoryMetricsExporter {
    async fn record_metric(
        &self,
        name: &str,
        kind: &str,
        value: f64,
        attributes: Vec<(String, String)>,
    ) {
        let metric_kind = match kind {
            "counter" => MetricKind::Counter,
            "gauge" => MetricKind::Gauge,
            "histogram" => MetricKind::Histogram,
            _ => MetricKind::Gauge,
        };
        let attrs: HashMap<String, String> = attributes.into_iter().collect();
        self.export_metric(MetricRecord::new(name, metric_kind, value, attrs));
    }

    async fn flush(&self) -> Result<(), String> {
        Ok(())
    }
}

/// SLA latency summary values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatencySummary {
    pub count: usize,
    pub min_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

impl Default for LatencySummary {
    fn default() -> Self {
        Self {
            count: 0,
            min_ms: 0.0,
            max_ms: 0.0,
            avg_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
        }
    }
}

/// In-memory latency tracker with percentile summary helpers.
#[derive(Debug, Clone, Default)]
pub struct LatencyTracker {
    samples_ms: Vec<f64>,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_ms(&mut self, duration_ms: f64) {
        if duration_ms.is_finite() && duration_ms >= 0.0 {
            self.samples_ms.push(duration_ms);
        }
    }

    pub fn count(&self) -> usize {
        self.samples_ms.len()
    }

    pub fn summary(&self) -> LatencySummary {
        if self.samples_ms.is_empty() {
            return LatencySummary::default();
        }

        let mut sorted = self.samples_ms.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let count = sorted.len();
        let sum: f64 = sorted.iter().sum();

        LatencySummary {
            count,
            min_ms: sorted[0],
            max_ms: sorted[count - 1],
            avg_ms: sum / count as f64,
            p50_ms: percentile(&sorted, 0.50),
            p95_ms: percentile(&sorted, 0.95),
            p99_ms: percentile(&sorted, 0.99),
        }
    }
}

pub fn percentile(sorted_samples: &[f64], q: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }

    let q = q.clamp(0.0, 1.0);
    let index = ((sorted_samples.len() - 1) as f64 * q).round() as usize;
    sorted_samples[index]
}
