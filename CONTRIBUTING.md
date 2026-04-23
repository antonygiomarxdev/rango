# Contributing to Rango

Thank you for your interest in contributing! Rango is an open-source project and we welcome contributions of all kinds.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How to Contribute](#how-to-contribute)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Testing Requirements](#testing-requirements)
- [Commit Convention](#commit-convention)
- [Architectural Decisions (ADRs)](#architectural-decisions-adrs)
- [Submitting a Pull Request](#submitting-a-pull-request)

---

## Code of Conduct

By participating in this project you agree to treat all contributors with respect.
Harassment or discriminatory behavior of any kind will not be tolerated.

---

## How to Contribute

| Type | Where to start |
|------|---------------|
| Bug report | [Open an issue](https://github.com/antonygiomarxdev/rango/issues/new?template=bug_report.md) |
| Feature request | [Open a discussion](https://github.com/antonygiomarxdev/rango/discussions) first |
| Documentation | PRs welcome — edit files under `docs/` |
| Code | Fork → branch → PR (see below) |
| Security vulnerability | See [SECURITY.md](docs/SECURITY.md) — **do not open a public issue** |

---

## Development Setup

**Prerequisites:**
- Rust stable ≥ 1.85 (managed by `rust-toolchain.toml`)
- `cargo-deny` for license/security checks: `cargo install cargo-deny`
- `cargo-nextest` (optional, faster test runner): `cargo install cargo-nextest`

```bash
# Clone
git clone https://github.com/antonygiomarxdev/rango.git
cd rango

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Lint (must pass before opening a PR)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Format check
cargo fmt --all -- --check

# License & security audit
cargo deny check
```

---

## Coding Standards

### Rust Style
- Formatting enforced by `rustfmt` — run `cargo fmt --all` before committing
- All clippy warnings are errors in CI — fix them, don't suppress with `allow` unless justified
- No `unwrap()` or `expect()` in library code — use `?` and proper error types
- `unsafe` blocks require a `// SAFETY:` comment explaining the invariant
- Public items must have doc comments (`///`)

### Design Principles
- **Traits over concrete types** — new storage backends, transports, etc. must implement the existing trait
- **No premature optimization** — profile first, then optimize with a benchmark proving the gain
- **Offline-first** — any feature that requires network must degrade gracefully when offline
- **No silent data loss** — errors must propagate; mutations must be durable before ack

---

## Testing Requirements

All PRs **must** include tests. No exceptions.

| Change type | Required coverage |
|-------------|------------------|
| New public API | Unit tests + doc tests |
| Bug fix | Regression test that would have caught the bug |
| Sync / conflict logic | Integration test with two nodes |
| Performance claim | Criterion benchmark proving the gain |
| Storage / recovery | Crash recovery test (kill mid-write, restart, verify) |

Run the full suite:
```bash
cargo test --workspace --all-features
# or with nextest:
cargo nextest run --workspace
```

Property-based tests use `proptest` — add them for any logic operating over arbitrary inputs.

---

## Commit Convention

This project uses [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short description>

[optional body]

[optional footer]
```

**Types:** `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `perf`, `ci`

**Scopes:** `types`, `storage`, `index`, `query`, `oplog`, `sync`, `core`, `server`, `sdk`, `cli`

**Examples:**
```
feat(sync): add exponential backoff on network failure
fix(query): correct $gt filter for nested fields
perf(core): reduce allocations in find_many hot path
docs(api): add bulk_write example to README
```

Breaking changes add `!` after the scope: `feat(sdk)!: rename insert_one to put`

---

## Architectural Decisions (ADRs)

Significant architectural changes **require an ADR** in `docs/adr/` before implementation.

ADR template: copy `docs/adr/ADR-001-storage-engine.md`, increment the number, fill in Context / Decision / Consequences.

What warrants an ADR:
- New storage backend
- Changes to sync protocol
- New ID scheme
- Security model changes
- Replacing a major dependency

---

## Submitting a Pull Request

1. **Fork** the repository and create a feature branch from `main`
   ```bash
   git checkout -b feat/my-feature
   ```
2. Make your changes following the standards above
3. Ensure all checks pass locally:
   ```bash
   cargo fmt --all
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace
   cargo deny check
   ```
4. Push and open a PR against `main`
5. Fill in the PR template — include the motivation, what changed, and how you tested it
6. A maintainer will review within a few days

PRs that fail CI will not be merged. PRs without tests will be asked to add them.

---

## License

By contributing you agree that your contributions will be licensed under
the same dual MIT / Apache-2.0 license as the rest of the project.
