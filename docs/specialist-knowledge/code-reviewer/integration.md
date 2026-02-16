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

## Integration: Cross-Crate Callback for Metrics (TokenManager Pattern)
**Added**: 2026-02-15

When `common` crate components (e.g., `TokenManager`) need service-specific metrics but cannot depend on the service crate, use the callback injection pattern: `Arc<dyn Fn(Event) + Send + Sync>` stored as an `Option` field with a builder method. The callback event struct should use `&'static str` for label fields (not `String`) to enforce cardinality bounds at the type level. The service wires the callback in `main.rs` to bridge into its own metrics module. Coordinate with Observability reviewer on metric naming and with Security reviewer that no secrets leak through the event struct.

---

## Integration: Replicating Metrics Patterns Across Services (GC -> MC)
**Added**: 2026-02-16

When a metrics pattern is established in one service (e.g., GC's `record_token_refresh()`, `error_type_label()`, `record_error()`, `.with_on_refresh()` wiring), replicating it to another service (MC) should match the pattern exactly in structure but adapt semantics to the service's protocol. Key review checklist: (1) function signatures match (`record_token_refresh(status, error_type, duration)`), (2) metric name prefix changes (`gc_` -> `mc_`), (3) histogram buckets match when SLOs are shared (token refresh uses identical buckets), (4) `error_type_label()` return type is `&'static str` with exhaustive match, (5) dashboard PromQL uses correct prefix, (6) SLO thresholds are consistent. The `status_code()` method signature is kept identical but returns protocol-specific values (HTTP for GC, signaling for MC) -- verify dashboard interpretation.

---

## Integration: Validation Guards vs Plan Agreements
**Added**: 2026-02-12
**Updated**: 2026-02-12

Project-wide validation guards (e.g., `instrument-skip-all` requiring all async functions with parameters to have `#[instrument(skip_all)]`) may conflict with plan-phase agreements. During TD-13 iteration 1, we agreed the generic function should not have `#[instrument]`, but the guard required it. **Resolution in iteration 2**: removing `#[instrument]` entirely from the generic function and using `.instrument()` chaining in callers avoids the guard entirely, since the guard only pattern-matches on `#[instrument(` attributes, not the `Instrument` trait method. This is the preferred resolution when the goal is caller-controlled spans. When reviewing, still check whether deviations from the plan are due to guard requirements before flagging them.

---
