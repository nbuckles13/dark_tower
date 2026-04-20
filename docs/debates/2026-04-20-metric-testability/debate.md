# Debate: Metric Testability

**Date**: 2026-04-20
**Status**: Complete — see ADR-0032 (`docs/decisions/adr-0032-metric-testability.md`)
**Participants**: observability (domain lead), test, code-reviewer, auth-controller, global-controller, meeting-controller, media-handler, operations

> **Note**: Security not included per user's explicit specialist list (no clear security domain stake; participants may invoke if PII/cardinality concerns surface).
>
> When cross-cutting specialists (Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

How do we close the **~28% uncovered + ~5% wrapper-only gap** in metric recording sites across AC/GC/MC/MH services?

### Confirmed direction (do not re-litigate)

Metric-recording branches MUST be reachable by direct function call from a test. This means:
- Closures registered in `main.rs` (e.g. `TokenManager::on_refresh`) are extracted into named private functions and unit-tested directly.
- Metric-recording branches inside accept loops and fire-and-forget spawns are extracted into testable units (e.g. `fn record_accept_decision(active, max) -> AcceptDecision`).
- Whatever pattern the debate chooses for the residual problem must NOT undo this principle.

### Open for debate

**(a) Additional structural / API patterns beyond extraction.** Even with extraction, there's a residual question: do tests assert on the metric value (counter delta, gauge state, histogram bucket), or just exercise the path? Candidates raised so far (do not consider this list closed):
- Prometheus scraping in tests (use `metrics::get_handle()` and assert deltas)
- Per-server result channels (e.g., `WebTransportServer::with_result_rx()` so tests get both `MhError` and metric coverage)
- A sanctioned metric-assertion test utility (e.g., `MetricAssertion::counter("foo").increments_during(|| ...)`)
- Move accept-loop metrics into the connection handler so they're recorded after `Ok` rather than before spawn
- Defer to load tests / canaries (accept that some sites can't be CI-tested)
- Something else entirely

**(b) Enforcement / monitoring.** How do we ensure new metric sites land tested?
- Open question — known constraints: `cargo llvm-cov` exists in CI but is informational; coverage tells you "line ran" but not "metric was tested alongside the logic that drives it" or "metric was validated"; semantic guards have a poor recent track record. Don't over-anchor on these — propose other approaches.

**(c) Per-service variation.** Acceptable that AC/GC/MC/MH adopt different patterns based on what their code shape demands? Or mandate a uniform pattern for code-reviewer's sanity?

## Context

- **Audit**: `docs/observability/metrics-coverage-audit-2026-04-20.md` — 778 sites, 5 cross-cutting failure patterns (accept loops, token-manager callbacks, repository error branches, fire-and-forget spawns, capacity-rejection paths).
- **MH WT bypass case**: `docs/specialist-knowledge/observability/TODO.md` and `crates/mh-service/tests/common/wt_rig.rs:14-21` — concrete example. The rig deliberately bypasses `accept_loop` because production drops the per-connection `Result`.
- **Existing partial guard**: `scripts/guards/simple/validate-application-metrics.sh` — already enforces metric-to-dashboard coverage, sets a precedent for a metrics-related guard.
- **Recent recurrence**: this gap has appeared in multiple devloops (MH integration tests devloop 2026-04-17 surfaced it most recently); user wants a generalizable fix rather than per-devloop deferrals.

## Positions

### Final Positions (Round 2 close)

| Specialist | Final Sat | Position summary |
|------------|-----------|------------------|
| observability (lead) | 93 | Three-layer (extract + MetricAssertion + bounded TestHooks) + 4-guard enforcement + per-failure-class table. Drafting ADR-0032. |
| test | 95 | Accept TestHooks given MC `TestServer::accept_loop` fork evidence. Label-tuple-scoped `Snapshotter` for parallel-test isolation; closure-form `MetricAssertion` canonical; 3-site TestHooks bound. |
| code-reviewer | 93 | Per-failure-class table satisfies uniformity (mechanism keyed on shape, not service discretion). Identical TestHooks shape MC+MH. 4-mechanism enforcement stack with kill criteria. |
| auth-controller | 92 | Tier A + MetricAssertion + error-injection-at-seam for AC's repo-error sites. AC has no closures-in-main and no accept loops; Tier C (TestHooks) does not apply. |
| global-controller | 92 | Per-failure-class framing (validation-branches direct-call; pg-error real-DB+fault-injection; non-DB error repo-trait). Diff-based guard. GC's spawns are named entry points — TestHooks doesn't apply. |
| meeting-controller | 90 | TestHooks struct (MC's proposed shape, MH concurred) for MC `webtransport/server.rs` accept_loop and `mh_client` retry. Lets MC delete `join_tests.rs::TestServer::accept_loop` 43-line fork. |
| media-handler | 92 | TestHooks struct adopted from MC's shape. `wt_rig` deletes its 30-line `accept_loop` copy and runs the production loop. Gauge updates handled via Tier D recorder snapshot reads. |
| operations | 91 | 3-category rollout scope (Cat A canary+/metrics evidence; Cat B byte-identical extractions no-canary; Cat C test-only no-canary). TestHooks MUST for ops-critical signals. Per-service SLO sub-targets. |

## Discussion

### Round 1: Initial positions
Observability anchored with three-layer pattern (extract + MetricAssertion + result-channel). Test pushed back on Prometheus HTTP scraping (parallel registry races) and on result-channel as production-API modification. Code-reviewer pushed for strict uniformity. Service specialists reported their concrete uncovered-site shapes.

### Round 2: Convergence
- Test withdrew result-channel objection on two pieces of evidence: (a) operations' SLO-blind-spot argument (capacity-reject can't be canary'd), (b) MC's existing `TestServer::accept_loop` fork showing the bypass-pattern already drifts.
- Per-failure-class framing emerged (MH's table) as the answer to code-reviewer's uniformity ask: same mechanism for the same shape, regardless of service.
- AC's research on `DebuggingRecorder::Snapshotter` with label-tuple-scoped snapshots resolved the parallel-test-recorder concern without needing nextest mandate.
- MH+MC aligned on `TestHooks { result_tx: Option<mpsc::Sender<ConnectionOutcome>> }` struct shape (rejected positional `Option<Sender>` because MC's constructor is already at `too_many_arguments`).
- Operations proposed and team adopted: 3-category rollout (Cat A canary+raw-/metrics-evidence, Cat B byte-identical extractions no-canary, Cat C test-only no-canary).
- 4-mechanism enforcement stack converged: baseline-ratchet, presence-guard-upgraded-to-require-MetricAssertion, closure-in-main grep, spawn-body grep, plus informational monthly trend.

## Consensus

**Reached.** All 8 specialists at 90+ satisfaction. Mean ~92.

Key decisions:
1. Extract metric-recording branches into named functions (universal, MUST).
2. `MetricAssertion` utility in `crates/common/src/observability/testing.rs`, `DebuggingRecorder::Snapshotter`-backed, label-tuple-scoped, closure-form canonical.
3. `TestHooks` struct on MH WT and MC WT (and MC `mh_client` retry) — bounded 3-site scope, doc-hidden, reviewer-gated for any future expansion.
4. Per-failure-class mechanism table (mechanism keyed on shape, not service).
5. 4-guard enforcement + monthly trend audit (info only). Each guard has a stated kill criterion.
6. 3-category rollout scope (Cat A canary+evidence, B+C lighter).
7. SLO ratchet: 28% uncovered → <10% by 2026-07, <5% by 2026-10, with per-service sub-targets.
8. Explicitly rejected: Prometheus HTTP scraping, post-spawn metric movement, canary-as-primary.

## Decision

ADR-0032 accepted. See `docs/decisions/adr-0032-metric-testability.md`.

---

## Post-Debate Addendum (2026-04-20)

After the team reached Round-2 consensus on the four-tier pattern (extraction + `MetricAssertion` + bounded `TestHooks` + recorder snapshots) with five-guard enforcement, the lead reviewed the drafted ADR with the user and the user pushed back on five points:

1. **Tier C (`TestHooks`) felt invasive.** Even with `pub(crate)` + `#[doc(hidden)]` visibility, opt-in result channels modify production API for test affordance. The debate did consider HTTP scraping (rejected), post-spawn metric movement (rejected), and defer-to-canary (rejected as primary), but did NOT explicitly consider: drive the real `accept_loop` from a test that lives outside the spawn and observe via in-process recorder snapshot. The team gravitated to TestHooks partly because MH's existing `WtRig` bypass made the inversion intuitive. With component-tier tests as the sanctioned home, TestHooks becomes unnecessary.

2. **Metrics with unpredictable values** (timing histograms, payload-size counters, gauge values that depend on runtime state) were under-addressed. The original ADR showed `assert_observation_count(N)` for histograms but didn't lean into the "metric was emitted" assertion shape. For these metrics the realistic assertion is `at_least_once` or `value_in_range`, and the ADR should state that explicitly.

3. **G1 (baseline ratchet) had no auto-generation mechanism.** The ADR specified the file but not how the file gets populated or maintained. Without a tool that re-derives the baseline from source, the file rots. Either we build that tool — at which point we essentially have the simpler "presence guard" — or G1 doesn't carry its weight.

4. **G2 (`#[must_use]` on snapshot types) caught the wrong failure mode.** `#[must_use]` only catches "started a snapshot, forgot to assert" — a narrow case. The dominant failure from the audit is "didn't write the test at all," which `#[must_use]` does not catch. The team had pivoted to `#[must_use]` because code-reviewer flagged the original presence-guard as trivially dodgeable, but the dodge concern was over-weighted relative to the actual frequency of dodge in practice.

5. **G3 (spawn-body grep) and G4 (PR template line) were decoration.** Real but soft, low signal-to-noise.

The user proposed: collapse to one mechanism. Mandate component tests in `crates/{service}/tests/` for metric coverage. One guard: every metric emitted in `crates/XX/src/**` must have a matching reference in `crates/XX/tests/**`. Drop TestHooks (not needed when the test lives outside the spawn). Drop the baseline ratchet (no auto-generation mechanism). Drop `#[must_use]` as load-bearing enforcement. Drop G3 and G4. Keep `MetricAssertion` as a convenience helper, not a structural tier. Keep the per-service SLO sub-targets and tiered ownership from operations' work.

The lead agreed: this is materially simpler, addresses the actual failure mode, doesn't modify production code, and the trivial-dodge concern is reviewer-catchable at PR time. Per ADR-0024 §reductive-amendments, reductive ADR amendments do not require fresh team consensus — the amendment removes machinery rather than adding it, and the original consensus (which approved a more invasive solution to the same problem) implicitly approves the simpler solution that solves the same problem with less.

The ADR was rewritten in place rather than amended; the original Round-2 four-tier approach is captured in this debate document (Round 1 + Round 2 + Final Positions sections above) and is recoverable if the simpler approach proves insufficient.

**Specialists not consulted on the revision** — all 8 had been shut down at the time of the rewrite. Implementation-time sanity check is recommended for:
- **Operations**: the SLO ratchet machinery is replaced by the simpler per-PR guard. SLO targets and stall policy carry forward.
- **Media-handler / Meeting-controller**: TestHooks removed; the work they would have done (component tests driving real `accept_loop`) is unchanged in content, simpler in mechanism.
- **Code-reviewer**: enforcement surface shrunk from 4 guards to 1; the per-failure-class table from the original consensus survives as a review heuristic but is no longer formalized in the ADR.

If implementation surfaces objections to the reductive amendment, the team can re-debate with the original four-tier pattern as a viable fallback.

## Consensus

Reached on Round-2 four-tier pattern. Post-debate reductive amendment to component-tier + presence guard accepted by lead under ADR-0024 reductive-amendment provision; team sanity check deferred to implementation time.

## Decision

See `docs/decisions/adr-0032-metric-testability.md`.
