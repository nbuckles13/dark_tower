# Devloop Output: Harden flaky gc-service health-checker integration tests

**Date**: 2026-06-20
**Task**: Remove the real-wall-clock timing dependency from the gc-service health-checker integration tests (task #52 Gate-2 flakes)
**Specialist**: test (paired with global-controller)
**Mode**: Agent Teams (v2) — full + `--paired-with=global-controller`
**Branch**: `feature/browser-client-join-task-55`
**Duration**: ~60m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6ad7049ab146d67eeafb93b4c7277d67942cca1d` |
| Branch | `feature/browser-client-join-task-55` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-ae8ab949` |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Paired Specialist | `paired-global-controller@session-ae8ab949` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |
| Semantic Guard | `CLEAR` |
| Paired global-controller | `CLEAR` |

---

## Task Overview

### Objective
Six (potentially seven — see framing note) gc-service health-checker integration tests are flaky under `scripts/layer-all.sh` full-workspace parallel load. Each test `tokio::spawn`s the real `start_health_checker` (5s-interval loop) then asserts DB state after a fixed `tokio::time::sleep(Duration::from_secs(6))`. Under saturation the background tick + DB UPDATE slips past the 6s window and the assertion fails. Remove the real-time dependency.

### Scope
- **Service(s)**: gc-service (test modules only — `crates/gc-service/src/tasks/health_checker.rs`, `mh_health_checker.rs`)
- **Schema**: No
- **Cross-cutting**: No (test-only; no production code change)

### Debate Decision
NOT NEEDED — bug fix within existing test-infrastructure patterns (ADR-0005/0009). No architectural decision.

### Framing note (mechanism vs. instance)
The task names **6** tests, but the flaky mechanism — `spawn real 5s-loop` + fixed `sleep(6)` over a real `#[sqlx::test]` Postgres DB — appears in a **7th same-owner sibling**: `test_health_checker_skips_already_unhealthy` (`health_checker.rs:338`). Per review-protocol §framing-lock, the implementer should surface this and fix the class, not just the named instances. The two `*_starts_and_stops` tests use a 100ms sleep to exercise cancellation only and are NOT part of the flaky class.

### Key technical observations (Lead, at setup)
1. `tokio::time::interval(5s)`'s **first tick fires immediately** — the loop calls `mark_stale_fn` within ms of spawn, not after 5s. The `sleep(6)` is mostly dead wait that still races under saturation.
2. **Negative-assertion tests** (`preserves_healthy`, `skips_draining`, `skips_already_unhealthy`) cannot use poll-with-timeout cleanly — you cannot poll for "state stayed the same" without a fixed wait. Option (a) synchronous (call the repo method directly, then assert) fits these best.
3. The loop→repo wiring + cancel-token teardown is independently covered by `*_starts_and_stops` and the unit `test_cancellation_token_stops_task`; that coverage must remain.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/src/tasks/health_checker.rs` (test module only) | Not mine, Minor-judgment | global-controller |
| `crates/gc-service/src/tasks/mh_health_checker.rs` (test module only) | Not mine, Minor-judgment | global-controller |

Rationale: test specialist editing gc-service-owned crate files. Test-only, bounded impact (no production code touched), not a Guarded Shared Area (`src/tasks/` matches no §6.4 criterion). Minor-judgment → owner confirmation required at Gate 1 + Gate 3, satisfied by `--paired-with=global-controller` (owner is an active collaborator + Gate 2 reviewer).

---

## Planning

**Implementer's proposed plan (iteration 1):**
- **Scope = 7 tests** (mechanism class, not the named 6): the 4 in `health_checker.rs` (`_marks_stale_controllers`, `_preserves_healthy_controllers`, `_skips_draining_controllers`, `_skips_already_unhealthy`) + the 3 in `mh_health_checker.rs`. The 7th sibling (`_skips_already_unhealthy`) is the same owner + identical mechanism; included per framing-lock guidance.
- **Approach (a) uniformly**: replace `[spawn loop + sleep(6) + cancel + timeout]` with a single direct `await` of `MeetingControllersRepository::mark_stale_controllers_unhealthy` / `MediaHandlersRepository::mark_stale_handlers_unhealthy` at the same threshold; keep setup + post-assertions unchanged. Rationale: the loop's closure calls these exact fns, and the staleness guard (`health_status != 'unhealthy' AND != 'draining'`) lives in the SQL — direct call exercises the identical production path minus the timer/select/spawn wrapper. Poll-with-timeout (b) rejected because 3 of the 7 are negative assertions (can't poll for "no change") and synchronous is strictly more deterministic for the positives.
- **Strengthened coverage**: assert returned count == 1 for `marks_stale`, == 0 for negatives (original only checked final row state).
- **Cancel-token teardown**: untouched — `*_starts_and_stops` + unit `test_cancellation_token_stops_task` retain it.
- **No production changes; no shared helper.**

**Gate-1 open point raised by Lead**: uniform (a) trades away the end-to-end *loop→closure→repo→DB* wiring assertion (the original `marks_stale` tests spawned the real loop and observed the row flip; `*_starts_and_stops` only proves clean start/cancel). Owner (global-controller) asked to choose: (1) uniform (a) accepting the gap, or (2) hybrid — keep the 2 positive `marks_stale` tests on poll-with-timeout (b) to retain wiring coverage, convert the negatives to (a).

### Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Paired global-controller (owner) | confirmed — chose **Option 2 (hybrid)** |

**Final decision (hybrid)** — owner's domain call, supersedes the Lead's interim uniform-(a) approval:
- **2 positive tests** (`test_health_checker_marks_stale_controllers`, `test_mh_health_checker_marks_stale_handlers`) → **approach (b) poll-with-timeout** against the real spawned loop. Retains the loop→closure→repo→DB binding assertion that the generic-checker unit test cannot cover (the unit test proves the select loop ticks/cancels; only the wrappers bind the real repo method into the loop). Guardrail: identical SQL backdating setup; spawn loop; `tokio::time::timeout(10s, poll-getter every 50ms until flipped)`; assert timeout did not elapse AND status == Unhealthy; cancel token + await teardown as today.
- **5 negative tests** (`_preserves_healthy_controllers`, `_skips_draining_controllers`, `_skips_already_unhealthy`, `_mh_*_preserves_healthy_handlers`, `_mh_*_skips_draining_handlers`) → **approach (a) direct repo call**. Asserting "nothing happened" through a spawned loop can never prove a tick ran; direct-call is strictly better, faster, deterministic. Add return-count == 0 assertion.
- Split is unambiguously **2 positive + 5 negative = 7** (owner corrected the negative count from 4→5; `skips_already_unhealthy` is the 5th negative).
- Behavioral coverage of all four staleness outcomes preserved; `*_starts_and_stops` + unit cancellation test untouched; `refresh_controller_metrics` out of scope (never covered by these tests). Zero blind sleeps remain anywhere.

**Classification-sanity guard**: initially `STATUS=FAIL REASON=dt-guard-binary-missing` (binary not built — infra, not a classification defect). Lead built `dt-guard` (debug + release); guard now returns `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Both Minor-judgment rows have Owner=global-controller filled, neither path is a GSA, neither is Mechanical, owner confirmed at Gate 1. ✓

**Reviewer-panel timing (Lead decision)**: the 7-reviewer panel is engaged at Gate 3 on the real diff (where they add value) rather than for plan pre-confirmation. For a test-only timing fix the substantive Gate-1 review is the test + owner domain, both done. Full multi-lens review runs against the actual diff at review time.

**Phase → implementation. "Plan approved" issued to implementer.**

---

## Pre-Work

None.

---

## Implementation Summary

Removed the real-wall-clock timing dependency from 7 gc-service health-checker integration tests using the owner-approved **hybrid** approach. Test modules only — zero production code changes.

### Positive tests (2) → poll-with-timeout against the real spawned loop
| Test | Before | After |
|------|--------|-------|
| `test_health_checker_marks_stale_controllers` | spawn loop + `sleep(6)` + assert | spawn loop + `timeout(10s, loop{poll get_controller; break on Unhealthy; sleep 50ms})`; assert not-timed-out AND Unhealthy; cancel+await teardown |
| `test_mh_health_checker_marks_stale_handlers` | spawn loop + `sleep(6)` + assert | same shape with `get_handler` |

Retains the loop→closure→repo→DB wiring assertion; asserts on row-flip (no fixed window to miss).

### Negative tests (5) → direct synchronous repo call
| Test | After |
|------|-------|
| `test_health_checker_preserves_healthy_controllers` | `mark_stale_controllers_unhealthy(&pool, 60)` → `assert_eq!(marked, 0)` + status Healthy |
| `test_health_checker_skips_draining_controllers` | `mark_stale_controllers_unhealthy(&pool, 1)` → `marked == 0` + status Draining |
| `test_health_checker_skips_already_unhealthy` (7th sibling) | `mark_stale_controllers_unhealthy(&pool, 1)` → `marked == 0` + retains `updated_at` unchanged assertion |
| `test_mh_health_checker_preserves_healthy_handlers` | `mark_stale_handlers_unhealthy(&pool, 60)` → `marked == 0` + status Healthy |
| `test_mh_health_checker_skips_draining_handlers` | `mark_stale_handlers_unhealthy(&pool, 1)` → `marked == 0` + status Draining |

Calls the exact fn the loop closure invokes; the SQL staleness predicate is exercised identically. The `marked == 0` count assertion strengthens coverage over the original row-state-only checks.

### Untouched (coverage preserved)
`test_health_checker_starts_and_stops`, `test_mh_health_checker_starts_and_stops`, unit `test_cancellation_token_stops_task` — retain cancel-token teardown + loop start/stop coverage.

### Verification
- `cargo test -p gc-service` green; `cargo fmt`/clippy clean (Gate 2 Layers 1–5 ✅).
- Implementer stress run: **0/10 failures** looping the affected tests 10× under concurrent `cargo test --workspace` saturation.
- Zero blind fixed-duration sleeps remain in the 7 tests (only the 50ms poll interval + unchanged 100ms in `*_starts_and_stops`).

---

## Files Modified

```
 crates/gc-service/src/tasks/health_checker.rs    | 98 +++++++++++++-----------
 crates/gc-service/src/tasks/mh_health_checker.rs | 75 ++++++++++--------
 docs/TODO.md                                     |  2 +
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/gc-service/src/tasks/health_checker.rs` | 4 tests reworked (1 positive→poll, 3 negative→direct-call); production code + `starts_and_stops` untouched |
| `crates/gc-service/src/tasks/mh_health_checker.rs` | 3 tests reworked (1 positive→poll, 2 negative→direct-call); production code + `starts_and_stops` untouched |
| `docs/TODO.md` | +1 Observability Debt entry (pre-existing MH gauge asymmetry, surfaced during review) |

---

## Devloop Verification Steps

**Known external condition (Layer 6 audit)**: pre-existing pnpm/npm audit advisories are expected and are being addressed under task #54 (out of scope for this devloop, confirmed by user). This devloop is a test-only Rust change touching no `package.json`/lockfile, so npm-audit findings are not attributable to this diff. Gate 2 treats a Layer-6 failure as a PASS-for-this-devloop **iff** the only failing component is `pnpm audit` (npm advisories); any `cargo audit`, `buf breaking`, or other-layer failure is still a real Gate-2 failure to act on.

### Gate 2 — Attempt 1 (TOTAL_RESULT=FAIL)

| Layer | Verb | Result | Duration | Note |
|-------|------|--------|----------|------|
| 1 | Compile | OK | 22s | (budget WARN, non-blocking) |
| 2 | Format | **FAIL** | 2s | `cargo-fmt-failed` — one `assert_eq!` at `health_checker.rs:383` needs rustfmt wrapping. **Attributable to diff → routed to implementer.** |
| 3 | Guards | OK | 4s | all guards pass (incl. cross-boundary classification) |
| 4 | Test | OK | 155s | gc tests pass under pipeline; **ac-service flake did NOT reproduce this run** |
| 5 | Lint | OK | 19s | clippy + nx-lint clean |
| 6 | Audit | **FAIL** | 5s | `pnpm-audit-failed` ONLY (`cargo-audit-passed`, `buf-breaking-passed`). **Known external — task #54, not this diff.** |
| 7 | Env-tests | N/A | 0s | `wave2-pending` (self-justifying per ADR-0033 wrapper contract) |

**Disposition**: Layer 6 = the user-confirmed known pnpm advisories (task #54), pass-for-this-devloop. Layer 2 = a genuine, trivial formatting miss in the diff → fix-and-rerun (Gate-2 iteration 1).

### Gate 2 — Attempt 2 (after fmt fix)

| Layer | Verb | Result | Duration | Note |
|-------|------|--------|----------|------|
| 1 | Compile | OK | 6s | |
| 2 | Format | **OK** | 1s | fmt fix verified — the diff's only real Gate-2 failure is resolved ✅ |
| 3 | Guards | OK | 4s | |
| 4 | Test | **FAIL (infra)** | 118s | `key_rotation_tests::test_rotate_keys_without_scope_returns_403` panicked in the **sqlx test harness** (`sqlx-core/.../testing/mod.rs:255`): `failed to connect to test database … "Temporary failure in name resolution"`. **Transient DNS failure**, infra-class (not test-logic), NOT attributable to this diff (gc test modules only). ac-service `--lib` passed 378/0 twice in the same run. → retry, does not consume a real attempt. |
| 5 | Lint | OK | 6s | |
| 6 | Audit | FAIL | 4s | `pnpm-audit-failed` only (known task #54) |
| 7 | Env-tests | N/A | 0s | wave2-pending |

**Corrected ac-service-flake diagnosis**: the earlier-flagged "ac-service flake" reproduced here, but it is **not** a timing/logic flake — it is a **transient DNS name-resolution failure** in the `#[sqlx::test]` DB-connect path under parallel saturation. It can hit any `#[sqlx::test]`; it happened to land on this key-rotation test. Environmental, retryable; not an ac-service code defect, not this diff. Per protocol infra-failure handling → retry Layer 4.

**DNS root cause (investigated)**: Podman rootless network; DB host `devloop-browser-client-join-task-55-db` (10.89.7.2) is NOT in `/etc/hosts`, so every connect resolves via aardvark-dns @ 10.89.7.1. sqlx/tokio resolve per-connection (no cache); a burst of concurrent `#[sqlx::test]` connects under CPU saturation overwhelms aardvark-dns → `EAI_AGAIN`. Idle 50-resolve burst test: 0 failures (load-dependent). Durable mitigation is devloop-infra (pin `-db` in `/etc/hosts`, or `resolv.conf options timeout/attempts`), out of scope for this test-only Rust devloop.

### Post-commit independent stress verification (Lead, 2026-06-21)
Ran the hardened tests **30×** via the prebuilt test binary directly (bypassing the cargo build-lock to overlap a concurrent `cargo test --workspace` saturation loop): **30/30 runs all-pass, 270/270 executions, 0 failures** (0 health-checker assertion failures, 0 DNS connect failures). Per-run durations spanned **0.9s–8.0s**, confirming real concurrent load during the runs — the old fixed `sleep(6)` would have flaked at the 5–8s durations; the poll-on-flip / direct-call versions did not. One full `cargo test --workspace` saturation round independently ran the same tests with 0 health-checker failures. Race confirmed eliminated. (DNS infra issue: not pinned in this container's `/etc/hosts`, but fixed by the user in a separate session; no DNS hiccups surfaced during this verification.)

### Gate 2 — Targeted Layer 4 re-run (infra retry, attempt not consumed)
`scripts/layer4.sh` → `STATUS=OK REASON=cargo-test-passed` / `test-all-langs-ok`. Full workspace incl. ac-service key-rotation tests green when not hit by the DNS burst. Layer 4 ✅.

### Gate 2 — FINAL DISPOSITION: **PASS for this devloop**
Across attempt-2 + targeted retry: Layers 1✅ 2✅ 3✅ 4✅ 5✅ 7 N/A. Layer 6 = `pnpm-audit-failed` ONLY, the user-confirmed known pnpm advisories tracked under **task #54** (`cargo-audit` ✅, `buf-breaking` ✅) — excepted as a known external condition, not attributable to this test-only Rust diff. **→ Proceed to Gate 3 (review).**

**Commit-gate note (Gate-2 authority hook)**: the `.githooks/pre-commit` Gate-2 authority gate (task #51) keys purely off `layer-all.sh`'s exit code, which is non-zero solely because of the Layer-6 pnpm advisories (task #54). It therefore cannot emit a PASS verdict until task #54 lands. Per the hook's own documentation it is **LOCAL-ONLY / ADVISORY (anti-drift, not a security boundary)** and explicitly sanctions `git commit --no-verify` for intentional cases. This devloop's commit was made with `--no-verify` for this single known-external reason (fmt/clippy independently verified clean; all 8 Gate-3 verdicts CLEAR). **CI remains the non-bypassable enforcement point** and will stay red on pnpm audit until task #54 lands — that is expected and correct.

---

## Code Review Results

| Reviewer | Verdict | Findings | Notes |
|----------|---------|----------|-------|
| Security | **CLEAR** | 0 | No security surface touched; synthetic fixtures, no secrets; test-only |
| Test | **CLEAR** | 0 | Gating sleep gone everywhere; poll loop has no false-positive path; count==0 strengthens coverage; sibling `assignment_cleanup.rs` already consistent |
| Observability | **CLEAR** | 0 | No metrics coverage dropped; positives still exercise `refresh_controller_metrics`. Pre-existing non-blocking note: MH loop lacks a registered-handlers gauge (unrelated to this diff, optional TODO) |
| Code Quality | **CLEAR** | 0 | ADR-0005/0009/0024 compliant; verified `marked==0` is backed by the SQL predicate (not a tautology); idiomatic poll loop; no dead imports; Ownership Lens recorded (Minor-judgment, not GSA) |
| DRY | **CLEAR** | 0 | No `common` reimplementation; poll pattern at 2 sites < ADR-0019 extraction bar; no TODO added |
| Operations | **CLEAR** | 0 | No deploy/runbook impact; confirmed genuine de-flake (removes race + ~42s of fixed sleeps); DNS infra item noted as out-of-scope observation |
| Semantic Guard | **SAFE → CLEAR** | 0 | Test-only; credential-leak / actor-blocking / error-context / metrics-path all clear |
| Paired global-controller (Ownership Lens) | **CLEAR** | 0 | Cross-boundary edit sound; all 4 staleness outcomes intact + strengthened; no production behavior changed; hybrid landed exactly as ruled |

**Gate 3 result: ALL CLEAR — 8/8 reviewers, zero findings, zero deferrals, zero escalations.**

Informational observations (NOT findings in this diff, NOT deferrals):
- **MH registered-handlers gauge asymmetry** (observability): pre-existing — MH health-checker loop refreshes no gauge while the controller loop calls `refresh_controller_metrics`. Appended to `docs/TODO.md` §Observability Debt (owner: global-controller + observability).
- **`#[sqlx::test]` DNS flakiness** (operations + Lead): transient `EAI_AGAIN` from aardvark-dns under saturation (test-DB host unpinned in `/etc/hosts`). Devloop-infra, out of scope; disposition deferred to user decision.

---

## Accepted Deferrals

(none) — Gate 3 was unanimous CLEAR with zero findings; nothing was deferred or spun out, so there is no finding remaining in the diff. The two informational items above are pre-existing/out-of-scope observations, not deferrals of findings from this devloop:
- `docs/TODO.md` §Observability Debt — GC MH-health-checker missing registered-handlers gauge (pre-existing; surfaced during review)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `6ad7049ab146d67eeafb93b4c7277d67942cca1d`
2. Review all changes: `git diff 6ad7049..HEAD`
3. Soft reset (preserves changes): `git reset --soft 6ad7049`
4. Hard reset (clean revert): `git reset --hard 6ad7049`
5. No schema/infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

### Issue 1: Crossed plan-approval messages (uniform-(a) vs hybrid)
**Problem**: The Lead issued an interim "Plan approved (uniform a)" while the owner was concurrently refining their Gate-1 call to Option 2 (hybrid); the messages crossed, and the implementer briefly held a uniform-(a) tree while the recorded decision said hybrid.
**Resolution**: Lead issued a single authoritative gatekeeper ruling (hybrid is final, recorded in main.md Gate-1 section), owner corroborated, implementer reverted only the 2 positive tests to poll-with-timeout. No wasted cycles beyond the rework of 2 tests.

### Issue 2: Gate-2 Layer 2 (format) failure
**Problem**: One `assert_eq!(marked, 0, …)` exceeded rustfmt width; the implementer had run clippy but not `cargo fmt --check`.
**Resolution**: `cargo fmt -p gc-service` produced the wrap; re-verified clean. Implementer added `cargo fmt --check` to their pre-"Ready" checklist.

### Issue 3: Gate-2 Layer 4 transient DNS failure
**Problem**: An ac-service `#[sqlx::test]` panicked with `EAI_AGAIN` ("Temporary failure in name resolution") under parallel saturation — not this diff (gc test modules only).
**Resolution**: Diagnosed as a transient aardvark-dns failure (Podman test-DB host unpinned in `/etc/hosts`, resolved per-connect under load). Infra-class → targeted `layer4.sh` retry passed clean (attempt not consumed). Recorded for a possible devloop-infra follow-up.

---

## Lessons Learned

1. **First `tokio::time::interval` tick is immediate** — the original `sleep(6)` was mostly dead wait; the race only bit under saturation. Understanding the loop's actual timing was key to choosing the right fix per test.
2. **Poll-with-timeout fits positive assertions; direct-call fits negative ones.** You cannot poll for "state stayed the same" without a fixed wait, so negative-assertion tests are strictly better as synchronous direct calls. The hybrid split preserved end-to-end loop wiring exactly where it adds value (the 2 positives).
3. **Mechanism-language framing caught the 7th sibling.** Restating "fix the 6 tests" as "remove the spawn+sleep(6) mechanism" surfaced `skips_already_unhealthy` as the same-owner, same-mechanism 7th — fixed in-class rather than left flaky.
4. **Gatekeeper must serialize gate decisions.** Interim approvals that cross with a refined owner call cause churn; a single authoritative ruling recorded in main.md resolves it cleanly.
5. **`cargo fmt --check` belongs in pre-"Ready" verification** — clippy does not catch formatting.
