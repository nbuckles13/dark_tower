# ADR-0032: Metric Testability — Component Tests + Presence Guard

## Status

Accepted

**Date**: 2026-04-20

**Deciders**: observability (domain lead), test, code-reviewer, auth-controller, global-controller, meeting-controller, media-handler, operations

> **Revision note**: The original consensus (see debate, Round 2) landed on a four-tier pattern with bounded production-code hooks (`TestHooks`), a monotonic baseline ratchet, and four guards. Post-debate review by the lead surfaced concerns: TestHooks invasive, the baseline ratchet had no auto-generation mechanism, `#[must_use]` G2 caught the wrong failure mode, and G3/G4 were decoration. The decision below is the reductive amendment — same diagnosis, simpler mechanism, no production-code modification. See debate addendum for the conversation that drove the revision.

---

## Context

A cross-service audit on 2026-04-20 (`docs/observability/metrics-coverage-audit-2026-04-20.md`) surveyed 778 metric-recording sites across AC, GC, MC, and MH and found ~28% uncovered (~218 sites) plus ~5% wrapper-only (~40 sites). The gap follows five cross-cutting failure patterns:

1. **Accept loops** (MH, MC) — production `accept_loop` spawns per-connection handlers as fire-and-forget tasks. Tests bypass the loop, so accept-path counters and the active-connections gauge are unreachable.
2. **Token-manager refresh callbacks** (GC, MC, MH) — closures registered with `TokenManager::on_refresh` record metrics; no test drives a refresh cycle.
3. **Repository error branches & error-classification labels** (GC, AC) — services record on success and typed-error branches; happy paths are tested, error branches aren't.
4. **Fire-and-forget spawns** (MC→MH notify retry) — spawned task records, returns `Err`, the `Err` is dropped.
5. **Capacity-rejection paths** (MH, MC) — `record_*("rejected")` only fires under load; tests don't push load.

The MH accept-loop case (`docs/specialist-knowledge/observability/TODO.md`) is the canonical representative — it has recurred in five recent devloops. The existing metric guard (`scripts/guards/simple/validate-application-metrics.sh`) enforces metric↔dashboard coverage but does not address test reachability.

This ADR defines the testing tier and enforcement mechanism that closes the gap without modifying production code.

## Decision

### Principle

**Every metric emitted by `crates/{service}/src/**/*.rs` MUST be referenced by at least one test in `crates/{service}/tests/**/*.rs`.** Reference is by metric name (string match); per-label keying is optional and recommended for metrics with cardinality > 1.

Production code is not modified to add test affordances. No opt-in result channels, no test hooks, no visibility carve-outs in service-side code. Tests adapt to production, not the other way around.

### Testing tier: component tests

Metric coverage lives in `crates/{service}/tests/` — the **component-test tier**, between unit tests and env-tests:

- Above unit tests: component tests can drive a real server (real `accept_loop`, real spawns, real handlers) so accept-path metrics are reachable without bypassing production code.
- Below env-tests: component tests run in-process against `DebuggingRecorder`, so negative cases (capacity exhaustion, error branches, expired tokens) are easy to trigger without infrastructure setup.

Each service owns its component tests under `crates/{service}/tests/` per existing convention. Patterns the tier supports:

| Failure shape | Component-test approach |
|---|---|
| Accept-loop status counters | Spin up the real server with `max_connections=1`; open one connection (asserts `accepted`), open a second (asserts `rejected`), close (asserts `error` if applicable, asserts gauge decrement). |
| Token-refresh callbacks | Drive AC token issuance + simulate refresh-failure path; observe `mc_token_refresh_total{status="failed"}` etc. |
| Repository error branches | Inject error at the repository seam (real DB + targeted fault, or repo trait + fake) and observe the error-label counter. |
| Fire-and-forget spawn classification | Run the spawning code path against a mock downstream that returns `Err`; observe the metric after a bounded `tokio::time::timeout` poll on the registry. |
| Capacity rejection | Configure low capacity; saturate; observe. No load test needed. |

Where a metric branch is genuinely unreachable from a component test because of how the surrounding code is structured (e.g. metric inside a closure captured by a long-lived task with no test seam), refactor the surrounding code first — extract the closure body into a named function and call that from the component test. This is the only refactor the ADR mandates, and only when reachability fails.

### `MetricAssertion` test helper

A shared utility in `crates/common/src/observability/testing.rs`, backed by `metrics-util::debugging::DebuggingRecorder`. Used by component tests for delta/value assertions:

```rust
let snap = MetricAssertion::snapshot();
run_code_under_test().await;

// Predictable values
snap.counter("ac_errors_total")
    .with_labels(&[("operation", "create"), ("outcome", "error")])
    .assert_delta(1);

// Unpredictable values (durations, sizes, etc.)
snap.histogram("mh_webtransport_handshake_duration_seconds")
    .assert_observation_count_at_least(1);

snap.gauge("mh_active_connections").assert_value_in_range(0.0..=10.0);
```

Snapshots are **label-tuple-scoped** so parallel tests asserting on the same metric name with different labels do not collide. Recorder stays process-global; isolation is at query time.

For metrics whose **value** is unpredictable (timing histograms, queue-depth gauges, payload-size counters), the assertion shape is "the metric was emitted" — `assert_observation_count_at_least(N)` for histograms, `assert_value_in_range(lo..=hi)` for gauges. The point is to detect *absence* of emission, not to verify exact values; production observability dashboards verify the value distributions in aggregate.

### Explicitly rejected

- **HTTP scraping in tests.** Slow, couples to exposition format, parallel-registry races without scoped recorders.
- **Production-code modification for test affordance** (test hooks, opt-in result channels, `#[doc(hidden)]` test constructors). Test-shape should not leak into production API even when crate-scoped. The component-test tier eliminates the need.
- **Moving accept-loop metrics post-handler.** Loses pre-handler accept stats including capacity-rejection.
- **Defer to load tests / canaries as primary.** Canaries detect live traffic, not label-rename regressions or absent emission.
- **A baseline-ratchet guard with no auto-generation mechanism.** Without a tool that re-derives the baseline from source, the file rots.
- **`#[must_use]` enforcement on snapshot return types as the primary guard.** Catches "started snapshot, forgot to assert" — a tiny fraction of the failure surface. The dominant failure is "didn't write the test at all," which `#[must_use]` does not catch. (`#[must_use]` is fine as an ergonomic helper on the `MetricAssertion` API itself; it's not load-bearing for enforcement.)
- **Spawn-body grep guards, PR-template checklist lines.** Decoration, not enforcement.

## Enforcement

**One guard.** `scripts/guards/simple/validate-metric-coverage.sh`:

1. For each service crate (`crates/{ac,gc,mc,mh}-service/`):
   - Scan `src/**/*.rs` for metric emission sites: `metrics::counter!`, `histogram!`, `gauge!`, `record_*(...)` calls referencing a metric name.
   - Extract the set of unique metric names emitted.
2. For each emitted metric name:
   - Scan `tests/**/*.rs` (component tests) for any string occurrence of the metric name.
   - Fail the guard if zero occurrences.
3. **Optional per-label scoping**: for metrics declared with cardinality > 1 in `docs/observability/metrics/{service}.md`, fail if the test references the metric name but does not also reference each label-value emitted by source. (Implementation can defer this — the metric-name presence check is the primary lever.)

This guard is mechanical, deterministic, and decays cleanly: when a metric is renamed in source, the guard fails until the test is updated. When a metric is removed, the guard fails until the dead test reference is removed. No baseline file, no manual ratchet.

**Trivial-dodge concern**: a developer could write `let _ = "ac_errors_total";` in a test to satisfy the grep. Mitigations:
- The reference must be in the same service's `tests/` directory — out-of-scope dodges fail.
- Reviewer enforcement at PR time (the PR diff shows both the source change and the test change side by side).
- The audit shows the actual failure mode is "no test at all," not "trivial test to satisfy grep" — empirical risk of dodge is low.

If the dodge becomes a real problem in practice, upgrade the guard to require the metric name to appear inside a `MetricAssertion::*` call (the same shape `MetricAssertion::counter("ac_errors_total")` uses), which is structurally close to "actually asserts on it." Defer that upgrade until evidence shows it's needed.

## Consequences

### Positive

- **One guard, one rule.** Reviewers know what to enforce; engineers know what to satisfy.
- **Production code untouched.** No `TestHooks`, no opt-in surfaces, no visibility carve-outs.
- **Component-test tier is the obvious home** for cross-cutting test work that doesn't fit unit or env-test tiers — establishing it for metric coverage builds the muscle for other cross-cutting concerns.
- **Capacity-rejection becomes CI-verifiable** by configuring low capacity in a component test, no load test required.
- **Unpredictable-value metrics get an honest treatment**: `at_least_once` / `value_in_range`, not fake exact-value assertions.

### Negative

- **Component-test directory may grow significantly** for services with many uncovered sites (AC, MC). Per-service SLO dates account for this.
- **Trivial-dodge surface exists** but is reviewer-catchable and low-frequency in practice.
- **No per-PR ratchet** — a regression that drops a metric and its test in one PR will pass the guard. PR review is the catch.
- **Component-test infrastructure** (server harness, mock-downstream helpers) needs to exist per service. AC, MC, MH already have it; GC's `gc-test-utils` covers it. No new harness work required at the test-tier level.

### Neutral

- `MetricAssertion` API exists in `crates/common/src/observability/testing.rs` but is now a convenience helper, not a structural tier.
- Existing `validate-application-metrics.sh` (metric↔dashboard coverage) is unaffected and continues to operate.

## Rollout & Operations

### Per-service PR scope

Three categories from the original consensus carry forward, with one change: production-code modification is now rare under this approach (most work is test-only).

**Category A — Production-code change** (canary deploy gate + raw `/metrics` evidence)
- Closure-extraction-with-captures (rare; only when reachability from a component test demands it).
- Server/service constructor changes (uncommon under this ADR).

**Category B — Pure extraction, byte-identical behavior** (no canary, no `/metrics` evidence)
- Stateless closure-to-named-fn extractions where the closure captured nothing (no `move`, no named captures).
- Pure `fn foo()` extractions.

**Category C — Test-only** (no canary, no `/metrics` evidence)
- Adding component tests that exercise existing metric sites.
- Adding repo-trait fakes or DB fault injection harness in `tests/`.

The bulk of ADR-0032 implementation work is Cat C. Per-service Cat A PRs land separately with `/metrics` evidence; Cat B and Cat C can batch within a service.

### Per-service SLO sub-targets

Aggregate target: 28% uncovered → <10% by 2026-07, <5% by 2026-10. Per-service:

| Service | Current uncovered | 2026-07 target | 2026-10 target |
|---------|-------------------|----------------|----------------|
| AC | 22% (~41 sites) | <10% (<19 sites) | <5% (<10 sites) |
| GC | 11% (~21 sites) | <6% (<11 sites) | <3% (<6 sites) |
| MC | 24% (~65 sites) | <12% (<32 sites) | <6% (<16 sites) |
| MH | 11% (~14 sites) | <6% (<7 sites) | <3% (<4 sites) |

### Tiered ownership

| Layer | Owner |
|-------|-------|
| Per-service component-test backfill | Service owner (AC/GC/MC/MH) |
| `validate-metric-coverage.sh` guard implementation | Observability |
| `MetricAssertion` helper | Observability |
| SLO dates and stall policy (4-week → ticket; 8-week → escalate) | Operations |

### CI cost

The guard runs inside the existing `run-guards.sh` pipeline. Estimated added runtime: <5 seconds (pure file scanning, no test execution).

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| `MetricAssertion` helper in `crates/common/src/observability/testing.rs` | ❌ Pending | `DebuggingRecorder`-backed; `assert_delta` / `assert_value_in_range` / `assert_observation_count_at_least` variants. |
| `validate-metric-coverage.sh` guard | ❌ Pending | Source scan + test reference check. Wires into `run-guards.sh`. |
| Audit correction | ❌ Pending | `docs/observability/metrics-coverage-audit-2026-04-20.md` amendment: AC `main.rs:115` is `init_key_metrics()`, not an `on_refresh` closure. AC has no Pattern #2 sites. |
| AC component-test backfill | ❌ Pending | ~41 sites. Cat C (test-only). |
| GC component-test backfill | ❌ Pending | ~21 sites. Mix of Cat B (token-refresh closure extraction) + Cat C. |
| MC component-test backfill | ❌ Pending | ~65 sites. Mix of Cat B + Cat C. Includes deletion of `join_tests.rs:213-246` `TestServer::accept_loop` fork once a real-`accept_loop` component test replaces it. |
| MH component-test backfill | ❌ Pending | ~14 sites. Mix of Cat B + Cat C. Includes simplification of `crates/mh-service/tests/common/wt_rig.rs` — the bypass becomes unnecessary once a component test exercises the real `accept_loop`. |

**Status values**: ✅ Done | 🚧 In Progress | ❌ Pending | ⏸️ Deferred

## Implementation Notes

- **Suggested specialists**: observability (lead, owns guard + helper), media-handler (canonical-case backfill first), then meeting-controller, then auth-controller and global-controller in parallel.
- **Phasing**:
  1. Land `MetricAssertion` utility + `validate-metric-coverage.sh` guard + audit correction in a single PR. Guard is gating from the start; the lead sequences subsequent per-service backfill PRs to keep the pipeline manageable.
  2. MH component-test backfill (canonical case — accept-loop and `wt_rig` simplification).
  3. MC component-test backfill (mirrors MH; deletes `TestServer::accept_loop` fork).
  4. AC and GC component-test backfill (parallel — no shared structure with MH/MC, no dependency on each other). AC is Cat C only; GC is Cat B token-refresh + Cat C error branches.
- **Audit correction lands first** so the baseline counts are accurate before any backfill PRs.
- **Where a metric resists a component test**: extract the surrounding code (Cat A or B refactor), don't bend the test infrastructure to reach into production internals.

## Participants

Original Round-2 consensus: 8 specialists at 90+ satisfaction (mean 94) on the four-tier pattern. Final positions in `docs/debates/2026-04-20-metric-testability/debate.md` Final Positions table. The post-debate revision recorded in this ADR is reductive (less production-code modification, fewer guards, single rule) and was not re-presented for fresh consensus per ADR-0024 §reductive-amendments.

Specialists with the largest stake in the revision should be sanity-checked at implementation time:
- **Operations**: SLO ratchet machinery is replaced by the simpler per-PR guard. Per-service SLO targets and stall policy carry forward.
- **Observability**: domain ownership of guard implementation + `MetricAssertion` helper unchanged.
- **Media-handler / Meeting-controller**: TestHooks no longer needed — confirms simplification, no work removed from their plate, only added (component tests they would have written anyway).
- **Code-reviewer**: enforcement surface shrunk from 4 guards to 1; the per-failure-class table from the original consensus still applies as a *review heuristic* even though the ADR no longer formalizes it.

## Debate Reference

See: `docs/debates/2026-04-20-metric-testability/debate.md` (includes post-debate addendum capturing the revision rationale).

## References

- `docs/observability/metrics-coverage-audit-2026-04-20.md` — audit data (778 sites, 5 cross-cutting patterns).
- `docs/specialist-knowledge/observability/TODO.md` — canonical MH accept-loop case.
- `crates/mh-service/tests/common/wt_rig.rs:14-21` — bypass workaround the component-test approach makes unnecessary.
- `scripts/guards/simple/validate-application-metrics.sh` — existing metric↔dashboard guard (unaffected).
- ADR-0011 — Observability framework.
- ADR-0019 — DRY reviewer (single mechanism, not per-service variation).
- ADR-0024 — Validation pipeline (guards methodology, reductive-amendment provision).
- ADR-0029 — Dashboard metric presentation (unaffected).
