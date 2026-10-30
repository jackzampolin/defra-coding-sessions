use prometheus::{
    Encoder, Gauge, IntCounter, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};

/// Prometheus metrics exported by coding-data.
pub struct IngestMetrics {
    /// Watch scan outcomes. Labels: status ("success" | "failed").
    pub watch_scans_total: IntCounterVec,
    /// Current number of deferred (backlogged) watch files.
    pub deferred_watch_files: IntGauge,
    /// Total deferred watch file events (cumulative).
    pub deferred_watch_events_total: IntCounter,
    /// Total GraphQL retries (transaction conflict retries).
    pub graphql_retries_total: IntCounter,
    /// Unix timestamp of the last successful operation.
    pub last_success_unixtime: Gauge,
    /// Dedicated registry (not the global one, to keep things hermetic).
    registry: Registry,
}

impl IngestMetrics {
    /// Create and register all metrics.
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new();

        let watch_scans_total = IntCounterVec::new(
            Opts::new(
                "coding_data_watch_scans_total",
                "Total watch scan attempts by outcome",
            ),
            &["status"],
        )?;

        let deferred_watch_files = IntGauge::new(
            "coding_data_deferred_watch_files",
            "Current number of deferred (backlogged) watch files",
        )?;

        let deferred_watch_events_total = IntCounter::new(
            "coding_data_deferred_watch_events_total",
            "Total deferred watch file events",
        )?;

        let graphql_retries_total = IntCounter::new(
            "coding_data_graphql_retries_total",
            "Total GraphQL retries due to transaction conflicts",
        )?;

        let last_success_unixtime = Gauge::new(
            "coding_data_last_success_unixtime",
            "Unix timestamp of the last successful operation",
        )?;

        registry.register(Box::new(watch_scans_total.clone()))?;
        registry.register(Box::new(deferred_watch_files.clone()))?;
        registry.register(Box::new(deferred_watch_events_total.clone()))?;
        registry.register(Box::new(graphql_retries_total.clone()))?;
        registry.register(Box::new(last_success_unixtime.clone()))?;

        Ok(Self {
            watch_scans_total,
            deferred_watch_files,
            deferred_watch_events_total,
            graphql_retries_total,
            last_success_unixtime,
            registry,
        })
    }

    /// Record a successful watch scan.
    pub fn record_scan_success(&self) {
        self.watch_scans_total.with_label_values(&["success"]).inc();
        self.last_success_unixtime.set(now_unix_secs());
    }

    /// Record a failed watch scan.
    pub fn record_scan_failure(&self) {
        self.watch_scans_total.with_label_values(&["failed"]).inc();
    }

    /// Update the deferred watch files gauge and increment the event counter.
    pub fn record_deferred_file(&self, current_backlog_size: usize) {
        self.deferred_watch_files.set(current_backlog_size as i64);
        self.deferred_watch_events_total.inc();
    }

    /// Update the deferred watch files gauge without incrementing the event counter.
    pub fn set_deferred_file_count(&self, current_backlog_size: usize) {
        self.deferred_watch_files.set(current_backlog_size as i64);
    }

    /// Record a GraphQL retry.
    pub fn record_graphql_retry(&self) {
        self.graphql_retries_total.inc();
    }

    /// Encode the registry to Prometheus text format.
    pub fn encode(&self) -> String {
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        let metric_families = self.registry.gather();
        encoder
            .encode(&metric_families, &mut buffer)
            .unwrap_or_default();
        String::from_utf8(buffer).unwrap_or_default()
    }
}

fn now_unix_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_encode_produces_prometheus_format() {
        let m = IngestMetrics::new().unwrap();
        m.record_scan_success();
        m.record_scan_failure();
        m.record_deferred_file(3);
        m.record_graphql_retry();
        let output = m.encode();
        assert!(output.contains("coding_data_watch_scans_total"));
        assert!(output.contains("coding_data_deferred_watch_files"));
        assert!(output.contains("coding_data_deferred_watch_events_total"));
        assert!(output.contains("coding_data_graphql_retries_total"));
        assert!(output.contains("coding_data_last_success_unixtime"));
    }

    #[test]
    fn scan_counters_increment_correctly() {
        let m = IngestMetrics::new().unwrap();
        m.record_scan_success();
        m.record_scan_success();
        m.record_scan_failure();
        let output = m.encode();
        assert!(output.contains(r#"status="success"} 2"#));
        assert!(output.contains(r#"status="failed"} 1"#));
    }

    #[test]
    fn deferred_gauge_tracks_current_backlog() {
        let m = IngestMetrics::new().unwrap();
        m.record_deferred_file(5);
        m.record_deferred_file(3);
        let output = m.encode();
        assert!(output.contains("coding_data_deferred_watch_files 3"));
        assert!(output.contains("coding_data_deferred_watch_events_total 2"));
    }
}
