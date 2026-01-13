# Security Specialist Integration Guide

What other specialists need to know about security requirements in Dark Tower.

---

## Integration: Test Specialist Requirements
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Security tests MUST cover: (1) Boundary values for security parameters, (2) Invalid input rejection, (3) Error message uniformity, (4) Timing consistency (manual verification). Use `#[cfg_attr(coverage, ignore)]` for timing tests.

---

## Integration: Auth Controller Compliance
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Bcrypt cost must be 10-14 (OWASP 2024). Default 12. JWT clock skew 1-600 seconds, default 300 (NIST recommendation). Token lifetime 1 hour. Rate limit: 5 failures in 15 min triggers lockout.

---

## Integration: Database Specialist - Secrets Storage
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/registration_service.rs`

Never store plaintext secrets. Password hashes use bcrypt. Encryption keys use AES-256-GCM with master key. Parameterized queries only (sqlx compile-time check). Log client_id but NEVER client_secret.

---

## Integration: Operations - Security Config
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Production deployments MUST set: `AC_MASTER_KEY` (32-byte base64), `AC_HASH_SECRET` (not default). Verify `BCRYPT_COST` is at least 12. TLS/SSL required for DATABASE_URL. Alert on bcrypt cost warnings in logs.

---

## Integration: Observability - Security Metrics
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/observability/metrics.rs`

Track: auth failures (rate), rate limit triggers, bcrypt latency (p99), JWT validation errors. Alert thresholds: >10 auth failures/min, bcrypt p99 >500ms, rate limit bursts. Never include secrets in traces.

---

## Integration: Protocol Specialist - Error Contracts
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/errors.rs`

API errors must be generic for security endpoints. Use HTTP 401 for all auth failures (not 404 for missing users). Error bodies: `{"error": "invalid_client"}` - no details. Document allowed error codes only.

---

## Integration: Code Review - Security Checklist
**Added**: 2026-01-11
**Related files**: `.claude/agents/security.md`

All PRs touching auth/crypto need Security specialist review. Check: no timing leaks, no error enumeration, input validation, parameter bounds, secret handling. Block merge on security concerns.

---

## Integration: Code Review - SecretBox/SecretString Verification
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

When reviewing code with secrets: (1) Grep for `.expose_secret()` - each call is a potential leak, verify necessity, (2) Check custom Debug impls redact with `[REDACTED]`, (3) Verify custom Serialize only on "one-time reveal" response types, (4) Confirm Clone impls re-wrap in SecretBox. Any raw `String`/`Vec<u8>` holding secrets is a finding.

---

## Integration: Infrastructure - Key Management
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Master keys via environment variables or secrets manager. Never in code/config files. Key rotation: `POST /internal/rotate-keys` with proper scopes. Old keys valid 24 hours post-rotation.

---
