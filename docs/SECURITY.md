# Security Policy — Rango (Project Atlas)

**Last updated:** 2026-04-23
**Scope:** All crates in the Rango workspace, CLI, server, and SDK.

---

## 1. Threat Model

### Assets
- **Local database files** (documents, oplog, sync queue, checkpoints)
- **Sync credentials** (Bearer tokens, node identifiers)
- **User data** (BSON documents with potentially sensitive fields)
- **Server state** (primary node oplog, authentication tokens)

### Threat Actors
- **Local attacker with filesystem access** — can read/copy database files
- **Network attacker** — can intercept or manipulate sync traffic (MITM)
- **Malicious client** — can send crafted sync requests to server
- **Supply-chain attacker** — can compromise dependencies

### Trust Boundaries
- **Trust boundary 1:** Application process ↔ filesystem (encrypted at rest mitigates)
- **Trust boundary 2:** Edge node ↔ network ↔ primary server (TLS + auth tokens)
- **Trust boundary 3:** User input ↔ engine (validation, size limits, path sanitization)

---

## 2. Security Audit Findings

### Findings Addressed in This Session

| ID | Severity | Finding | Mitigation | Status |
|----|----------|---------|------------|--------|
| SEC-001 | **Critical** | PBKDF2 with 100k iterations (below OWASP 2023 minimum) | Increased to **600,000 iterations** (OWASP compliant for PBKDF2-SHA256) | Fixed |
| SEC-002 | **Critical** | Document size validation ran **after** metadata injection, allowing documents slightly under limit to exceed it after `_rev`, `_id`, etc. were added | Moved validation **before and after** metadata injection | Fixed |
| SEC-003 | **High** | CLI accepted arbitrary paths including `../` sequences, enabling path traversal to sensitive directories | Added `sanitize_path()` function that rejects paths containing parent directory references | Fixed |
| SEC-004 | **High** | Server endpoints had **no request body size limit**, enabling DoS via multi-gigabyte JSON payloads | Added `DefaultBodyLimit::max(10MB)` to axum router | Fixed |
| SEC-005 | **Medium** | Auth tokens stored as plain `String` in memory with no secure clearing | Documented as accepted risk (see below); `zeroize` integration planned for Phase 7 completion | Accepted |
| SEC-006 | **Medium** | Sync protocol uses HTTP by default without TLS enforcement | Documented — **operators must use HTTPS/reverse proxy in production** | Accepted |
| SEC-007 | **Medium** | No rate limiting on server push/pull endpoints | Planned for Phase 7 completion; currently mitigated by body size limit and network perimeter | Accepted |
| SEC-008 | **Medium** | Single static Bearer token per node with no expiration/refresh | Documented as MVP limitation; JWT or mTLS planned for post-pilot | Accepted |
| SEC-009 | **Low** | No audit log of who performed which mutation | Planned for server-side logging enhancement | Accepted |
| SEC-010 | **Low** | `MemoryStorage` backend does not zero memory on drop | Documented — only for dev/testing; production will use file-based backend with encryption | Accepted |

### Dependency Security Review

| Dependency | Version | Known CVEs | Assessment |
|------------|---------|------------|------------|
| `bson` | 3.1.0 | None known | Official MongoDB driver crate, actively maintained |
| `axum` | 0.8.9 | None known | Tokio ecosystem, security-responsive team |
| `aes-gcm` | 0.10.3 | None known | RustCrypto, audited implementation |
| `pbkdf2` | 0.12.2 | None known | RustCrypto, standard implementation |
| `sha2` | 0.10.9 | None known | RustCrypto |
| `rand` | 0.8.6 | None known | Rust standard randomness |
| `tokio` | 1.52.1 | None known | Async runtime, heavily audited |
| `reqwest` | 0.12.x | None known | HTTP client, widely used |
| `uuid` | 1.17 | None known | UUID generation |
| `clap` | 4.5 | None known | CLI parser |

> **Note:** Automated `cargo audit` could not be executed in this environment due to missing tooling. A full `cargo audit` run **must** be performed in CI before any production release.

---

## 3. Cryptography

### At-Rest Encryption

- **Algorithm:** AES-256-GCM (authenticated encryption)
- **Key derivation:** PBKDF2-HMAC-SHA256 with **600,000 iterations**
- **Salt:** 16 bytes, randomly generated per database, stored in `salt` file
- **Nonce:** 12 bytes, randomly generated per encryption operation, prepended to ciphertext
- **Scope:** Oplog, sync queue, and checkpoint files are encrypted when `--passphrase` is provided at `rango init`
- **Limitation:** Document storage encryption is pending the storage engine decision (fjall vs redb). Current `MemoryStorage` does not persist to disk.

### Sync Transport Security

- **Protocol:** HTTP/JSON (MVP)
- **Authentication:** Bearer token in `Authorization` header
- **Required operator action:** Deploy server behind HTTPS reverse proxy (nginx, Traefik, Caddy) or cloud load balancer with TLS termination
- **Future:** Native TLS support in `SyncClient` and server planned for post-pilot

---

## 4. Secure Deployment Checklist

Before running Rango in production or pilot environments:

- [ ] Initialize database **with passphrase**: `rango init --passphrase "..."`
- [ ] Store the passphrase in a secrets manager (never in shell history or env vars)
- [ ] Deploy server behind HTTPS reverse proxy with valid TLS certificate
- [ ] Generate strong, unique token per node: `openssl rand -hex 32`
- [ ] Restrict server network access (firewall/VPC) to known edge nodes
- [ ] Enable OS-level filesystem encryption as defense-in-depth
- [ ] Run `cargo audit` in CI and address all HIGH/CRITICAL findings
- [ ] Monitor server logs for repeated 401/403 errors (potential brute force)
- [ ] Backup `salt` file alongside database — **loss of salt = irrecoverable data**
- [ ] Schedule `rango doctor` periodically to check storage integrity

---

## 5. Reporting Security Issues

If you discover a vulnerability in Rango:

1. **Do NOT open a public issue.**
2. Email security details to: [security@rango-db.dev] (placeholder — update when real)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Affected versions/commits
   - Severity assessment
4. Allow 90 days for remediation before public disclosure.

---

## 6. Accepted Risks (MVP)

The following risks are explicitly accepted for the MVP/pilot phase with justification:

| Risk | Justification | Remediation Timeline |
|------|---------------|---------------------|
| No mTLS / client certificates | Complexity; Bearer tokens sufficient for initial pilot scope | Post-pilot |
| No rate limiting | Network perimeter + body size limit provide partial mitigation | Phase 7 completion |
| Tokens in memory as String | `zeroize` crate adds dependency complexity; acceptable for pilot | Phase 7 completion |
| No formal crypto audit | Well-known crates (RustCrypto) used; audit budgeted for v1.0 | Pre-v1.0 release |
| MemoryStorage for dev only | File-based persistent storage engine decision pending | Phase 2 completion |

---

## 7. Security Testing

### Automated
- `cargo audit` in CI (dependency CVE scanning)
- `cargo test --workspace` (unit + integration tests including encryption round-trips)
- Property-based testing with `proptest` for parser/serializer robustness

### Manual
- Path traversal attempts via CLI arguments
- Oversized document injection (>16MB)
- Malformed BSON/JSON in sync payloads
- Replay attacks with duplicate `write_id`
- MITM simulation with invalid TLS certificates

---

*This document is a living document. Review and update at each phase transition and before every release.*
