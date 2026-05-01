# Adversarial Benchmark Suite

This directory contains adversarial benchmarks for Rango. They exercise
memory-safety and isolation claims under adversarial conditions.

## Running

Run the full suite:

```bash
cargo bench --bench adversarial
```

Compile without running (CI gate):

```bash
cargo bench --bench adversarial --no-run
```

## Benchmarks

| Benchmark | Purpose |
|-----------|---------|
| `poisoning_rejection_latency` | Measures control-plane `write_path` latency when rejecting low-trust mutations (`trust_score < 0.25`). |
| `cross_tenant_leak_check` | Generates deterministic mutations for **tenant-a**, then pulls as **tenant-b** and asserts zero leakage. |
| `replay_determinism` | Snapshots an oplog, replays entries into a fresh oplog, and asserts checkpoint convergence. |
| `push_throughput` | Sustained ops/sec for the push handler (CPU-bound path). |
| `pull_latency` | End-to-end latency for the pull handler with a warm oplog. |
| `audit_persistence` | Latency of writing an audit record through `FileOplog` (includes disk sync). |

## Seeds

All deterministic benchmarks load fixed seeds from
`fixtures/adversarial_seeds.json`. This guarantees reproducible
"fuzz" across runs and machines.

## Reports

Criterion generates an HTML report under `target/criterion/` after each
run. Open `target/criterion/report/index.html` in a browser for
interactive charts and historical comparisons.
