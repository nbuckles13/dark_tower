# Devloop Output: ADR-0032 Step 1 — MetricAssertion + Guard + Audit Correction

**Date**: 2026-04-20
**Task**: Land MetricAssertion test utility, validate-metric-coverage.sh guard, and audit correction per ADR-0032 §Implementation Notes phasing step 1
**Specialist**: observability
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-tests`
**ADR**: `docs/decisions/adr-0032-metric-testability.md`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `bc8c0d352cdc714da15d9eeb002e15ea4e6af792` |
| End Commit | `7347ceb` |
| Branch | `feature/mh-quic-mh-tests` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@adr-0032-step-1` |
| Implementing Specialist | `observability` |
| Iteration | 2 |
| Security | `security@adr-0032-step-1` |
| Test | `test@adr-0032-step-1` |
| Observability | `implementer@adr-0032-step-1` (implementer fills domain) |
| Code Quality | `code-reviewer@adr-0032-step-1` |
| DRY | `dry-reviewer@adr-0032-step-1` |
| Operations | `operations@adr-0032-step-1` |

---

## Task Overview

### Objective
Land the foundational testing infrastructure for ADR-0032 in a single PR:
1. `MetricAssertion` test utility (shared across services).
2. `validate-metric-coverage.sh` guard wired into `run-guards.sh`.
3. Audit correction (AC main.rs:115 is `init_key_metrics`, not `on_refresh`).

### Scope
- **Service(s)**: `crates/common/` (new module), all 4 service `tests/` directories will eventually use it
- **Schema**: No
- **Cross-cutting**: Yes — new shared module + new guard + accepted interim CI breakage
- **Mode rationale**: Full — touches `crates/common/` and adds `Cargo.toml` deps (both explicit `--light` exclusions per SKILL)

### Three deliverables

1. **MetricAssertion** at `crates/common/src/observability/testing.rs`:
   - Backed by `metrics-util::debugging::DebuggingRecorder` + Snapshotter
   - API per ADR-0032 §Tier B:
     - `MetricAssertion::snapshot()` → returns Snapshot
     - `snap.counter(name).with_labels(&[(k,v)]).assert_delta(N)`
     - `snap.gauge(name).assert_value(N)` and `assert_value_in_range(lo..=hi)`
     - `snap.histogram(name).assert_observation_count(N)` and `assert_observation_count_at_least(N)`
   - Label-tuple-scoped snapshots so parallel tests on different label tuples don't collide
   - Snapshot return types `#[must_use]` (ergonomic, NOT load-bearing for enforcement)
   - Document `#[serial_test::serial]` as fallback for same-(metric,labels) parallel cases
   - Unit tests for the helper itself

2. **validate-metric-coverage.sh** at `scripts/guards/simple/validate-metric-coverage.sh`:
   - For each service crate (`crates/{ac,gc,mc,mh}-service/`), scan `src/**/*.rs` for metric emission sites
   - Extract metric names (from `metrics::counter!`, `histogram!`, `gauge!`, `record_*(...)`)
   - Scan `crates/{service}/tests/**/*.rs` for matching string occurrences
   - Fail if any emitted metric has zero test references
   - **Wire into `scripts/guards/run-guards.sh`** — gating from start per ADR-0032 phasing
   - The workspace will fail this guard for ~218 currently-uncovered sites; lead has accepted this interim state

3. **Audit correction** at `docs/observability/metrics-coverage-audit-2026-04-20.md`:
   - AC `main.rs:115` is `init_key_metrics()`, not an `on_refresh` closure
   - AC has no Pattern #2 sites
   - Adjust AC uncovered count by -1 (~41 → ~40)

### Validation note (likely Layer 3 escalation)
The new guard, once wired, will fail for the ~218 uncovered sites across all services. Per ADR-0032, this is an accepted interim state — the lead has explicitly chosen "guard is gating from the start; lead sequences subsequent per-service backfill PRs." The Step 1 devloop's Layer 3 (guards) will fail because of this. Lead will override the validation failure once it's confirmed to be the new guard's expected failure (not a regression in any other guard).

---

## Mid-Flight Revision 2026-04-23

Iteration 1 (Apr 20 session) implemented `MetricAssertion` on the
process-global `DebuggingRecorder::install()` pattern — see `testing.rs:119-126`,
the `SNAPSHOTTER: OnceLock<Snapshotter>` path. On resync, lead flagged that
the `metrics` facade 0.24 (already in the workspace) exposes thread-scoped
recorder APIs that give true per-test isolation:

- `metrics::set_default_local_recorder(&recorder) -> LocalRecorderGuard` —
  RAII guard binding the recorder to the current thread for its lifetime.
- `metrics::with_local_recorder(&recorder, || { ... })` — closure-scoped.

These let each `MetricAssertion::snapshot()` call own a fresh
`DebuggingRecorder`. Tests never share recorder state across threads, the
same-`(metric, labels)` tuple footgun disappears, and the label-tuple
parallel-safety rationale baked into the current `testing.rs` collapses to
a short note about not holding two overlapping snapshots on one thread.

### Scope of rework

**Only `crates/common/src/observability/testing.rs` changes.** The guard
(`scripts/guards/simple/validate-metric-coverage.sh`), the audit correction
(`docs/observability/metrics-coverage-audit-2026-04-20.md`), and the cargo
wiring (`Cargo.toml`, `crates/common/Cargo.toml`, `crates/common/src/lib.rs`,
`crates/common/src/observability/mod.rs`) stay as-is in the worktree —
those three deliverables were correct and are landing-ready.

Required changes to `testing.rs`:

1. Remove the `SNAPSHOTTER: OnceLock<Snapshotter>` static and the global
   `recorder.install()` call in `snapshotter()`.
2. `MetricAssertion::snapshot()` instantiates a fresh `DebuggingRecorder`,
   captures its `Snapshotter`, and calls
   `metrics::set_default_local_recorder(&recorder)` to bind it to the
   current thread.
3. `MetricSnapshot` owns the `LocalRecorderGuard` (and the `Snapshotter`)
   so the thread-local binding releases when the snapshot drops.
4. `counter_value` / `gauge_value` / `histogram_count` / `take_entries`
   read from the per-snapshot `Snapshotter`, not a global one. The
   `pre: Vec<(CompositeKey, DebugValue)>` cache can stay or be dropped —
   with per-thread isolation, re-snapshotting at assert time is cheap and
   always captures only this test's emissions.
5. Rewrite the "Parallel tests", "Delta semantics", and `#[must_use]`
   docstring sections to match the new model: snapshots are per-thread;
   label-tuple scoping is no longer load-bearing; `#[serial_test::serial]`
   is no longer needed as a parallel-safety fallback (retain mention only
   if required for the in-file unit tests' own shared state).
6. In-file unit tests: drop `#[serial_test::serial]` attributes where the
   per-thread recorder makes them unnecessary; simplify or remove the
   `unique_name()` helper if the per-snapshot recorder gives each test a
   clean slate regardless of metric name.
7. Drop the `serial_test` dev-dep from `crates/common/Cargo.toml` if it is
   no longer used by the unit tests after step 6.

### API surface unchanged

Public API (`snap.counter(name).with_labels(...).assert_delta(N)`,
`snap.gauge(name).assert_value_in_range(r)`,
`snap.histogram(name).assert_observation_count_at_least(n)`) stays identical.
No consumer-side change required for Step 2+ backfill PRs.

### Plan confirmations carried forward

The 6-reviewer plan consensus (Plan Confirmations table below) stands for:
guard, audit correction, cargo wiring, public API shape of `MetricAssertion`.
The recorder-mechanism change is an internal implementation detail that
preserves every externally-observable property from the original plan.
Reviewers re-confirm only the narrower `testing.rs` change in iteration 2.

### Loop state

- Iteration bumped to 2.
- Phase remains `implementation` — scope shrunk, not restarted.
- Review verdicts stay `pending` — the iteration-1 pass never completed.

## Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (test-only, no prod exposure; `test-utils` feature gate appropriate) |
| Test | confirmed (API shape, isolation, must_use ergonomic-only, absent-pre-present-post unit test) |
| Observability | n/a (implementer fills domain) |
| Code Quality | confirmed (serial_test narrow dev-dep only for unlabeled gauges; 3-case assert_delta semantics; guard narrowed to metrics.rs; pub mod testing gated at mod.rs) |
| DRY | confirmed (helper in common, metrics-util workspace-level, guard sources common.sh) |
| Operations | confirmed (exit code conventions, output format, no ci.yml changes) |

### Iteration 2 re-confirmations (narrower scope: testing.rs mechanism swap)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (iter-1 carries forward; per-thread isolation strengthens it; PII/secrets-in-panic docstring must be preserved) |
| Test | confirmed (API shape unchanged; per-thread isolation removes `(metric, labels)` tuple footgun; `#[serial]` expected to drop) |
| Observability | confirmed (API call shape correct; watch-items at review time: LocalRecorderGuard `!Send` doc note; 3-case assert_delta table under single-snapshot model; consumer API byte-for-byte identical) |
| Code Quality | confirmed (RAII LocalRecorderGuard correctness load-bearing; `SNAPSHOTTER` static fully removed; serial_test dev-dep drop if unused; clippy clean) |
| DRY | confirmed (no structural change; helper location, workspace deps, gating all intact) |
| Operations | confirmed (downstream of all iter-1 ops concerns; guard/CI/manifests untouched) |

---

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Verdict delivered early during design discussion (pre-empted Gate 2 signal). Verified pure-leak, test-utils gate, PII/secrets-in-panic warning, `!Send` enforcement. |
| Test | CLEAR | 0 | 0 | 0 | All 5 review-brief criteria pass: API shape, 13-case coverage, per-thread isolation via `parallel_snapshots_on_different_threads_do_not_collide`, no coverage reduction, docstring accuracy. |
| Observability | RESOLVED | 2 | 2 | 0 | Both docstring findings addressed in-place: §Delta semantics now distinguishes counter/gauge idempotent reads from histogram drain; §Parallel tests reorganized into Captured / NOT captured buckets with correct tokio guidance. |
| Code Quality | RESOLVED | 1 | 1 | 0 | `#![allow]` → `#![expect]` per ADR-0002 (with `reason = ...`); fulfillment verified via clean clippy. ADR Compliance section delivered (ADR-0002, -0015, -0019, -0024, -0032 all compliant). |
| DRY | CLEAR | 0 | 0 | 0 | Helper canonical location, workspace dep level, gating all intact. Extraction opportunity (Step 2+) recorded as tech debt below. |
| Operations | CLEAR | 0 | 0 | 0 | No testing.rs findings. Flagged CI-gate breakage analysis: `run-guards.sh` is invoked by `.github/workflows/ci.yml:62` and `scripts/verify-completion.sh:202`, both will exit non-zero on this commit. ADR-0032 phasing anticipates this as interim state on integration branches. See "CI Gate Coordination" note below. |

---

## CI Gate Coordination (Operations-flagged)

Per operations review: `run-guards.sh` will exit 1 on this commit because of the lead-accepted `validate-metric-coverage` interim state. Two consumers:

1. `.github/workflows/ci.yml:62` — `Run simple guards` step (not `continue-on-error`). PRs targeting `main`/`develop` would go red; merge button blocked; subsequent clippy/tests/build steps skipped.
2. `scripts/verify-completion.sh:202` — `verify_simple_guards` layer records any non-zero as failure. Future devloops' Gate-3 completion checks would fail their guard layer until backfill lands.

Operations recommendation — pick one before merging upward:

- **Option A (preferred per ADR-0032 §Implementation Notes)**: Step-1 + per-service backfill PRs land as a stacked chain into integration branches, so no intermediate commit on `main`/`develop` is red.
- **Option B**: Temporary allowlist in `run-guards.sh` distinguishing `validate-metric-coverage` during the phasing window (contradicts ADR-0032's "no baseline file, no ratchet" spirit).
- **Option C (matches current branch setup)**: Keep Step-1 off `main`/`develop` on a long-lived feature branch (`feature/mh-quic-mh-tests` → `feature/user-connect-to-mh`) until backfill complete. Next merge upward must batch the backfill.

Current branch topology supports Option C: `feature/mh-quic-mh-tests` is 2 layers deep from `main` (via `feature/user-connect-to-mh`). **Lead decision (2026-04-24): Option C.** Work stays on feature branches through the phasing window; nothing pushed to `main`/`develop` until all Step 2-5 backfill lands and all guards pass. Option A effectively becomes the upward-merge moment of Option C (stacked or batched, lead's call at merge time). Option B (allowlist) is not needed — CI on main/develop never sees an intermediate red commit.

## Gate 2 (Validation) Results — Iteration 2 Pure-Leak

| Layer | Command | Verdict | Notes |
|-------|---------|---------|-------|
| 1 | `cargo check --workspace` | PASS | |
| 2 | `cargo fmt --all --check` | PASS | |
| 3 | `bash scripts/guards/run-guards.sh` | EXPECTED-FAIL | 15/16 guards pass; `validate-metric-coverage` reports exactly 78 uncovered names (AC:17 GC:25 MC:25 MH:11), matching pre-iteration-2 baseline with zero regression. Lead-accepted interim state per ADR-0032 phasing. |
| 4 | `bash scripts/test.sh --workspace` | PASS | common: 170/170; workspace: exit 0 under `set -e` |
| 5 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS | |
| 6 | `cargo audit` | PASS | 7 pre-existing advisories (exit 0 — project-acceptable); iter-2 introduced zero new deps |
| 7 | semantic-guard agent | SAFE | Two non-blocking caveats: (a) run-guards.sh now red — expected per ADR-0032; (b) guard remediation-hint uses wrong API (`MetricAssertion::counter(...)` shortcut that doesn't exist) — cosmetic, queued for post-Gate-3 batched commit |
| 8 | `dev-cluster rebuild-all` + env-tests | SKIPPED | Lead decision: change is strictly test-scope (`cfg(test) / feature = "test-utils"` gated); no service binary's runtime behavior can be affected; nothing consumes `observability::testing` yet (that's Step 2+). Additionally, `dev-cluster status` subcommand the skill references does not exist in this environment's helper. Disproportionate cost (10-15 min) for zero plausible coverage. |

## Tech Debt

- **Hand-rolled `DebuggingRecorder::install()` sites pre-dating this PR** (DRY reviewer observation): `crates/mh-service/src/observability/metrics.rs:391-427` and `crates/mc-service/src/observability/metrics.rs:766-827` both instantiate their own `DebuggingRecorder` + global install pattern inside `test_prometheus_metrics_endpoint_integration` tests. The MC site explicitly comments on global-recorder isolation pain. Exact migration target for ADR-0032 Step 2+ (MH, MC backfills) — flag for sequencing to ensure the backfill catches these alongside the uncovered-emission sites. Recorded in `docs/TODO.md` Cross-Service Duplication section by DRY reviewer during reflection.
- **Stale prose in `docs/specialist-knowledge/observability/INDEX.md:67`** (implementer reflection note): "Metric-test coverage ratchet, MetricAssertion utility, TestHooks bounded scope, 4-guard enforcement stack (G1-G4), failure-class table -> ADR-0032" references the original ADR-0032 proposal that was reductively amended to a single-guard, no-ratchet, no-TestHooks model. Out of scope for this devloop; flag for a future observability-docs sweep.
- **CI-gate coordination for upward merge** (Operations reviewer observation, see § "CI Gate Coordination" above): **Resolved — Option C** (long-lived feature branch until backfill complete). Work stays on `feature/mh-quic-mh-tests` / `feature/user-connect-to-mh` through the phasing window; nothing pushed to `main`/`develop` until all Step 2-5 backfill lands and all guards pass.

**Fixed in this devloop (not carried as debt):**

- Guard remediation-hint API string (semantic-guard caveat #2) — fixed in `scripts/guards/simple/validate-metric-coverage.sh:106,131` during post-verdict batched commit; now prints the real two-step `let snap = MetricAssertion::snapshot();` API.

---

## Reflection

### What worked

- **Mid-flight pivot off the process-global `DebuggingRecorder::install()` path was caught before commit.** Iteration 1's implementation used `SNAPSHOTTER: OnceLock<Snapshotter>` + `.install()` — label-tuple-scoping was the only isolation, with `#[serial]` as fallback. During resync, lead noticed `metrics` 0.24's `set_default_local_recorder` offered true per-thread isolation. Converting to per-thread via `Box::leak`'d recorder collapsed the label-tuple rationale and `#[serial]` fallback to a short "don't hold two overlapping snapshots on one thread" note, and the public API stayed byte-identical so Step 2+ backfill is unaffected.
- **Design-level reviewer feedback surfaced early.** Code-reviewer's pushback on `unsafe { Box::from_raw }` + security's Drop-ordering correctness note converged on pure-leak before commit. Zero unsafe in the landed code.
- **The semantic-guard's second caveat (guard remediation-hint API mismatch) caught a real user-facing copy-paste footgun** that no human reviewer flagged. The hint now prints the real two-step API.
- **Parallel-safety claim is backed by a directly-collision-sensitive test** (`parallel_snapshots_on_different_threads_do_not_collide` — two threads emit to the same `(name, labels)` tuple with distinct values; bleed would make one thread see 10 and panic).

### What could have gone smoother

- **Implementer signaled "Ready for validation" before reviewer-preferred design (`unsafe` vs pure-leak) settled.** Caught by lead after peer-DM summaries surfaced; validation was paused and redriven against the revised code. Process note captured and acknowledged by implementer.
- **Layer 8 (env-tests against Kind cluster) was skipped** with written justification (test-only, gated, nothing consumes it yet) because the skill's "always runs" directive is disproportionate for a helper that no service binary can reach. Skip was explicitly lead-approved, not silent.
- **INDEX-size guard tripped after reflection** (test/INDEX.md: 75 → 76). Test reviewer collapsed two adjacent `crates/common/` pointers into one line to restore 75.

### Artefacts to watch post-devloop

- **CI-gate breakage**: `run-guards.sh` now exits 1 on this commit (validate-metric-coverage reports 78 uncovered metric names). Upward-merge sequencing needs Option A/B/C decision (§ CI Gate Coordination). Current branch topology (`feature/mh-quic-mh-tests` 2 layers from `main`) supports Option C.
- **Step 2+ backfill targets**: 78 uncovered metric names (AC:17 GC:25 MC:25 MH:11) + 2 hand-rolled `DebuggingRecorder::install()` sites (MH, MC metric tests) are the migration set for ADR-0032 Step 2+ PRs. DRY reviewer's TODO.md entry links the hand-rolled-install sites to the uncovered-emission backfill work so they're caught together.
