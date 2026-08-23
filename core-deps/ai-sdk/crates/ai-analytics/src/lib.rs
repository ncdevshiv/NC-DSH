//! Analytics (spec §19): metric aggregation from [`ai_observability`]
//! events — counts, latencies, token usage, estimated cost, cache
//! hit/miss, retries, errors — aggregatable by provider/model/agent/tool/
//! workflow/time window. Low-cardinality by design.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use ai_errors::{AiError, InternalError};
use ai_observability::{EventCollector, EventStatus};

/// A single metric value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    /// Aggregation dimension (e.g. `provider:openai`).
    pub dimension: String,
    pub count: u64,
    /// Total latency in milliseconds.
    pub total_latency_ms: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// Estimated cost in USD (when pricing metadata was attached).
    pub total_cost_usd: f64,
    pub errors: u64,
    pub retries: u64,
}

impl Metric {
    pub fn avg_latency_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.count as f64
        }
    }
}

/// Aggregates metrics from collected execution events.
///
/// Metrics are bucketed by (metric name, dimension) where the dimension is
/// taken from the event's `dimension` metadata (set by emitters such as
/// providers/agents). Token usage and estimated cost are read from event
/// metadata keys (`input_tokens`, `output_tokens`, `cost_usd`).
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    metrics: HashMap<(String, String), Metric>,
    started: Instant,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self {
            metrics: HashMap::new(),
            started: Instant::now(),
        }
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingests all events from a collector since the last call.
    pub fn ingest(&mut self, collector: &EventCollector) {
        for event in collector.events() {
            let dimension = event
                .metadata
                .get("dimension")
                .and_then(|d| d.as_str())
                .unwrap_or("global")
                .to_string();
            let name = format!("{:?}", event.kind).to_lowercase();
            let metric = self
                .metrics
                .entry((name.clone(), dimension.clone()))
                .or_insert_with(|| Metric {
                    name: name.clone(),
                    dimension: dimension.clone(),
                    count: 0,
                    total_latency_ms: 0,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    total_cost_usd: 0.0,
                    errors: 0,
                    retries: 0,
                });

            metric.count += 1;
            if let Some(duration) = event.duration_ms {
                metric.total_latency_ms += duration;
            }
            metric.total_input_tokens += event
                .metadata
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            metric.total_output_tokens += event
                .metadata
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            metric.total_cost_usd += event
                .metadata
                .get("cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            match event.status {
                EventStatus::Failed => metric.errors += 1,
                EventStatus::Retrying => metric.retries += 1,
                _ => {}
            }
        }
    }

    /// All aggregated metrics.
    pub fn metrics(&self) -> Vec<&Metric> {
        let mut metrics: Vec<&Metric> = self.metrics.values().collect();
        metrics.sort_by(|a, b| a.name.cmp(&b.name).then(a.dimension.cmp(&b.dimension)));
        metrics
    }

    /// Metrics matching a name and/or dimension (empty = wildcard).
    pub fn filter(&self, name: &str, dimension: &str) -> Vec<&Metric> {
        self.metrics()
            .into_iter()
            .filter(|m| {
                (name.is_empty() || m.name == name)
                    && (dimension.is_empty() || m.dimension == dimension)
            })
            .collect()
    }

    /// Total estimated cost across all metrics (USD).
    pub fn total_cost_usd(&self) -> f64 {
        self.metrics.values().map(|m| m.total_cost_usd).sum()
    }

    /// Total tokens consumed.
    pub fn total_tokens(&self) -> (u64, u64) {
        let input: u64 = self.metrics.values().map(|m| m.total_input_tokens).sum();
        let output: u64 = self.metrics.values().map(|m| m.total_output_tokens).sum();
        (input, output)
    }

    /// Rolling requests-per-second estimate based on collected counts.
    pub fn throughput_rps(&self) -> f64 {
        let elapsed = self.started.elapsed().as_secs_f64().max(0.001);
        let count: u64 = self.metrics.values().map(|m| m.count).sum();
        count as f64 / elapsed
    }

    /// Serializes a summary report (JSON) for dashboards/export.
    pub fn summary_json(&self) -> serde_json::Value {
        serde_json::json!({
            "metrics": self.metrics(),
            "total_cost_usd": self.total_cost_usd(),
            "total_tokens": {
                "input": self.total_tokens().0,
                "output": self.total_tokens().1
            },
            "throughput_rps": self.throughput_rps(),
        })
    }
}

/// A rate-limited counter (e.g. tokens per minute) with a window.
#[derive(Debug, Clone)]
pub struct RateCounter {
    window: Duration,
    events: Vec<Instant>,
    capacity: usize,
}

impl RateCounter {
    pub fn new(window: Duration, capacity: usize) -> Self {
        Self {
            window,
            events: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn record(&mut self) {
        let now = Instant::now();
        self.events
            .retain(|t| now.duration_since(*t) <= self.window);
        if self.events.len() >= self.capacity {
            // Bound memory: drop the oldest.
            self.events.remove(0);
        }
        self.events.push(now);
    }

    /// Events within the current window.
    pub fn count(&self) -> usize {
        let now = Instant::now();
        self.events
            .iter()
            .filter(|t| now.duration_since(**t) <= self.window)
            .count()
    }

    /// Events per second in the window.
    pub fn rate_per_sec(&self) -> f64 {
        self.count() as f64 / self.window.as_secs_f64().max(0.001)
    }
}

/// Reports an internal analytics error.
pub fn analytics_error(message: impl Into<String>) -> AiError {
    AiError::Internal(InternalError::new(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_observability::{EventCollector, EventKind};
    use std::collections::BTreeMap;

    fn event(
        collector: &EventCollector,
        kind: EventKind,
        status: EventStatus,
        duration: u64,
        dimension: &str,
    ) {
        let mut metadata = BTreeMap::new();
        metadata.insert("dimension".to_string(), serde_json::json!(dimension));
        metadata.insert("input_tokens".to_string(), serde_json::json!(100));
        metadata.insert("output_tokens".to_string(), serde_json::json!(50));
        metadata.insert("cost_usd".to_string(), serde_json::json!(0.001));
        collector.record_with_ids(
            kind,
            "op",
            status,
            metadata,
            "t".into(),
            "s".into(),
            None,
            Some(duration),
        );
    }

    #[test]
    fn aggregates_counts_and_tokens_by_dimension() {
        let collector = EventCollector::new();
        for _ in 0..3 {
            event(
                &collector,
                EventKind::ModelCall,
                EventStatus::Succeeded,
                100,
                "openai:gpt-4o",
            );
        }
        event(
            &collector,
            EventKind::ModelCall,
            EventStatus::Failed,
            50,
            "openai:gpt-4o",
        );

        let mut registry = MetricsRegistry::new();
        registry.ingest(&collector);

        let model_metrics = registry.filter("modelcall", "openai:gpt-4o");
        assert_eq!(model_metrics.len(), 1);
        let metric = model_metrics[0];
        assert_eq!(metric.count, 4);
        assert_eq!(metric.errors, 1);
        assert_eq!(metric.total_input_tokens, 400);
        assert_eq!(metric.total_output_tokens, 200);
        assert_eq!(metric.total_latency_ms, 350);
        assert_eq!(metric.avg_latency_ms(), 87.5);
    }

    #[test]
    fn cost_and_tokens_totals() {
        let collector = EventCollector::new();
        event(
            &collector,
            EventKind::ModelCall,
            EventStatus::Succeeded,
            10,
            "openai:gpt-4o",
        );
        event(
            &collector,
            EventKind::ToolCall,
            EventStatus::Succeeded,
            5,
            "tool:calc",
        );

        let mut registry = MetricsRegistry::new();
        registry.ingest(&collector);
        assert_eq!(registry.total_cost_usd(), 0.002);
        assert_eq!(registry.total_tokens(), (200, 100));
    }

    #[test]
    fn retries_are_counted() {
        let collector = EventCollector::new();
        event(
            &collector,
            EventKind::Retry,
            EventStatus::Retrying,
            0,
            "provider:openai",
        );
        let mut registry = MetricsRegistry::new();
        registry.ingest(&collector);
        let retries = registry.filter("retry", "provider:openai");
        assert_eq!(retries[0].retries, 1);
    }

    #[test]
    fn rate_counter_limits_window() {
        let mut counter = RateCounter::new(Duration::from_millis(50), 100);
        for _ in 0..5 {
            counter.record();
        }
        assert_eq!(counter.count(), 5);
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(counter.count(), 0, "events expire after the window");
    }
}
