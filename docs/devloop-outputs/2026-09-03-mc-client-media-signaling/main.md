# Devloop Output: MC Client-Facing Media Signaling (ADR-0036 Loopback)

**Date**: 2026-09-03
**Task**: Client-facing signaling for the ADR-0036 loopback media session over the client-to-MC WebTransport channel (receive-capability in; send directive + slot assignment out; mute held-directive semantics)
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2), full
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: 2026-09-03 → 2026-09-05 (three sessions; two interruptions plus one container rebuild)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1d755455ec617b181780458c1c9b86a61528b919` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Story | `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` task 14 |
| Headless | yes (`DEVLOOP_HEADLESS=1`) |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (meeting-controller) |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-FIXED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `CLEAR` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |
| Infrastructure (cross-boundary owner) | `RESOLVED-DEFERRED` |
| Resumed | 2026-09-04: session interrupted after Gate 2 PASS; full roster (implementer + 8 reviewers) respawned at `review`. Lead addressable as `main`, not `team-lead` — corrected mid-spawn and broadcast to the panel. |
| Resumed (2) | 2026-09-05: session interrupted a SECOND time, mid-Gate-3. Review fixes for S-1/O-1..O-12/F1-F3/D1-D2/I-A/I-B are IN THE TREE (see §Review fixes) but **no reviewer verdict was ever recorded** — §Code Review Results is still the unfilled template, which is Issue 1's failure mode repeating. Full roster respawned at `review`; every reviewer instructed to re-derive findings from the current diff rather than trust the empty verdict table. Lead addressable as `main`. |
| Resumed (3) | 2026-09-05: resumed after the container was rebuilt for the PID-exhaustion defect (commit `c01aaf7e`). All eight verdicts were already in and the tree was already quiescent, so **no roster was respawned** — the only outstanding work was the Gate 2 authority run the container had blocked. Two stale ledger rows (O-27, O-28) corrected against the tree first; the Layer-3 `inline_debt_body` violation that run surfaced was fixed; then re-run. |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
-->

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed (token spelling: `slot_id_not_planned`) |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Infrastructure (cross-boundary owner, ADR-0024 §6.3) | confirmed (three manifest rows confirmed at Minor-judgment) |

### Lead rulings during planning

1. **Slot-namespace scoping (option 3).** Task 13 pushes MH forwarding policy once at first-participant
   join, before any capability declaration exists, so declared slots cannot be an input to
   `compute_assignment` this story and a capability-triggered re-push is task 13's excluded scope.
   `compute_assignment` stays the single producer; the declaration is validated and joined against it.
   Both alternatives land additively later.
2. **Slot-id namespace mismatch REJECTS.** A conformant declaration whose audio slot id does not match
   the planned egress slot id is rejected whole, with its own outcome token, rather than reported as
   `SLOT_STATE_FEWER_SOURCES_THAN_SLOTS` (false — MC has a source and MH is forwarding it) or bent onto
   `SLOT_STATE_SOURCE_UNREACHABLE` (semantic drift on a published wire enum). Implementer refined the
   predicate to "reject iff MC holds a planned audio egress whose slot id is absent from the
   declaration"; dry-reviewer added the zero-declared-audio-slots exemption
   (`SLOT_STATE_ZERO_REQUESTED`, not a rejection). Retired by the re-push story.
3. **`MEDIA_KIND_UNSPECIFIED` REJECTS** (security said unfulfillable, operations said reject; operations
   is right). `signaling.proto` already fixes fail-closed semantics for zero values on `Codec` and
   `TransportMode`; proto3 yields zero by omission, so a missing required field is a client
   malformation, not an unsatisfiable constraint. Triage attribution decides it.
4. **security's F2 classification upgrade accepted**: the three `infra/services/mc-service/*.yaml` rows
   move Mechanical → Minor-judgment, owner `infrastructure`, who was added to the panel as a
   cross-boundary owner-reviewer per ADR-0024 §6.3.
5. **Directive emission is capability-triggered**, per the task brief's own integration criterion; a
   client that joins and never declares is never told to send. Recorded as a contract, not an accident.

---

## Gate 2 — Validation Record (Lead)

Run via `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (unattended caller → run-all, per ADR-0033 §4).

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | |
| 2 Format | OK | |
| 3 Guards | OK | all guards + self-tests; `cross-boundary-scope-no-drift`; budget-breach WARN only (49s vs 20s) |
| 4 Test | OK (aggregate N/A) | `cargo-test-passed`, `nx-test-passed`; aggregate `N/A` from proto's intentional-gap placeholder (`not-applicable-to-this-lang`, ADR-0033 §6 self-justifying) |
| 5 Lint | OK | clippy, buf-lint, nx-lint |
| 6 Audit | OK (aggregate N/A) | `cargo-audit-passed`, `buf-breaking-passed`; `SKIPPED-NO-DIFF no-dep-changes` per the Layer-6 dep-manifest gate |
| 7 Env-tests | OK | `env-tests-passed` + `browser-e2e-passed` (8/8 Playwright) |

### Two failures on the first pass, both diagnosed rather than retried blind

**Layer 4 FAIL (first pass, 18s).** Not a code regression. 18s is far short of the ~163-180s a real Rust run
takes; the Rust wrapper aborted early before `run_and_emit` (its documented
`wrapper-aborted-early-exit` trap) because the `dark-tower-postgres-test` container was still coming up.
Re-ran against a warm DB: `cargo-test-passed`, 392 mc-service lib tests + all integration suites, 0 failures.
Precondition-shaped, so it did not consume a Gate-2 attempt.

**Layer 7 PRECONDITION_FAILURE → the diff's own predicted failure mode, live.** `dev-cluster setup` failed on
`port allocation failed: port 24500 (prometheus) is already in use`, which was a *symptom*: the cluster was
already up, and setup only re-ran because `mc-0`/`mc-1` were unhealthy. Both were in `CrashLoopBackOff` with
`Error: MissingEnvVar("MC_MAX_RECEIVE_SLOTS")` — the new MC image deployed against the **stale** in-cluster
ConfigMap and Deployment, neither carrying the five required keys.

This is **exactly** infrastructure's I7 raised against their own I1, and operations' §Config-failure triage,
reproduced in the cluster before either document shipped: making the keys required converts a benign
misconfiguration into a two-pod startup outage, and `kustomization.yaml` uses `resources:` not
`configMapGenerator:`, so there is no content hash and no automatic roll. The diff itself is correct — all
five keys are present in `configmap.yaml` (`MC_AUDIO_MAX_BITRATE_BPS: "48000"` per the Lead ruling) and both
deployments carry the per-key `configMapKeyRef`. Applying the ConfigMap alone left the pods crashing, because
the Deployment still had no ref for the key; applying both, then rolling, brought MC to `1/1 Running`.
The manifest change and the code change roll together — the corrected posture claim, demonstrated.

### Gate 2 re-run on resume (2026-09-05) — VOID, and why it is not a diff defect

The resumed Lead re-ran `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` against the post-review-fix tree
**concurrently with the respawned reviewer panel**. It went `TOTAL_RESULT=FAIL` on
`LAYER=3 RESULT=FAIL REASON=run-story-selftest-failed` (1, 2, 5, 7 OK; 4 and 6 the documented
aggregate-`N/A`). `scripts/layer3.sh` re-run standalone reproduced it; `scripts/workflow/run-story.test.sh`
run standalone exited **0** with `290 passed, 0 failed`.

The self-test's own containment check is what fired: `sha256(git diff HEAD)` changed across the suite
while `sha256(git status --porcelain)` did not — an in-place edit to a file already in the changeset.
Its message concludes *"the post-run state is STABLE across two samples, so no concurrent writer
explains this: the suite is the actor. Do NOT weaken this check."*

**That inference is wrong in this configuration, and it was checked rather than assumed.** The
layer-3 retry window was `START=1788577416 END=1788577466`; `crates/mc-service/src/actors/meeting.rs`
carries mtime `1788577464` and three `docs/runbooks/*.md` carry `1788577466`/`1788577486` — the
implementer and reviewers were landing Gate-3 fixes throughout. Two-sample stability only establishes
that the writers had stopped by sampling time, which is exactly what a review round that has just gone
quiet looks like. The check is correct to be loud about an unexplained tree mutation and **was not
weakened, disabled, or annotated around**; the defect is the Lead's sequencing.

Consequences, recorded so the green run that follows is not mistaken for a retry of this one:
- This run is **void as a Gate-2 verdict** — it measured a moving tree. It is not an attempt against
  the 3-attempt budget either, being neither a code failure nor a `PRECONDITION_FAILURE` the pipeline
  itself declared.
- The authority run is taken only against a **quiescent** tree, after every reviewer verdict is in and
  the implementer confirms no further writes.
- Generalisation for this skill, and the reason it is written here rather than in a scratch note: a
  Lead must not overlap the validation pipeline with an active review round. Layer 3 contains at least
  one self-test that samples the real repository, so "run validation while the panel works, to save
  wall-clock" is not a free optimisation — it manufactures a red layer whose message actively argues
  against the true cause.

---

### Gate 2 authority run (2026-09-05, quiescent tree) — BLOCKED on a container defect, NOT on the diff

Run: `CARGO_BUILD_JOBS=4 DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` against the quiescent 41-file tree,
after all eight reviewer verdicts and the implementer's "tree quiescent" signal.

```
LAYER=1 RESULT=OK    DURATION=36
LAYER=2 RESULT=FAIL  DURATION=4     nx-format-failed
LAYER=3 RESULT=OK    DURATION=52    <- the quiescent-tree diagnosis was correct
LAYER=4 RESULT=FAIL  DURATION=221   nx-test-failed   (cargo-test-passed)
LAYER=5 RESULT=FAIL  DURATION=15    nx-lint-failed   (cargo-clippy-passed)
LAYER=6 RESULT=N/A   DURATION=2
LAYER=7 RESULT=FAIL  DURATION=581   browser-e2e-failed (8 failed; env-tests-passed)
TOTAL_RESULT=FAIL
```

**Layer 3 went green**, which closes the earlier question: the containment abort really was concurrency,
and quiescing the tree really was the fix. Every Rust-side wrapper is green — `cargo-fmt-passed`,
`cargo-build-passed`, `cargo-clippy-passed`, `cargo-test-passed`, `env-tests-passed`, `buf-*`, all
guards. **Every failing wrapper is a Node one**, and the diff contains **zero** TypeScript, Svelte or
`packages/**` files (verified by enumeration, not by filter).

**Root cause, diagnosed rather than retried.** All four failures are one fault. Each Node wrapper dies
identically at nx addon load:

```
panicked at rayon-core-1.13.0/src/registry.rs:171:
The global thread pool has not been initialized.:
ThreadPoolBuildError { kind: IOError(Os { code: 11, kind: WouldBlock, ... }) }
```

`EAGAIN` on thread creation. The container's PID accounting:

| Limit | Value |
|-------|-------|
| `nproc` | 32 |
| `ulimit -u` | 63473 |
| `/proc/sys/kernel/threads-max` | 126946 |
| **`/sys/fs/cgroup/pids.max`** | **2048** |
| **`/sys/fs/cgroup/pids.current`** | **1917** |

**1 890 of those 1 917 tasks are zombies** — `bash` 581, `sleep` 456, `chrome-headless` 330, `node` 216,
`cat` 187, `sh` 90. Every one has `PPid: 1`, and **PID 1 in this container is `sleep infinity`**, which
never calls `wait()`. So every orphaned child re-parents to a non-reaping init and holds its PID slot
for the container's lifetime. A long multi-agent devloop is a zombie generator: nine subagents plus a
Playwright suite plus the Lead's own polling loops, and the count only ever rises.

The binding limit is the **cgroup** `pids.max`, which is the one limit neither `nproc` nor `ulimit -u`
reveals. This corrects a claim made during review — "`nproc` 32, `ulimit -u` 63473, so not a configured
cap". There *is* a configured cap; it is simply not visible from either place anyone looked.

**Not maskable, and I did not mask it.** `--parallel=1` does not help: the panic is in nx's native addon
at *module load*, before nx's own concurrency setting applies, so nx cannot start at all in this
container. Zombies cannot be killed — they are already dead — and PID 1 cannot be replaced from inside.
There is no in-container remedy, and the only remedies that exist would be gate-weakening: skipping the
Node layers, or re-running until a transient window opened. Both were rejected.

**Why this matters beyond this loop, and why it is dangerous.** PID exhaustion does not announce itself.
It surfaces as a *compiler ICE*, an *nx SIGABRT*, or *8 browser tests failing* — three signatures that
each read as a code defect in the diff under test. The implementer hit the same fault earlier as 21
rustc ICEs and correctly refused to retry it into green. Under the story runner this reds a gate and
escalates against a clean diff, which is the same class of misattribution as the Layer-3 containment
check, arriving through a different mechanism. The fix is a container one: an init that reaps
(`--init` / `tini`), or a larger `pids.max`, or both.

**Consequence for this devloop.** The gates did not pass, so per Step 8 there is **no commit**. Work is
complete and all eight verdicts are in; the blocker is entirely environmental and clears on a container
restart, at which point the same tree should be re-validated and committed. Escalation written to
`.devloop-escalation.json`.

---

### Gate 2 authority run, attempt 2 (2026-09-05, post-container-fix) — the container defect is gone; one real diff defect surfaced underneath it

The container was rebuilt against commit `c01aaf7e` ("Run the devloop dev container with a reaping init
and a higher PID limit"). Verified before spending a pipeline run rather than after: `pids.current` 27 of
2048 with **zero zombies** (against 1 917/2048 and 1 890 zombies at the blocked run), and
`pnpm exec nx --version` returns cleanly instead of panicking in the native addon at module load.
Note that PID 1 is still `sleep` and `pids.max` is still 2048 in this container — what actually changed
is that the container is *fresh*, so the headroom exists. **The reaping-init half of the fix is not
demonstrated by this run**, and a long enough session in this container could still exhaust the cgroup;
that is a claim about what was measured, not a clean bill of health.

```
LAYER=1 RESULT=OK    DURATION=2
LAYER=2 RESULT=OK    DURATION=1     nx-format-passed   <- was FAIL
LAYER=3 RESULT=FAIL  DURATION=51    guard-violations
LAYER=4 RESULT=N/A   DURATION=181   cargo-test-passed + nx-test-passed  <- nx was FAIL
LAYER=5 RESULT=OK    DURATION=2     nx-lint-passed     <- was FAIL
LAYER=6 RESULT=N/A   DURATION=4
LAYER=7 RESULT=OK    DURATION=514   env-tests-passed + browser-e2e-passed  <- was FAIL (8 failed)
TOTAL_RESULT=FAIL
```

**All four Node failures cleared at once**, which is the diagnosis confirmed: they were one fault
(cgroup PID exhaustion surfacing as an `EAGAIN` rayon panic), not four defects in a diff containing zero
TypeScript files. Layer 7's browser E2E went 8-failed → passed with no change to any `packages/**` file,
because there is none in the changeset.

**The one remaining red is a genuine diff defect, and it was hiding behind the container fault.**

```
FAILED: validate-todo-tracking (exit 1)
VIOLATION: docs/devloop-outputs/…/main.md:1810 [inline_debt_body]
           tech-debt body inlined; use pointer bullets instead
```

§Accepted Deferrals is **pointer-only by contract**: `dt-guard todo-tracking` reds on two consecutive
non-bullet lines inside it, so that each deferral's durable body lives in `docs/TODO.md` and cannot fork
from the copy in a devloop output. The six-line paragraph classifying the eight pointers had been written
into that section during the interrupted round's write-up — *after* the previous authority run measured
Layer 3, which is why that run saw Layer 3 green and this one does not. Not a regression from the
container fix.

Fixed by **re-homing the paragraph, not by reshaping it to satisfy the matcher**. Wrapping it into one
long bullet or breaking it with blank lines would both have cleared the guard while leaving prose in a
section whose whole point is that prose does not live there. It now sits as `### Deferral classification`
immediately above §Accepted Deferrals, inside §Code Review Results, with a line recording why it is there
— so the next writer does not move it back.

Attempt accounting: this is **attempt 1 of 3** consumed (a `FAIL`/exit 1 in the implementer lane, per the
Limits table). The blocked run before it consumed none, being `PRECONDITION_FAILURE`-shaped in substance.

**A sequencing error, recorded because §Gate 2 re-run already warned against exactly it.** The
`inline_debt_body` fix above was written to `main.md` while this pipeline was still inside Layer 7. Layer
3 — the layer carrying `run-story.test.sh`'s containment check — had completed 600s earlier, so this run's
verdict is unaffected and is not void. But the rule that section states is "do not overlap the pipeline
with tree writes", and "the layer I raced had already finished" is luck, not compliance. The clean re-run
below was therefore started only after every write to the tree was complete.

---

### Gate 2 authority run, attempt 3 (2026-09-05, quiescent tree) — PASS

Re-run after the `inline_debt_body` fix, started only once every write to the tree was complete.

```
LAYER=1 RESULT=OK   DURATION=2
LAYER=2 RESULT=OK   DURATION=2
LAYER=3 RESULT=OK   DURATION=53    guards-passed (40 guards, 0 violations, 0 timeouts)
LAYER=4 RESULT=N/A  DURATION=185   cargo-test-passed + nx-test-passed
LAYER=5 RESULT=OK   DURATION=1     cargo-clippy-passed + buf-lint + nx-lint-passed
LAYER=6 RESULT=N/A  DURATION=2     cargo-audit-passed + buf-breaking-passed
LAYER=7 RESULT=OK   DURATION=274   env-tests-passed + browser-e2e-passed
TOTAL_DURATION=519 TOTAL_RESULT=N/A  (WRAPPER_EXIT=0)
```

**`TOTAL_RESULT=N/A` here is a PASS, and the reason is checked rather than assumed.** No layer is `FAIL`
and none is `NOT-RUN`; the aggregate is `N/A` because worst-child aggregation propagates the two
documented self-justifying statuses of ADR-0033 §6 — Layer 4's and Layer 6's proto intentional-gap
placeholders (`REASON=not-applicable-to-this-lang`) and Layer 6's dep-manifest gate
(`SKIPPED-NO-DIFF no-dep-changes`, no dependency manifest in the changeset). Every substantive wrapper
under both layers is `OK`. That this top-line is not self-describing on a fully-green run is a known
aggregator wart already filed in `docs/TODO.md` §Observability Debt; it is not this diff's.

Layer 3's containment check passed again on a quiescent tree, third time. Layer 7 also came in at 274s
against the blocked run's 581s — the cluster was warm, and the browser suite no longer burns its
timeout budget failing on thread exhaustion.

**Gate 2 verdict: PASS.** One attempt consumed of three (attempt 2's `inline_debt_body` violation);
the container-blocked run consumed none.

**Deferral pointers verified before Step 8**, as §Accepted deferral (infrastructure I-E) requires of the
Lead — all four named `docs/TODO.md` sections exist and all eight pointer bodies resolve, including the
two observability class entries. Checked, not assumed: the ledger's own recurring failure mode in this
loop is a pointer or status that stopped being true while nobody re-read it.

**Gate 3 verdict: all eight reviewer verdicts already in** from the round-2 panel (§Code Review Results),
with no reviewer left `pending` and no finding `ESCALATED`. The panel was **not** respawned for this
resume: the outstanding work was a pipeline run the container had blocked, not review, and the diff's
only changes since the verdicts are (a) two stale ledger statuses corrected to match the tree and (b)
relocating a paragraph out of a pointer-only section. Neither touches code or reviewer-owned policy, so
re-opening review would have manufactured a third round over a bookkeeping edit. Two ledger rows (O-27,
O-28) still read `open` against fixes that had in fact landed — verified against the tree by content
before correcting them, not taken from the verdict table that disagreed with them.

---

## Task Overview

### Objective
Land the client-facing half of the ADR-0036 loopback media session in MC: validate the client's
receive-capability declaration, emit the send directive that names the one audio media stream the
client must produce (encoding parameters, derived header version, datagram target set), and emit the
stream assignment that fills the client's declared audio slot with its own audio and states the
slot's condition explicitly on the wire. Record reported client mute without touching the directive.

### Scope
- **Service(s)**: `mc-service` only (plus its ConfigMap/deployment mirror and the observability catalog/dashboard).
- **Schema**: No.
- **Cross-cutting**: No new service-to-service path. Client<->MC WebTransport signalling only; the
  MC->MH control plane (task 13) is consumed read-only and is not re-pushed.

### Debate Decision
NOT NEEDED - ADR-0036 is Accepted and prescriptive for every decision here (SS2, SS5, SS6, SS7, SS11);
the proto shapes landed in task 3. One scoping call is surfaced to `team-lead` below rather than
debated (SS Open question).

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_signaling/**` (new: `mod.rs`, `capability.rs`, `directive.rs`, `assignments.rs`, `outcome.rs`) | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/participant.rs` | Mine | — |
| `crates/mc-service/src/config.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/src/lib.rs` | Mine | — |
| `crates/mc-service/src/main.rs` | Mine | — |
| `crates/mc-service/tests/media_client_signaling_integration.rs` (new) | Mine | — |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | Mine | — |
| `crates/mc-service/src/grpc/gc_client.rs` (test fixture) | Mine | — |
| `crates/mc-service/tests/gc_integration.rs` (test fixture) | Mine | — |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` (test fixture) | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/TODO.md` | Not mine, Minor-judgment | operations |
| `docs/devloop-outputs/2026-09-03-mc-client-media-signaling/main.md` | Mine | — |
| `docs/observability/metrics/mc-service.md` | Mine | — |
| `infra/grafana/dashboards/mc-overview.json` | Mine | — |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | operations |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alert-conventions.md` | Not mine, Minor-judgment | observability |
| `infra/services/mc-service/configmap.yaml` | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-incident-response.md` | Not mine, Minor-judgment | operations |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/kustomization.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/kustomization.yaml` | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/kustomize.rs` | Not mine, Minor-judgment | infrastructure |
| `crates/common/src/observability/testing.rs` | Not mine, Minor-judgment | observability |

**On the three rows added during review — same principle, different trigger.** The two
`kustomization.yaml` rows and `crates/dt-guard/src/kustomize.rs` are infrastructure's work in
infrastructure's domain, authored by them during Gate 3 and not edited by me: they found that
`mc-service-tls` is consumed by both Deployments but created imperatively by
`infra/kind/scripts/setup.sh`, so `kubectl apply -k` on a fresh namespace yields pods that cannot
start with nothing declarative saying why, and they fixed the MH sibling in the same shape rather
than the reported instance alone. The guard-crate row is the comment-stripping fix in
`extract_declared_resources` that their own first comment tripped over. All three are rows here for
the same reason the runbook rows are: the table is the plan-set `dt-guard`'s Layer A checks the
whole diff against, and `git add -A` does not care who typed a hunk. Guard **machinery** is
infrastructure's per CLAUDE.md, which is why `kustomize.rs` carries their name and not mine.

**On the `crates/common/src/observability/testing.rs` row — added during the resumed
Gate 3.** `CounterQuery` gained a `delta()` reader alongside its existing `assert_delta`.
It is `crates/common/`, not `crates/mc-service/`, and the metric-assertion harness is
observability's per ADR-0032, so it is Minor-judgment with observability as owner rather
than Mine — the classification follows the artifact, not the fact that only an MC test
consumes it today. It is **not** a Guarded Shared Area: it is neither on ADR-0024 §6.4's
enumerated list nor matched by any §6.4 criterion (no wire-format runtime coupling, no
auth-routing policy, no detection/forensics contract, no schema evolution). Additive only —
no existing signature or behaviour changed, so no consumer elsewhere in the workspace can
be affected. Why it was needed rather than worked around is in §Review fixes under test's
T-1: the mute rate-limit test asserted an exact per-label split that is only correct when
the suppressed messages form a single run, and the run structure depends on how many refill
boundaries the client's writes crossed. The sum over the two labels is exact for every run
structure; asserting it needs a read, and the API was assert-only.

**On the four `docs/runbooks/**` rows — authorship versus declaration.** Operations authored every
hunk in those four files and I have edited none of them; their authorship is the hunk-ACK and they
confirm at Gate 3. The rows are here anyway because this table is **not** a claim about who typed
the edit: `dt-guard`'s Layer A compares the changed-file set of the whole diff against the plan
table, Step 8 commits with `git add -A`, and `docs/runbooks/**` is not on the exemption list
(`docs/TODO.md`, `docs/specialist-knowledge/*/INDEX.md`, `docs/user-stories/*.md`, the
devloop-output tree). Four modified runbooks with no rows is four `scope_drift_inbound` findings.
The table is the plan-set the diff is checked against, and I own it as the plan's author. The causal
split — what this diff created versus the pre-existing class defect — is in §Implementation Summary,
and is the more important half of the record.

**Explicitly NOT touched (Guarded Shared Areas, ADR-0024 SS6.4):** `proto/**`,
`crates/media-protocol/**`, `crates/common/src/webtransport/**`. Every shape this task needs already
landed in task 3; `media-protocol` is consumed read-only for `PROTOCOL_VERSION`.

---

## Planning

### Mechanism restatement (task framing vs. the actual problem)

**Instance-language (the task's own nouns):** "after a client joins, send it a send directive naming
one audio stream, and assign its own audio to its own slot."

**Mechanism-language:** MC composes each subscriber's per-slot view by **joining two
independently-timed inputs** — (a) the forwarding policy already pushed to MH, which fixes what MH
stamps into the frame's relay-region `stream_id`, and (b) the subscriber's later capability
declaration, which fixes what that subscriber will accept — and **every partial case of that join
must be explicit on the wire and observable in telemetry, never inferred from absence** (ADR-0036
§6: "Absence of frames is not a signal").

The restated mechanism forbids the task's nouns: it never says "loopback", "one", or "audio". The
wider class it produces has same-owner siblings the task does not name — the identical
declared-vs-assigned join recurs for video and screen-share slots (§6), for §7 pending switches
(where the join gains a third input, the switch command id), and structurally it is the same shape
as MC's existing `MediaConnectionUpdate` handling (client-reported connectivity joined against
MC-assigned handlers). I am therefore building the **join with its partial cases enumerated**, not a
one-slot happy path: the cost is one `match` over an enumerated slot-state set that ADR-0036 §6
already fixes at seven values, and the alternative (a happy-path zip that silently drops
non-matching cases) is precisely the failure the ADR bars.

### Open question surfaced to `team-lead` (scoping, not design)

Task 13 pushes MH's forwarding policy **once, at first-participant join**, before any capability
declaration exists, so `EgressStreamPlan.slot_id` is fixed to `MAIN_AUDIO_SLOT_ID = 0` at that
moment. The subscriber then declares its own slot ids. Three options:

1. **Re-push MH policy when the declaration arrives.** Correct in the limit (`internal.proto`'s
   `egress_stream_id` comment anticipates exactly this: "`slot_id` is client-chosen and is
   renumbered whenever the subscriber re-declares its receive capability"). Out of scope: task 13
   explicitly excludes structural re-push, and it would change MC→MH behaviour this task does not own.
2. **Make declared slots an input to `compute_assignment`** (which `assignment.rs`'s
   `MAIN_AUDIO_SLOT_ID` rustdoc pre-commits to, and which dry-reviewer reads as the intent).
   Rejected **for this story only**: at join the participant has declared nothing, so the join-time
   push would carry an empty policy and the loopback would never forward. It also breaks task 13's
   landed loopback fixtures and env-test D.
3. **CHOSEN: keep the assignment as the single producer of "which slot MC routes into", validate the
   declaration, and join the two with the partial cases explicit.** A declared slot with no matching
   plan is conveyed as `SLOT_STATE_FEWER_SOURCES_THAN_SLOTS` with `sender_id` absent; a *planned*
   slot the subscriber never declared is counted and warned (bounded), never silently emitted. A
   conformant loopback client (one audio slot, id 0) is fully served; a non-conformant one gets an
   explicit, honest wire state plus a loud server-side signal. No silent failure in either direction,
   and option 1 or 2 lands additively on top when its story arrives.

This is the one item where I would take a different instruction from `team-lead` without argument.

### Design

**New module `crates/mc-service/src/media_signaling/`** — MC→client media signalling, sibling to
`media_routing` (MC→MH control plane). Pure and testable without QUIC, actors, or prost round-trips
where possible.

- `capability.rs` — **parse, don't validate.** `ReceiveCapabilityDeclaration::parse(&v1::ReceiveCapability, max_slots)`
  returns a type that *cannot* hold an invalid declaration:
  - slot count > configured cap → reject **whole declaration** (`slot_count_over_cap`)
  - `u16::try_from(slot_id)` fails → reject whole declaration (`slot_id_out_of_range`) — a real
    range check compiled into release, never `debug_assert!`, never clamp/truncate
  - duplicate `slot_id` → reject whole declaration (`duplicate_slot_id`) — never last-write-wins
  - `pinned_sender_id == Some(0)` → reject (`pinned_sender_id_zero`)
  - `pinned_sender_id > 65535` → reject (`pinned_sender_id_out_of_range`)
  - `MEDIA_KIND_UNSPECIFIED` → **NOT a rejection.** Per ADR-0036 §6 every constraint is an upper
    bound and an unsatisfiable request cannot be expressed, so an unspecified kind is an
    *unfulfillable kind*: it matches no source and resolves to
    `SLOT_STATE_FEWER_SOURCES_THAN_SLOTS` on the slot-state channel. It is still fail-closed — it can
    never be silently read as audio, because matching is by explicit `MediaKind::Audio` equality.
    (Answering observability's explicit question; no seventh token.)
  - Local `SlotId(u16)` newtype. **Deliberately not shared with `SenderId`** and no
    `MAX_16BIT_ID` / `validate_16bit_id()` helper: `signaling.proto:610` bars collapsing an
    allocation bound with a validation bound. A boundary comment at each of the three lookalike
    numeric bounds (`SlotId` 16-bit relay-region, `SenderId` 16-bit key-id/non-recycling,
    `stream_number` 8-bit key-id) names the other two.
  - `CapabilityError` with `label() -> &'static str` (the six frozen tokens) and
    `client_message() -> &'static str` (bounded static; no client value echoed back).

- `policy.rs` — `MediaStreamPolicy`: the stream-number → (media kind, encoding parameters) table MC
  directs from. One entry today: `MAIN_AUDIO_STREAM_NUMBER` (imported from
  `media_routing::assignment`, per its own "task 14 consumes this same constant" rustdoc) →
  `MediaKind::Audio` + the configured audio encoding. A plan naming a stream number not in the table
  is an **internal error, fail loud** — never a guess, never a default.
  `AudioEncoding` is constructed only by validated config, so `Codec::Unspecified` is structurally
  unrepresentable at the emit site rather than checked there.

- `compose.rs` — two pure functions with **disjoint inputs**, and that disjointness is the structural
  mute property:
  - `build_send_directive(publisher, &MeetingAssignment, &HandlerUrls, &MediaStreamPolicy) -> Result<SendDirective, ComposeError>`
    **has no mute parameter and no access to mute state.** It is type-level impossible for a
    mute/unmute cycle to alter the directive; the test asserting byte-identity across the cycle is
    the behavioural backstop, not the mechanism. Targets are derived by walking the *same*
    `compute_assignment` output for handlers carrying a plan whose `candidate_sources` contain this
    publisher — no parallel loopback derivation. Empty target set = "send nothing" (§5), a computed
    output. `transport_mode` comes from the plan; `TransportMode::Unspecified` in a plan is
    fail-closed error, never defaulted to datagram. **No priority group** (§5 vs §7 — it is an MH
    egress property on the internal contract and a client able to name it could promote itself past
    the salience bound).
    `header_version: u32::from(media_protocol::frame::PROTOCOL_VERSION)` — derived, one site, no
    literal `2` anywhere in MC. The configured-floor/allowlist obligation is explicitly **not**
    allocated to this task (`signaling.proto` says so, `docs/TODO.md` §Media Path Obligations tracks
    it); I derive the value and do not claim the floor control, and add no code path that could
    express a lower version.
    `stream_number` is a `u8` in `EgressStreamPlan`, widened with `u32::from` (never `as`). The
    reverse direction is the fail-loud one: any path that could produce a wider value goes through
    `u8::try_from` and returns an **internal error**, not a client-rejection counter (per operations'
    amendment) and never a truncation into the key-id stream field.
  - `build_stream_assignments(subscriber, &ReceiveCapabilityDeclaration, &MeetingAssignment, &HandlerUrls, &SourceMuteView) -> (StreamAssignments, Vec<SlotOutcome>)`
    The join. Per declared slot, in declared order:
    | Case | Wire result |
    |---|---|
    | plan matches (slot id + `MediaKind::Audio`), source present, source not audio-muted | `SLOT_STATE_ACTIVE`, `sender_id: Some(..)`, handler url |
    | plan matches, source reports itself audio-muted | `SLOT_STATE_SOURCE_MUTED`, `sender_id: Some(..)`, handler url |
    | plan matches but its handler url cannot be resolved | `SLOT_STATE_SOURCE_UNREACHABLE`, `sender_id` **absent**, url empty |
    | no plan for this slot (incl. video/unspecified kinds, extra slots) | `SLOT_STATE_FEWER_SOURCES_THAN_SLOTS`, `sender_id` **absent**, url empty |
    `sender_id` is `Option<u32>` end to end; there is no `unwrap_or(0)`, no `unwrap_or_default()`, and
    absence is never coerced. `SLOT_STATE_ZERO_REQUESTED` and `SLOT_STATE_SWITCH_PENDING` are
    unreachable in this story (there is no slot to carry the former; switching is §7/story 5) and are
    recorded as such rather than fabricated. A **planned slot the subscriber never declared** is
    returned as an outcome for the caller to count and warn on — never emitted as an assignment the
    client would reject.
    Source identity comes from the authenticated session's assignment output, never from
    `pinned_sender_id`: a pin is a request, and in this story the only satisfiable source is the one
    the assignment already chose.

**Dispatch (`webtransport/connection.rs`)** — `handle_client_message` gains a per-connection context
(owned by the single-threaded bridge loop, so no lock): meeting handle, handler-url map built
**once** per connection from `mh_data` (`HandlerId -> media_handler_url`, feeding `SendTarget` and
`StreamAssignment` from one resolution — no second walk, and no URL derived from pod naming), own
`SenderId`, participant id, config, the last accepted declaration, and a one-shot
`capability_warn_emitted` flag.
- `ReceiveCapability` → parse; on error: counter (always) + WARN **first-occurrence-per-connection
  only**, carrying the outcome token and no `slot_id`/`pinned_sender_id` value, plus a bounded
  `ErrorMessage` to the client. On success: store the declaration, read current meeting state
  (`get_state()` — authoritative roster: sender ids + `audio_self_muted`), run `compute_assignment`
  over the same input builder task 13 uses, then emit `SendDirective` **then** `StreamAssignments`.
- `MuteRequest` → `meeting_handle.update_self_mute(..)` (records reported state; the existing actor
  path), then, **only if a declaration exists**, recompose and emit `StreamAssignments` alone. The
  directive is not rebuilt, not re-sent, not withdrawn. Nothing on this path can reach
  `build_send_directive`.
- `ServerMuteRequest` → no behaviour change. MC has zero references to the old `HostMuteRequest`
  spelling (verified); the actor-side `MeetingMessage::ServerMute` rename already landed. Nothing to
  do beyond confirming no stale symbol remains; enforcement is story 2.
- Never-declared capability: **explicitly a no-op.** No directive, no assignments, no disconnect,
  no error — the session stays healthy and every other post-join message keeps working. Covered by a
  test.
- Emission goes through the existing `ParticipantActorHandle::send(SignalingPayload::Raw)` choke
  point so ordering with roster broadcasts is FIFO. On a full participant mailbox the existing
  behaviour is a WARN; I add the bounded drop counter at that same site so the loss is countable
  rather than log-only.

**Controller** gains `get_meeting_handle(meeting_id) -> Result<MeetingActorHandle>` (one actor
round-trip, once per connection at join) so the bridge loop can reach meeting state and the
self-mute path.

### Configuration (R-32: every new knob env-configured, documented default, startup validation)

All fail-loud parse (the `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` pattern, **not** the lenient
`.parse().ok().unwrap_or(DEFAULT)`), all rejecting `0`, all added to the redacting `Debug` impl, all
`MC_`-prefixed, all set in `configmap.yaml` **and** referenced from both `mc-0-deployment.yaml` and
`mc-1-deployment.yaml` (per-key `configMapKeyRef`; otherwise `orphan_configmap_key` FAILs):

| Key | Default | Why that value | Why `0` is rejected |
|---|---|---|---|
| `MC_MAX_RECEIVE_SLOTS` | `8` | ADR-0036 §11 ships two media kinds; a realistic grid is ≤5 video + audio slots, so 8 is headroom while bounding per-declaration work and per-subscriber egress fan-out | `0` means no client can ever receive media — a silent total outage |
| `MC_AUDIO_CODEC` | `opus` | ADR-0036 §3/§5 and the client's WebCodecs pipeline | unknown/unspecified rejected at load, so `CODEC_UNSPECIFIED` is unrepresentable downstream |
| `MC_AUDIO_MAX_BITRATE_BPS` | `32000` | §3's 32 kbps Opus reference point, from which the ~225 B on-wire frame is derived | a zero-bitrate directive is an unproducible stream |
| `MC_AUDIO_FRAME_RATE_HZ` | `50` | 20 ms frames (§3); halving to 40 ms is the named signature-overhead mitigation | a zero-rate directive is an unproducible stream |

MC directing these makes MC the single source of truth for encoding parameters the client currently
hardcodes.

### Observability (ADR-0031 structured block)

> **SUPERSEDED — see §Final state, as shipped for the metric names, label keys and value domains
> that actually exist.** The table below is the round-2 *proposal*. In particular it specifies
> `mc_media_slot_outcomes_total{outcome}`, **which does not exist**: observability's O-2 split it
> into `mc_media_slot_states_total{slot_state}` and `mc_media_unmatched_plan_slots_total`. It also
> gives the capability counter six `outcome` values where ten shipped, and spells the
> directive counter's empty-target success `empty_targets` where `emitted_empty_targets` shipped.
>
> This marker is at the top rather than at the bottom because the defect was not that a planning
> record contains a superseded proposal — that is what makes it a trail — but that **the superseded
> statement is the one a reader reaches first** while the correction sits 650 lines below with
> nothing linking them. That is not hypothetical: operations wrote `mc_media_slot_states_total{outcome}`
> into `mc-incident-response.md` from this table and was one cross-check against `metrics.rs` away
> from shipping a label key that does not exist into an incident runbook. The table is left intact
> deliberately — rewriting it would erase why O-2 happened.

`key_custody` on every metric below via `common::observability::labels::{KEY_CUSTODY_LABEL,
KEY_CUSTODY_OPERATOR}` — derived, never a string literal, never from config. **No meeting id (raw or
hashed), no participant id, no `slot_id`, no `sender_id`, no `stream_number`, no
`switch_command_id`** as a label, span attribute, or log dimension. No exemplars. Every label domain
is an exhaustive `match` over a Rust enum with an `ALL` const, so a new variant is a compile error.
No new alert (five of six capability tokens name *client* bugs; paging on a remote client's
malformed declaration fires on someone else's deploy).

| Metric | Type | Labels (bounded domain) | Operational question it answers |
|---|---|---|---|
| `mc_media_receive_capability_declarations_total` | counter | `outcome` ∈ {`accepted`, `duplicate_slot_id`, `slot_count_over_cap`, `slot_id_out_of_range`, `pinned_sender_id_zero`, `pinned_sender_id_out_of_range`} (6); `key_custody` (1) | "Are clients' capability declarations landing, and if not, which client bug?" `accepted` supplies the ratio denominator. |
| `mc_media_send_directives_total` | counter | `outcome` ∈ {`emitted`, `empty_targets`, `unknown_stream_number`, `transport_mode_unspecified`, `handler_url_unresolved`, `meeting_state_unavailable`, `assignment_failed`} (7); `key_custody` (1) | "Is MC actually telling clients to send — and when it silently isn't, why?" This is the failure mode a missing directive would otherwise hide. |
| `mc_media_slot_outcomes_total` | counter | `outcome` ∈ the slot states MC emits {`active`, `source_muted`, `fewer_sources_than_slots`, `source_unreachable`} + `plan_slot_not_declared` (5); `key_custody` (1) | "How many slots are filled vs. dark, and is MC planning into slots nobody declared?" The bucketed §11 slot-state signal, joined to no identity. Named `outcome`, not `slot_state`, precisely because one value (`plan_slot_not_declared`) is a server-side condition and not a wire state — flagging for observability to ratify or split. |

Companions in the same diff, per metric: catalog entry in `docs/observability/metrics/mc-service.md`
(`metric_no_catalog`), panel in `infra/grafana/dashboards/mc-overview.json` with explicit
`editorMode` + `range`/`instant` on every target (`metric_no_dashboard`, `target_query_fields`), and
a test reference under `crates/mc-service/tests/**` (`metric-coverage`).

**Coverage demonstration (ADR-0036 "a control's coverage must be demonstrated, not asserted"):**

| Control | Does it FIRE? (injected adverse condition) | Does it APPLY? (premise confirmed against the artifact) |
|---|---|---|
| `mc_media_receive_capability_declarations_total` | Declaration with duplicate slot ids → `duplicate_slot_id` increments; over-cap, out-of-range, `Some(0)` pin each assert their own token | Normal one-audio-slot declaration increments `accepted`, so the denominator is real, not always-zero; asserted through the live framed-`ClientMessage` dispatch seam, not a direct call |
| `mc_media_send_directives_total` | Meeting with no resolvable handler url → `handler_url_unresolved`; a subscriber nobody watches → `empty_targets` | The loopback declaration increments `emitted` on the same path the wire assertion reads the directive from |
| `mc_media_slot_outcomes_total` | Two declared audio slots with one source → one `active` + one `fewer_sources_than_slots`; mute → `source_muted` | Values come from the same `SlotState` the wire assertion decodes, so metric and wire cannot disagree |
| Mute holds the directive | Full mute→unmute cycle over the wire | Exactly one `SendDirective` is ever observed on the connection, byte-identical to the first; `build_send_directive` takes no mute argument, so the compiler is the primary control |

### Security posture

Post-join dispatch is reachable only after the JWT/meeting-binding accept gate (unchanged). Source
identity comes from the authenticated session's assignment, never from client-declared
`pinned_sender_id`. No key material (meeting KEK, transmit key, key id, thumbprint) in any log,
metric, span, or error payload — `media_signaling` reads none. Error types are unit variants with
`&'static str` messages, so no client-controlled value is echoed. Relayed/client strings keep the
existing UTF-8-safe `truncate_utf8` 256-byte bound (no new naive slicing — a `&s[..256]` would panic
mid-codepoint, an ADR-0002 violation). Blast radius of a bad declaration is bounded to the one
subscriber, which is why reject-the-whole-declaration suffices — and that bound rests on MH keying
its routing table on `(subscriber, slot_id)`, which task 13's contract already requires.

### Operations posture

No persisted state: nothing here writes Redis or Postgres, so a previous MC version reads nothing
new, and an MC restart mid-session loses the composed assignment along with the actor state that was
already volatile — no new *data* rollback surface. **But the unqualified R-31 claim this paragraph
originally made is false under I1** and is corrected here rather than left standing, because
operations approved it on the original wording: once the five keys are `MissingEnvVar`,
`mc-service-config` becomes a hard startup dependency of the MC image. `kustomization.yaml` lists
the ConfigMaps under `resources:`, not `configMapGenerator`, so there is no content hash and a
ConfigMap edit does not roll the pods; env is read once at container start. Rolling the *image* back
while the manifests keep the new keys is harmless (surplus env is ignored), but reverting a
**manifest** independently of the image, or applying the image against stale manifests, puts
**both MC pods into CrashLoopBackOff** — every replica failing identically at startup, which the PDB
cannot mitigate, on the signalling plane. That is a *correct* failure and still preferable to two
pods silently directing different encoder parameters.

**The coupled set is THREE artifacts, not two: the ConfigMap, BOTH Deployments, and the image** —
corrected after Gate 2 demonstrated it empirically. The `configMapKeyRef` lives in the **Deployment**,
so the Deployment is what declares that a pod *consumes* a key; the ConfigMap merely supplies the
value. Applying the ConfigMap alone therefore did **not** clear the CrashLoop, because the in-cluster
Deployment still had no ref for `MC_MAX_RECEIVE_SLOTS`. The direction of that error is the dangerous
one: an operator mid-incident reaches for the ConfigMap first — it is the artifact with the values in
it and the obvious "fix the config" move — and the apply produces *no observable change at all*: same
pods, same crash, same key in the message. The natural next inference is "my apply didn't take" or
"the key name is wrong", and the search goes to the wrong artifact. Our own Gate 2 run followed
exactly that path. The accurate statement is: **the ConfigMap, both Deployments and the image roll
together, and none may be reverted independently.** The ConfigMap banner says so in the file an operator reads mid-incident.

**The runbook half is operations' diff, not mine, and is already landed** — four files, 325
insertions, deliberately NOT a row on the table above and not to be duplicated here. Their
§Config-failure triage section carries the detail that makes it usable: required keys produce **two**
startup failures with **opposite first steps**. A new Deployment against an old ConfigMap gives
`CreateContainerConfigError` — no container ever ran, `logs` is empty, `exec` impossible — so the
standard reflex misreads it as a wedged pod when `describe` is what names the missing key. A new
image against an old pod template gives `CrashLoopBackOff`, where `logs --previous` names the
variable. They also verified `kustomization.yaml` uses `resources:` not `configMapGenerator:`,
confirming there is no content hash and therefore that **editing the ConfigMap does not roll the
pods** — a wrong value stays latent until the next restart, which is the fact that makes the
roll-together rule above load-bearing rather than advisory. Work
is per-signalling-message, not per-frame: no retry loop, no unbounded buffer, one bounded actor
mailbox whose overflow is now counted. No runbook scenario numbers taken (15/16 are task 21's); the
frozen outcome tokens are the raw material task 21 keys triage off.

### Tests

Integration — `crates/mc-service/tests/media_client_signaling_integration.rs`, driving the real
framed-`ClientMessage` decode+dispatch seam over a live WebTransport connection (the
`media_connection_update_integration.rs` + `AcceptLoopRig` pattern), asserting **wire bytes and
metric deltas**, never logs and never a stub:
1. **Required (a)** receive-capability in → `SendDirective` + `StreamAssignments` out, with the
   expected fields: one stream, `stream_number == MAIN_AUDIO_STREAM_NUMBER`, `media_kind == AUDIO`,
   `codec == OPUS`, configured bitrate/frame rate, `header_version == PROTOCOL_VERSION` (compared to
   the constant, not to `2`), one target = the seeded MH url + `TRANSPORT_MODE_DATAGRAM`, and **no
   priority-group field on the wire**; one assignment: declared `slot_id`, `sender_id ==` the
   `JoinResponse` sender id, `media_kind == AUDIO`, handler url, `SLOT_STATE_ACTIVE`.
2. **Required (b)** mute → unmute: exactly one `SendDirective` ever crosses the wire and its encoded
   bytes are identical before and after; `StreamAssignments` flips `ACTIVE` → `SOURCE_MUTED` →
   `ACTIVE`.
3. Rejection paths, one test each, each asserting **whole-declaration rejection** (no directive, no
   assignments, an `ErrorMessage`) plus its metric token: duplicate slot ids; slot count over the
   configured cap; `slot_id = 65536`; `pinned_sender_id = Some(0)`; `pinned_sender_id = 65536`.
4. `sender_id` absent is not coerced: two declared audio slots, one source → slot 2 asserts
   `sender_id.is_none()` (not `== 0`) and empty `media_handler_url`.
5. Valid-but-unsatisfiable ≠ rejection: a declared video slot resolves to
   `FEWER_SOURCES_THAN_SLOTS` and the rejection counter does **not** move.
6. A client that never declares: no directive, no assignments, session stays healthy (a subsequent
   `MediaConnectionUpdate` still lands).

Unit — in `media_signaling`: the validation table (including that a rejected declaration leaves no
partially-accepted state), header-version derivation, unknown-stream-number and
unspecified-transport-mode fail-closed, target-set composition at N=1 and with an unwatched
publisher, and the join's four partial cases.

### Plan revisions from Gate 1 reviewer feedback

Recorded as deltas rather than rewritten in place, so the trail from finding to change is legible.

**R1 (dry-reviewer, masked failure) — a slot-numbering disagreement is a rejection, not a source
shortage.** My original table sent "client declared slot 7, MC planned slot 0" to
`SLOT_STATE_FEWER_SOURCES_THAN_SLOTS`, which is false on the wire: MC *has* a source, MH is stamping
0, and the client would drop every frame while being told there is nobody to show it. That is the
exact state §6's explicit slot states exist to prevent. **Corrected predicate:** reject the whole
declaration iff MC holds a planned audio egress for this subscriber whose `slot_id` is not present
in the declaration with a matching media kind. Note the direction — *not* "a declared slot has no
plan", which would wrongly reject the legitimate N-slot case (declare {0,1} with one source: slot 1
is a genuine shortage and keeps `FEWER_SOURCES_THAN_SLOTS`). Loopback: declare {0} accepted;
{0,1} accepted; {7} or {1,2} rejected. `plan_slot_not_declared` therefore moves from a
counted-and-warned drop to a **rejection reason**, and `FEWER_SOURCES_THAN_SLOTS` keeps only its
true meaning. A module rustdoc states plainly that the assigned slot id is not negotiable in this
story and why, so the next reader does not re-derive the ordering constraint.

**R2 (dry-reviewer) — no second home for mute state.** `media_signaling` stores no mute state at
all. `MuteRequest` routes through the existing `MeetingActorHandle::update_self_mute` →
`MeetingMessage::UpdateSelfMute` → `ParticipantInfo.audio_self_muted`. `SLOT_STATE_SOURCE_MUTED`
*reads* `audio_self_muted` from the meeting actor's `get_state()` snapshot as a read-only projection
built per call and not retained. The directive-held rule attaches to client mute (`MuteRequest`)
only; `ServerMuteRequest` is a pure rename with no behaviour (story 2 owns enforcement).

**R3 (dry-reviewer) — module naming.** `policy.rs` dropped: "policy" already means the MC→MH
forwarding assignment in the sibling `media_routing` module. Final layout is `capability.rs` (parse
and validate), `directive.rs` (stream table + `build_send_directive`), `assignments.rs`
(`build_stream_assignments`, the join, the only mute-aware code), `mod.rs`. This also upgrades the
mute-invariance from "this function takes no mute parameter" to "**this module has no path to mute
state**" — a stronger and more legible boundary than a signature.

**R4 (security F1) — client-driven amplification against the shared meeting actor.** Both new
dispatch paths reach an O(N) roster clone serialized on the meeting actor's single mailbox, and MC
has no rate limiting anywhere. Three fixes, all in this diff:
  - (a) `meeting.rs::handle_self_mute` becomes **idempotent**: unchanged audio+video flags return
    without broadcasting. Independently a correctness fix — broadcasting a state change that did not
    occur is a lie on the wire.
  - (b) An identical re-declaration short-circuits **before** the `get_state()` round trip
    (the last accepted declaration is already stored).
  - (c) A per-connection bound on *accepted distinct* declarations, since (a)+(b) leave a client
    alternating between two valid declarations. `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` (default
    64 — generous for legitimate layout changes over a long session) with a counted rejection,
    following the `MAX_MH_STATUSES_PER_PARTICIPANT` precedent. Not deferred: it is a counter and a
    comparison.

**R5 (security F2) — infra rows upgraded.** The three `infra/services/mc-service/*.yaml` rows move
from `Not mine, Mechanical` to `Not mine, Minor-judgment`. Adding keys whose values are chosen
judgments (a security cap; the encoding parameters MC will direct every client's encoder with) is
not value-neutral and no find/replace produces it. Owners: `configmap.yaml` → operations (the
default-value judgment); the two deployment files → infrastructure (the wiring).

**R6 (security F3) — the cap needs its own ceiling.** `MC_MAX_RECEIVE_SLOTS` is validated to
`1..=64` at load with a loud failure outside it. Rejecting `0` alone stops the silent-outage
direction only; a cap whose own value is unbounded is not a cap.

**R7 (security F4) — check ordering is enforced, not commented.** The O(1) slot-count comparison
runs **before** any per-slot iteration or duplicate-detection set is built, matching the discipline
already documented at `connection.rs::handle_media_connection_update`.

**R8 (security F5) — the two url-keyed maps are named apart.** `SendTarget.media_handler_url` and
`StreamAssignment.media_handler_url` come only from `MhAssignmentData` (server-derived). An
invariant comment at the map construction names `ParticipantActor::mh_statuses` — keyed by
**client-supplied** truncated `mh_url` — as the thing it must never be, because a client-controlled
url echoed into `SendTarget` is a redirect primitive: it is MC telling a client where to send its
media. A test asserts a `MediaConnectionUpdate` carrying an attacker url changes no subsequently
emitted `SendTarget` or `StreamAssignment`.

**R9 (security, on `MEDIA_KIND_UNSPECIFIED`) — behaviour unchanged, telemetry split.** Still not a
rejection (§6: an unsatisfiable request cannot be expressed) and still fail-closed (matching is
explicit `MediaKind::Audio` equality). But it is an *unset field*, not a *constraint*, so folding it
into `fewer_sources_than_slots` would make a fleet-wide client regression that stops setting
`media_kind` present as "everyone's grid is slightly under-filled" — indistinguishable from the
honest case. It gets its own `slot_kind_unspecified` outcome on `mc_media_slot_outcomes_total`.

**R10 (semantic-guard) — non-claim made explicit.** An emit-site comment at `header_version` points
at the open floor/allowlist obligation (`signaling.proto` + `docs/TODO.md` §Media Path Obligations)
so a future reader cannot mistake derivation for enforcement.

**Net metric changes from the above.** `mc_media_receive_capability_declarations_total{outcome}`
gains two rejection tokens (`plan_slot_not_declared` per R1, and one for the R4(c) declaration
budget) — observability to ratify the spellings. `mc_media_slot_outcomes_total{outcome}` loses
`plan_slot_not_declared` (now a rejection) and gains `slot_kind_unspecified`, leaving it a set of
wire `SlotState` values plus that one unset-field signal.

### Plan revisions, round 2 (lead ruling + second-pass reviewer findings)

**L1-L3 (lead ruling on the open question).** Option 3 confirmed as the only in-scope option, with
three conditions, all accepted:
  - **L1** — a comment at the match site naming the cross-service namespace coupling: MH stamps the
    egress stream id from the policy task 13 pushed (`MAIN_AUDIO_SLOT_ID`), and
    `ReceiveSlot.slot_id` shares the relay-region `stream_id` value space *precisely so a receiver
    can validate an arriving id against its own declared slots* — so the loopback works iff the
    client declares that same id. Without the comment, a later reader relaxes the equality match
    into a "more flexible" kind-only match and darkens the media path.
  - **L2** — record the obligation in `docs/TODO.md` §Media Path Obligations: the declared audio
    slot id must equal the MH egress stream id until the capability-triggered re-push lands, which
    is the story that retires the constraint. One entry, pointing at both sides.
  - **L3** — the WARN names the namespace coupling as the likely cause, not the bare token. Note
    R1 already strengthens this: the misleading `FEWER_SOURCES_THAN_SLOTS` case the lead was
    guarding against is now a **rejection**, so the client is told plainly rather than left with a
    healthy-looking session and no audio.

**R11 (dry-reviewer) — zero-audio declarations are exempt from the R1 rejection.** As first stated
the predicate over-rejected `{}` and `{1: video}`: planned audio slot 0 is absent, so it fired. But
a participant who wants to send and not receive audio is making a declaration the contract
anticipates, and there is no false claim for MC to make when nothing was requested — unlike `{7}`,
where the client *did* ask for audio, MC *has* a source, and the numbering kills it silently.
**Refined predicate:** reject only when the declaration contains **at least one audio slot** and
none of them matches the planned slot id. One caveat I am not fabricating around: `{}` yields an
empty `StreamAssignments` list, and `SLOT_STATE_ZERO_REQUESTED` stays unreachable in this story
because there is no slot to carry it. A video-only slot resolves to `FEWER_SOURCES_THAN_SLOTS`.

**R12 (test, condition A) — `stream_number` fail-closed needs a real seam to test.** With
`EgressStreamPlan.stream_number` typed `u8`, an out-of-range value is unconstructible and the
required test is unwritable — a structural guarantee with no way to demonstrate it. So `directive.rs`
gets a `StreamNumber` newtype with `from_wire(u32) -> Result<_, DirectiveError>` (`u8::try_from`) at
the one place a wider value could ever enter, plus `from_u8` for the infallible plan path. The unit
test drives 256 and 65536 and asserts they **error rather than wrap to 0**.

**R13 (test, condition B) — both codec controls.** A named config-load test asserting an
unspecified/unrecognised codec is **rejected at load**, plus the belt-and-braces emit-site check.
Accepting the ~2 LoC on test's reasoning: the structural guarantee evaporates silently if a future
refactor builds the directive codec from a path other than validated config, and there would be no
test to catch it.

**R14 (test, condition C) — the non-zero slot id case, two halves.** `{7}` alone → whole-declaration
rejection (R1). And `{0, 7}` → **accepted**: slot 0 is `ACTIVE`, slot 7 is
`FEWER_SOURCES_THAN_SLOTS` **carrying `slot_id == 7`**. That second half is what distinguishes "MC
echoes the client's declared numbering" from "MC and the client coincidentally both said 0" — the
coincidence trap test named, which neither of my original cases could catch.
Confirming test's (D): no priority-group field exists anywhere on
`SendDirective`/`SendStream`/`SendTarget`, so it is structurally unrepresentable and needs no
assertion.

**R15 (operations F1) — the audio encoding knobs are cross-service coupled and get startup
validation.** `MC_AUDIO_FRAME_RATE_HZ` and MH's `AUDIO_FRAME_DURATION_MS = 20` are one fact in two
services in different units, and MH's compile-time assertion cannot see a runtime value MC hands the
client. Setting 25 Hz silently doubles MH's datagram latency ceiling to ~1.28 s while the ConfigMap
still reads 32 frames and MH still logs `nominal_audio_frame_bytes = 236` — nothing fails, nothing
is counted, in the one service whose §1 goal is *prefer loss to unbounded latency*. Both values are
validated at load against the band MH was sized against, failing loudly outside it, with a doc
comment naming `crates/mh-service/src/config.rs::{AUDIO_FRAME_DURATION_MS,
SUPPORTED_AUDIO_BITRATE_FLOOR_BPS, SUPPORTED_AUDIO_BITRATE_CEILING_BPS}` as the counterpart and one
sentence on why. A shared home for the constants is **not** built here — `crates/media-protocol/**`
is a GSA and neither owner is on this devloop; it is recorded as a spin-out following this story's
Assumption 7 shape (one shared ConfigMap both services read and validate locally). The ConfigMap
comment states that `MC_AUDIO_*` has **no effect until task 19** lands the client honouring the
directive.

**R16 (operations F2) — `MC_AUDIO_CODEC` parse.** Trim and case-fold, then fail loud at startup on
anything unrecognised; `CODEC_UNSPECIFIED` rejected if explicitly configured; the video codecs
rejected for an audio-prefixed knob.

**R17 (operations, light question) — answered by R6.** `MC_MAX_RECEIVE_SLOTS` is bounded `1..=64`.
The 64 KiB `MAX_MESSAGE_SIZE` backstop caps a mis-set value at ~6.5k slots rather than catastrophe,
but ADR-0036 §6 leans on this cap for "resource-amplification-by-request becomes structurally
impossible rather than rate-limited", and a cap of 100000 quietly retires that property.

**O-1 — `empty_targets` renamed `emitted_empty_targets`.** ADR-0036 §5 makes an empty target set a
specified success ("that means send nothing"), so it is a sub-case of `emitted`, not an alternative
to it; `outcome!="emitted"` would have counted a legal state as a failure. It also migrates from
anomalous to routine once §7 turns off unwatched publishers — a token whose meaning moves while the
query stays still is a latent false page. Catalog records the success set as
`{emitted, emitted_empty_targets}` and the failure predicate as
`outcome!~"emitted|emitted_empty_targets"`, citing the `mc_media_policy_pushes_total`
`outcome!~"match|handler_id_mismatch"` precedent. It also leaves my fire/apply table: it is not an
adverse condition.

**O-2 — slot metric SPLIT.** My proposed mixing broke the very property my fire/apply table claimed
of it (metric and wire cannot disagree), since `plan_slot_not_declared` corresponds to no wire value.
Two metrics:
  - `mc_media_slot_states_total{slot_state, key_custody}` — label key named after the wire enum, and
    **exhaustive over all eight proto `SlotState` variants including `unspecified`**, not the four
    reachable today. A hand-picked subset needs editing every time a state becomes reachable, and a
    `SLOT_STATE_UNSPECIFIED` reaching the wire is an MC bug that must be visible rather than absent.
    Comparability with the client's distribution is the point, which is why the vocabulary must stay
    the wire's.
  - `mc_media_unmatched_plan_slots_total{key_custody}` — standalone; its denominator is per-planned-slot,
    not per-directive. Still has a job after R1: the R11 zero-audio exemption accepts a declaration
    that leaves planned slot 0 unmatched, so MH forwards egress the subscriber will drop. Catalog
    records that a non-zero value is **client-conformance-driven under option 3**, not necessarily an
    MC defect, so a responder does not go hunting in the wrong codebase.

**O-3 — internal-defect tokens marked as such.** The catalog entry for
`mc_media_send_directives_total` marks `unknown_stream_number` and `transport_mode_unspecified` as
"ANY non-zero value indicates a bug" (the `mc_actor_panics_total` convention), leaving
`handler_url_unresolved` and `meeting_state_unavailable` as environmental. Answering the shadowing
question: they are **disjoint by construction**. The emit path is sequential stages — read meeting
state, compute assignment, resolve handler urls, build streams — and the first failing stage returns
its own token, so a `compute_assignment` failure can never also be reported as an unresolved handler
url, because url resolution is never reached.

**O-4 — the unnamed metric, named.** `mc_participant_outbound_messages_dropped_total{payload_kind}`,
counter, `payload_kind` ∈ {`signaling_raw`, `participant_update`} from an exhaustive enum over the
two `try_send` sites in `actors/participant.rs`. Deliberately a **new** metric rather than a fourth
`actor_type` on `mc_messages_dropped_total`: that one is fed by `MailboxMonitor::drop` on the
*inbound* path, and a pseudo-value would corrupt the `topk` on the cross-service
`errors-overview.json` observability owns. **No `key_custody`** — this is the generic outbound
signalling choke point, not a media metric, and fleet-wide `key_custody` rollout is R-26 / task 22.
Not `reason`: that key is spoken for by the frame-reject vocabulary. Catalog + panel + test
reference land with it.

**O-5 — span discipline, stated.** **No `#[instrument]` anywhere in `media_signaling`** — the
compose functions are synchronous and cheap and a span buys nothing, while their parameters
(`&ReceiveCapability`, `SlotId`, `SenderId`, `&MeetingAssignment`) would auto-record slot and sender
identity as span attributes, violating label-taxonomy R2 on a surface **no guard covers**. If the
dispatch-side handler in `connection.rs` is instrumented at all it is `#[instrument(skip_all)]` with
every recorded field named explicitly and carrying no identity, matching
`handle_media_connection_update`.

**O-6 — ownership correction.** Per ADR-0031 §Ownership split, `docs/observability/metrics/mc-service.md`
and `infra/grafana/dashboards/mc-overview.json` are the **service specialist's**, i.e. mine;
observability owns cross-service dashboards, fleet SLO views, the conventions set and guard
infrastructure. Both rows corrected to **Mine**. This diff touches none of `label-taxonomy.md`,
`dashboard-conventions.md`, `alert-conventions.md`.

**OPEN — `MEDIA_KIND_UNSPECIFIED`: operations and observability have ruled opposite ways.**
Observability ratified "unfulfillable, not rejected" (§6: an unsatisfiable request cannot be
expressed); operations ruled "reject" on three grounds, of which the third is operational and, I
think, decisive: proto3 gives zero for free by omission, so a client that never sets `media_kind` is
the single most likely defect on this field, and under the unfulfillable reading it presents to
on-call as an under-filled grid — sending triage to "who else should be in this meeting", the wrong
team and the wrong service. The file's own house rule is also on operations' side (`TransportMode`:
the zero value is not a convenient default, reject rather than guess), and the `MediaStream` tag-3
reservation exists precisely because an unspecified media kind is the **stale-peer marker**, i.e. a
malformed declaration rather than a request for a kind we happen not to have. §6's "unsatisfiable
request" reasoning does not reach it, because an unset field is not a request. **My recommendation
is rejection with a seventh capability token**; routed to observability as the vocabulary owner,
per operations' instruction. Not implemented until they rule.

### Plan revisions, round 3 (lead arbitrations — settled, not open)

**S1 — slot-id namespace mismatch REJECTS.** Lead ruling, superseding the earlier warn-and-report
disposition and closing code-reviewer's blocking finding. A declared audio slot whose id does not
match the planned egress slot id rejects the **whole declaration**, through the same machinery as
the client-malformation tokens but with **its own outcome token**, because it is not a
malformation — it is a conformant declaration MC cannot serve yet. `SLOT_STATE_SOURCE_UNREACHABLE`
was considered and rejected by the lead: that state is defined in §9 visibility-graph terms, and
bending a published wire enum to cover an MC-internal provisional constant is semantic drift on a
contract other services and stories read. The code comment says plainly that MC is declining a
declaration the wire contract permits, because task 13's join-time push fixed the egress slot id
before the client could declare, and that the rejection is retired by the capability-triggered
re-push story rather than weakened here.

**S2 — `MEDIA_KIND_UNSPECIFIED` REJECTS.** Lead arbitration; observability reversed their Gate 1
ratification and supplied the decisive argument, which is stronger than the triage one both
operations and I were using. The proto documents this zero at three sites (`MediaKind`,
`MediaStream` tags 2/3, `StreamAssignment` tag 3) as what an **old peer's `AUDIO = 0` decodes to** —
so the likeliest producer is not a client that forgot a field, it is a **version-skewed client that
meant audio**. Under the unfulfillable reading, a client-fleet rollback presents fleet-wide as
"everyone's meetings are empty" with no counter naming version skew — the same hazard
`label-taxonomy.md` §Frame reject reason guards when it requires `unknown_version` to stay
individually visible. My fail-closed argument (explicit `MediaKind::Audio` equality) was true but
aimed at the wrong risk: it is fail-closed against **misrouting** and silent against **skew**, and
this design needs both. §6's "an unsatisfiable request cannot be expressed" does not reach it —
`UNSPECIFIED` is the *absence* of a constraint, not a constraint that cannot be met, so there is no
set to take a subset of. The file is uniform on zero values (`TransportMode` "reject rather than
guess"; `Codec` "zero is not a silent Opus", enforcement owner *story task 14*) and `MediaKind`
would have been the lone exception.
Token: `media_kind_unspecified`, in the client-malformation family. Whole-declaration rejection,
consistent with `ReceiveSlot`'s own rule — a stale peer's entire declaration is suspect, not one
slot. Catalog remedy names the version-skew reading first, since it is both likelier and higher
blast radius than a forgotten field. The valid-but-unsatisfiable test narrows to **only** the video
slot, which is the case it actually pins; keeping unspecified in it would assert the reversed
behaviour.

**S3 — capability-triggered emission is the contract, not an accident.** Lead confirmed the ordering
security asked about: a client that joins and never declares is never told to send. Stated
explicitly in the module doc as a causal chain, and recorded in `docs/TODO.md` §Media Path
Obligations so the client-side tasks know a capability declaration is the **precondition for
publishing**, not an independent step.

**S4 — infra rows: owner is infrastructure.** Lead accepted security's F2 upgrade and set the owner:
all three `infra/services/mc-service/*.yaml` rows are `Not mine, Minor-judgment`, owner
**infrastructure** (correcting my round-2 split that put `configmap.yaml` on operations). §6.3
requires their confirmation at Gate 1 and again at Gate 3.

**S5 — the TODO entry now carries three halves.** One `docs/TODO.md` §Media Path Obligations entry,
pointing at both sides of each: (i) the declared audio slot id must equal the MH egress stream id
until re-push lands, (ii) S1's rejection is what enforces it in the meantime, and (iii) a capability
declaration is the precondition for publishing.

**S6 — the accept/reject matrix, pinned on both sides** (test's condition C, with dry-reviewer's
zero-audio exemption):

| Declaration | Disposition | Wire result |
|---|---|---|
| `{0: audio}` | ACCEPT | slot 0 `ACTIVE` (`SOURCE_MUTED` across the mute cycle) |
| `{0: audio, 1: audio}` | ACCEPT | slot 0 `ACTIVE`; slot 1 `FEWER_SOURCES_THAN_SLOTS`, `sender_id` absent, empty url — proves the S1 rejection did not swallow the genuine source-shortage meaning |
| `{0: audio, 7: audio}` | ACCEPT | slot 0 `ACTIVE`; slot 7 `FEWER_SOURCES_THAN_SLOTS` **carrying `slot_id == 7`** — proves MC echoes the client's numbering rather than emitting its own 0 |
| `{7: audio}` | **REJECT** (S1) | `ErrorMessage`; no directive, no assignments |
| `{}` | ACCEPT | `SendDirective` emitted; **empty** `StreamAssignments`; `mc_media_unmatched_plan_slots_total` increments |
| `{1: video}` | ACCEPT | slot 1 `FEWER_SOURCES_THAN_SLOTS` — a declared video slot with no video source is a genuine shortage |
| any slot `MEDIA_KIND_UNSPECIFIED` | **REJECT** (S2) | `ErrorMessage`; token `media_kind_unspecified` |

The `{0,1}`, `{0,7}`, `{}` and `{1: video}` accepts are what stop the rejection predicate being
defined too widely — `{7}` alone does not distinguish "reject unroutable numbering" from "reject
anything that is not exactly `{0}`".

**S7 — `SLOT_STATE_ZERO_REQUESTED` stays unreachable, and I am not fabricating a carrier for it.**
Test's matrix asks `{}` to produce `ZERO_REQUESTED`. It cannot honestly: that state rides on a
`StreamAssignment`, whose `slot_id` the proto defines as echoing "the `ReceiveSlot.slot_id` the
subscriber chose" — and a subscriber that declared nothing chose nothing, so there is no slot to
carry it. Emitting the *planned* id 0 instead would break the echo property on a published contract
to satisfy a test. The `{}` accept is therefore asserted as an **acceptance** (no `ErrorMessage`,
`accepted` increments, directive emitted, assignments empty), which is the property that actually
matters — the suite still fails against an implementation that wrongly rejects the empty
declaration. Whether `ZERO_REQUESTED` is emittable **at all** is now a flagged
protocol question rather than a settled "reachable with video": the state means "requested zero of
this kind", but it rides a message whose `slot_id` echoes a **declared** slot — and a subscriber who
declared a slot of that kind did not request zero of it. It may be unemittable in principle. That is
`protocol`'s question, not task 14's, it changes nothing here (unemitted either way), and it is
recorded in `docs/TODO.md` as a question so story 2 does not inherit "reachable" as established
fact.

### Plan revisions, round 4 (infrastructure Gate 1 + observability O-1 correction)

**I1 — the new keys are REQUIRED (`ConfigError::MissingEnvVar`); the default constants are deleted.**
Infrastructure verified that `orphan_configmap_key` fires only when **no** workload references a
key, so one deployment is enough to satisfy it; per-workload enforcement lives only in
`missing_in_manifest`, whose `required` set is built by regex-matching the literal
`MissingEnvVar("KEY")` in `config.rs`. So a Rust-defaulted key wired into `mc-0` and forgotten on
`mc-1` **passes the guard green**, and the two MC instances then direct different encoder parameters
to their respective clients with nothing failing anywhere. A fail-loud *parse* does not help — the
value parses fine on both pods; the fault is that one pod never sees the key. Precedent is one
commit old in this same story (`511e14fd`, MH's five §1 transport keys). This also discharges
CLAUDE.md's SSoT rule for free: making them required **is** the drift guard. R-32's "documented safe
default" is satisfied the way MH does it — the default lives in the ConfigMap comment, which is the
artifact an operator actually reads, not in a Rust constant nobody greps. Follow-on: `Config`'s test
fixtures must supply all of them (in-crate, mechanical).

**I2 — fifth key added to the config table.** `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` (R4(c)) was
in the design text but not the config table or the manifest list. It joins both, with its own
startup-validated upper bound by R6's own argument that a budget whose value is unbounded is not a
budget.

**I3 — ROUTED, not resolved here.** `MC_AUDIO_MAX_BITRATE_BPS = 32000` is numerically equal and
semantically opposite to MH's `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS = 32_000`: mine is a maximum, MH's
is a minimum, and MH sizes `NOMINAL_AUDIO_FRAME_BYTES` at that floor behind a `const _: () =
assert!` whose message records that sizing at the ceiling was proposed, ruled for and reversed by
three readers. A VBR encoder ceilinged at 32 kbps emits below it much of the time, so MH's own rule
("if the SDK's bitrate floor ever drops below this value, this constant must drop with it") is
engaged. Bounded blast radius — `NOMINAL_AUDIO_FRAME_BYTES = 236` is mostly fixed header and crypto
overhead, so the 640 ms budget loosens toward ~960 ms rather than becoming unbounded — but it stops
being the number MH's ConfigMap tells an operator it is. Narrowing infrastructure's three options:
**(b) is not expressible** — `EncodingParameters` carries `max_bitrate_bps` and no minimum, so a
directive cannot carry a floor. That leaves (a) MC directs 48000, matching the range MH already
covers and task 19 is specified against, with the *floor* remaining the client encoder's obligation;
or (c) MH's floor moves. My recommendation is (a). Routed to `main` and `media-handler`; whichever
lands, the ConfigMap comment states it.

**I4 — ConfigMap comment discipline follows `infra/services/mh-service/configmap.yaml`**, the
convention set one commit ago in the sibling service in this story: every key carries the quantity
it bounds, why *this* value, an explicit `Enforced by:` (which process, at which point) and
`Where an operator sees it:` (metric/alert/runbook, or a frank "nothing today"). R6's `1..=64`
self-ceiling is recorded **in the ConfigMap**, because the operator who would set it to 100000 is
reading that file and not `config.rs`. `MC_AUDIO_FRAME_RATE_HZ`'s comment states outright that 50 Hz
**is** MH's `AUDIO_FRAME_DURATION_MS = 20` in the reciprocal unit and names what breaks. The bitrate
anchor is written as an **inequality**, not a lock, honouring MH's own caveat.

**I5 — placement and lockstep.** One banner block at the end of `configmap.yaml` after the OTel
block; in both deployments one contiguous block after `DEPLOYMENT_ENVIRONMENT` and before the
`POD_IP` downward-API section (that region is derived/per-instance and these keys are neither); same
key order in all three files; the existing lockstep comment extended to cover the new block. Keys go
in the shared `mc-service-config`, never in `mc-{0,1}-config`. No `envFrom` (it would trip
`unsupported_env_source` and make the guard decline coverage service-wide). Acceptance criterion,
which I will run myself before handing over:
`diff <(sed 's/mc-0/mcX/g' mc-0-deployment.yaml) <(sed 's/mc-1/mcX/g' mc-1-deployment.yaml)` must
still emit exactly the one lockstep-comment hunk — "both files contain the keys" is not sufficient.

**I6 — `configmap.yaml` Owner corrected to `infrastructure, operations`.** Classification stays
Minor-judgment. `Approved-Cross-Boundary: infrastructure — <reason>` trailer on the commit for the
three manifest files.

**O-1 correction (observability, after operations caught it) — cite the precedent's SHAPE only.**
`mc_media_policy_pushes_total`'s `outcome!~"match|handler_id_mismatch"` and this counter's
`outcome!~"emitted|emitted_empty_targets"` look identical and have **opposite lifetimes**: that one
is a temporary workaround for a defect with a recorded revert trigger; this one is a permanent,
correct classification of a legal state that gets *more* common as §7 lands. Writing "one decision
with one revert trigger" would make this catalog entry a fourth site under that spin-out's cleanup
instruction, and whoever performs it would strip both alternations and start paging on a routine
success while believing they completed a documented checklist — a failure arriving disguised as
housekeeping. The entry therefore says **"Permanent — no revert trigger"**, cites ADR-0036 §5 as the
authority, states the site count minus the revert trigger, and carries a **self-contained**
disarming line that names neither `mc_media_policy_pushes_total` nor the spin-out slug (a
comparative note dies with its comparand). Also confirmed with operations: the MC-internal-defect
tokens get the "ANY non-zero indicates a bug" marker and deliberately **no** alert in task 21 —
they are file-and-fix diagnostics, because if they fire the client-facing counters show the damage
first.

### Plan revisions, round 5 (security F6 — the R4(c)/S1 interaction)

**F6 — accepted, taking the preferred structural fix.** R4(c) charges its budget on *accepted*
declarations, but S1 (added a round later) is a rejection that, as planned, needed MC's own planned
egress slot id and therefore ran *after* the `get_state()` roster clone. `{7: audio}` in a loop
passes every cheap check, costs an O(N) clone plus a `compute_assignment` on the shared meeting
actor, and is then rejected — never charging the budget. R4(b)'s short-circuit does not help either,
because it compares against the last **accepted** declaration and `{7}` is never accepted. That
reinstates F1 in full through a ~6-byte message.

**Fix**: the subscriber's planned audio slot id is resolved **once per connection at join**,
alongside the handler-url map, and cached. It is connection-stable in this story for the same reason
S1 exists at all — task 13 pushes policy once at join and this task excludes re-push — so the value
cannot change mid-connection. To keep it derived rather than re-derived, `compute_assignment` now
runs at join for **every** connection (it is pure, no I/O, no actor hop; the MH push still happens
only for the first participant, unchanged), and the cached slot id is read out of that same
assignment output rather than reconstructed from `MAIN_AUDIO_SLOT_ID` independently.

This yields an invariant worth stating in the module doc and worth more than the accounting it
replaces: **every rejection disposition is decidable without touching the meeting actor.** All seven
rejection checks run against the message plus one cached `u16`; only *accepted* declarations reach
`get_state()`, and those are bounded at 64 per connection by R4(c). The budget then does the job it
was designed for instead of being bypassed.

The cache carries the same caveat comment S1 does: it is sound only because there is no re-push in
this story, and the capability-triggered re-push story retires the cache along with the rejection.

Traced and confirmed with security: `{}` is exempted from S1 by R11 and accepts, so it charges the
budget normally — no gap there.

**Round-1 field-validation rejection tests are retained, not displaced** (test's confirmation). The
S6 matrix is the accept/reject-by-slot-kind axis; the five field-validation cases sit alongside it
and each still asserts whole-declaration rejection with no directive, no assignments, an
`ErrorMessage`, and its frozen metric token: duplicate `slot_id`; slot count over the configured cap;
`slot_id = 65536`; `pinned_sender_id = Some(0)`; `pinned_sender_id = 65536`. With S2's
`media_kind_unspecified` and S1's namespace-mismatch token that is seven rejection paths, each with
its own test.

**`{}` → `ZERO_REQUESTED` pushback accepted by test**, on the correctness ground rather than as a
deferral: emitting it would require fabricating `slot_id = 0` in a field the proto defines as
echoing a slot the subscriber chose, which is the same wire-falsehood class as the masking removed
in S1.

**Lead ruling on I3 — `MC_AUDIO_MAX_BITRATE_BPS = 48000`.** Options (b) and (c) are outside this
devloop: (b) needs a floor field on `EncodingParameters`, a `proto/**` GSA change; (c) is
media-handler-owned and they are not on this panel. 48000 is the top of the 32-48 kbps band MH
already documents and task 19 specifies the SDK against, so MH's sizing and its `const assert!` stay
untouched. Two things land with it, because changing the digit removes the coincidence but not the
dependency: the ConfigMap comment names `mh-service/src/config.rs::SUPPORTED_AUDIO_BITRATE_FLOOR_BPS`
as a related-but-different constant that must **not** be unified with this one, and
`docs/TODO.md` §Media Path Obligations records the live coupling — MH sizes its datagram buffer at
the *SDK's* VBR floor, so if task 19's SDK floor lands below 32 kbps, MH's constant must move with
it. That obligation is media-handler's and client's, which is exactly why it needs a written home
rather than living in this devloop's transcript.

**Owner cell**: the three `infra/services/mc-service/*.yaml` rows carry `infrastructure` alone so the
classification parser reads one owner per row; operations is co-reviewer on the ConfigMap by prose,
per the Lead's table fix.

**Round 5 addendum (test) — F6's invariant becomes compiler-enforced, not wire-observed.** The
rejection-decision function's signature carries **only** the declaration plus the per-connection
cached planned slot id, and **no handle to the meeting actor or meeting state**. "A rejection cannot
touch the actor" is then a type property rather than something a wire test tries to observe, in the
same family as `StreamNumber::from_wire` and `build_send_directive`'s missing mute parameter. A
focused unit test over that function documents it; the wire assertions (rejected `{7}`: `accepted`
does not increment, no `StreamAssignments`) stay as the behavioural backstop. If the signature route
proves impossible I fall back to the wire assertion and say why at review.

**Constant-shape conflict resolved (infrastructure vs operations), verified against the commit.**
`511e14fd`'s own message reads: `MH_MAX_CONNECTIONS` "(`unwrap_or(DEFAULT_MAX_CONNECTIONS)` **and
the constant deleted**; relabelled an accept-time resource-exhaustion guard, not capacity)", and the
diff removes `pub const DEFAULT_MAX_CONNECTIONS`, the `unwrap_or`, and the fixture assertion against
it. The `DEFAULT_*` constants still in `mh-service/src/config.rs` belong to keys that remain
genuinely optional. So MC matches MH: **no `DEFAULT_*` constant for the five new keys, no
`unwrap_or`, `ConfigError::MissingEnvVar` with the literal repeated at each site** — repeated
deliberately, because `dt-guard env-config` discovers required vars by regex-matching that literal
and a helper would blind check 1 on all of them while it kept printing STATUS=OK. The documented
defaults and their why-that-number reasoning live in the ConfigMap comment, which is also where I4
puts the `1..=64` ceiling; operations' concern that the documented default must not vanish is
satisfied by that placement rather than by a compromise.

**Infrastructure's Gate 3 checklist, recorded verbatim so it survives the transcript.** Verified by
me before handing over, not after:
1. `diff <(sed 's/mc-0/mcX/g' mc-0-deployment.yaml) <(sed 's/mc-1/mcX/g' mc-1-deployment.yaml)`
   emits exactly the one lockstep-comment hunk.
2. All five keys present in `configmap.yaml` and both deployments, in the same order in all three.
3. **(amended by infrastructure after this note)** Five `MissingEnvVar("MC_…")` literals in
   `crates/mc-service/src/config.rs`; **no `DEFAULT_*` constant for any of the five anywhere in the
   file, test module included**; fixtures supply the five values as literals, never by reference to
   a constant. `dt-guard env-config` green — and the green run means something, because check 1 now
   covers these keys per-workload.
4. `MC_AUDIO_MAX_BITRATE_BPS = 48000` with the "related-but-different, do not unify" comment naming
   `mh-service/src/config.rs::SUPPORTED_AUDIO_BITRATE_FLOOR_BPS`.
5. `Enforced by:` and `Where an operator sees it:` on every new ConfigMap key.
6. No `envFrom` (it trips `unsupported_env_source` and makes the guard decline coverage
   service-wide).
7. The corrected §Operations posture sentence in place.

Note on (3): the deletion that matters most in `511e14fd` is the **fixture assertion**
`assert_eq!(config.max_connections, DEFAULT_MAX_CONNECTIONS)`, not the load-path `unwrap_or`. A test
asserting a default is exactly what keeps a constant alive as "still referenced" after the load path
stops using it, so a `DEFAULT_*` surviving only in a fixture looks retired to a grep of the load path
while remaining a live second encoding of the same value — and it is the easiest line to leave
behind. Item 3's original wording ("literals present, constants deleted") would have passed against
exactly that residue; infrastructure amended it above once the generalisation was clear. This is the
check MH's diff actually passes.

### Plan revisions, round 6 (operations close-out)

**Runbook rows removed from this diff.** Operations owns `docs/runbooks/**` and has already landed
the edits (four files, 325 insertions), so the row is gone from the table above and the §Operations
posture paragraph points at their work instead of promising mine. Nothing runbook-shaped ships here.

**`MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` stays a cap; the token bucket is not built.** Operations'
ruling, and the reasoning is worth keeping because it is the deferral criterion rather than a
preference: a bucket needs a refill clock, a burst parameter and its own tests, and it replaces one
value an operator can reason about ("64 declarations, then rejected") with two coupled values they
cannot. The cap's failure mode is legible and its blast radius is one connection. Revisit when a real
user re-lays-out a grid — story 4 or later; nothing in loopback re-declares. **The property that
makes deferring cheap is that a bucket swaps in without changing the call site, so it must be
preserved**: the budget check stays behind one function that takes the declaration and returns a
decision, never inlined into the dispatch arm.

**Constant-shape conflict fully closed.** Operations re-read `511e14fd` and withdrew their
instruction: no `DEFAULT_*` constants for the five new keys. All three of us now read the commit the
same way, on evidence rather than recollection.

**Do not write `deployment/mc-service` anywhere in this diff.** Operations found it 77 times across
four runbooks and it resolves to nothing — MC is `mc-0`/`mc-1`; `mc-service` is a Service, a PDB and
the container name. Two consequences reach my files: the health port is **8081**, not 8080 (29 of
those occurrences were `port-forward 8080:8080`, so every MC metrics-check step failed twice over),
which is the port any dashboard or catalog "how to scrape this" note must name; and `kubectl scale
--replicas=N` on MC is **actively harmful**, because mc-0/mc-1 are `replicas: 1` singletons whose
`MC_WEBTRANSPORT_ADVERTISE_ADDRESS` comes from a per-instance ConfigMap — extra replicas all
advertise one address and clients get routed to a pod that does not hold their session, an ADR-0023
binding failure presenting as intermittent join failures. Recorded here because task 21 keys its
triage off my outcome tokens and must not inherit the wrong topology.

### Plan revisions, round 7 (infrastructure I8)

**I8 — accepted.** `replicas: 1` in `mc-{0,1}-deployment.yaml` is a correctness constraint, not a
capacity choice, and nothing in those files says so. The `mc-service-0` NodePort
(`service.yaml:36-53`) selects `instance: mc-0`, so every scaled replica becomes an endpoint; all
replicas read `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` from the single `mc-0-config` and therefore
advertise an identical client URL, while `MC_ID` and `MC_GRPC_ADVERTISE_ADDRESS` are per-pod. GC
gets N registrations advertising one client-facing URL and the NodePort picks an endpoint
independently of GC's assignment — a 4-in-5 miss at `--replicas=5`, presenting as intermittent join
failures, whose natural next move is to scale further.

The constraint exists in `service.yaml:27-30`, which is not the file anyone is reading when they
type `kubectl scale deployment/mc-0` — SSoT-adjacency: the knowledge is one file away from where it
is needed. Fix is a two-line comment above `replicas: 1` in both deployment files, identical wording
so it survives Gate 3 item 1's byte-identity check, pointing at `service.yaml`'s per-instance
NodePort and per-instance ConfigMap as the reason.

Taken rather than deferred or routed: both files are already in this changeset (so infrastructure
would collide with my edits), it is ~4 lines with no design ambiguity, and the protocol's
suspicious-deferral check makes deferring the wrong call at that size. It is genuinely adjacent
rather than in-task, and the adjacency is real — the operator reading the new ConfigMap block during
a CrashLoop is the same one who reaches for `scale`. Joins Gate 3 as item 8.

### Final state, as shipped (the revision rounds above are the trail, not the answer)

Five rounds of reviewer findings moved several names; this is what actually landed, so a reader does
not have to reconstruct it.

**Metrics** (six new). The cardinality column names the artifact that FIXES the
bound rather than restating its length as an integer — that is the catalog's
discipline (`docs/observability/metrics/mc-service.md` declines to restate it for
exactly this reason) and it is not pedantry here: the first version of this table
said "four new" over six rows and gave the directive counter `(7)` after
`no_planned_egress_slot` was added during review, so the section the §Planning
superseded marker points readers TO was itself wrong, in the column someone sizing
a query reads. Raised as O-17 by observability and OPS-4 by operations, independently.

| Metric | Labels | Cardinality bounded by |
|---|---|---|
| `mc_media_receive_capability_declarations_total` | `outcome`, `key_custody` | `CapabilityOutcome::ALL` |
| `mc_media_send_directives_total` | `outcome`, `key_custody` | `DirectiveOutcome::ALL` |
| `mc_media_slot_states_total` | `slot_state` (exhaustive over the wire enum), `key_custody` | the proto `SlotState` enum |
| `mc_media_unmatched_plan_slots_total` | `key_custody` | — (no variable label) |
| `mc_media_mute_requests_total` | `outcome`, `key_custody` | `MuteOutcome::ALL` |
| `mc_participant_outbound_messages_dropped_total` | `payload_kind`, no `key_custody` | its call sites (not an exhaustive enum) |

`mc_media_mute_requests_total` was added **during review**, not planning — see §Review fixes S-1
below. Its vocabulary is `MuteOutcome` in `media_signaling::outcome`, deliberately separate from
`CapabilityOutcome` whose documented partition is over *declarations*: folding mute dispositions in
would have falsified that partition claim and silently changed what every query over it counts.

`mc_media_slot_outcomes_total` — my round-2 proposal mixing a server-side condition into a wire-state
label — **does not exist**; observability's O-2 split it into the last two rows above, which is why
`mc_media_slot_states_total` can claim that metric and wire cannot disagree.

**Capability outcome vocabulary** (10): `accepted`, `accepted_unchanged`, `duplicate_slot_id`,
`slot_count_over_cap`, `slot_id_out_of_range`, `pinned_sender_id_zero`,
`pinned_sender_id_out_of_range`, `media_kind_unspecified`, `slot_id_not_planned`,
`declaration_budget_exhausted`. Success set is `{accepted, accepted_unchanged}`; **`accepted` alone
is the ratio denominator**, because `accepted_unchanged` is client-inflatable.

**Both late tokens are now ratified by observability**: `declaration_budget_exhausted` (security's
F1(c) budget) and `accepted_unchanged` (security's S2). Every spelling on these metrics came from the
vocabulary owner.

**Module layout**: `capability.rs`, `directive.rs`, `assignments.rs`, `outcome.rs`, `mod.rs`.
`policy.rs` from the round-1 plan does not exist (dry-reviewer's R3: "policy" already means the
MC→MH forwarding assignment in the sibling module); `outcome.rs` was added during implementation to
give the three label vocabularies one home.

### Guard/lint expectations
`cargo fmt`, clippy `-D warnings`, no `unwrap`/`expect` in the new non-test code, and **no numeric
`as` cast anywhere in the diff** — the five widening casts the first
pass carried (`unmatched_plan_slots as u64`, and four `AUDIO_* as usize` bound arguments) are gone:
the metrics facade now owns the width conversion, and a `bounded_u32` sibling to `bounded_usize`
bounds the `u32`-domain knobs in their own domain, which also deleted three unreachable
"does not fit u32" error arms. Corrected rather than weakened, because this line is what a future
reader checks the code against.

The claim is stated over *numeric* casts because that is what it actually is, and the earlier
wording — "no `as` except the idiomatic proto-enum `as i32` at wire-encode sites" — was a flat
"no `as` except X" with a Y in the diff. Enumerated over `git diff HEAD -- '*.rs'`, the added lines
contain **exactly two** `as` classes and nothing else: 43 proto-enum `as i32` at wire-encode and
assertion sites, and 2 `as Arc<dyn Trait>` **unsized trait-object coercions** in the new integration
test's stack bring-up, which follow the identical line already present in six sibling test files
(`media_admission_integration.rs:56`, `media_connection_update_integration.rs:41`,
`webtransport_accept_loop_integration.rs:64`, `otel_webtransport_integration.rs:48`, and
`main.rs:462` in production). A trait-object coercion is not a cast in the sense the `clippy::cast_*`
family or this line's widening-truncation concern is about, and the code was right; the sentence was
the defect, so the repair narrows the claim rather than touching the code (§Lessons Learned 1).
Found by re-deriving the claim at the second resume instead of inheriting it.

No `debug_assert!` used for input validation, `metric-coverage` / `metric_no_catalog` /
`metric_no_dashboard` / `target_query_fields` satisfied by the companions above, `dt-guard
env-config` satisfied by the ConfigMap+both-deployment rows.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Runbook scope, split by cause

The runbook changes in this commit are **operations' work in operations' own domain**, and they
divide into two kinds. Stating the split so the larger number does not ride on the smaller
justification:

**Caused by this diff** — the §Config-failure triage section in `docs/runbooks/mc-deployment.md` and
the five new required-key rows. Infrastructure's I1 makes the five `MC_*` keys required
(`ConfigError::MissingEnvVar`, no Rust default), which converts a benign misconfiguration into a
two-pod startup failure on the signalling plane. That runbook is therefore this diff's triage
surface, and shipping the keys against the table as it stood — `Required` effectively inverted, six
phantom variables, all ten genuinely-CrashLooping keys absent — would have been shipping a known
trap.

**Pre-existing, merely surfaced here** — the inverted `Required` column, the wrong Secret block for
TLS, and the 77 `deployment/mc-service` occurrences across four runbooks (29 of them also on the
wrong health port, 8 of them `kubectl scale` commands that *succeed* and then break ADR-0023 session
binding). This damage predates task 14 and none of it is task scope. It was found because the
required-key work sent operations into those files, not because this task needed it.

The Lead allowed both in one commit rather than forcing a second devloop: it is docs-only, squarely
in operations' domain, and a separate loop for documentation this diff made load-bearing costs more
than it protects. The commit message states the same split and does not imply the sweep was task
scope.

### Priority 1 — client-facing media signalling (the task)
| Item | Before | After |
|------|--------|-------|
| `webtransport/connection.rs` post-join dispatch | `MediaConnectionUpdate` only; everything else logged "unhandled" | adds `ReceiveCapability` and `MuteRequest` arms |
| Send directive | did not exist | `SendDirective` composed from `compute_assignment` output; derived header version; datagram target set; no priority group |
| Slot assignment | did not exist | `StreamAssignments` joining declared slots against computed plans, every partial case explicit |
| Client mute | unhandled post-join | recorded via the meeting actor; directive structurally untouchable from the mute path |

### Priority 2 — supporting changes
| Item | Before | After |
|------|--------|-------|
| `MediaSignalingContext` | did not exist | per-connection; planned slot id + handler urls resolved once at join, so **every rejection is decidable without touching the meeting actor** |
| `ControllerMessage::GetMeetingHandle` | only `GetMeeting` (a state snapshot) | returns the live handle, taken once per connection, so post-join dispatch does not serialize on the controller mailbox |
| `MeetingActor::handle_self_mute` | broadcast on every report | idempotent — an unchanged pair returns without broadcasting (a `MuteChanged` for a transition that did not occur is a lie on the wire, and it was the cheapest client-driven O(N) fan-out) |
| Participant outbound `try_send` failure | WARN only | WARN + `mc_participant_outbound_messages_dropped_total{payload_kind}` |
| Config | 15 keys | +5 REQUIRED `MC_*` keys, bounded on both sides at load, no Rust defaults |

### Additional Changes
- **Five new required config keys** wired into `configmap.yaml` and *both* deployments, with the `mh-service` comment discipline (`Enforced by:` / `Where an operator sees it:`) and an explicit rollback-ordering banner.
- **I8 (infrastructure)**: a comment above `replicas: 1` in both deployment files stating that 1 is a *correctness* constraint, with the 4-in-5 arithmetic. The constraint existed only in `service.yaml`, which is not the file open in front of someone typing `kubectl scale`.
- **Six new metrics + catalog entries + dashboard panels** (one new dashboard row).
- **Four `docs/TODO.md` entries** under §Media Path Obligations.

### Two defects found by running a guard's own logic rather than reading the code

Both were in my code, both would have shipped green, and neither is caught by `cargo check`:

1. **A wrapped `MissingEnvVar` literal silently un-declares a required key.** `dt-guard env-config` discovers required variables with the regex `MissingEnvVar\("([A-Z_][A-Z0-9_]*)"`, which does not span lines. My first pass had rustfmt-style wrapping between `MissingEnvVar(` and the string for `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`, so the guard **missed that key entirely** while reporting `STATUS=OK` — exactly the blinding failure infrastructure's I1 warned about, arriving through formatting rather than through a helper. Found by running the guard's regex over `config.rs` and diffing the result against the five keys I expected.
2. **An example of the pattern in a doc comment registers a phantom required variable.** My rustdoc explaining the rule contained `MissingEnvVar("KEY")`, which the same regex dutifully discovered as a required env var named `KEY` that no ConfigMap declares.

Both are now stated in the helper's rustdoc as the two ways to break discovery, because neither is visible by reading the file.

### One assertion that would have passed for the wrong reason

The redirect test's premise assertion (*"the attacker url actually reached the client-supplied map"*) initially ran immediately after the write and **failed**, because a `MediaConnectionUpdate` produces no reply and the assertion raced the bridge loop. Had the race gone the other way it would have passed while proving nothing. Fixed with the sibling test's established settle; recorded because a premise assertion that can pass vacuously is worse than no premise assertion — it converts an unproven control into an apparently-proven one.

### Review fixes (Gate 3), by finding

Recorded as a round rather than folded into the sections above, so the trail from finding to change
stays legible — same convention as the planning revisions.

> **SUPERSEDED IN TWO PLACES by later findings in this same Gate 3 — see the ledger below.**
> **(a)** The O(N) `broadcast_update` this bound was sized against **was removed by S-2**
> (`MuteChanged` has no consumer: `encode_participant_update` returns `None` and the roster
> `Participant` proto carries no mute field, so the fan-out delivered zero bytes to zero clients).
> The surviving cost is one `GetState` roster snapshot per composition on the shared meeting actor's
> mailbox, plus the per-connection composition. The bound is still required; it is **not** a
> meeting-wide contention control, and `rate_limited` must not be read as one.
> **(b)** **`MuteWorkLimiter` DOES NOT EXIST.** S-3 generalised it to `ClientWorkLimiter`
> (burst and refill interval per instance, two instances: mute, and the capability-rejection reply).
> `grep -rn "MuteWorkLimiter" crates/` returns zero; this paragraph was the last place in the tree
> naming the dead type.
>
> The text below is left intact deliberately — it is *why* S-2 and S-3 happened, and rewriting it
> would erase the trail. The marker is at the top rather than the bottom for the reason §Lessons
> Learned 5 gives: this is the copy a reader reaches first, and the corrections sit ~200 lines below
> with nothing linking them. That is the second occurrence of this exact mechanism in this one
> document — the first was `mc_media_slot_outcomes_total` in §Planning, from which operations copied
> a dead symbol into an incident runbook and was one cross-check from shipping it. Raised as O-28 by
> observability.

**S-1 (security) + O-6 (observability) — client mute was an unbounded amplifier against the SHARED
meeting actor, and this diff created it.** Verified against base `1d755455`: `handle_client_message`
had no `MuteRequest` arm, and `MeetingActorHandle::update_self_mute` has exactly one non-test caller
in the tree, which is the new line in `connection.rs`. So this diff makes `MeetingActor::broadcast_update`
— an O(N) fan-out **awaited per participant on the meeting actor's own task**, head-of-line-blocking
joins, leaves and every other connection's `get_state()` — client-reachable for the first time. Both
existing short-circuits are *equality* checks, so an alternating toggle defeats both. Three fixes:

  - **A per-connection token bucket** (`MuteWorkLimiter`, burst 8, sustained 4/s) gating the actor
    report **and** the recomposition. Security's option (i) bounded only the recomposition; that
    leaves the meeting-wide half unbounded, which is the same asymmetry the finding raised, one level
    down. Security's option (ii) — a shared cumulative budget — was **rejected and the rejection
    conceded**: a budget on the mute *report* permanently freezes that participant's
    `audio_self_muted`, so every other client renders a live speaker as muted for the rest of the
    session. That is a correctness failure strictly worse than the amplification it prevents.
    **The criterion, now in the `media_signaling` module doc**: a cumulative budget bounds total work
    and permanently denies, and is right for a *rare* action; a rate limit bounds work per unit time
    and never permanently denies, and is required for a *repeatable steady-state user action*.
    Tokens are spent **after** the no-op short-circuit, so a client repeating one state cannot drain
    its own bucket and suppress its next genuine toggle.
  - **The audio-only recomposition predicate.** `last_reported_mute` is the `(audio, video)` pair —
    the right dedupe key for "report this to the actor", the wrong one for "recompose", because
    `SourceMuteView` reads `audio_self_muted` alone. A video-only toggle previously ran the full
    O(N) composition to emit a **byte-identical** `StreamAssignments` and moved
    `mc_media_slot_states_total` with provably zero information delivered. Not an attack: it fires
    the first time a client wires a camera button.
  - **`mc_media_mute_requests_total{outcome}`** over a bounded `MuteOutcome`, so every disposition is
    countable, with catalog entry and dashboard panel.

  **The residual, stated rather than buried**: a client that hammers past the limiter and stops on a
  suppressed edge leaves MC's roster holding a stale self-reported mute flag. Safe because ADR-0036
  §5 enforces client mute **at capture on the client** — the audio genuinely stops and only the
  indicator others see is stale, so there is no window in which someone believes they have stopped
  transmitting and has not. That reasoning holds for a merely *buggy* client (a repeating button, a
  reactive loop), which the alternative argument — "the client already controls the value" — does
  not; security supplied the better one and it is what the code comment says.

**Test F1/F2/F3** — the two fail-closed guards *inside* `build_send_directive` had no test (the
existing ones covered their neighbours, `policy.lookup` and `StreamNumber::from_wire`), so an
unspecified transport mode and an unknown stream number are now driven through the build path with a
hand-built `MeetingAssignment`, plus a positive control so a `build_send_directive` that rejected
every hand-built assignment could not pass both vacuously. `DirectiveOutcome` gained the
vocabulary-distinctness test its two siblings already had — it was the vocabulary most exposed to the
`ALL`-drift gap `outcome.rs`'s own module doc describes — along with a cross-vocabulary
no-shared-spellings test. The idempotent `handle_self_mute` is now pinned at the actor level;
`MuteChanged` is not wire-serialized, so the assertion is on delivered actor messages, and deleting
the guard makes the no-op cost 2 instead of 1 and fails.

**O-1..O-8 (observability)** — `no_planned_egress_slot` was missing from the catalog and uncoloured on
the dashboard, and it is the value the code itself documents as most severe (it silences a client for
its whole session). The catalog's environmental list said "two" where four is right, the cardinality
restated an integer that had already rotted, and the recording-site line named only
`compose_and_emit` when the severe value is join-time only. Panel 58 still carried the falsified
`outcome!="accepted"` predicate. **The meeting-id-in-logs invariant was stated more strongly than the
code keeps it** — fixed in the docs, not the logs: §11's flat prohibition is scoped to metrics, and
`meeting_id` is what makes the context-unavailable WARN actionable. The outbound-drop WARN became
one-shot per actor with the counter still unconditional, which was only affordable *because* this
diff added the counter. And the §Planning observability block now carries a superseded marker **at
the top**, naming the dead metric by name, because operations had already copied a label key out of
it into an incident runbook.

**code-reviewer F1/F2, dry D1/D2, infrastructure I-A/I-B** — the five widening `as` casts were removed
rather than the claim weakened (a `bounded_u32` sibling bounds the `u32`-domain knobs in their own
domain, deleting three unreachable "does not fit u32" arms as a side effect); the meeting-wide
assumption behind the cached handler list is now documented with its retirement condition; the second
inline `MeetingRoutingInput` constructor was extracted into `routing_input_for`, with the
empty-handlers guard moved *inside* it so the rule cannot hold at one call site and not the other;
the OTel fixture now sources the media triple from the shared fixture whose rustdoc carries the
matches-the-ConfigMap claim; the ConfigMap's frame-rate magnitude paragraph was wrong in two opposite
directions across two revisions and now states the fixed-overhead reason; and the `AudioEncoding::new`
config failure names all three `MC_AUDIO_*` inputs, asserted by message content rather than by
`InvalidValue(_)`, which any message would have satisfied.

---

## Files Modified

```
 crates/common/src/observability/testing.rs         |   53 +
 crates/dt-guard/src/kustomize.rs                   |  159 +-
 crates/mc-service/src/actors/controller.rs         |   37 +
 crates/mc-service/src/actors/meeting.rs            |  200 ++-
 crates/mc-service/src/actors/messages.rs           |   16 +
 crates/mc-service/src/actors/participant.rs        |  157 +-
 crates/mc-service/src/config.rs                    |  452 ++++-
 crates/mc-service/src/grpc/gc_client.rs            |   16 +
 crates/mc-service/src/lib.rs                       |    1 +
 crates/mc-service/src/main.rs                      |   41 +
 .../mc-service/src/media_signaling/assignments.rs  |  423 +++++
 .../mc-service/src/media_signaling/capability.rs   |  554 ++++++
 crates/mc-service/src/media_signaling/directive.rs |  827 +++++++++
 crates/mc-service/src/media_signaling/mod.rs       |  113 ++
 crates/mc-service/src/media_signaling/outcome.rs   |  452 +++++
 crates/mc-service/src/observability/metrics.rs     |  252 +++
 crates/mc-service/src/webtransport/connection.rs   | 1279 ++++++++++++-
 crates/mc-service/src/webtransport/server.rs       |    6 +
 crates/mc-service/tests/common/accept_loop_rig.rs  |   31 +
 crates/mc-service/tests/common/mod.rs              |   25 +
 crates/mc-service/tests/gc_integration.rs          |   12 +
 .../tests/media_client_signaling_integration.rs    | 1178 ++++++++++++
 .../tests/otel_grpc_outbound_integration.rs        |    9 +
 docs/TODO.md                                       |  157 +-
 .../2026-09-03-mc-client-media-signaling/main.md   | 1884 ++++++++++++++++++++
 docs/observability/alert-conventions.md            |    2 +-
 docs/observability/alerts.md                       |    4 +-
 docs/observability/metrics/mc-service.md           |  156 ++
 docs/runbooks/gc-incident-response.md              |  117 +-
 docs/runbooks/mc-deployment.md                     |  541 +++++-
 docs/runbooks/mc-incident-response.md              |  655 ++++++-
 docs/runbooks/mh-incident-response.md              |   32 +-
 .../meeting-controller/INDEX.md                    |   16 +-
 docs/specialist-knowledge/observability/INDEX.md   |    6 +-
 infra/docker/prometheus/rules/mc-alerts.yaml       |    2 +-
 infra/grafana/dashboards/mc-overview.json          |  823 ++++++++-
 infra/services/mc-service/configmap.yaml           |  138 ++
 infra/services/mc-service/kustomization.yaml       |   23 +
 infra/services/mc-service/mc-0-deployment.yaml     |   49 +
 infra/services/mc-service/mc-1-deployment.yaml     |   49 +
 infra/services/mh-service/kustomization.yaml       |   27 +
 41 files changed, 10694 insertions(+), 280 deletions(-)
```

> **Regenerated from `git diff HEAD --stat`, not hand-edited.** The previous copy of
> this block was a pre-review snapshot claiming 26 files / 2 608 insertions against an
> actual 37 / 10 436, and it omitted `crates/dt-guard/src/kustomize.rs` and both
> `kustomization.yaml` files. Raised as part of O-17. This block is a place where the
> output IS the claim, so it is enumerated rather than adjusted — the same rule that
> §Lessons Learned 1 states about counts taken from a filter.


### Key Changes by File
| File | Changes |
|------|---------|
| `media_signaling/capability.rs` | Parse-don't-validate for `ReceiveCapability`; `SlotId`/`PinnedSenderId` newtypes; the seven whole-declaration rejections; `PlannedAudioSlot` cache |
| `media_signaling/directive.rs` | `build_send_directive` (**no mute parameter, no import through which mute could arrive**); `StreamNumber::from_wire` fail-closed seam; `AudioEncoding` (makes `CODEC_UNSPECIFIED` unrepresentable); `MediaStreamPolicy`; `HandlerUrls`; derived `header_version` |
| `media_signaling/assignments.rs` | The declared-vs-planned join with all four partial cases explicit; `SourceMuteView` (a projection, not a second home for mute); exhaustive `slot_state_label` |
| `media_signaling/outcome.rs` | The three bounded telemetry vocabularies, each with `ALL` + `label()` |
| `webtransport/connection.rs` | Two new dispatch arms; `MediaSignalingContext`; `compose_and_emit` from one meeting-state snapshot; `handle_client_message_without_media` for the degraded path |
| `actors/controller.rs`, `actors/messages.rs` | `GetMeetingHandle` |
| `actors/meeting.rs` | Idempotent `handle_self_mute` |
| `actors/participant.rs` | Outbound-drop counter at both `try_send` sites |
| `config.rs` | Five required keys, `bounded_usize`, `client_media_config()` |
| `observability/metrics.rs` | Six recording functions |
| `infra/services/mc-service/*.yaml` | Five keys in ConfigMap + both deployments; I8 `replicas: 1` comment |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Output**: `cargo check -p mc-service --all-targets` clean.

### Layer 2: cargo fmt
**Status**: PASS
**Output**: `cargo fmt --all --check` clean. Verified after formatting that rustfmt did **not** reflow the one-line `MissingEnvVar` literals — all five new keys are still discovered by `dt-guard env-config`'s regex, which does not span lines.

### Layer 3: Simple Guards
**Status**: ALL PASS (full sweep of `scripts/guards/simple/*.sh`)

| Guard | Status |
|-------|--------|
| env-config | PASS — `env-config-clean-4-services-6-workloads`; all five new `MC_*` keys discovered as required and present in both deployments |
| cross-boundary-classification | PASS — `cross-boundary-classification-clean-21-files` |
| cross-boundary-scope | PASS — `cross-boundary-scope-no-drift` (initially FAILed with 4 `scope_drift_inbound`; see below) |
| knowledge-index | PASS — `knowledge-index-clean-15-files` (initially FAILed at 82/75 lines; see below) |
| metric-coverage | PASS — `metric-coverage-all-covered` (initially FAILed on the new drop metric; see below) |
| dashboard-panels | PASS — `dashboard-panels-clean-14-files` |
| application-metrics | PASS — `application-metrics-clean` |
| doc-citations-no-line-numbers | PASS (after operations fixed two bare-line cites in their runbook edits — routed, not edited by me) |
| all others | PASS |

**Three of my own guard failures, and what each caught:**
- `cross-boundary-scope` flagged four files touched but unplanned — `gc_client.rs`, `accept_loop_rig.rs`, `gc_integration.rs`, `otel_grpc_outbound_integration.rs`, all `Config`-literal fixture fallout from the five new required fields. Rows added. This is exactly the reconciliation operations warned had to happen against `git diff --name-only` rather than at planning time.
- `knowledge-index` flagged `INDEX.md` at 82 lines against a max of 75 — it was **already at the ceiling** and my additions pushed it over. Resolved by merging the new config section with the pre-existing single-bullet advertise-address section (two headings for three bullets was structure costing more than it bought) and collapsing two pairs of adjacent same-question lines. Now exactly 75.
- `metric-coverage` flagged `mc_participant_outbound_messages_dropped_total` as referenced by no test. The guard scans only `crates/<svc>/tests/**`, so the two unit tests that actually **drive** the drop (filling a one-slot channel in `actors::participant::tests`) did not satisfy it. Rather than add a token reference, the loopback integration test now asserts the counter is **flat**: the wire assertions fail by timeout if a message never arrives, but they cannot distinguish "MC never composed it" from "MC composed it and the mailbox dropped it" — and only the second is silent.

### Layer 4: Unit Tests
**Status**: PASS
**Output**: 384 `mc-service` lib tests (343 before this task; +41). New: 17 capability-validation, 12 directive, 10 assignment-join, 2 outbound-drop.

### Layer 5: All Tests (Integration)
**Status**: PASS
**Tests**: `mc-service` + `common` + `media-protocol` + `mc-test-utils` — all green, 0 failed. New file `media_client_signaling_integration.rs`: 16 tests.

`cargo test --workspace` additionally shows pre-existing `ac-service` failures from Postgres tests with no reachable database in this environment (`failed to lookup address information`) — environmental, unrelated to this diff, and expected to pass under the pipeline's `DATABASE_URL`.

### Layer 6: Clippy
**Status**: PASS
**Output**: `cargo clippy --workspace --all-targets -- -D warnings` clean. No `unwrap`/`expect` on any new non-test path, and no `as` other than the idiomatic proto-enum `as i32` at wire-encode sites (see §Guard/lint expectations for the five widening casts removed during review); no `debug_assert!` used for input validation.

### Layer 7: Env-tests
**Status**: NOT RUN by the implementer — requires a dev-cluster rebuild. The Lead runs `./scripts/layer-all.sh`.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Gate 3 — Open Findings Ledger (Lead-maintained, live)

**Why this section exists.** §Issues Encountered Issue 1 records that a confirmed finding was lost
across the first interruption because open findings lived only in a reviewer↔implementer message
thread, and `main.md` carried *phase* but not *findings*. That gap then repeated across the second
interruption: every fix in §Review fixes had landed, and §Code Review Results was still blank, so the
recovery record was phase-complete and finding-incomplete twice over. This ledger is the Lead writing
findings down **as they are raised**, before any verdict exists, so a third interruption loses
nothing. It is superseded by §Code Review Results once verdicts land — until then it is the
authoritative list of what is outstanding.

Round 2 (resumed 2026-09-05). Reviewers re-derived from `git diff HEAD` and were told to treat the
blank verdict table as zero-information. **That instruction paid for itself**: the panel raised 27
new findings against a diff that had already been through a full review round, including two
(security S-2, infrastructure I-C) that are defects in fixes made during the *interrupted* round.

### Fixed during round 2

| # | Reviewer | Finding | Fixed by |
|---|----------|---------|----------|
| S-2 | security | Client-reachable mute fan-out delivered **zero bytes to zero clients** (`MuteChanged` is never wire-serialized — two producers, no consumer) while costing an O(N) clone-and-await on the shared meeting-actor task. `MuteWorkLimiter` is per-connection; the resource is per-meeting; MC has no per-meeting roster cap (`MC_MAX_PARTICIPANTS` is an MC-wide accept-loop cap). Aggregate O(N²), and reachable by a buggy client *release*, not only an attacker. Confirmed diff-created: at base `1d755455`, `update_self_mute`'s only caller was a test. | implementer — broadcast removed, `handle_self_mute` now synchronous, retirement condition at the site |
| S-3 | security | Capability-rejection responder was the only client-driven path in the new dispatch with no bound of any kind, emitting a ~10x-larger `ErrorMessage` per rejected message through two actor hops and a QUIC write. | implementer — `MuteWorkLimiter` generalised to `ClientWorkLimiter`; a **separate** bucket for the rejection reply, so neither path can deny the other; token spent after the counter and the one-shot WARN |
| S-3(b) | security | The control added for S-3 was wired but **undemonstrated** — every rejection test sends one rejection on a fresh connection, so deleting `try_spend`, or building the bucket with `u32::MAX`, or moving the spend after the send would all have left the suite green. The loop's recurring vacuous-control shape, this time sitting on a control added *during review*. | implementer — `capability_rejection_replies_are_rate_limited_but_never_uncounted`; security then verified by perturbation (burst 8 → 100 000, confirmed the edit landed by grep **and file hash**, test failed exit 101, restored from a byte copy, `git diff --stat` unchanged) |
| I-C | infrastructure | `dt-guard`'s R-20 dashboard parser carried the same line-oriented-comment defect fixed in R-16 **90 lines above it in the same file** — the interrupted round had fixed the reported instance, not the mechanism. Worse than R-16: with no `- ` anchor it failed **fail-open**, so a standalone comment naming a dashboard path *declared* it and a genuinely-unlisted dashboard passed R-20 silently. | infrastructure — one shared `strip_inline_comment` across both parsers plus the missing bullet anchor; pin verified to fail against the unfixed parser **in both directions**, after a first fixture draft was caught vacuous on the fail-open half |
| I-D | infrastructure | Both `kustomization.yaml` files named the wrong failure signal for a missing TLS Secret ("fail the cert file-existence check"). Neither `mc-tls` nor `mh-tls` is `optional:`, so kubelet blocks the mount and the container never starts: `FailedMount`, empty logs, no cert error. A comment written specifically to stop a responder hunting in the wrong artifact was naming the wrong artifact. | infrastructure — corrected in both (class, not instance) |
| OPS-1 | operations | Both rollback paths in `mc-deployment.md` named `mc-0` only, while §Update Container Image was made two-instance in the same diff. Stopping after mc-0 leaves a mixed-version signalling plane GC keeps assigning to — intermittent and per-meeting, and a half-rolled pair cannot answer "did that fix it". The 3am path was the one still relying on a blanket "repeat for mc-1". | operations |
| OPS-2 | operations | The `deployment/mc-service` sweep missed a **third form** naming no workload kind: six `curl http://mc-service…:8080/{health,ready}` sites across three runbooks. Broken twice — wrong port (8081), and correcting the port alone makes it **worse**, because MC's NetworkPolicy admits 8081 from Prometheus only, so the corrected command times out and a hang reads as "MC is wedged" — the one conclusion a dependency-health check exists to rule out. The AC and GC lines beside it genuinely work, so the block manufactured a false incident conclusion. | operations — replaced all six; §MC Topology preamble byte-identity preserved; both `docs/TODO.md` entries asserting this class CLOSED corrected |
| OPS-3 | operations | No post-deploy verification of the five new required keys' **effective** values; the ConfigMap banner's own claim (missing key loud, wrong-but-valid key silent with the startup log as its only record) was never cashed, and the frame-rate coupling WARN was unreachable because §Logs review greps `error\|panic\|fatal`. | operations — reads the `Configuration loaded successfully` line from **both** pods and requires them identical (a difference means one Deployment's `configMapKeyRef` is stale — the guard-green split-brain) |
| OPS-6 | operations | Three places in `mc-deployment.md` asserted a `MC_TLS_CERT_PATH` config-load failure that **cannot occur** when the Secret is missing, telling an operator to look for an error they will never see. Surfaced by I-D. | operations — fixed by **scoping rather than deleting** (the file check is the right symptom for a Secret that exists but is malformed); §Config-failure triage heading renamed, because "two different signals" sat above a three-row table — a completeness assertion falsified by the table beneath it, this loop's most-repeated defect |

### Round-2 findings raised after the first ledger snapshot, and their disposition

The ledger below was itself caught **stale in the safe direction** by `@observability` — it still showed
O-13..O-23 as `open` after all eleven had landed. That is the same mechanism the ledger exists to
defeat, arriving in the ledger, and it is recorded rather than quietly corrected.

| # | Reviewer | Finding | Status |
|---|----------|---------|--------|
| T-1 | test | Rate-limit test's exact even-split was timing-dependent (skews under 2+ mid-stream refills), an ADR-0028 zero-retry flake vector. | fixed — replaced with timing-independent invariants: `applied` exact, `rate_limited + unchanged == remaining`, and `rate_limited >= 1` so the partition cannot pass on a limiter that never engaged. Needed the `delta()` reader |
| CR-F1 | code-reviewer | Reported "reded" typo in `crates/dt-guard/src/kustomize.rs`. | **withdrawn as mis-located**, settled by grep and `git blame`: 0 occurrences in that file (it reads "turned Layer 3 red"); the only in-tree instance is `docs/TODO.md:733` on a line commit `d125f5d3b`/2026-08-17 introduced and this diff did not touch. Implementer correctly refused to edit on a mis-located report |
| O-13..O-23 | observability | Eleven findings, ten of them the §Lessons Learned 1 second failure mode. | all fixed, re-verified **by content** rather than by the original line citations, which had shifted under S-2/S-3 |
| O-24 | observability | The log-coverage tiers added to fix O-16 enumerate two of three `build_send_directive` failure values — `handler_url_unresolved` missing. An enumeration defect inside the fix for an enumeration defect. | fixed |
| O-25 | observability | `mc-alerts.yaml` entering the diff created a corrected/uncorrected pair: `alerts.md:714` still carried the sentence OPS-5 deleted from the YAML, `alerts.md:620` listed three alerts that exist nowhere, and `alert-conventions.md:47` used one of the ghosts as the **`page` tier anchor example**. | fixed (MC three); wider 14-name class filed to `docs/TODO.md` §Observability Debt |
| O-26 | observability | **The sharpest finding of the round.** S-2 removed the meeting-wide fan-out, and seven artifacts still cited `broadcast_update` as the live justification for the mute limiter — two of them mis-directing triage (`metrics/mc-service.md:470` gave the metric's Usage as "is any connection driving the meeting-wide fan-out hard enough to be clamped", and `mc-incident-response.md:2065` routed an operator chasing meeting-wide latency to a metric that can no longer indicate it). **No hunk in S-2's diff contains the word "limiter" or names `mc_media_mute_requests_total`** — findable only by sweeping for the relationship. First instance in this loop where the falsifying change came from a *different reviewer's* fix mid-round rather than the implementer's own edit. | fixed, incl. a seventh site the implementer found and self-reported |
| O-27 | observability | `tests/media_client_signaling_integration.rs:565` still claims a mute toggle "would otherwise drive the meeting actor's O(N) `broadcast_update`". The implementer's O-26 class sweep used four *phrasings* of the claim rather than the symbol, so it could not match. | fixed — the comment now states the surviving cost (a `GetState` roster snapshot per composition), names S-2 as the remover, and says explicitly that the bound is still load-bearing so a reader checking `handle_self_mute` and finding no fan-out does not delete a live control |
| O-28 | observability | `main.md:1313` names `MuteWorkLimiter`, which `grep -rn` finds in zero code files — S-3 renamed it `ClientWorkLimiter`. The correction sits 218 lines below with nothing linking them: the §Lessons Learned 5 shape, in the same document that has already had a dead symbol copied out of it into an incident runbook once. | fixed — a SUPERSEDED marker now sits at the TOP of the paragraph (the copy a reader reaches first), recording both (a) S-2's removal of the fan-out the bound was sized against and (b) that `MuteWorkLimiter` does not exist. The original text is left intact deliberately: it is *why* S-2 and S-3 happened |
| D-3 | dry-reviewer | `gc_integration.rs` restated the `{8, 64, Opus/48000/50}` triple D2 had just fixed in its byte-identical sibling — the diff *created* an asymmetry between two functions identical before it. D1's own defect, one file over. | fixed |
| D-4 | dry-reviewer | `bounded_usize`/`bounded_u32` verbatim-identical modulo integer type; the rustdoc's own criterion favoured collapsing. | fixed — one generic `bounded<T>`, which code-reviewer confirms **strengthens** the no-widening-casts property rather than relocating the casts |
| D-5 | dry-reviewer | `mc-deployment.md` copied the ConfigMap's "this file is the source" claim into a file where the deixis pointed at the wrong artifact. | fixed |
| OPS-4 | operations | `main.md` §Final state, as shipped — "(four new)" over six rows, `outcome (7)` where `DirectiveOutcome::ALL` is 8. | fixed — and better than asked: the Cardinality column now names the **bounding artifact** instead of restating an integer, removing the encoding rather than correcting it |
| OPS-5 | operations | `MCJoinLatencyHigh`'s description cited an aggregate `MCHighLatency` page alert that has never existed — the notification text a responder reads first, claiming page-severity coverage of the processing SLO that nothing provides. | fixed |
| §6.4 | Lead | `crates/common/src/observability/testing.rs` entered the diff post-scoping; ADR-0024 §6.4 requires DRY + code-reviewer approval, escalating to Minor-judgment if call-site semantics change. | **approved by both**, and separately ACKed by `observability` as owner. Call-site semantics unchanged (purely additive, `#[must_use]`, consumes `self`), so it stays review-only and does not escalate to owner-implements |

**Lead ruling — DRY verdict label.** `@dry-reviewer` fixed all three of their fix-or-defer findings and
deferred none, which reads as RESOLVED-FIXED, but filed five §Accepted Deferrals pointers for tracked
extraction opportunities. They flagged the tension rather than taking the flattering label. Ruled
**RESOLVED-DEFERRED**: ADR-0019 governs *routing* (extraction opportunities do not reach the
implementer's fix-or-defer flow, and none did), while the verdict format governs *labelling* and names
DRY extraction opportunities alongside deferrals, stating that a non-empty §Accepted Deferrals forces
RESOLVED-DEFERRED. Its rationale — a cost shift should not be averaged away by the fixes beside it —
applies squarely to seven tracked duplications. The two texts are genuinely ambiguous read side by
side; closing that is the devloop skill's job, not this diff's.

**Lead ruling — `mc-alerts.yaml` ownership.** Filed as `observability` (alert taxonomy, ADR-0031);
corrected to **`operations`**. ADR-0011's Documentation Ownership table allocates
`infra/docker/prometheus/rules/` to Operations **by name**, the edit is description-only (`expr`,
`for:`, `labels:` and severity all untouched), and all four in-tree precedents split the way that table
predicts. Owner-attribution, not a tier challenge, so no auto-ESCALATE under §6.2. Three reviewers
reasoned from three starting points to two answers on a one-line row and none opened that table first
— a pointer to it from the Cross-Boundary Classification instructions would have saved the round trip,
and that is a devloop-skill improvement, not a diff defect.

**One correction to a Gate 3 ruling, carried forward because it is load-bearing.** Security recorded
`take_entries` as "misleadingly named but non-destructive". `@dry-reviewer` showed the premise is false
in general — counters and gauges load non-destructively, **histograms drain** (`obs.drain(..)`).
Security's *conclusion* is safe, both call sites being counters, but as recorded the premise licenses
the refactor the implementer declined: `assert_delta` delegating to `delta()` and re-taking entries for
its failure dump would drain histograms twice, silently wiping data for any later `HistogramQuery` in
the same test whenever a counter assertion failed. A stronger reason to keep them separate than the one
originally given, and it belongs in the code rather than in this thread.

### Accepted deferral (infrastructure I-E)

`scripts/workflow/run-story.test.sh`'s containment check reds Layer 3 whenever any agent writes to
`/work` during the run, and its diagnostic **asserts the opposite cause** ("the post-run state is
STABLE across two samples, so no concurrent writer explains this: the suite is the actor… Do NOT
weaken this check"). Two post-run samples cannot distinguish "the suite wrote" from "another writer
wrote once and stopped", which is the normal case in a multi-agent devloop. Deferral accepted by the
Lead: the file is outside the changeset, and the correct predicate must distinguish the suite's own
writes from a third party's — the cheap version of which is to relax a containment control, exactly
what its own comment warns against. That is "requires a design decision" plus "significant regression
risk", not sunk-cost framing. `docs/TODO.md` tracking entry routed to operations (whose row it is)
and confirmed outstanding by infrastructure rather than assumed landed — **the Lead verifies it exists
before Step 8.**

Both operations and infrastructure independently mis-read or nearly mis-read this same Layer-3 red,
and operations corrected their own misattribution on the record rather than letting it stand. Three
agents reached the true cause only by checking file mtimes against the run window. The check's
message actively argues against its own true cause, which is why it is written up in three places
here rather than one.

### Confirmed fixed on re-derivation and explicitly NOT re-opened

S-1 (all three checks hold, including `refill()`'s backwards-clock and `Duration` overflow edges),
O-1..O-12, test F1/F2/F3 (positive controls verified genuinely non-vacuous), code-reviewer F1/F2,
D1/D2, infrastructure I-A/I-B (magnitude arithmetic re-checked digit by digit).

---

## Code Review Results

Round 2 (resumed 2026-09-05). All eight reviewers re-derived from `git diff HEAD` and were told to treat
the previously-blank table as zero-information. That instruction produced **27 new findings** against a
diff that had already been through a complete review round.

| Reviewer | Verdict | Found | Fixed | Deferred |
|----------|---------|-------|-------|----------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 |
| Test | RESOLVED-FIXED | 1 | 1 | 0 |
| Observability | RESOLVED-FIXED | 16 | 16 | 0 |
| Code Quality | CLEAR | 1 (withdrawn) | — | 0 |
| DRY | RESOLVED-DEFERRED | 3 | 3 | 0 fix-or-defer; 5 extraction pointers |
| Operations | RESOLVED-FIXED | 6 | 6 | 0 |
| Semantic Guard | CLEAR | 0 | — | 0 |
| Infrastructure (cross-boundary owner) | RESOLVED-DEFERRED | 3 | 2 | 1 |

### Security Specialist
**Verdict**: RESOLVED-FIXED — 3 found, 3 fixed, 0 deferred.
S-1 confirmed fixed on re-derivation and not re-opened. **S-2**: the diff's new `MuteRequest` arm made
`broadcast_update` client-reachable, but `MuteChanged` has no consumer — the O(N) awaited fan-out
delivered **zero bytes to zero clients** while a per-connection limiter tried to bound a per-meeting
resource, with no per-meeting roster cap in MC. Removed structurally. **S-3**: the capability-rejection
responder was the one unbounded client-driven path; given its own bucket, spent after the counter.
**S-3(b)**: security then found the S-3 control was *undemonstrated* — every rejection test sent one
rejection, so deleting `try_spend` left the suite green — and verified the new test by perturbation
(burst 8 → 100 000, edit confirmed landed by grep **and file hash**, test red at exit 101, restored from
a byte copy). Also caught the diff growing 36 → 39 files after their own Step 0 scoping and re-reviewed
all three additions rather than letting a stale scope claim stand.

### Test Specialist
**Verdict**: RESOLVED-FIXED — 1 found, 1 fixed, 0 deferred.
**T-1**: the rate-limit test's exact even-split was timing-dependent and skews under 2+ mid-stream token
refills — an ADR-0028 zero-retry flake vector. Replaced with run-structure-independent invariants plus a
`rate_limited >= 1` floor so the partition cannot pass on a limiter that never engaged. Confirmed the
prior F1/F2/F3 fixes genuinely non-vacuous rather than merely present.

### Observability Specialist
**Verdict**: RESOLVED-FIXED — 16 found (O-13..O-28), 16 fixed, 0 deferred.
Fifteen of sixteen were prose asserting a relationship the code does not keep. **O-23**: a claim already
found false and narrowed in the catalog was still standing in two code sites, with the true version
twenty lines above one of them. **O-26**: security's S-2 falsified the mute limiter's rationale in seven
artifacts, two of them mis-routing triage — and no hunk in S-2's diff contains the word "limiter" or
names the metric, so it was reachable only by sweeping for the relationship. **O-19** was the one
non-prose finding: `or vector(0)` on the panel the runbook designates as *contradicting evidence*,
making it unable to distinguish "no drops" from "metric absent". Corrected their own ownership error on
`mc-alerts.yaml` and fixed the cause in their INDEX rather than only the instance.

### Code Quality Reviewer
**Verdict**: CLEAR — 1 raised and **withdrawn as mis-located**, 0 deferred.
CR-F1 was settled by `grep` and `git blame` rather than recollection: 0 occurrences in the file named.
The implementer was right to refuse the edit. Ownership Lens re-derived from `git diff HEAD --name-only`
rather than the earlier figure — 13 non-`Mine` rows, every one of the 41 files matched to a row, glob or
exemption, no uncovered file, no GSA touched, no classification warranting upgrade. Granted the
code-reviewer half of the ADR-0024 §6.4 approval on `crates/common/src/observability/testing.rs` and
ruled call-site semantics unchanged, so it stays review-only.

### DRY Reviewer
**Verdict**: RESOLVED-DEFERRED (Lead ruling — see §Gate 3 ledger).
**True duplication findings**: D3 (`gc_integration.rs` restated the media triple D2 had just fixed in its
byte-identical sibling — the diff *created* the asymmetry), D4 (`bounded_usize`/`bounded_u32` collapsed
into one generic, which code-reviewer confirms *strengthens* the no-widening-casts property), D5 (a
"this file is the source" claim copied into a file where the deixis pointed at the wrong artifact). All
three fixed. Also corrected a Gate 3 ruling: security's recorded `take_entries` premise was false in
general (histograms **drain**), and as written licensed the refactor the implementer declined.
**Extraction opportunities** appended to `docs/TODO.md` — four under §Cross-Service Duplication (DRY),
one under §From DRY Reviewer (Ongoing); see §Accepted Deferrals.

### Operations Reviewer
**Verdict**: RESOLVED-FIXED — 6 found, 6 fixed, 0 deferred.
**OPS-2** is the substantive one: the `deployment/mc-service` sweep missed a third form naming no
workload kind, and the six sites were broken twice — wrong port, and correcting the port alone makes it
*worse*, because MC's NetworkPolicy admits 8081 from Prometheus only, so the corrected command times out
and a hang reads as "MC is wedged". Also OPS-1 (rollback left single-instance while deploy was made
two-instance), OPS-3 (no post-deploy verification of the five new required keys' effective values),
OPS-6 (three sites promising a config-load error that cannot occur, since a missing Secret blocks the
mount first). Corrected their own misattribution of the Layer-3 red on the record. Ownership Lens: ACK
on all five owned rows.

### Semantic Guard Reviewer
**Verdict**: CLEAR. **Native verdict**: SAFE. 0 found.
First real semantic-guard pass on this diff. Credential Leak, Client Credential Lifetime, Actor
Blocking, Error Context Preservation, Metrics Path Completeness — all clean, each verified by following
values rather than names. No `guard:ignore` annotations in the diff.

### Infrastructure Reviewer (cross-boundary owner, ADR-0024 §6.3)
**Verdict**: RESOLVED-DEFERRED — 3 found, 2 fixed, 1 deferred.
All six owned rows CONFIRMED. **I-C**: `dt-guard`'s R-20 parser carried the same defect "fixed" in R-16
ninety lines above it, and failed **fail-open** — the interrupted round had fixed the instance, not the
mechanism. **I-D**: both `kustomization.yaml` files named the wrong failure signal for a missing TLS
Secret; a comment written to stop a responder hunting in the wrong artifact was naming the wrong
artifact. **I-E** deferred (see §Accepted Deferrals). Verified I1's consequence past the source files by
rendering the Kind overlay — 5 keys, 10 `configMapKeyRef`s, `replicas: 1` in both.

### Deferral classification (read with §Accepted Deferrals below)

Only the first is a deferral of a *finding* (infrastructure's I-E, accepted by the Lead: the file is
outside the changeset and the correct predicate is a design question in a containment control whose own
comment warns against weakening it). The DRY entries are ADR-0019 extraction opportunities, which do not
enter fix-or-defer but do force the RESOLVED-DEFERRED label under the verdict format — see the Lead
ruling in §Gate 3 ledger. The two observability entries are class entries naming pre-existing gaps wider
than this changeset, with each in-scope instance fixed here.

This paragraph lives here rather than under §Accepted Deferrals because that section is
pointer-only by contract — `dt-guard todo-tracking`'s `inline_debt_body` rule reds Layer 3 on two
consecutive non-bullet lines inside it, so that the durable body of every deferral stays in
`docs/TODO.md` and cannot fork from it. The guard caught this paragraph in the Gate 2 authority run.

## Accepted Deferrals

- `docs/TODO.md` §Infrastructure Validation in Devloops — run-story containment check misattributes concurrent writers
- `docs/TODO.md` §Cross-Service Duplication (DRY) — MC/MH duplicate required-env-numeric bounds parser
- `docs/TODO.md` §Cross-Service Duplication (DRY) — MC media-signalling config fixture has five homes
- `docs/TODO.md` §Cross-Service Duplication (DRY) — `enum + ALL + label()` hand-maintained at seven sites
- `docs/TODO.md` §Cross-Service Duplication (DRY) — three MC tests carry a local `encode_framed` the shared home exports
- `docs/TODO.md` §From DRY Reviewer (Ongoing) — `ClientWorkLimiter` shared home, triggered by first non-MC consumer
- `docs/TODO.md` §Observability Debt — `dashboards.md` MC Overview inventory duplication (fix the duplication, not the staleness)
- `docs/TODO.md` §Observability Debt — 14 doc-only alert names across AC/MC/MH; AC half is a design question

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied

---

## Issues Encountered & Resolutions

### Issue 1: The resume lost an open reviewer finding

**What happened.** This loop was interrupted after Gate 2 and the roster was respawned at `review`.
The pre-interruption DRY instance had already raised what the new instance re-filed as **D2** (the
OTel fixture restating the media triple instead of sourcing it from `test_common::client_media_config()`).
It survived the resume untouched, and the second dry-reviewer only caught it because they re-derived
it from the diff and explicitly chose to re-raise rather than assume it had been triaged.

**Why it was lost.** §Code Review Results was still the unfilled template. The Loop State table
records *phase* and *per-reviewer status*, and `main.md` is the recovery document — but neither
carries **open findings**. A finding raised, acknowledged and not yet fixed lives only in the
message thread between one reviewer instance and one implementer instance, and respawning either end
drops it. Nothing detected the loss; a reviewer's diligence did.

**Resolution and the general point.** D2 is fixed. The narrower lesson is that the reviewer's
instinct — re-raise rather than assume triage — is the correct default across a resume boundary, and
they were right to say so out loud. The wider one is that this loop's recovery record is
**phase-complete and finding-incomplete**: had that reviewer trusted the empty verdict table as
"nothing outstanding", a confirmed finding would have shipped with a green board. Filed as a process
gap; it is the devloop skill's to close, not this diff's.

### Issue 2: A verification claim that could not fail, reported to four reviewers

**What happened.** I told @security, @test, @observability and @code-reviewer that
`cargo clippy --workspace --all-targets -- -D warnings` was clean. It was not: Layer 6 was red with
four `clippy::panic` errors in the `config.rs` test module, which my own I-B(b) test edit had
introduced. @security caught it by running the pipeline's exact command.

**Why the check passed.** I ran clippy piped into `grep -E "^error|^warning: "` followed by an
`echo`. Clippy's output is ANSI-coloured, so every error line begins with an escape sequence and
`^error` never matches; the trailing `echo` then supplied exit 0. **The filter could not have
printed anything different had the code been wrong.**

**Resolution.** `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` on the test
module, matching the existing convention in `actors/participant.rs` and `webtransport/handler.rs`.
Verified by exit code (`PIPESTATUS`) and by running the layer scripts, which report `RESULT=`/`STATUS=`
for precisely this reason. The 100-character `MissingEnvVar` line was re-measured after the edit and
is unchanged, and `./scripts/layer3.sh` re-run green.

Recorded because it is the fourth instance of one mechanism in this loop (see §Lessons Learned 1) and
the only one where the false claim was *transmitted to other agents* rather than caught in place. A
second nearly-identical trap fired minutes later: `cargo test … | grep … | head` reported
`PIPESTATUS[0]=101` purely from `head` closing the pipe, which I nearly read as a test failure. Same
root cause in the opposite direction — a filtered pipeline is not a status.

---

## Lessons Learned

1. **A negative control must assert that it APPLIED.** Three separate vacuous-control incidents in
   one devloop, and the third is the one that generalises it. (a) The redirect test's premise
   assertion raced the bridge loop and could have passed while proving nothing. (b) `@test`'s F1
   found that the fail-closed branches inside `build_send_directive` had no test, and my fix needed a
   *positive* control so that a helper mis-populating a field — making every hand-built assignment
   rejected — could not pass both guard tests vacuously. (c) `@infrastructure` removed their own
   guard fix with a single-line string replace to confirm the regression test would fail; it passed,
   and they nearly recorded that as verification — the replace had matched nothing because `cargo
   fmt` had reflowed the target across four lines. (d) Verifying my config bounds, infrastructure
   wrote a check that printed seven lines reading `reject MC_AUDIO_FRAME_RATE_HZ:24:25..=50` — it
   asserted nothing and echoed strings they had typed, and it looked exactly like a passing
   verification. **The check is not "did the control fail", it is "did my perturbation reach the
   code" — and more generally, output that RESEMBLES verification is not verification. The tell is
   whether the check could have printed something different had the code been wrong.**

   The **prose** version of the same rule, which cost three findings to learn (O-4, O-9, O-11):
   for any sentence asserting an invariant, ask *what would have to be true for this to be false, and
   does the code actually prevent it?* All three of those errors over-claimed and none under-claimed,
   because the person writing the prose is the person holding the design in their head and the strong
   form is the one already there. O-11 is the sharpest instance — the entry was **most confidently
   wrong in the exact scenario it was written for** — and it was ratified by the reviewer who had
   proposed this very check an hour earlier, which is the evidence that the bias is structural rather
   than personal. Prose is the only encoding in this diff with no compiler and no guard behind it.
   Note the correct direction of repair: in all three cases the code was right and the sentence was
   the defect, so the fix narrows the claim — never weaken working code to match an overstated doc.

   **A second, harder failure mode sits underneath O-11 and O-12: a CORRECT change making a
   PREVIOUSLY ACCURATE sentence wrong.** Nothing was mistaken when either sentence was written. The
   send-directive counter's "this is where that shows up" was true until the record site settled
   above `send_signaling`; the drop counter's "previously WARN-only and therefore unqueryable" was
   true until O-5 made the WARN one-shot and inverted which artifact carried the magnitude. Neither
   is caught by asking "is this claim true of the code I am writing" — both need "which sentences
   described the relationship I just changed". **Every observability change in this round altered a
   relationship between two artifacts** (counter and send site; log and counter), and the prose
   describing the old relationship is not adjacent to the edit that invalidates it.

   I proved the point against myself while writing this list up. I reported "clippy clean" to four
   reviewers on the strength of a background command whose filter was
   `grep -E "^error|^warning: "` — which never matches, because clippy's output is ANSI-coloured, so
   the line begins with an escape sequence rather than `error`. The trailing `echo` then supplied
   exit 0. Layer 6 was red with four `clippy::panic` errors the whole time and @security caught it.
   The failure was not the missing `clippy::panic` allow; it was that **I filtered a result instead
   of checking a status**, and every layer script in the tree reports `RESULT=`/`STATUS=` for exactly
   that reason. Check the exit code (`PIPESTATUS`), or run the layer script that does.

   @security nearly made the identical mistake with `... | tail -5; echo "EXIT: $?"`, which reported
   `EXIT: 0` while clippy exited 101 — `$?` was `tail`'s status. The honest framing is theirs: **it
   was not that the careful reviewer used exit codes and the hurried implementer did not; both of us
   wrote a pipeline whose exit status described the filter rather than the command**, and the
   difference in outcome was only which filter we happened to reach for. `tail` prints content;
   `grep "^error"` on ANSI-coloured output does not. That is luck.

   The mechanism then caught me a **third** time, in the message reporting it. I told `main` that the
   only non-mc-service modifications were **two** files. There were three: my `git status | grep -v ...`
   exclusion list, written to hide my own changeset, also hid
   `infra/services/mc-service/kustomization.yaml`. @observability caught it by enumerating instead of
   filtering. Nothing was wrong in the tree — all three carry rows — but the count I asserted came
   from a filter I had built for readability, which is the exact error in the paragraph above,
   committed while writing that paragraph. **When the output IS the claim, enumerate; a filter is a
   hypothesis about what you can safely ignore, and stating a count over it asserts that hypothesis
   without testing it.**

2. **Line-oriented matching over formatted source is unsafe for everyone, not just for guards.**
   The `MissingEnvVar` discovery regex breaking on a rustfmt wrap looked like a guard-specific
   quirk when I found it. It recurred ten minutes later in infrastructure's *verification step*, and
   again in `extract_declared_resources` failing on an inline `# comment`. One mechanism, three
   instances, two tools. Sweep for the mechanism rather than fixing reported instances — which is
   how the `env-config` TODO entry is now written.

3. **Choose the KIND of bound from the shape of the action, not from the nearest precedent.** The
   receive-capability path takes a cumulative budget; client mute needed a rate limit, and applying
   the budget pattern to it — which is what both security and I initially reached for — would have
   frozen a participant's `audio_self_muted` for the rest of the session, so every other client
   rendered a live speaker as muted. A cumulative budget bounds total work and **permanently denies**,
   which is only acceptable for a *rare* action whose omission costs nothing; a rate limit bounds work
   per unit time and never permanently denies, which is what a *repeatable steady-state user action*
   requires. The criterion now lives in the `media_signaling` module doc rather than only in this file.

4. **One cache key cannot answer two questions that read different fields.** `last_reported_mute`
   was the right dedupe key for "report this to the meeting actor" and the wrong one for "recompose
   the slot view", because the actor stores both mute flags while the slot view derives from
   `audio_self_muted` alone. The symptom was a video-only toggle running a full O(N) composition to
   emit a byte-identical message. Two consumers, two questions, two answers.

5. **A superseded statement that a reader reaches FIRST is worse than no correction at all.** The
   §Planning observability table was corrected 650 lines below itself, and operations copied a
   label key out of the uncorrected copy into an incident runbook, caught only by their own
   cross-check against `metrics.rs`. The fix is a marker at the top naming the dead metric by name —
   a generic "may be outdated" is the kind of hedge a reader discounts. Same shape as the
   `deployment/mc-service` runbook sweep in this same diff: a corrected artifact and an uncorrected
   one coexisting, with the uncorrected one more discoverable.

6. **A defect the diff CREATES is the diff's to close, even when the mechanism predates it.**
   `MeetingActor::broadcast_update` and `update_self_mute` both existed before this task; what did
   not exist was any client-reachable path to them. Adding the `MuteRequest` dispatch arm made an
   O(N) meeting-wide fan-out client-drivable for the first time, which is a new amplification
   surface regardless of how old the code underneath is. `git show <base>:file | grep` settled it in
   one command.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
