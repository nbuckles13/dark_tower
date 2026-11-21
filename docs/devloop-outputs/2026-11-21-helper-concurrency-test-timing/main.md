# Devloop Output: devloop-helper concurrency tests must not race real kind/kubectl against a fixed write duration

**Date**: 2026-11-21
**Task**: Fix the class of devloop-helper concurrency tests (test_status_audit_drift_invariant, test_cancel_pending_visible_in_status, and any sibling) whose outcome depends on real kind/kubectl latency in cmd_status vs a fixed-duration test write; no longer-sleeps/retries. Also correct docs/TODO.md:2228 "K1–K20" → K1–K26.
**Specialist**: infrastructure (paired with test)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/layer-all-devloop-helper`
**Duration**: ~2h (setup 19:05 through Gate 2)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b183b2f638ed9229f19ad9499ef2ed12a56434b1` |
| Branch | `feature/layer-all-devloop-helper` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test (paired-test) | `paired-test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | not spawned (test-timing fix in devloop-helper tests; no checks.md surface) |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

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
| `crates/devloop-helper/src/main.rs` | Mine | — |
| `crates/devloop-helper/src/commands.rs` | Mine | — |
| `crates/devloop-helper/src/protocol.rs` | Mine | — |
| `crates/devloop-helper/src/error.rs` (fixture rename) | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `docs/devloop-outputs/2026-11-21-helper-concurrency-test-timing/main.md` | Mine | — |

---

## Planning

### Mechanism restatement
Instance: two tests saw "cancel: no write in flight" because a 10s/5s test write ended while `status` was still in real kind/kubectl.

Mechanism: **a test assumes a write is still in flight at step N, but holds it open only for a fixed lifetime (wall-clock `sleep N`, or a CPU-count busy-loop in `TestSleepIgnoringTerm`) that races steps whose latency the test does not control.** The `status` call is the main such step: it runs `kind get clusters`, `kubectl get pods`, and `<runtime> inspect` plus a TCP probe. The fix is to make the write's lifetime test-controlled: it ends only when the test cancels it or releases it. The 4 `seconds` stubs go away, so a fixed-lifetime hold can no longer be written.

### Inventory (every concurrency test whose write must outlive an uncontrolled step)
| Test | Stub / lifetime | Step it races | Status |
|---|---|---|---|
| `test_status_audit_drift_invariant` | TestSleep 10s | status (kind/kubectl), then cancel | **named; red on CI** |
| `test_cancel_pending_visible_in_status` | IgnoringTerm "5s" (5M sh iterations, CPU-bound) | status, then cancel, then status during grace | **named; red on CI** |
| `test_status_does_not_block_on_write` | TestSleep 10s | status, then asserts the slot is still held | sibling: red if status takes >10s |
| `test_status_audit_drift_invariant_cancel_path` | IgnoringTerm "5s" busy-loop | the loop can finish on its own before the post-cancel status (fast CPU) | sibling (CPU-speed variant) |
| `test_cancel_pending_false_when_busy_not_cancelling` | TestSleep 3s, natural end | snapshot is taken before kind runs; the end is wall-clock | convert (natural end becomes a release) |
| `test_status_cluster_write_in_progress_only_for_cluster_writes` | TestSleep 5s | same as above | convert |
| `test_write_while_write_returns_busy`, `test_busy_rejection_is_audit_logged` | TestSleep 5s | deploy wire round trip (no kind before the busy check) | convert (same mechanism, lower exposure) |
| `test_cancel_mid_write_terminates_handler`, `test_cancel_completed_is_audit_logged` | TestSleep 30s | cancel round trip | convert |
| `test_cancel_after_write_completes_is_noop` | TestSleep 1s, run to completion | none | convert (release, then join) |
| `test_sigkill_escalation_logged` | IgnoringTerm "30s" busy-loop | cancel; CPU-bound lifetime | convert |
| `test_cancel_kills_grandchild_*` (2) | WithChild* 30s | cancel | convert; the grandchild polls the latch, so the pipe-holding shape is unchanged |

### Design (rev 2 — folds Gate-1 input from all reviewers + Lead rulings)
The rule this devloop establishes: **no helper concurrency test synchronizes on a fixed duration.** Every "is X true yet?" is an event the test waits on, with a deadline that fails loudly. The write ends only on cancel or on the test's release.

1. **One test write variant** (commands.rs/protocol.rs, cfg(test)): `HelperCommand::TestHold { latch, ready, shape }` with `enum HoldShape { Plain, IgnoringTerm, WithChild, WithChildIgnoringTerm }`. It replaces the 4 `TestSleep*{seconds}` variants and their 4 stub fns, so there is now 1 test-only arm in `execute_write`, 1 script builder and no duration parameter. Script: `sh -c '<body>' sh <latch> <ready>`, with paths passed only as positional args. The innermost process writes `: > "$2"` after `trap '' TERM` for the TERM shapes, and inside the backgrounded subshell for the child shapes. It then polls `[ -e "$1" ]` with `sleep 0.05`, so it has no busy-loop and doesn't depend on CPU speed. `name()` stays per-shape (`test-hold`, `test-hold-ignoring-term`, ...). `args_for_log` = `[latch]`. Both paths live under the per-test `TempDir`.
2. **Security pin as SSoT**: add a cfg(test) `test_only_variants()` built on an exhaustive `match` over `HoldShape`, so adding a shape without listing it is a compile error. The guard asserts each `name()` is rejected by `parse_command` and is absent from `VERBS`. Correct the protocol.rs:26-28 "none takes an argument" sentence with a cfg(test) carve-out.
3. **Cancel grace becomes Context config** (Lead #2): add `Context.cancel_grace: Duration`. Production `main.rs` sets it from `CANCEL_GRACEFUL_TIMEOUT` (still the one constant). There is no flag and no env var, so it is not reachable from production. `CancelSignal` carries it to `run_command_streaming`. The two cancel_pending tests use a long grace and end via latch release, which is clean exit after cancel and maps to `cancelled`. The two SIGKILL tests keep the production grace and assert the escalation suffix, which is the event.
4. **One test Context builder** (DRY): the four Context literals (main.rs:644 `build_test_context`, commands.rs:2538, commands.rs:3646 `test_ctx`, plus a new field) collapse into one cfg(test) `commands::test_context(dir)` with a grace override. main.rs:100 stays the single production literal.
5. **Harness** (main.rs `concurrency_tests`):
   - `const WAIT_DEADLINE: Duration = 10s` is the single fail-safe bound.
   - `poll_until(deadline, what, f)` is the one poll kernel and panics with `what` on timeout. Claim-wait, ready-wait and connection-count use it.
   - `HeldWrite::spawn(h, shape)` returns after both the slot claim AND the ready file. `release()` creates the latch.
   - `join_within()` uses an mpsc `recv_timeout(WAIT_DEADLINE)` and returns the `CommandResult`. On timeout it releases and then panics. This is a channel, not a poll: it hands back the result directly (code-reviewer #3).
   - `Drop` releases the latch, including on the panic path.
   - `spawn_write(ctx, cmd)` is the single write spawner.
   - This retires `docs/TODO.md` `wait_for_busy` (line ~262).
6. **Every fixed sleep in `concurrency_tests` goes**:
   - The 20ms claim polls move to `poll_until`.
   - The 150/200ms trap and fork sleeps are replaced by the ready file.
   - The 100ms "audit flush" sleeps are dead by ordering: `handle_connection` appends the audit entry (a synchronous `writeln`) before it returns, and `request()` reads to the EOF that only comes after it returns. A comment on `read_audit_log` records this.
   - In `test_connection_cap_rejects_at_32`, the 5ms/100ms accept sleeps become `poll_until(conn_count == 32)` with `TestHelper` exposing the counter. Its trailing 100ms goes too: `maybe_log_cap_reject` runs before `drop(stream)`, so EOF on the 33rd connection implies the entry is logged.
   - Every cancel result a test relies on is asserted, with no `let _ =` before a "completed" expectation.
   - Args assertions compare against the HeldWrite's known latch path.
7. **Wall-clock bounds replaced by state/event** (Lead #3; to be agreed with @paired-test, record in main.md):
   - `cancel < 1s` becomes: cancel returned while the TERM-ignoring hold under a long grace still holds the slot. That proves cancel did not wait for the writer, the same principle as fdc67632.
   - `write < 3/5/7s` after cancel becomes: the write terminated (join_within) with the right error string. The clean-SIGTERM tests can only end by the signal, because the latch is never released. Without pgid, the grandchild tests hang to the deadline, release and fail. The SIGKILL tests assert the escalation suffix.
   - No elapsed assertion remains.
8. **Production: cluster write during the status probe** (Lead #1, security #4):
   - `WriteState` gains `cluster_write_gen: u64`. The slot claim is factored into `claim_slot(ctx, cmd)`, which bumps the gen when `is_cluster_write(op)`.
   - `cmd_status` observes `(in_progress, gen)` at t0 and again at t1, after the probes. It calls the pure `cluster_write_during_probe(t0, t1) = t0.in_progress || t1.in_progress || t0.gen != t1.gen`.
   - The doc comment at the fold and on `cmd_status` says: `busy`/`in_flight`/`cancel_pending` are the t0 snapshot, while `cluster_write_in_progress` covers the whole probe window. `busy:false` with `cluster_write_in_progress:true` is deliberate, and must not be "simplified" back to t0. A write starting mid-status is unsafe; one ending mid-status reads as conservative.
   - Tests:
     - a full truth table on the fold, including (none, cluster) and the equal-flags/gen-changed case;
     - a wiring test: the probe section runs through `observe_cluster_writes_around(ctx, probe)`, which is cmd_status's only path. The test's probe closure claims and releases a `deploy` slot via `claim_slot`, and the result must be true. This covers both the gen bump in `claim_slot` and the t1 call site, with no test branch in `cmd_status`.
   - The consumers only read the field and never cross-check it against `busy`: `dev-cluster:411` (jq `.cluster_write_in_progress`), `layer7.sh` `__cluster_ready`/`__cluster_write_in_progress`, and the self-heal.
9. **docs/TODO.md:2228**: `K1–K20` becomes `K1–K26`.

10. **No orphaned hold shells** (ops #1 / Lead (a)): the stub gets the test process's pid as `$3` (`std::process::id()`), and the loop is `while [ ! -e "$1" ] && kill -0 "$3" 2>/dev/null; do sleep 0.05; done`. The `( loop ) &` grandchild shapes use the same `$3`. A killed `cargo test` (nextest SIGKILL, OOM, Ctrl-C into another process group) therefore leaves nothing polling: each shape exits within ~50ms of the test process dying, including the TERM-trapped one. **Proof test**: run the builder's script for every `HoldShape` with `$3` = the pid of a throwaway child the test spawns. Wait for ready, kill and reap the throwaway, then assert the shell exits on its own via `poll_until` (latch never created). Assert the same for the grandchild by having it write its own pid into the ready file.
11. **Production semantics documentation** (ops #2 / Lead (b)):
    - Doc comment on `cmd_status` and `cluster_write_during_probe`: "a cluster write held the slot at the start or end of this status call, or claimed it during the call". It can be true with `busy:false` and no in-flight banner, which means a write began mid-call: re-run status.
    - `docs/runbooks/devloop-validation.md` row :724 (`cluster-self-heal-probe-inconclusive`, which lists `Cluster write in progress: true`) gets the same statement and the same "re-run `dev-cluster status`" action.
    - The fold test covers (None, deploy) and (None, None + gen changed) explicitly.
    - Rollback: the change is helper-binary-only with no wire-shape change. To revert, rebuild the helper on the host from the prior commit (re-run `devloop.sh`).
12. **Security tightening** (security, carried into implementation): `fn is_wire_verb(&HelperCommand) -> bool` is an exhaustive match over `HelperCommand` with no `_` arm. The test asserts that the wire variants' names == `VERBS` and that every test-only name is rejected by `parse_command`. This subsumes `test_parse_command_accepts_exactly_verbs`. `CancelSignal`'s grace comes only from `ctx.cancel_grace`.

### Proof — baseline (start commit `b183b2f6`, recorded before any edit)
- Slow-PATH command: `PATH=<scratch>/slowbin:$PATH cargo test -p devloop-helper concurrency_tests`. The slowbin holds fake `kind`/`kubectl`/`podman`/`docker`, each `sleep 15; exit 1`. Result: **FAILED, 18 passed / 3 failed**:
  - `test_status_does_not_block_on_write` (main.rs:1275)
  - `test_cancel_pending_visible_in_status` (main.rs:1941, the CI line)
  - `test_status_audit_drift_invariant` (main.rs:1591, the CI line)
  
  This run had no markers. The new-tree run will add marker-writing fakes, and the baseline will be re-run with the same fakes for parity.
- Runtime, normal PATH: `cargo test -p devloop-helper` reported 184 passed in 3.07s of test time, 5.6s wall including cargo.

### Proof — new tree (after the change)
- Same slow-PATH command, run with marker-writing fakes (each fake appends `<tool> start <epoch> <argv>` / `<tool> end <epoch>` to a markers file). Result: **ok, 21 passed / 0 failed** (60.2s test time).
  - Markers: 10 `kind start … get clusters`, each followed ~15–17.5s later by `kind end`, then `podman start … inspect -f {{.State.Running}} devloop-test-control-plane`. Podman has no `end` line because the 5s `INSPECT_TIMEOUT_SECS` kills it. So every status call took ≥20s.
  - Fake kind prints nothing, so `cluster_exists=false` and `kubectl` never ran, as expected.
  - The longest test was `test_cancel_pending_visible_in_status`: three ~20s status calls in series. Nothing it asserts waited on them.
- Start commit, re-run with the same marker fakes (a worktree at `b183b2f6`, since removed): **FAILED, 18 passed / 3 failed**, the same 3 tests at the same lines (1591, 1275, 1941), with 8 `kind start` markers.
- Runtime, normal PATH, `cargo test -p devloop-helper`:

  | Tree | Test time | Tests |
  |---|---|---|
  | Start commit | 3.07–3.18s | 184 |
  | New tree | 2.20–2.23s (3 runs) | 189 |

  The new floor is the two SIGKILL-escalation tests at the production 2s grace.

### Agreed wall-clock mapping (with @paired-test, Lead ruling #3)
No elapsed assertion remains. **Revised at Gate 3 (paired-test F1, Lead ruling):** the planned `LONG_GRACE = 6 * WAIT_DEADLINE` (60s) was still a fixed post-cancel lifetime racing status latency. Under a 120s status hang it SIGKILLed mid-status and turned the two cancel_pending tests red. It is replaced by `NEVER_ESCALATE = Duration::MAX`. The production comparison is `elapsed() >= grace` with no addition, so this cannot overflow, and it is set only through `commands::test_context_with_grace`, so production has no "never" path. A held-after-cancel write now ends only on release, and a broken SIGTERM path fails at the join deadline. The SIGKILL tests use `CANCEL_GRACEFUL_TIMEOUT` itself, and a `const` assert pins `WAIT_DEADLINE > CANCEL_GRACEFUL_TIMEOUT`. Verified with a 70s status hang (above the old 60s): both cancel_pending tests are green, with ~75s per status.

| Old bound | Replacement |
|---|---|
| `cancel < 1s` (mid_write) | Under NEVER_ESCALATE with the TERM-ignoring shape, the slot is still held after cancel returns, which proves cancel does not wait for the writer. As implemented, this is folded into `test_cancel_pending_visible_in_status`: `status` shows `busy:true, cancel_pending:true` after cancel returned, until the test releases the write. |
| `write < 3s` after cancel (mid_write, clean SIGTERM) | NEVER_ESCALATE: the test ends via `join` with exactly `HelperError::Cancelled{escalated:false}`'s string. Only the SIGTERM path can end it; anything else hits the deadline and panics. |
| grandchild `< 5s` (holding_pipes) | Same as above: NEVER_ESCALATE plus `escalated:false`. A broken pgid SIGTERM leaves the grandchild polling, so the deadline panics. |
| SIGKILL `< 5s` / `< 7s` (sigkill_escalation, grandchild_with_sigkill) | Production grace (`CANCEL_GRACEFUL_TIMEOUT`) plus `join_within`, with the "escalated to sigkill" suffix as the event. |
| cancel_pending tests | NEVER_ESCALATE, ended by latch release after the pending observation. The result is `cancelled` because `cancel_set()` remaps the result. |

Implementation notes carried from code-reviewer:
- `observe_cluster_writes_around<T>(ctx, impl FnOnce() -> T) -> (T, bool)` is a plain generic.
- `Context.cancel_grace` has no Default or fallback; every construction site names it.
- `test_context(dir)` (production grace) and `test_context_with_grace(dir, grace)` are `#[cfg(test)] pub(crate)`.
- `HoldShape` derives `Copy`/`Debug` and is cfg(test), next to the variant.

Orphan check: an explicit `$3` = `std::process::id()` replaces `$PPID`. It has the same effect, and the proof test can point it at a throwaway pid.

### Crate-wide residue audit (Lead ruling, post-Gate-3)
`cmd_deploy_cancel_kills_script_child` was fixed in this loop. It had used a `sleep 30` grandchild plus `elapsed() < 20s`. Its grandchild now runs the shared `commands::READY_AND_HOLD` loop and is held until the test's latch or the test process is gone. `cmd_deploy` runs on a scoped thread, and the test proves the kill by state:
- the result is `Cancelled`;
- the latch was never created;
- the grandchild pid is gone (`poll_until`).

Inside the scope, a `ReleaseOnDrop` guard creates the latch on ANY unwind (a failed ready wait or the deadline) before `scope` joins the deploy thread. It is disarmed on success. Mutation checks:
- a leader-only kill (pgid SIGTERM removed) goes red at the 10s `WAIT_DEADLINE`, with no hang;
- a broken ready marker (paired-test F3) goes red in about 10s instead of hanging.

`hold_command_exits_when_owner_is_gone` holds its `sleep 600` owner in a `KillOnDrop` guard (paired-test F4), so a failing wait leaks neither the owner nor the stub.

`grep -rn 'sleep [0-9]|"sleep"|elapsed() <|.elapsed()|thread::sleep' crates/devloop-helper/src/` turns up these remaining hits, each kept on purpose:

| Site | Why it stays |
|---|---|
| `commands.rs` `READY_AND_HOLD` `sleep 0.05` | the hold loop's poll interval; the loop ends on an event (latch or owner gone), never on a count |
| `commands.rs` `poll_until` `thread::sleep(10ms)` | THE poll kernel's interval, bounded by `WAIT_DEADLINE` |
| `commands.rs` `hold_command_exits_when_owner_is_gone` `sleep 600` owner | a throwaway stand-in for the test process; the test kills it explicitly, so its lifetime is never relied on |
| `commands.rs` `start.elapsed()` → `duration_ms` | production: the result's reported duration, not an assertion |
| `commands.rs` `t0.elapsed() >= signal.grace` | production: the SIGTERM→SIGKILL grace check |
| `main.rs` :178/:183 `thread::sleep(100ms)` (production accept loop) and :1246 `thread::sleep(10ms)` (test harness accept loop) | non-blocking `accept` idle poll / accept-error backoff, not a synchronization |

No `elapsed() <` assertion and no fixed-lifetime test child remain.

### CI reliability (ops #3)
After this change the tests no longer depend on kind/kubectl for correctness, but status still spawns them when they are on PATH. A slow kind now only costs runtime. A hung `kind get clusters`, which has no timeout (commands.rs:1940), is bounded by nextest's 300s terminate, so it fails loudly with the test named. The Proof section records `cargo test -p devloop-helper` wall time on the start commit vs the new tree with the normal PATH. The holds used to last 3–30s each; they now end on release or cancel.

### Not changed (notes, not deferrals)
- **Untargeted cancel**: it is interactive only, `dev-cluster` prints the actual `CANCEL_TARGET`, and it is a different mechanism.
- **Status still execs the host's real `kind`/`kubectl`/`<runtime> inspect` in the tests that call status.** These are `test_status_*`, `test_cancel_pending_*`, `test_status_audit_drift_invariant*`, and `test_status_cluster_write_in_progress_only_for_cluster_writes`. No outcome depends on their latency or presence anymore (code-reviewer: no probe seam). Results can still depend on host state (a real `devloop-test` cluster) only in fields these tests do not assert.
- **`kind get clusters` has no timeout in prod** (ops #5): @operations is checking whether it is tracked. It is not this class.

### Proof
A one-off experiment, recorded here, with the same command on the start commit and on the new tree, at default test threads. Put a fake slow `kind`/`kubectl`/`podman`/`docker` (`sleep 15`) on PATH. Each fake appends a marker line, and the record shows the markers and that each status call took ≥15s. Fake kind prints nothing, so kubectl never runs, which is expected. Command: `PATH=<slowbin>:$PATH cargo test -p devloop-helper concurrency_tests`. Expect the named tests and the `status_does_not_block` sibling to go red. Run the same command on the new tree and expect green. After the change, the type system carries the guard: a held test write has no duration parameter.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (rev 2) |
| Test (paired-test) | confirmed (rev 2) |
| Observability | confirmed (rev 2) |
| Code Quality | confirmed (rev 2) |
| DRY | confirmed (rev 2) |
| Operations | confirmed (rev 3) |

### Lead rulings during planning (2026-11-21)
1. Production sibling (cmd_status t0-only `cluster_write_in_progress`) is IN SCOPE: t0||t1 fold, a unit test on the pure fold, and the rationale in a comment at the fold. Same mechanism; layer7 `__cluster_ready` and the self-heal gate on it.
2. The residual 2s `CANCEL_GRACEFUL_TIMEOUT` dependence in the cancel_pending tests is in the class (uncontrolled accept latency vs a fixed duration). Make the grace a test-settable ctx/config value if that can be done without a production-reachable knob; only the SIGKILL-escalation test keeps a short grace.
3. Wall-clock upper bounds on cancel latency stay only where each pins a production contract with a load-safe margin; agreed with paired-test and recorded.
4. Untargeted cancel: out of scope (human-time TOCTOU, different mechanism); a note, not a deferral.
5. Proof: slow-PATH experiment on the start commit and the new tree, with commands and results recorded.
6. dry-reviewer/paired-test/code-reviewer: the 150–200ms trap/fork sleeps are the same class (uncontrolled shell startup vs a fixed duration); replaced by a ready file written by the innermost process.
7. security item 4: a cluster-write generation counter, not a documented "writes take minutes" limit. Fold = in_progress(t0) || in_progress(t1) || gen(t0) != gen(t1).
8. Rev 2 sent back for operations' two asks: the hold loop also ends when the test process is gone, and a runbook row for the new `cluster_write_in_progress` semantics.

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Test writes: lifetime owned by the test
| Item | Before | After |
|------|--------|-------|
| cfg(test) write variants | 4 × `TestSleep*{seconds}`: wall-clock `sleep N`, or a CPU-count busy-loop | 1 × `TestHold{latch, ready, shape}` + `HoldShape` (4 shapes). No duration can be expressed. |
| Stub fns | 4 near-identical `cmd_test_sleep*` | 1 `commands::hold_command(shape, latch, ready, owner_pid)` |
| Hold end conditions | the clock | cancel, latch release, or owner pid gone (`kill -0 "$3"`) |
| Readiness | 150/200ms sleeps | the innermost holder writes its pid to `ready` (after the trap / inside the forked subshell) |
| Harness | per-test 50/100×20ms loops that silently fell through; `let _ =` cancels | `commands::poll_until` + `WAIT_DEADLINE` (crate-level cfg(test), used by both test modules); `HeldWrite` (spawn = claim + ready; `release`; `join` via mpsc `recv_timeout`, release-then-panic; `Drop` releases); every relied-on cancel asserted |
| Audit-flush / accept sleeps | 100ms × 6, 5ms × 32 | removed: EOF ordering (doc'd on `read_audit_log`); `poll_until(conn_count == 32)` |
| Elapsed asserts | `<1s`, `<3/5/7s` | none: state/event per the agreed mapping |
| Cancel grace | hard-coded `CANCEL_GRACEFUL_TIMEOUT` in `run_command_streaming` | `Context.cancel_grace` → `CancelSignal.grace`. Production sets it from the const (no flag/env). Tests: `NEVER_ESCALATE` (Duration::MAX, test-only) or the production const. |
| Test Contexts | 4 literals (main.rs `build_test_context`, commands.rs ×2 + `test_ctx`) | 1 builder: `commands::test_context(dir)` / `test_context_with_grace(dir, grace)` |
| Wire pin | hand-typed list of 4 names | `is_wire_verb` (exhaustive over `HelperCommand`, no `_`) + `test_only_commands()` + `HoldShape` and `HoldShape::ALL` generated from ONE macro list, so a shape cannot miss `ALL` |

### Production: `cluster_write_in_progress` covers the status probe window
- `WriteState.cluster_write_gen` is bumped (under the claim lock) by the factored `claim_slot` for cluster writes.
- `cmd_status` brackets its slow reads (`probe_cluster_health`: kind/kubectl/inspect/TCP) with `observe_cluster_writes_around`. That fn folds through `cluster_write_during_probe(t0, t1) = t0.in_progress || t1.in_progress || t0.gen != t1.gen`.
- `busy`, `in_flight` and `cancel_pending` stay the t0 snapshot.
- The doc comments state that `busy:false` + `true` is deliberate.
- The wire shape is unchanged; the gen counter is internal.

### Additional Changes
- `docs/runbooks/devloop-validation.md` row :724: the whole-call semantics plus "re-run `dev-cluster status`".
- `docs/TODO.md`: K1–K20 → K1–K26; the `wait_for_busy` entry is marked DONE.
- protocol.rs doc: the "none takes an argument" sentence now carves out the cfg(test) variant.
- New tests:
  - `cluster_write_during_probe_truth_table`
  - `claim_slot_bumps_gen_only_for_cluster_writes`
  - `observe_cluster_writes_around_sees_a_write_inside_the_probe` (wiring; probe claims+releases a deploy / starts a provision)
  - `hold_command_exits_when_owner_is_gone` (all 4 shapes; ready carries the innermost holder's pid)
  - `test_only_commands_are_unreachable_from_the_wire`
  - `test_hold_shape_all_is_complete`

---

## Files Modified

```
 crates/devloop-helper/src/commands.rs | 612 ++++++++++++------
 crates/devloop-helper/src/main.rs     | 1128 +++++++++++++--------------------
 crates/devloop-helper/src/protocol.rs |  196 ++++--
 docs/TODO.md                          |    4 +-
 docs/runbooks/devloop-validation.md   |    2 +-
```

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree, 2026-11-21. Result: `TOTAL_DURATION=2560 TOTAL_RESULT=OK`, exit 0. Verdict: `SCHEMA=gate2-verdict/v2 GATE2=PASS REASON=none LAYER_ALL_EXIT=0 SIGNAL=none MISSING=`.

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 cargo check | OK | 7s | |
| 2 fmt | OK | 8s | APPLY mode; no rewrites |
| 3 guards + script self-tests | OK | 128s | `WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=129 BUDGET=90`, pre-existing (setup.test.sh; docs/TODO.md) |
| 4 unit tests | OK | 151s | includes devloop-helper 189/189 |
| 5 integration | OK | 1s | |
| 6 clippy | OK | 1s | |
| 7 env-tests | OK | 2264s | dev-cluster rebuild + full env-test run with the rebuilt helper |

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | seam cfg(test), exhaustive is_wire_verb; HoldShape::ALL completeness note fixed anyway (Lead) |
| Test (paired-test) | RESOLVED-FIXED | 4 | 4 | 0 | F1 LONG_GRACE raced status → NEVER_ESCALATE; F2 status_with_probe + JSON-level test; F3 deploy-cancel hang path → ReleaseOnDrop; F4 owner-gone leak → KillOnDrop; mutations M2a/M2b/M3/M4 red; 120s-hang + CPU-saturated 21/21; sweep matches |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | stale test-sleep in is_cluster_write_exact_set |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 | expect(clippy::panic); exact Cancelled Display; command(verb); shared poll kernel; stale comment |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | shared poll_until/WAIT_DEADLINE; test_context defaults |
| Operations | CLEAR | 0 | 0 | 0 | filed `kind get clusters` no-timeout in TODO (separate class) |

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

- **Finding 1 (fixed)**: `is_cluster_write_exact_set` still listed the deleted `test-sleep` op, so the `test-hold*` ops that tests assert `cluster_write_in_progress:false` against were not checked against the cluster-write list. The negative set now iterates `HoldShape::ALL` → `op_name()`. The Busy fixtures in `error.rs`/`protocol.rs` were also renamed to `test-hold`.
- **Sync-contract check**: tests synchronize on in-process slot state and the latch/ready files, not on log lines. Audit reads rely on a real happens-before: `handle_connection` (and the cap-reject path in both the production and test accept loops) appends synchronously before dropping the stream, and `request()` reads to EOF. The 100ms pre-read sleeps are removed and the ordering is documented on `read_audit_log`.
- **Production surface**: no eprintln, audit-entry shape or status wire field changed. `cluster_write_gen` is internal and never serialized. `cluster_write_in_progress` now spans the probe window (t0 || t1 || gen changed), and the comments on the fold and on `cmd_status` state the two time bases and that `busy:false` with `cluster_write_in_progress:true` is deliberate.

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `{the whole commit | <unit + safe direction>}` (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

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
