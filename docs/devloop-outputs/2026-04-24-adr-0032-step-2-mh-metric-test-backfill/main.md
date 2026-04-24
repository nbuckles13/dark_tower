# Devloop Output: ADR-0032 Step 2 — MH Metric-Test Backfill

**Date**: 2026-04-24
**Task**: MH component-test backfill per ADR-0032 §Implementation Notes phasing step 2 (canonical case)
**Specialist**: media-handler
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-tests`
**ADR**: `docs/decisions/adr-0032-metric-testability.md`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `695b9e787b3c1ee0025332f172f8c91d2e8e5365` |
| End Commit | `5b4e119` (reflection); implementation tip: `35adea3` |
| Branch | `feature/mh-quic-mh-tests` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@adr-0032-step-2` |
| Implementing Specialist | `media-handler` |
| Iteration | 1 |
| Security | `security@adr-0032-step-2` |
| Test | `test@adr-0032-step-2` |
| Observability | `observability@adr-0032-step-2` |
| Code Quality | `code-reviewer@adr-0032-step-2` |
| DRY | `dry-reviewer@adr-0032-step-2` |
| Operations | `operations@adr-0032-step-2` |

---

## Task Overview

### Objective
Drain MH's 11 uncovered metric names from `validate-metric-coverage.sh` to 0, with per-failure-class fidelity (each metric observed via a real code path that emits it).

### Scope
- **Service(s)**: `crates/mh-service/` (src + tests)
- **Schema**: No
- **Cross-cutting**: No (common helper is consumed as-is, not modified)
- **Mode rationale**: Full — touches `metrics::` instrumentation call sites (indirect, via tests), likely touches production code for Cat B token-refresh closure extraction

### Exit criterion
`bash scripts/guards/simple/validate-metric-coverage.sh` reports **0 uncovered metric names** for mh-service. Per-failure-class fidelity required — a test that only name-references a metric satisfies the guard but not the ADR-0032 contract.

### Four deliverables

1. **Accept-loop component test + wt_rig bypass deletion** (coupled work).
   - `mh_webtransport_connections_total{status=accepted|rejected|error}` and `mh_active_connections` emit inside `accept_loop` at `crates/mh-service/src/webtransport/server.rs:174,179,205`.
   - Current `crates/mh-service/tests/common/wt_rig.rs` calls `handle_connection` directly because `accept_loop` drops per-connection `Result` (justification at `wt_rig.rs:14-21`).
   - Write a component test driving real `WebTransportServer::bind() → accept_loop()` with a real WebTransport client. Observe via `common::observability::testing::MetricAssertion` snapshots.
   - Delete the `wt_rig.rs` bypass once the accept-loop-driving test lands.

2. **Cat B extraction for token-refresh closure** (Pattern #2 per audit).
   - `mh_token_refresh_total`, `mh_token_refresh_duration_seconds` emit inside `TokenManager::with_on_refresh` closure at `crates/mh-service/src/main.rs:114`.
   - Lift the closure body into a stateless fn (e.g., `record_token_refresh_metrics(&TokenRefreshResult)`), unit-test both success and failure paths, call the fn from the closure. Production behavior identical; emission path identical; testability gained.

3. **Remaining uncovered names** (~7, likely Cat C test-only).
   - Inspect `bash scripts/guards/simple/validate-metric-coverage.sh 2>&1 | grep mh-service` for the full list; classify each by pattern before implementing. Most likely already exercised by existing integration tests (`auth_layer_integration.rs`, `mc_client_integration.rs`, `register_meeting_integration.rs`, `webtransport_integration.rs`, `gc_integration.rs`) — only need a test-side reference + `MetricAssertion` on the path.

4. **Ride-along migration** (DRY tech debt).
   - `crates/mh-service/src/observability/metrics.rs:391-427` — hand-rolled `DebuggingRecorder::install()` pattern in `test_prometheus_metrics_endpoint_integration` predates ADR-0032. Explicitly comments on global-recorder isolation pain that per-thread `MetricAssertion` solves. Migrate to `MetricAssertion`. Removes one of two sites flagged in `docs/TODO.md`.

### Scope boundaries (hard)
- No changes to `crates/common/src/observability/testing.rs`.
- No changes to `scripts/guards/simple/validate-metric-coverage.sh`.
- No changes to MC/AC/GC code or tests.
- No deployment/manifest changes.

### Branch / merge strategy
Stay on `feature/mh-quic-mh-tests` (Option C confirmed in Step 1 main.md § "CI Gate Coordination"). Steps 2-5 stack here before merging upward to `feature/user-connect-to-mh`.

### References
- `docs/decisions/adr-0032-metric-testability.md` §Implementation Notes phasing step 2
- `docs/devloop-outputs/2026-04-20-adr-0032-step-1-metricassertion-guard/main.md` for Step 1 context (MetricAssertion API, guard behavior, lead-accepted interim state)
- `docs/observability/metrics-coverage-audit-2026-04-20.md` for the failure-pattern classification

---

## Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (tempdir PEMs SAN-scoped, Cat B preserves PII-safe TokenRefreshEvent interface, real-auth-layer exercises — no JWT/TLS bypass) |
| Test | confirmed (per-test fidelity audit accepted; #2/#6 translations pending final confirmation before commit 3) |
| Observability | confirmed (sharpened 2026-04-24: Gate 1 stands / no `results_rx` / no label amendment; Pattern B for test #6 timing; triple-assertion shape for test #2; current_thread + live-hold gauge + counter-first-then-histogram discipline; TODO.md accept-loop gap removal on land) |
| Code Quality | confirmed (Cat B purity verified, current_thread flavor, wt_rig.rs deletion scope clear, DebuggingRecorder migration preserves Prometheus-text intent) |
| DRY | confirmed (new accept_loop_rig replaces bypass, record_token_refresh_metrics MH-scoped, cross-service consolidation is post-Step-4 tech debt) |
| Operations | confirmed (CI scope clean, tempfile dev-dep pinned to "3", runtime-generated PEMs, no canary gate) |

### Lead approval (2026-04-24)

Plan approved. Sequencing:
- **Commits 1 + 2** (Cat B extraction + Cat C non-WT assertions + metrics.rs DebuggingRecorder migration): proceed immediately, no reviewer-gated preconditions.
- **Commit 3** (accept-loop component test + wt_rig.rs deletion + webtransport_integration migration): gated on @test confirming #2 (negative-assertion pattern) and #6 (polling pattern).

Results-channel mechanism concern resolved: option (3) metric-label-based assertion migration adopted; `results_rx` dropped; rig is byte-identical to production `accept_loop` invocation. Per-test fidelity audit showed 5-of-7 strict-preserving; #2 mitigated via negative assertion on `mh_jwt_validations_total{}.assert_delta(0)`; #5's documented fidelity delta was resolved to zero loss after unit-tier verification (`auth/mod.rs::tests::test_validate_meeting_token_rejects_wrong_token_type` catches in-validator deletion; component-tier metric-label assertion catches call-site refactor to permissive method). #6 mitigated via counter-delta-with-bounded-elapsed polling.

---

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Delivered early during design discussion. Verified self-signed cert SAN-scoped to localhost/127.0.0.1, dev-deps not runtime, accept_loop_rig byte-identical to production, no TLS/JWT/capacity gates disabled in tests, Cat B extraction preserves PII-safe TokenRefreshEvent interface. Flagged pre-existing `gc_registration_total{status=success}` always-success bug to observability (tech debt, out of scope). |
| Test | CLEAR | 3 | 3 | 0 | Upgraded RESOLVED→CLEAR — all 3 fix-class findings fixed inline in commits `97b23b1` + `35adea3` (no deferred): F1 retired-test docstring (fixed); F2 both-bounds + location attribution (fixed); F3 ADR-0011 plumbing-exception label at `metrics.rs:403-423` (fixed with "do NOT propagate to tests/" forward-warning). Informational observations logged in `observability/TODO.md` (gc_registration_total always-success P1 bug; guard-regex multi-line deficiency). |
| Observability | CLEAR | 2 nits | | | Non-blocking doc-nits flagged to implementer: `tests/common/mod.rs:8` and `tests/common/wt_client.rs:5` have stale references to deleted `wt_rig.rs`. Implementer's call to sweep or defer. Verified: Cat B byte-identical, per-failure-class fidelity via companion metrics, current_thread pinned, drain-on-read discipline, gauge live-hold, two-fixed-point timing pattern. Template for MC/AC/GC Steps 3-5. |
| Code Quality | RESOLVED | 2 | 2 | 0 | Fixed: 3 stale `wt_rig` doccomments (commits `cf3f6fd`, `97b23b1`); dead `#[allow(clippy::unwrap_used, clippy::expect_used)]` on `metrics.rs::tests` mod removed (deletion, not migration to `#[expect]`, which would fail under unfulfilled-lint-expectations). Full ADR Compliance section (ADR-0001/-0002/-0019/-0024/-0032 all compliant). |
| DRY | CLEAR | 1 | 1 | 0 | `test_token_receiver` duplicated across 3 test files — consolidated to `tests/common/mod.rs` in commit `6b1213b`. Two tech-debt extraction opportunities logged to `docs/TODO.md`: (1) 3-service `with_on_refresh` closure + `record_token_refresh` sibling duplication (extract after Steps 3/4); (2) `write_self_signed_pems` PEM-roundtrip helper (potential MC adoption in Step 3). |
| Operations | CLEAR | 0 | 0 | 0 | Exit criterion verified (mh:0, ac:17, gc:25, mc:25), CI scope clean, no manifests/K8s/runbooks touched, dev-deps `rcgen`/`tempfile` are [dev-dependencies] only, Cat B stateless byte-identical, no canary gate needed. |

### Gate 2 (Validation) Results

| Layer | Command | Verdict | Notes |
|-------|---------|---------|-------|
| 1 | `cargo check --workspace` | PASS | |
| 2 | `cargo fmt --all --check` | PASS | |
| 3 | `bash scripts/guards/run-guards.sh` | EXPECTED-FAIL | 15/16 pass; validate-metric-coverage reports 67 uncovered (mh-service: 0, AC: 17, GC: 25, MC: 25). Lead-accepted interim state per ADR-0032 phasing (down from 78 baseline). |
| 4 | `bash scripts/test.sh --workspace` | PASS | 2331 pass, 0 failed. Focused mh re-run on HEAD (5e68fcf): 147 pass. |
| 5 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS | |
| 6 | `cargo audit` | PASS | 7 pre-existing advisories (exit 0 — project-acceptable); iter-2 introduced `rcgen = "0.13"`, `tempfile = "3"` as dev-deps only. |
| 7 | semantic-guard agent | SAFE | 8/8 fidelity checks passed. Non-blocking observations: rig docstring line-drift (said 258-260, actual 252-254), briefing mentioned 3 commits but 4 in range, pre-existing `gc_registration_total` design smell flagged as out-of-scope. |
| 8 | `dev-cluster rebuild-all` + env-tests | SKIPPED | Lead decision per Step 1 precedent: Cat B extraction preserves emission byte-identical (no service runtime behavior change); accept-loop test is test-only; no deployment/config/manifest touched; env-tests cannot meaningfully exercise `observability::testing` paths not reachable from deployed service binaries. |

---

## Tech Debt

### Guard-scope follow-up

`scripts/guards/simple/validate-metric-coverage.sh` extracts emitted metric names from `crates/{service}/src/observability/metrics.rs` via a single-line regex (`(?:metrics::)?(?:counter|histogram|gauge)!\s*\(\s*"[^"]+"`). Multi-line macro invocations where the name string literal lives on a different line from the macro head are silently skipped — the guard never sees them as emitted metrics, so they are neither covered nor reported as uncovered. In MH this affects four metric names that are in fact emitted in production:

- `mh_errors_total` (emitted at `crates/mh-service/src/observability/metrics.rs:138`, multi-line `counter!(...)`)
- `mh_grpc_requests_total` (emitted at `:124`, multi-line)
- `mh_mc_notifications_total` (emitted at `:179`, multi-line)
- `mh_token_refresh_failures_total` (emitted at `:111`, multi-line — note: MH does have a Cat B-extracted test wrapper for this name in `src/token_refresh_metrics.rs::tests::assert_failure_emits`; coverage *exists* but the guard is oblivious)

The worse failure mode is *not covered* + *not counted* — a metric can be emitted in production and have zero test references, and the guard will still pass. This is a Step 1 guard deficiency, not a Step 2 backfill gap. Out of scope for this devloop; flagged for a separate small devloop to extend the regex to span multi-line macro invocations (likely using `pcregrep -M` or a short Rust/`ripgrep` preprocessor that normalizes macro-call whitespace before running the existing scan).

### `gc_registration_total` label mis-categorization — P1 observability-integrity (Cat A follow-up owned by @observability)

`crates/mh-service/src/grpc/gc_client.rs:146-184` unconditionally emits `mh_gc_registration_total{status=success}` on any `Ok(response)` from the RPC, including the case where `response.accepted == false`. The `status=error` branch fires only on RPC-transport failures. Consumers of the mis-labeled signal:

- Dashboard panel `infra/grafana/dashboards/mh-overview.json:324` — `sum by(status) (increase(mh_gc_registration_total[$__rate_interval]))` is SRE's primary "registration healthy?" view.
- Docs PromQL at `docs/observability/metrics/mh-service.md:30-31` — `rate({status="error"}) / rate()` evaluates to 0 during a 100% GC-rejection incident.
- MH ready-gating (per `docs/specialist-knowledge/observability/INDEX.md`): MH reports ready after GC registration. Systemic rejection is a high-impact failure mode the primary metric hides.
- No mitigation metric — `record_error` is not called from this path.

Classification: ADR-0032 §Context Pattern #3 (typed-error branch mis-categorized into success bucket). Disposition: separate Cat A follow-up PR owned by observability, pre-next-MH-release, with canary deploy gate + raw `/metrics` evidence per ADR-0032 §Rollout. This devloop documented the status quo via `gc_integration.rs::test_gc_client_registration_rejected` inline comment + assertion that matches the bug (commit `db96423`); the follow-up PR flips the assertion when the fix lands.

Sister metric check: `mh_gc_heartbeats_total` at `gc_client.rs:251-266` has no `accepted`-flag pattern (heartbeat RPCs either transport-succeed or transport-fail). No sister bug.

---

## Reflection

### What worked

- **Option (3) metric-label migration as substitute for `results_rx`**: @team-lead's Gate-1 call to strike the `results_rx: mpsc::Receiver<Result<(), MhError>>` from the rig was the single most consequential design decision in this devloop. The fidelity audit I sent after the Gate-1 decision (per-test translation table for all 7 `webtransport_integration.rs` cases) reframed the migration from "lose `MhError`-variant assertions" to "substitute metric labels + session-manager state + mock_mc channels — each preserves the test's observable invariant." That framing held through six reviewer passes and two refinement iterations; three reviewers (@observability, @dry-reviewer) who initially pushed back on it both retracted once the substitution shape was concrete.
- **Reviewer review-phase refinements strictly improved the landed code**: @test's two review-phase proposals — (a) restore `wrong_token_type_guest_rejected_on_wt_accept_path` with a session-manager-state distinguishing assertion, (b) refine the provisional-timeout test to two-fixed-point bracketed check — both landed as strictly-better-than-plan outcomes. Pre-review discipline ("resolve design-level feedback BEFORE signaling Ready for validation" — Step-1 process note carried forward) caught one round of this but not both; review-phase refinements absorbed the remainder without architectural churn.
- **Pre-existing-bug surfacing protocol**: @security's discovery of the `gc_registration_total` label mis-categorization during their review was routed to @observability via the agreed "not-security, domain-owner takes it" process. Observability produced a sharper severity diagnosis (P1, multiple consumer hits), confirmed the tech-debt disposition, owned the follow-up. No scope creep into this devloop; breadcrumb documented inline + in the tech debt section; test in commit `db96423` correctly asserts the buggy label with a pointer comment so future-maintainer-who-reads-the-test-first understands WHY.
- **`current_thread`-flavor discipline**: the file-header comment at `tests/webtransport_accept_loop_integration.rs:1-6` pinning the flavor as load-bearing is exactly the signal a future maintainer needs to see before flipping the test to `multi_thread` and silently breaking metric-capture. Reviewer engagement (one reviewer initially believed the default was `multi_thread` — corrected during planning) validated the comment's ex-ante value.
- **Drain-on-read histogram discipline**: surfaced unexpectedly during commit 1 authorship (histogram observations zeroed after first counter assertion). Escalated to a reviewer-facing heuristic ("histogram first, counters after") that now lives in @observability's INDEX.md for Step 3/4/5 MC/AC/GC backfill precedent.

### What could have gone smoother

- **Plan-prose precision affects reviewer signal quality**: my initial plan text described the rig's `results_rx: mpsc::Receiver<Result<(), MhError>>` in ADR-ambiguous terms ("preserving per-handler MhError-variant assertions") which invited two reviewers (@observability, @dry-reviewer) to independently conclude the rig must fork `accept_loop` or modify production. @team-lead's Gate-1 decision was correct but my plan text did not make the ADR-non-compliance of option (1)/(2) obvious enough at first pass. Post-Gate-1 revision with the explicit three-option framing + per-test fidelity audit cleared the signal. Lesson for Step 3+ plan drafting: if a proposed test mechanism could be implemented in an ADR-forbidden way, explicitly rule out those implementations in the plan text.
- **Commit-message staleness vs code-state drift during review phase**: commit `0739f3b`'s message said `wrong_token_type_guest_rejected_on_wt_accept_path: deleted`; commit `046f808` restored it per @test's review-phase refinement; the git log line is immutable. @code-reviewer's Finding 1.c caught a matching staleness in the file header; I fixed the file (commit `97b23b1`) but the git-log drift is permanent. Mitigation for future devloops: if review-phase produces commits that undo or restore decisions documented in earlier commit messages, add an explicit "superseded by commit X" breadcrumb in the earlier commit's trailing commit message or (if policy allows) squash-before-merge to collapse the contradiction.
- **`#[expect]` vs `#[allow]` edge case — unfulfilled-expectation footgun**: for `tests/token_refresh_integration.rs` + the `tests` mod in `observability/metrics.rs`, `#[expect(clippy::unwrap_used, clippy::expect_used, reason = "...")]` would have FAILED the build because neither lint actually fires (zero `.unwrap()`/`.expect()` in the bodies). The `-D unfulfilled_lint_expectations` workspace setting catches this. Lesson: `#[expect]` is only a strict improvement over `#[allow]` if the lint reliably fires. For files that might not actually use the suppressed lints, either (a) drop the attribute entirely, (b) narrow to only the lints that do fire, or (c) document the scenario in a comment pre-empting reviewer pushback.

### Artefacts to watch post-devloop

- **CI-gate breakage continues** (Option C confirmed at Step 1): `run-guards.sh` exits 1 on this commit — 67 uncovered across AC/GC/MC remaining. No change from Step 1's accepted interim state. Upward merge to `feature/user-connect-to-mh` gates on Steps 3-5 draining the rest.
- **`gc_registration_total` label bug owned by @observability**: P1 observability-integrity, Cat A follow-up pre-next-MH-release. Test in this devloop (`gc_integration.rs::test_gc_client_registration_rejected`) asserts the buggy label with a descriptive inline comment; when the production fix lands, that test's assertion must flip from `status=success` to `status=error` and the comment must be removed.
- **Guard-regex single-line deficiency**: 4 MH multi-line counter!() emissions (`mh_errors_total`, `mh_grpc_requests_total`, `mh_mc_notifications_total`, `mh_token_refresh_failures_total`) are invisible to the guard entirely. Separate small devloop to extend `validate-metric-coverage.sh`.
- **Cat B extraction as template**: Step 3 MC backfill should mirror `record_token_refresh_metrics` structurally in `crates/mc-service/src/observability/metrics.rs`. @dry-reviewer flagged the 3-service closure-body duplication as a post-Step-4 extraction candidate.
- **`write_self_signed_pems` extraction candidate**: if Step 3 MC backfill adopts the real-accept-loop component-test pattern (rather than MC's current in-memory `Identity::self_signed` path), the helper moves to `common` or a new shared test-utils crate. @dry-reviewer flagged this during their plan review as a no-action observation for this devloop, logged for Step 3 implementer.
