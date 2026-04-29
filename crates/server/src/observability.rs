use opentelemetry::metrics::{Counter, Meter, UpDownCounter};
use opentelemetry::KeyValue;
use opentelemetry_sdk::metrics::PeriodicReader;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::testing::metrics::InMemoryMetricExporter;
use std::time::Duration;

/// Rango server metrics following OpenTelemetry conventions.
pub struct RangoMetrics {
    /// Counter: total push operations
    push_operations_total: Counter<u64>,
    /// Counter: total pull operations
    pull_operations_total: Counter<u64>,
    /// Counter: total promote operations
    promote_operations_total: Counter<u64>,
    /// Counter: total retrieval operations
    retrieval_operations_total: Counter<u64>,
    /// Counter: rejected operations by policy
    rejected_operations_total: Counter<u64>,
    /// UpDownCounter: active connections (if applicable)
    active_connections: UpDownCounter<i64>,
}

impl RangoMetrics {
    pub fn new(meter: Meter) -> Self {
        Self {
            push_operations_total: meter
                .u64_counter("rango_push_operations_total")
                .with_description("Total number of push operations")
                .build(),
            pull_operations_total: meter
                .u64_counter("rango_pull_operations_total")
                .with_description("Total number of pull operations")
                .build(),
            promote_operations_total: meter
                .u64_counter("rango_promote_operations_total")
                .with_description("Total number of promote operations")
                .build(),
            retrieval_operations_total: meter
                .u64_counter("rango_retrieval_operations_total")
                .with_description("Total number of retrieval operations")
                .build(),
            rejected_operations_total: meter
                .u64_counter("rango_rejected_operations_total")
                .with_description("Total number of operations rejected by policy")
                .build(),
            active_connections: meter
                .i64_up_down_counter("rango_active_connections")
                .with_description("Number of active connections")
                .build(),
        }
    }

    pub fn record_push(&self, tenant_id: &str, namespace: &str, decision: &str) {
        let attrs = vec![
            KeyValue::new("tenant_id", tenant_id.to_string()),
            KeyValue::new("namespace", namespace.to_string()),
            KeyValue::new("decision", decision.to_string()),
        ];
        self.push_operations_total.add(1, &attrs);
    }

    pub fn record_pull(&self, tenant_id: &str, namespace: &str, decision: &str) {
        let attrs = vec![
            KeyValue::new("tenant_id", tenant_id.to_string()),
            KeyValue::new("namespace", namespace.to_string()),
            KeyValue::new("decision", decision.to_string()),
        ];
        self.pull_operations_total.add(1, &attrs);
    }

    pub fn record_promote(&self, tenant_id: &str, namespace: &str, decision: &str) {
        let attrs = vec![
            KeyValue::new("tenant_id", tenant_id.to_string()),
            KeyValue::new("namespace", namespace.to_string()),
            KeyValue::new("decision", decision.to_string()),
        ];
        self.promote_operations_total.add(1, &attrs);
    }

    pub fn record_retrieval(&self, tenant_id: &str, namespace: &str, decision: &str) {
        let attrs = vec![
            KeyValue::new("tenant_id", tenant_id.to_string()),
            KeyValue::new("namespace", namespace.to_string()),
            KeyValue::new("decision", decision.to_string()),
        ];
        self.retrieval_operations_total.add(1, &attrs);
    }

    pub fn record_rejection(&self, tenant_id: &str, namespace: &str, reason: &str, stage: &str) {
        let attrs = vec![
            KeyValue::new("tenant_id", tenant_id.to_string()),
            KeyValue::new("namespace", namespace.to_string()),
            KeyValue::new("reason", reason.to_string()),
            KeyValue::new("stage", stage.to_string()),
        ];
        self.rejected_operations_total.add(1, &attrs);
    }
}

/// Initialize a test meter provider with an in-memory exporter.
pub fn init_test_meter_provider() -> (SdkMeterProvider, InMemoryMetricExporter) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone(), opentelemetry_sdk::runtime::TokioCurrentThread)
        .with_interval(Duration::from_secs(60))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .build();

    (provider, exporter)
}
