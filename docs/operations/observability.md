# Rango Observability Contract

**Version:** 0.1.0  
**Last updated:** 2026-04-29  
**Scope:** `rango-server` (hub) and embedded SDK runtimes

## Overview

This document defines the OpenTelemetry metrics and tracing contract for Rango. All metrics follow [OTel semantic conventions](https://opentelemetry.io/docs/specs/semconv/) where applicable.

## Metric Catalog

### Counters

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `rango_push_operations_total` | Counter | 1 | `tenant_id`, `namespace`, `decision` | Total push operations |
| `rango_pull_operations_total` | Counter | 1 | `tenant_id`, `namespace`, `decision` | Total pull operations |
| `rango_promote_operations_total` | Counter | 1 | `tenant_id`, `namespace`, `decision` | Total promote operations |
| `rango_retrieval_operations_total` | Counter | 1 | `tenant_id`, `namespace`, `decision` | Total retrieval operations |
| `rango_rejected_operations_total` | Counter | 1 | `tenant_id`, `namespace`, `reason`, `stage` | Total rejected operations |

### UpDownCounters

| Metric Name | Type | Unit | Labels | Description |
|-------------|------|------|--------|-------------|
| `rango_active_connections` | UpDownCounter | 1 | - | Number of active connections |

### Decision Values

- `allow` — operation accepted
- `reject` — operation rejected by policy
- `empty` — pull returned no mutations
- `degraded` — retrieval fell back to canonical

## Span Taxonomy

### Control-Plane Paths

| Span Name | Parent | Attributes |
|-----------|--------|------------|
| `rango.write_path` | HTTP handler | `tenant_id`, `namespace`, `actor`, `trust_score` |
| `rango.read_path` | HTTP handler | `tenant_id`, `namespace`, `tier`, `limit` |
| `rango.promotion_path` | HTTP handler | `tenant_id`, `namespace`, `from_tier`, `to_tier` |

### Sync Operations

| Span Name | Parent | Attributes |
|-----------|--------|------------|
| `rango.sync.push` | HTTP handler | `node_id`, `tenant_id`, `namespace`, `mutation_count` |
| `rango.sync.pull` | HTTP handler | `node_id`, `tenant_id`, `namespace`, `since_checkpoint` |

## Cardinality Budget

To prevent metric explosion, the following cardinality limits are enforced:

| Label | Max Cardinality | Enforcement |
|-------|----------------|-------------|
| `tenant_id` | 10,000 | Hard limit — reject unknown tenants |
| `namespace` | 1,000 per tenant | Soft limit — warn and sample |
| `decision` | 4 (allow, reject, empty, degraded) | Fixed enum |
| `reason` | 50 | Hard limit — truncate to "other" |
| `stage` | 10 | Hard limit — truncate to "other" |

## Exporter Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RANGO_OTEL_ENABLED` | `false` | Enable OpenTelemetry export |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | OTLP collector endpoint |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `grpc` | Protocol: `grpc` or `http/protobuf` |
| `RANGO_OTEL_METRIC_INTERVAL_SECS` | `60` | Metric export interval |

### Feature Flag

OTel is wired via the `RangoMetrics` struct in `crates/server/src/observability.rs`. When no meter is configured, metrics are no-ops (zero overhead).

```rust
use rango_server::observability::{RangoMetrics, init_test_meter_provider};

let (provider, _) = init_test_meter_provider();
let metrics = RangoMetrics::new(provider.meter("rango-server"));
let state = ServerState::new(oplog).with_metrics(metrics);
```

## Testing

The observability contract is validated by:

- `crates/server/tests/opentelemetry_contract.rs` — structural wiring tests
- `crates/server/tests/anomaly_signals.rs` — anomaly signal span coverage

Run with:

```bash
cargo test -p rango-server --test opentelemetry_contract -- --nocapture
cargo test -p rango-server --test anomaly_signals -- --nocapture
```

## Out of Scope

- Managed dashboards (see issue #36 Grafana reference dashboard)
- Alert routing
- Log correlation (structured logs are issue #25)
