# OpenClaw Integration Guide

This guide shows how to connect OpenClaw (or any agent product) to Rango as the durable memory substrate.

## Integration goals

- Persist agent memory across runs without prompt stuffing.
- Keep context bounded and cheaper per request.
- Separate current truth from historical episodes.

## Quick start

1. Install binaries:

```bash
cargo install --path crates/cli
cargo install --path crates/server
```

2. Bootstrap a workspace:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/openclaw-bootstrap.ps1 -WorkspacePath .rango-openclaw
```

3. (Optional) start a sync hub:

```bash
rango-server --bind 0.0.0.0 --port 8080 --token dev-token --oplog-path ./server-oplog.rgo
```

4. Configure OpenClaw environment:

```bash
RANGO_PATH=.rango-openclaw
RANGO_NAMESPACE=openclaw
RANGO_TENANT=default
RANGO_NODE_ID=openclaw-node
RANGO_SYNC_URL=http://localhost:8080
RANGO_SYNC_TOKEN=dev-token
```

## CI Validation

Every pull request runs `integration-openclaw` to validate that the memory contract hasn't drifted and that the SDK surface remains compatible. This smoke test exercises the locked Rango SDK surface against the baseline contract, ensuring that write, read, and promotion paths work correctly for all four collections (`agent_state`, `task_state`, `episodes`, `facts`). If the test fails, either the contract changed without a corresponding fixture update, or the SDK API was modified in a breaking way.

To intentionally update the contract fixture after modifying the memory contract, compute and update the SHA-256 hash fixture:

```bash
# On Linux/macOS:
sha256sum docs/integrations/openclaw-memory-contract.json | cut -d' ' -f1 > examples/openclaw-smoke/contract.sha256

# On Windows (PowerShell):
Get-FileHash -Path docs/integrations/openclaw-memory-contract.json -Algorithm SHA256 | Select-Object -ExpandProperty Hash | Out-File -NoNewline examples/openclaw-smoke/contract.sha256
```

Commit the updated hash with the contract change. Note that contract changes require an ADR (see `docs/adr/`).

The smoke test uses only the locked and stable SDK surface documented in [`docs/reference/sdk-stability.md`](../reference/sdk-stability.md).

## Memory contract

Use [openclaw-memory-contract.json](openclaw-memory-contract.json) as the baseline schema contract.

Collections:

- `agent_state`: mutable, single-source "now" view.
- `task_state`: mutable task lifecycle.
- `episodes`: append-only events (tool calls, outcomes, errors).
- `facts`: derived semantic memory (promoted, attributed facts).

## Read/write policy (token control)

Write path:

1. Write every important event to `episodes`.
2. Update only active fields in `agent_state` and `task_state`.
3. Promote to `facts` only when confidence/repetition threshold is met.

Read path:

1. Read `agent_state` and active `task_state` first.
2. Pull latest `episodes` in a strict bounded window.
3. Pull `facts` only for the current entity/task.
4. Enforce max memory budget before prompt assembly.

Recommended default budget:

- `agent_state`: always include.
- `task_state`: include only active tasks.
- `episodes`: last 20-50 events.
- `facts`: top 10 trusted facts.

This prevents drift and keeps prompt size predictable.

## Runtime patterns

Mode 1: embedded-only

- OpenClaw writes/reads local Rango workspace directly.
- Best for offline or single-node scenarios.

Mode 2: hub-and-spoke sync

- OpenClaw writes locally, periodically `rango sync`.
- Multiple nodes converge via `rango-server`.

## Minimal operational checklist

- Run `rango doctor <workspace>` in CI smoke checks.
- Keep tenant/namespace boundaries explicit.
- Treat `facts` as derived memory, not canonical truth.
- Keep retrieval bounded by policy, never "load everything."
