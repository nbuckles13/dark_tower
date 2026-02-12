# Code Reviewer - Integration Notes

Working with other specialists in Dark Tower.

---

## Integration: Security Specialist Handoff
**Added**: 2026-01-11

Flag security-critical findings (crypto, auth, validation) as MAJOR/CRITICAL for Security specialist. Verify cryptographic parameters match OWASP/NIST guidance. Defense-in-depth recommendations should be explicit.

---

## Integration: Test Specialist Collaboration
**Added**: 2026-01-11

After review, coordinate with Test specialist: boundary conditions covered, error paths exercised, security-critical paths have P0 tests. For config changes, verify both valid and invalid input tests exist.

---

## Integration: ADR Compliance Check
**Added**: 2026-01-11

Cross-reference code changes against ADRs. Key: ADR-0002 (no-panic), ADR-0003 (error handling), ADR-0001 (actor pattern), ADR-0023 (MC architecture). Flag violations as MAJOR requiring remediation.

---

## Integration: Service Foundation Patterns
**Added**: 2026-01-14
**Updated**: 2026-01-28

**Auth Controller** (most mature):
- Config: constants with OWASP/NIST refs, defense-in-depth validation
- Crypto: SecretBox for sensitive fields, custom Debug/Clone
- Error: ADR-0003 compliant with From implementations

**Global Controller**:
- Config: from_vars() for testing, fails on invalid security settings
- AppState: Arc<PgPool> + Config, all Clone
- Health: always 200 with status field, never error on probe failure
- Error context: preserve in error variants, log server-side, generic client message

**Meeting Controller**:
- Config: builder pattern with #[must_use], custom Debug redacts secrets
- Actors: Handle/Actor separation (ADR-0001), async state queries
- GC integration: unified task ownership (no Arc), never-exit resilience
- Error variants: match protocol (Grpc, Redis, not mixed)

---

## Integration: Common Crate Shared Utilities
**Added**: 2026-02-02

`token_manager.rs`: OAuth 2.0 client credentials with watch channel, spawn-and-wait API
`secret.rs`: SecretString/SecretBox for all credentials
`jwt.rs`: JWT validation constants and utilities

Check if code can use shared TokenManager instead of implementing OAuth logic.

---

## Integration: Observability Specialist (Prometheus Wiring)
**Added**: 2026-02-05

When reviewing internal metrics, coordinate on: (1) module-level docs clarifying which structs ARE wired, (2) naming conventions (ADR-0023), (3) label cardinality (ADR-0011), (4) emission frequency patterns. Flag missing docs when struct has increment methods but isn't wired (e.g., ControllerMetrics for GC heartbeat vs ActorMetrics for Prometheus).

---
