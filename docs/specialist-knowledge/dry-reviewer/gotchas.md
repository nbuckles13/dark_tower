# DRY Reviewer - Gotchas to Avoid

This file captures pitfalls and anti-patterns discovered during DRY reviews.

---

## Don't Flag Convention-Based Patterns as Duplication

**Added**: 2026-01-29
**Related files**: N/A (general principle)

**Gotcha**: Don't flag repeated patterns as duplication if they represent architectural conventions that are intentionally consistent across services. Examples include error handling patterns, logging formats, or metric naming schemes. Each service should own its domain-specific implementations while following project-wide conventions.

**How to distinguish**:
- **Harmful duplication**: Copy-pasted business logic, shared utilities coded multiple times, identical algorithms
- **Healthy alignment**: Consistent patterns with domain-specific context (error types, service names, operation descriptions)

**Rule of thumb**: If extracting the pattern would require creating abstractions that are more complex than the repetition itself, it's likely a convention, not duplication.

---

## Acceptable Internal Duplication (Same-File, Same-Purpose)

**Added**: 2026-01-31
**Related files**: `crates/mc-service/src/grpc/gc_client.rs:357-363, 436-442` (NOT_FOUND detection), `crates/mc-service/src/grpc/gc_client.rs:178-195, 471-488` (RegisterMcRequest construction)

**Gotcha**: Don't immediately flag duplication within a single file if:
1. Only 2 occurrences (not N occurrences)
2. Both serve similar purposes (e.g., fast_heartbeat vs comprehensive_heartbeat)
3. Change together (if one changes, the other should too)
4. Extraction would create helper with single caller pair

**Examples from ADR-0023 Phase 6c**:
- **NOT_FOUND detection** in both `fast_heartbeat()` and `comprehensive_heartbeat()` - identical 4-line pattern
- **RegisterMcRequest construction** in both `register()` and `attempt_reregistration()` - identical struct initialization

**Severity**: TECH_DEBT (not BLOCKER) - Note for future consolidation but don't block.

**When to escalate to BLOCKER**: If duplication appears 3+ times, spans multiple files, or represents business logic that could diverge.

**Rule of thumb**: 2 occurrences in same file = TECH_DEBT. 3+ occurrences or cross-file = consider BLOCKER.

---

## Test Code Structural Similarity is Often Justified

**Added**: 2026-01-31
**Related files**: `crates/mc-service/tests/gc_integration.rs:547-611` (test_heartbeat_not_found_detection vs test_comprehensive_heartbeat_not_found_detection)

**Gotcha**: Don't flag test code duplication if tests are structurally similar but test different code paths. Tests should prioritize clarity over DRY.

**When structural similarity is acceptable**:
- Each test calls a different method (fast_heartbeat vs comprehensive_heartbeat)
- The production code has duplication (e.g., TECH_DEBT-008 NOT_FOUND detection)
- Tests would be harder to understand if combined (parameterized tests can be opaque)

**Counter-pattern**: If tests are identical AND call the same code path with different inputs, use parameterized tests or table-driven tests.

**When reviewing**: For test code, ask "Does this test a different code path?" If yes, structural similarity is fine. If no, suggest consolidation.

---

## Proactive Common Code Placement Validates Pattern

**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`, `crates/mc-service/src/main.rs`

**Gotcha**: When shared code is placed in `common` proactively (before second consumer exists), don't flag the single-consumer case as "premature extraction." Instead, verify proper usage and note as validation.

**Example**: TokenManager was extracted to `common` on 2026-02-01 with only AC as consumer. MC integrated on 2026-02-02 and correctly used the shared code - no duplication occurred. The proactive placement was validated.

**How to review**:
1. Check that consumer uses `common` imports (not copy-paste)
2. Verify service-specific wrappers are appropriate (e.g., `McError::TokenAcquisition` wrapping `TokenError`)
3. Note as "Positive: Correct Use of Shared Code" in review
4. Track any patterns that emerged that should have been in common but weren't

**Anti-pattern**: Flagging proactive extractions as "over-engineering" - the whole point is to prevent duplication before it happens.

---

## Health Status Conversion Must Be Fail-Closed Consistent

**Added**: 2026-01-31
**Related files**: `crates/mc-service/src/grpc/health.rs`, `crates/mh-service/src/grpc/health.rs`

**Gotcha**: When services convert internal health status to gRPC health status (or vice versa), ensure **fail-closed** semantics are consistent across all services. If one service returns `Serving` when unknown and another returns `NotServing`, the system has inconsistent failure semantics - this is a security concern.

**Pattern**: Always default to the most restrictive status (`NotServing` or `Unknown`) when conversion is ambiguous. The safer pattern is:
- Unknown internal state -> `NotServing` (fail-closed)
- NOT: Unknown internal state -> `Serving` (fail-open, dangerous)

**When reviewing**: Flag inconsistent failure semantics as Priority 1 TECH_DEBT even if the duplication itself is minor. Escalate to Security specialist if production-critical.

---

## Service-Prefixed Metrics Are Convention, Not Duplication

**Added**: 2026-02-04 | **Updated**: 2026-02-10
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/ac-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`

**Gotcha**: Don't flag service-prefixed metric names (`gc_http_requests_total`, `ac_http_requests_total`, `mc_gc_heartbeats_total`) as duplication. Each service MUST have its own metric prefix per Prometheus best practices (ADR-0011). The prefix enables:
- Service identification in federated queries
- Per-service SLO alerting
- Isolation of cardinality explosions

**What IS duplication**: The recording helper functions (`record_http_request`, `record_db_query`, etc.) when they have identical signatures and logic. The middleware that calls them. The path normalization algorithms.

**What is NOT duplication**:
- Different metric names with service prefixes (`mc_gc_heartbeats_total` vs `gc_http_requests_total`)
- Different label sets appropriate to service domain (MC uses `type` label for heartbeat type, GC uses `endpoint` for HTTP paths)
- Service-specific observability modules in each crate (`crates/mc-service/src/observability/`, `crates/gc-service/src/observability/`)

**Architectural context**: Per ADR-0011, each service maintains its own observability module. There is NO shared metrics infrastructure in `crates/common/`. This is intentional - metrics are service-specific.

**When reviewing**:
1. Check if `crates/common/` has the pattern (if yes and not imported → BLOCKER)
2. If pattern exists only in other services (not in common) → TECH_DEBT
3. Focus on function/algorithm duplication, not metric naming conventions

---

## Infrastructure Artifacts Follow Reference Pattern, Not DRY

**Added**: 2026-02-08
**Related files**: `infra/grafana/dashboards/gc-overview.json`, `infra/docker/prometheus/rules/gc-alerts.yaml`, `docs/runbooks/gc-deployment.md`, `docs/runbooks/gc-incident-response.md`

**Gotcha**: Infrastructure artifacts (Grafana dashboards, Prometheus alerts, operational runbooks) follow a **reference pattern** approach where:
- First implementation (AC) establishes the pattern
- Subsequent implementations (GC, MC, MH) follow the same structure with service-specific content
- Structural similarity is **intentional consistency**, not harmful duplication

**This differs from code DRY because**:
- These artifacts are configuration, not executable code
- Operators benefit from consistent structure across services
- Templating tools (Grafonnet, Jsonnet) add build complexity
- No common crate equivalent exists for infrastructure config

**Classification**:
- BLOCKER: Never for following the established pattern
- TECH_DEBT: Boilerplate that could be templated (dashboard JSON, alert YAML)
- NOT duplication: Service-specific metrics, scenarios, configuration values

**When reviewing**: If new service follows AC/GC pattern exactly but with different service name and metrics, mark as "Positive: Follows Reference Pattern" not as duplication. Only flag TECH_DEBT for boilerplate that would benefit from templating.

---

## Clear Responsibility Separation Means Different Data Types May Coexist

**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/actors/metrics.rs:248-323` (ControllerMetrics), `crates/mc-service/src/actors/metrics.rs:325-425` (ActorMetrics)

**Gotcha**: Don't flag two very similar metric structs as DRY violation if they serve fundamentally different purposes and are used by different subsystems. In MC:
- `ControllerMetrics` (u32): For heartbeat reporting to GC, reported via RPC
- `ActorMetrics` (usize/u64): For Prometheus emission and internal tracking

They happen to both track `meetings` and `connections`, but:
- ControllerMetrics uses atomic u32 and `SeqCst` ordering (cross-thread visibility for heartbeat)
- ActorMetrics uses atomic usize and `Relaxed` ordering (fast internal tracking)
- ControllerMetrics.current_participants never flows to Prometheus
- ActorMetrics flows only to Prometheus, not to GC

**Rule**: If two metrics structs serve different consumers with different synchronization needs, they're not duplication. Document the purpose of each and explain why they can't be merged.

**When reviewing**: Ask "What system consumes this metric?" If different, separation is intentional.

---

## Thin Wrapper Similarity After Extraction is Healthy Convention

**Added**: 2026-02-12 | **Updated**: 2026-02-12 (iteration 2)
**Related files**: `crates/gc-service/src/tasks/health_checker.rs:37-65`, `crates/gc-service/src/tasks/mh_health_checker.rs:37-65`

**Gotcha**: After extracting shared logic into a generic function, the remaining thin wrappers will still look structurally similar (~25 lines each: startup log, delegation call with `.instrument()` chaining, shutdown log). Do NOT flag this residual similarity as duplication requiring further extraction. The wrappers differ in:
- `.instrument(tracing::info_span!("..."))` span names (string literals)
- `target:` values in lifecycle logs (must be string literals for tracing)
- Entity name parameter (e.g., `"controllers"` vs `"handlers"`)
- Closure body (different repository method)

These differences cannot be further extracted without macros or losing tracing fidelity. The wrapper layer is the minimum viable domain-specific surface.

**Rule**: After a successful extraction, verify the wrappers contain ONLY domain-specific configuration and delegation. If they still contain shared logic (loops, error handling, control flow), the extraction is incomplete. If they contain only config + delegation, the extraction is complete and the wrapper similarity is acceptable.

---

## Same Label Name with Different Semantics Across Services

**Added**: 2026-02-16
**Related files**: `crates/mc-service/src/errors.rs` (`status_code()`), `crates/gc-service/src/errors.rs` (`IntoResponse`)

**Gotcha**: The `status_code` label in `mc_errors_total` and `gc_errors_total` has different semantics:
- **GC**: HTTP status codes (200, 400, 401, 403, 404, 429, 500, 503) — standard HTTP semantics
- **MC**: Signaling error codes (2, 3, 4, 5, 6, 7) — WebTransport signaling protocol codes

Both use `status_code` as the label name, but the values are incomparable. GC uses HTTP because it's an HTTP/3 gateway. MC uses signaling codes because it communicates via WebTransport, not HTTP.

**This is NOT duplication or a DRY violation.** The label name alignment is a convention (both track "what kind of error code"), but the value domains are domain-appropriate. Do NOT flag this as needing unification.

**When reviewing**: If a dashboard or alert query joins `status_code` across GC and MC metrics, flag it — the values are not comparable. Each service's dashboards should interpret `status_code` in the context of that service's protocol.

---

## Config Struct vs Plain Parameters in Generic Extractions

**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

**Gotcha**: When extracting a generic function, resist the temptation to group domain-differentiating values into a config struct if there are only 1-2 parameters. A `HealthCheckerConfig { display_name, entity_name }` struct was introduced in iteration 1 and removed in iteration 2 because:
- It added indirection (callers had to construct a struct just to pass 2 strings)
- The `display_name` field led to fragile API conventions (empty string `""` vs `"MH "` with trailing space)
- A plain `entity_name: &'static str` parameter is clearer and avoids the struct boilerplate

**Rule of thumb**: Use a config struct when 3+ domain-specific parameters differentiate wrappers. For 1-2 parameters, use plain function arguments. If a "display name" is only used to prefix log messages, consider whether the wrapper's own lifecycle logs (with literal strings) already handle differentiation, making the field unnecessary.

**When a config struct IS appropriate**: `AssignmentCleanupConfig` in the same crate has 4+ fields with meaningful semantics beyond simple string differentiation. That level of parameterization justifies a struct.

---
