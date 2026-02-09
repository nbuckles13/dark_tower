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
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs:357-363, 436-442` (NOT_FOUND detection), `crates/meeting-controller/src/grpc/gc_client.rs:178-195, 471-488` (RegisterMcRequest construction)

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
**Related files**: `crates/meeting-controller/tests/gc_integration.rs:547-611` (test_heartbeat_not_found_detection vs test_comprehensive_heartbeat_not_found_detection)

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
**Related files**: `crates/common/src/token_manager.rs`, `crates/meeting-controller/src/main.rs`

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
**Related files**: `crates/meeting-controller/src/grpc/health.rs`, `crates/media-handler/src/grpc/health.rs`

**Gotcha**: When services convert internal health status to gRPC health status (or vice versa), ensure **fail-closed** semantics are consistent across all services. If one service returns `Serving` when unknown and another returns `NotServing`, the system has inconsistent failure semantics - this is a security concern.

**Pattern**: Always default to the most restrictive status (`NotServing` or `Unknown`) when conversion is ambiguous. The safer pattern is:
- Unknown internal state -> `NotServing` (fail-closed)
- NOT: Unknown internal state -> `Serving` (fail-open, dangerous)

**When reviewing**: Flag inconsistent failure semantics as Priority 1 TECH_DEBT even if the duplication itself is minor. Escalate to Security specialist if production-critical.

---

## Service-Prefixed Metrics Are Convention, Not Duplication

**Added**: 2026-02-04
**Related files**: `crates/global-controller/src/observability/metrics.rs`, `crates/ac-service/src/observability/metrics.rs`, `crates/meeting-controller/src/actors/metrics.rs`

**Gotcha**: Don't flag service-prefixed metric names (`gc_http_requests_total`, `ac_http_requests_total`, `mc_mailbox_depth`) as duplication. Each service MUST have its own metric prefix per Prometheus best practices. The prefix enables:
- Service identification in federated queries
- Per-service SLO alerting
- Isolation of cardinality explosions

**What IS duplication**: The recording helper functions (`record_http_request`, `record_db_query`, etc.) when they have identical signatures and logic. The middleware that calls them. The path normalization algorithms.

**What is NOT duplication**: Different metric names, different service prefixes, different label sets appropriate to service domain.

**When reviewing**: Focus on function/algorithm duplication, not metric naming conventions.

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
