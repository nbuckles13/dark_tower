# Devloop Output: Helper Concurrency — Concurrent Reads + Serialized Writes + Cancel

**Date**: 2026-05-03
**Task**: Replace single-client-at-a-time helper semantics with concurrent reads, serialized writes, and explicit cancel. Surfaced 2026-05-03 during R-35 image validation when `dev-cluster status` hung indefinitely while `setup`/`rebuild` was in flight.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/devloop-helper-fix`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e46bf96b112de9d27f48d6278812348dec5764ea` |
| Branch | `feature/devloop-helper-fix` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `review` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `plan-confirmed` |
| Test | `plan-confirmed` |
| Observability | `plan-confirmed` |
| Code Quality | `plan-confirmed` |
| DRY | `plan-confirmed` |
| Operations | `plan-confirmed` |

**Iteration 2 start commit**: `fa00bb790e61bd558f5dea97bda888a044cd684c` (after the iter-1 follow-up `Helper status + cancel display fixes from manual host-side validation`).

---

## Task Overview

### Objective
Rework the devloop helper's IPC concurrency model so that:

1. **Read-only commands** (`status`, `ports`, `version`) ALWAYS run concurrently with whatever holds the write lock. The `status` response includes `busy: true` plus the in-progress command kind/args, so callers can surface "helper busy with `<op>`".
2. **Write commands** (`setup`, `deploy`, `rebuild`, `rebuild-all`, `teardown`) acquire a per-helper write mutex. If a write arrives while another write is in progress, reject with a typed `busy` error naming the in-flight op; client surfaces `helper busy with <op>; run dev-cluster cancel to abort it.`
3. **New `cancel` command**: SIGTERM the in-flight write handler, let it return cancelled, release the write mutex. Idempotent — `cancel` when no write is running returns `no-op` success.

### Scope
- **Service(s)**: `crates/devloop-helper/` (helper binary), `infra/devloop/dev-cluster` (client CLI)
- **Schema**: No
- **Cross-cutting**: Wire-additive only — busy-hint optional field, new error code, new `cancel` command. Backward-compatible with older clients.
- **ADR Updates**: ADR-0030 §"Helper API" currently states "one-client-at-a-time semantics are sufficient" — must update to reflect the new model.

### Debate Decision
NOT NEEDED — implementation of an existing, narrowly-scoped concurrency contract owned by infrastructure. Wire-additive, no cross-service consequences. ADR-0030 update is a minor revision recording the new model, not a new architectural decision.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. Most rows are "Mine" — helper-internal infrastructure code. Cross-boundary rows are flagged as **Minor-judgment** with @observability + @operations as co-owners on the runbook-adjacent ADR section.

The table below is iter-2's scope (process-group cancel + `cancel_pending`). Iter-1's plan rows that iter-2 did not need to touch (`error.rs`, `logging.rs`, `docs/TODO.md`) are intentionally dropped — that work shipped in the iter-1 commits and is out of iter-2's edit surface.

| Path | Classification | Owner (if not mine) | Notes |
|------|----------------|---------------------|-------|
| `crates/devloop-helper/src/commands.rs` | Mine | — | Add `Command::process_group(0)` on writer spawns + `signal_process_group(child, sig)` helper used at four pgid-kill sites (cancel SIGTERM, cancel SIGKILL escalation, broken-pipe SIGKILL, process-shutdown SIGKILL); rewrite `CANCEL_GRACEFUL_TIMEOUT` doc + cancel-vs-shutdown comment block to correct iter-1's containerd/kubelet/etcd claim; extend `BusyHint` with `cancel_pending: bool` populated under the same `write_state` lock; add `cmd_test_sleep_with_child` and `cmd_test_sleep_with_child_ignoring_term` cfg-test stubs. |
| `crates/devloop-helper/src/protocol.rs` | Mine | — | Add `#[cfg(test)] HelperCommand::TestSleepWithChild { seconds }` and `TestSleepWithChildIgnoringTerm { seconds }` variants (no `Request::parse_command` arms); extend `name`, `args_for_log`, `is_write`, `Display` arms; rename + parameterize `test_release_does_not_expose_test_sleep` over all four cfg-test command names. |
| `crates/devloop-helper/src/main.rs` | Mine | — | Add five iter-2 tests under `concurrency_tests`: `test_cancel_kills_grandchild_holding_pipes`, `test_cancel_kills_grandchild_with_sigkill_escalation`, `test_cancel_pending_visible_in_status`, `test_cancel_pending_false_when_busy_not_cancelling`, `test_cancel_pending_false_when_idle`. |
| `infra/devloop/dev-cluster` | Mine | — | Status banner reads top-level `data.cancel_pending` and renders `[busy, cancelling] write in flight: <op>` plus the active-voice hint line "Cancel sent; the in-flight write should exit within ~2s (SIGTERM) or ~4s (escalates to SIGKILL of the process group)." when true; existing `[busy]` rendering retained when false. |
| `docs/decisions/adr-0030-host-side-cluster-helper.md` | **Minor-judgment** | @observability + @operations | §"Helper Process" cancel paragraph rewrote with process-group + correct-child-tree story; §"Helper API" §3 Cancel semantics rewrote with the process_group + 2s-window-is-for-cooperative-shell-tree clarification + cancel_pending observability sentence + explicit "no per-command cancel policy; cancelling teardown follows the same partial-state posture as cancelling setup" recovery story (operations + security ACK on file). |
| `docs/TODO.md` | Mine | — | Append `wait_for_busy` extraction-opportunity entry under §"Cross-Service Duplication" / §"Developer Experience" per ADR-0019 DRY-reviewer exception (surfaced by @dry-reviewer iter-2 verdict on the 6-site busy-wait pattern in `concurrency_tests`). Documentation append; not a fix-or-defer finding. (Note: now auto-exempted by validate-cross-boundary-scope as of this iter — see row below — so listing here is informational only.) |
| `scripts/guards/simple/validate-cross-boundary-scope.sh` | Mine | — | Add `docs/TODO.md` to the auto-exclusion regex, parallel to the existing `docs/specialist-knowledge/**/INDEX.md` exemption. Rationale: ADR-0019 + the review protocol document `docs/TODO.md` as the explicit append target for any reviewer's deferrals/spin-outs/extraction opportunities; listing it in every devloop's classification table is ceremony for an expected, reviewer-authored append pattern. Surfaced when iter-2's @dry-reviewer extraction write triggered the guard. |
| `crates/devloop-helper/src/error.rs` | Mine | - | Add errors related to helper being busy and for cancelled tasks |
| `crates/devloop-helper/src/logging.rs` | Mine | - | Add constants for outcome logging |

No GSA paths involved. main.md (this file) is auto-excluded by Layer A's self-reference rule. Tests live in `crates/devloop-helper/src/main.rs` `#[cfg(test)]` (helper-internal integration tests using cfg-gated stub commands).

---

## Planning

### Current Model (Single-Client-at-a-Time)

`main.rs:run()` runs a serial accept loop:

```
loop {
    accept() → handle_connection() → execute() (blocks for entire setup/rebuild) → close
}
```

A `status` request issued while `setup` is in-flight blocks at `accept()` until setup finishes, which can be 7+ minutes — this is the bug.

The process-wide `shutdown: Arc<AtomicBool>` is set by SIGTERM/SIGINT and checked by `run_command_streaming()` to kill the child process. There is no per-connection cancellation today.

### New Model

Three concurrency primitives, all in `commands::Context` (or wrapping it):

1. **`accept_loop` is concurrent**: each accepted stream is handed to a fresh `std::thread::spawn`. The auth token, log handle, and Context are shared across threads (Context becomes `Arc<Context>`; AuditLog already has interior `&self` semantics; token can be cloned `String` or `Arc<str>`).

2. **`WriteState` (new in `commands.rs`)**: shared state guarded by a `std::sync::Mutex`. Per Security S3 option (a) — see §"Cancel Mechanics" below — `InFlightOp` does NOT carry a child PID; the writer thread alone holds `Child` and is the sole signaller, eliminating the PID-recycle TOCTOU.
   ```rust
   struct WriteState {
       in_flight: Option<InFlightOp>,
   }
   struct InFlightOp {
       op: String,            // command kind name, e.g. "setup"
       args: Vec<String>,     // args_for_log()
       started_at: String,    // RFC3339
       cancel_token: Arc<AtomicBool>,  // dedicated to this write, NOT process shutdown
       // No `child_pid` field — Security S3 option (a). The PID stays on the
       // writer's stack inside run_command_streaming; signals are issued from
       // there before any wait() call, so the kernel cannot recycle the PID.
       // Do NOT add a child_pid field here; that pattern was the racy sketch
       // superseded by §"Cancel Mechanics".
   }
   ```
   The mutex guards the `Option`, NOT the running command — `tryLock` decides whether a new write may proceed; the running write does not hold the lock for its full duration. Instead it holds an exclusive `try_lock`-guarded slot: it inserts itself as `Some(InFlightOp)` at start, swaps back to `None` at end. A second write that finds `Some(...)` returns `HelperError::Busy { op, args }` immediately without touching the mutex during execution.

   Pseudocode (poison-safe per ADR-0002 — see §"Mutex Semantics + Slot Lifecycle" below for the canonical helper):
   ```rust
   // Try to claim the write slot
   {
     let mut guard = state.write.lock().unwrap_or_else(|e| e.into_inner());
     if let Some(in_flight) = guard.as_ref() {
       return Err(HelperError::Busy { op: in_flight.op.clone(), args: in_flight.args.clone() });
     }
     *guard = Some(InFlightOp { ... });
   }
   // Drop the lock before running the command — readers and `cancel` can observe state
   let result = run_write(...);
   // Clear the slot
   { let mut guard = state.write.lock().unwrap_or_else(|e| e.into_inner()); *guard = None; }
   ```

3. **Per-write cancel token**: `Arc<AtomicBool>` distinct from process-wide `shutdown`. `run_command_streaming` is extended to take a `cancel: &AtomicBool` (renamed from `shutdown`) — practically, we pass `cancel_or_shutdown` which is OR-ed together. Cleanest implementation: `run_command_streaming` takes `&AtomicBool` for cancel + the existing `&AtomicBool` for shutdown, or we replace its signature with a `&[&AtomicBool]` slice. **Decision**: pass a `CancelSignal` thin wrapper that holds two `Arc<AtomicBool>` (process shutdown + per-write cancel) and exposes `is_cancelled() -> bool`. Minimal, explicit, no functional regression.

### Command Classification

Add `HelperCommand::is_write(&self) -> bool` in `protocol.rs`:
- **Reads**: `Status`. (`ports`/`version` are mentioned in the spec — `ports` is effectively `status` data, `version` would be new. Decision: `Status` is the only read for now; defer adding `Ports`/`Version` since they are out-of-scope. The classification function will dispatch correctly when they are added later.)
- **Writes**: `Setup`, `Rebuild`, `RebuildAll`, `Deploy`, `Teardown`.
- **Special**: `Cancel` is neither a read nor a write — it is a **control** command. It does not acquire the write lock; instead it inspects the lock and sends SIGTERM to the in-flight handler (sets the cancel token + signals the child PID). It MUST be allowed to run concurrently with the write it is cancelling. Idempotent: when no write is in flight, returns success with `result_data = {"cancelled": false, "reason": "no-op"}`.

### Wire-Additive Protocol Changes

`protocol.rs`:

```rust
pub enum HelperCommand {
    // existing variants…
    Cancel,  // NEW
}

// Status data (returned via `data` field on the existing Response/CommandResult)
// is augmented with a busy hint:
{
  "cluster_exists": …,
  "pods_healthy": …,
  // NEW:
  "busy": true,
  "in_flight": { "op": "setup", "args": ["--skip-observability"], "started_at": "2026-05-03T…" }
}
// When idle:
{
  …,
  "busy": false,
  "in_flight": null
}
```

`error.rs` adds:

```rust
HelperError::Busy { op: String, args: Vec<String> }   // kind() => "busy"
HelperError::Cancelled                                 // kind() => "cancelled"
```

`Response::err` already serializes `error_kind` — for `Busy` we ALSO want the structured op/args so the client can render `helper busy with <op>`. Two options:
- (A) Encode op/args into `data` field of the error response (and keep the human message in `message`).
- (B) Add a new optional field to `Response`.

**Decision**: (A). The `data: Option<serde_json::Value>` field already exists on `Response`. Set `data: Some({"op":..., "args":...})` whenever `error_kind == "busy"`. Older clients ignore unknown fields; new clients parse `data.op`. Zero new wire fields.

### Mutex Semantics + Slot Lifecycle

Pseudocode for the writer's claim/release lifecycle (matches Security S3 mitigation; no shared `child_pid`):

```rust
struct WriteSlotGuard<'a> { state: &'a Mutex<WriteState> }
impl Drop for WriteSlotGuard<'_> {
    fn drop(&mut self) {
        // Clear the slot on ANY exit path (success, error, cancellation, panic).
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.in_flight = None;
    }
}

fn run_write(cmd, ctx, writer) -> Result<..., HelperError> {
    // 1. Try to claim the slot.
    let cancel_token = Arc::new(AtomicBool::new(false));
    {
        let mut g = ctx.write_state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref existing) = g.in_flight {
            return Err(HelperError::Busy {
                op: existing.op.clone(),
                args: existing.args.clone(),
            });
        }
        g.in_flight = Some(InFlightOp {
            op: cmd.name().to_string(),
            args: cmd.args_for_log(),
            started_at: now_rfc3339(),
            cancel_token: Arc::clone(&cancel_token),
        });
    }
    // 2. RAII guard releases the slot on any exit.
    let _guard = WriteSlotGuard { state: &ctx.write_state };

    // 3. Run, plumbing the cancel token into run_command_streaming.
    let signal = CancelSignal::new(Arc::clone(&ctx.shutdown), cancel_token);
    actually_dispatch_write(cmd, ctx, writer, &signal)
        // 4. Map child-killed-due-to-cancel to HelperError::Cancelled.
        .map_err(|e| if signal.cancel_set() { HelperError::Cancelled } else { e })
}
```

Cancel never touches the slot — only the writer (via `WriteSlotGuard::drop`) clears it. This guarantees no race between cancel-clears-slot and writer-clears-slot.

### Cancel Mechanics

**Revised per Security S3 — option (a): NO shared `child_pid` Arc.** The PID-recycle race goes away if cancel never touches a raw PID. `cmd_cancel` only flips a per-write `cancel_token: Arc<AtomicBool>`; the writer thread (which still owns the live `Child` handle, so the kernel cannot recycle the PID until it `wait()`s) does ALL signal sending from inside `run_command_streaming`. `InFlightOp` therefore stores ONLY:

```rust
struct InFlightOp {
    op: String,                    // "setup", "rebuild", etc.
    args: Vec<String>,             // args_for_log()
    started_at: String,            // RFC3339 ms
    cancel_token: Arc<AtomicBool>, // flipped by cmd_cancel; observed by run_command_streaming
}
```

Note: no `child_pid` field — that closes the TOCTOU. The writer thread holds `Child` on its own stack/inside `run_command_streaming`'s frame; signals are issued via `libc::kill(child.id() as libc::pid_t, ...)` from within that same frame, before any `wait()` call.

`cmd_cancel(ctx)`:
1. Lock `state.write`. If `None`: drop the lock, audit-log `cmd: "cancel", outcome: "no-op"`, return `Ok(Some(json!({"cancelled": false, "reason": "no-op"})))`.
2. If `Some(in_flight)`: clone `op`, `args`, `started_at`, `cancel_token: Arc<AtomicBool>`. Drop the outer lock immediately — cancel never lock-steals.
3. `cancel_token.store(true, Ordering::SeqCst)`.
4. Audit-log `cmd: "cancel", outcome: "completed", error: "<op>"` (the op being cancelled).
5. Return `Ok(Some(json!({"cancelled": true, "op": <op>, "started_at": <ts>})))` immediately. Cancel does NOT block on the write actually finishing.

In `run_command_streaming` (the only place that holds the `Child`):
- When `cancel.is_cancelled()` flips OR process `shutdown.is_set()` flips, send SIGTERM via `libc::kill(child.id() as libc::pid_t, libc::SIGTERM)`. Record `cancel_started_at = Instant::now()`.
- Continue draining output for up to `CANCEL_GRACEFUL_TIMEOUT = Duration::from_secs(2)`. If `child.try_wait()` returns `Some(_)`, child exited — proceed to release path.
- If 2s elapses and child still alive, escalate to `child.kill()` (SIGKILL).
- Return `Err(HelperError::Cancelled)` when the cancel token was set; return the existing `command_failed` error when only `shutdown` was set (preserves current SIGTERM-shutdown-of-helper behavior — process going down, no graceful escalation needed).

This implements Security S3-(a) cleanly: no raw PID ever leaves the owning thread; SIGTERM/SIGKILL come from inside the function that holds `Child`; PID-recycle is impossible because the parent hasn't `wait()`ed yet.

The cancelled write's handler thread completes, returns `HelperError::Cancelled` from `execute()`, and clears the write slot via the RAII guard described in the Mutex Semantics section.

### Threading Model

Each connection gets its own thread. Per Operations Gate 1 + Security S4, a hard cap of **32 concurrent connection-handler threads** is enforced via `Arc<AtomicUsize>` counter; the increment happens BEFORE `thread::spawn`, the decrement happens via a `ConnGuard` RAII type owned by the spawned closure (so a panic in the handler still releases the slot). On overflow:

- The accept loop closes the socket immediately without writing a response.
- A single audit-log entry with `cmd: "rejected", error: "connection_cap_reached", outcome: "rejected"` is written, with simple per-second deduplication: track the last-rejected timestamp in an `AtomicI64`; only log if the new rejection is more than 1s after the last logged one. Avoids audit-log spam if a wedged client retry-loops at high rate, while still surfacing the event to operators.

`AuditLog`: already safe — `append` opens the file fresh each call. Multiple threads writing append-mode is OS-level atomic for small writes; entry size is well under PIPE_BUF on Linux.

`Context`: becomes `Arc<Context>` and contains an `Arc<Mutex<WriteState>>`. The existing `shutdown: Arc<AtomicBool>` stays for process-wide SIGTERM.

`run_command_streaming` signature change: takes a `&CancelSignal` instead of `&AtomicBool`. CancelSignal wraps both process shutdown AND a per-write cancel token. Backward compat for tests: provide `CancelSignal::shutdown_only(Arc<AtomicBool>)` for callers that don't have a write-specific token (none in production after this change, but tests construct one).

### `dev-cluster` Client Changes

1. **`cancel` subcommand**: parses to `{"command":"cancel", ...}`. No service arg. Streams response like other commands. Post-result handling — when the helper's `data.cancelled` is `false`, the client prints `No write in flight; nothing to cancel.` and exits 0; when `true`, it prints `Sent cancel to in-flight write: <op>` and exits 0. No silent zero-exit.
2. **`busy` error handling**: when `error_kind == "busy"`, the client extracts `data.op` and `data.args` and prints:
   ```
   ERROR: helper busy with setup --skip-observability; run 'dev-cluster cancel' to abort it. (busy)
   ```
   Then exits 1.
3. **Status output**: when status data has `busy: true`, prepend a one-line banner:
   ```
   === Helper busy: setup --skip-observability (started 2026-05-03T…) ===
   ```
   Existing health summary still renders below.

### ADR-0030 Update

Two sections of `docs/decisions/adr-0030-host-side-cluster-helper.md` are revised:

**§"Helper Process" (around line 59)** — drop the sentence "No async runtime needed — one-client-at-a-time semantics are sufficient." Replace with:
> Concurrency: each connection is handled in its own OS thread, capped at 32 concurrent handlers. Read-only commands run concurrently with any in-flight write; write commands serialize on a per-helper write mutex; the `cancel` command interrupts an in-flight write at any time. The helper remains synchronous (`std::sync::Mutex` + `std::thread::spawn` + `libc::kill`); no async runtime dependency.

**§"Helper API" table (around line 92+)** — add a `cancel` row. Replace the existing single-paragraph framing with the four-part section requested by Operations Gate 1:

1. **Three-state semantics**
   - **Read commands** (`status`): always run concurrently with anything in flight; never blocked.
   - **Write commands** (`setup`, `deploy`, `rebuild`, `rebuild-all`, `teardown`): serialize on a per-helper write mutex. A second concurrent write is rejected with a typed busy error.
   - **Control commands** (`cancel`): not bound by the write mutex; signals the in-flight write to abort.

2. **Busy-hint shape**
   - On `error_kind == "busy"`: the response carries `data: { "op": "<command>", "args": ["<arg>", ...] }` so the client can render `helper busy with <op> <args>`.
   - On the `status` response: `data` includes `busy: bool` and `in_flight: { op, args, started_at } | null`. Snapshot taken at request time; clients should treat as "as of `started_at`".

3. **Error codes added**
   - `"busy"` — write-while-write rejection.
   - `"cancelled"` — in-flight write that exited because a `cancel` arrived.

4. **Cancel idempotency + return shape**
   - `cancel` always returns `success: true`. Its `data` field is `{ "cancelled": true, "op": "<name>", "started_at": "<ts>" }` when something was cancelled, or `{ "cancelled": false, "reason": "no-op" }` when nothing was in flight. Calling `cancel` repeatedly with no in-flight write is safe (always returns no-op).

**§"Audit logging" (around line 111)** — extend the existing sentence ("Every request logged to ... with timestamp, command, arguments, duration, exit code.") with a new sentence (per Obs O5):

> Cancel requests are logged with the cancelled op's kind and args (`args: ["target=<op>", "target_args=<args>"]`). Busy rejections are logged with `cmd: "rejected_busy"` and the full collision pair in `args` (`["rejected=<kind>", "rejected_args=<args>", "in_flight=<kind>", "in_flight_args=<args>"]`). Cancelled writes are logged with `error` field prefixed `"cancelled"` (suffix `" (sigterm timeout, escalated to sigkill)"` if SIGTERM grace expired).

This documents the audit-log surface contract so operators know exactly what to grep for. Owner: @observability + @operations (Minor-judgment).

### Tests

Add to `crates/devloop-helper/src/commands.rs` `#[cfg(test)] mod tests` (or a new sibling test module) using std-thread + `UnixListener` — same pattern as the existing socket roundtrip tests in `main.rs`. Final stub approach: introduce a test-only `HelperCommand::TestSleep { seconds: u64 }` behind `#[cfg(test)]` that runs `sleep N` via `run_command_streaming`, exercising the real concurrency code (write-lock claim, cancel token plumbing, child kill) end-to-end without a Kind cluster.

**Test stub constraints (per @test #1, #2, #7):**
- The `TestSleep` enum variant AND its `Request::parse_command` arm AND its dispatch arm in `execute()` are ALL `#[cfg(test)]`-gated. Production builds have no parse arm — a malicious socket client cannot invoke `test-sleep`. Verified by `test_release_does_not_expose_test_sleep` below.
- `TestSleep` is classified as a **write** in `HelperCommand::is_write()` so it claims the write lock — otherwise the busy/cancel paths it exists to test wouldn't be exercised.
- No tokio. Test-side concurrency uses `std::thread::spawn` + `Arc<Barrier>`. No additions to `[dev-dependencies]` beyond what's already there.

**Required tests (per task spec):**
- (a) `test_concurrent_status_during_long_write` — spawn `TestSleep { 10 }` write (per @test #8: 10s gives headroom on slow CI), immediately issue 5 parallel status requests synchronized by `Arc<Barrier::new(6)>`. Each individual status MUST return in `< 1s` (tight bound — catches accidental serialization regressions; the spec's 5s ceiling is the slow-CI fallback). All 5 succeed with `data.busy == true`, `data.in_flight.op == "test-sleep"`, `data.in_flight.args == ["10"]`, `data.in_flight.started_at` parses as RFC3339.
- (b) `test_write_while_write_returns_busy` — spawn `TestSleep { 5 }` write, immediately spawn another write (also `TestSleep`). Second returns `error_kind = "busy"`, `data.op == "test-sleep"`, `data.args == ["5"]`.
- (c) `test_cancel_mid_write_terminates_handler` — spawn `TestSleep { 30 }`, after 200ms send `cancel`. Cancel returns `cancelled: true` immediately. First write resolves with `error_kind = "cancelled"` within `3s` (SIGTERM-then-2s-SIGKILL escalation + slack — per @test #8 signoff).
- (d) `test_cancel_when_idle_is_noop` — `cancel` with no write running returns `success: true`, `data.cancelled == false`, `data.reason == "no-op"`.
- (e) (Layer 8) post-cancel fresh `setup` runs cleanly. Verified naturally by re-running the env-tests pipeline after a manual cancel during R-35-style validation.

**Additional tests (per security S1/S2/S6 + test #3-#5):**
- `test_release_does_not_expose_test_sleep` (Security S6 + Test #1) — construct `Request` from raw JSON `{"token":"<valid>", "command":"test-sleep"}` via `serde_json::from_str`, call `parse_command()`, assert `Err(HelperError::InvalidCommand("test-sleep"))`. Proves the wire surface stays clean even with `cfg(test)` on.
- `test_busy_rejection_is_audit_logged` (Security S1, updated for Obs option A) — trigger a busy rejection via socket; read `helper.log`, assert one entry has `cmd: "rejected_busy"`, `outcome: "rejected"`, `args` containing all four collision-pair keyed strings (`"rejected=test-sleep"`, `"rejected_args=..."`, `"in_flight=test-sleep"`, `"in_flight_args=..."`), `error` field absent (`None`).
- `test_cancel_completed_is_audit_logged` (Security S2) — assert `helper.log` contains `cmd: "cancel"`, `outcome: "completed"`, `error: "test-sleep"` (the cancelled op).
- `test_cancel_noop_is_audit_logged` (Security S2) — assert `helper.log` contains `cmd: "cancel"`, `outcome: "no-op"`, `error: "none"`.
- `test_cancel_after_write_completes_is_noop` (Test #3) — start `TestSleep { 1 }`, sleep 1.5s (write completes naturally and clears the slot), THEN send `cancel`. Cancel returns `cancelled: false`, `reason: "no-op"`. Validates the cancel-vs-natural-completion race resolves correctly: cancel sees the slot empty and reports no-op rather than racing the writer's slot-clear or producing a bogus `cancelled: true`.
- `test_repeated_cancel_when_idle` (Test #4) — send 5 cancels back-to-back, all return `cancelled: false`. No mutex deadlock, no panic. Cheap insurance.
- `test_status_when_idle_busy_false` (Test #5 + CR11) — `status` with no write in flight returns `data.busy == false` AND `data.in_flight == null` (LITERAL presence of both keys; no `skip_serializing_if` so the wire is deterministic per CR11). Pins the wire shape so a newer client can distinguish "old helper, field absent" from "new helper, idle" without ambiguity.
- `test_connection_cap_rejects_at_32` (Security S4) — spin up 33 long-poll connections; assert the 33rd is closed without a response AND `helper.log` contains exactly one `cmd: "rejected"`, `error: "connection_cap_reached"`, `outcome: "rejected"` entry.
- `test_cancelled_display_prefix` (Obs O3) — assert `HelperError::Cancelled { escalated: false }.to_string().starts_with("cancelled")` AND `HelperError::Cancelled { escalated: true }.to_string().starts_with("cancelled")`. Locks the prefix-grep contract.
- `test_status_audit_drift_invariant` (Obs O4) — extend test (a): after the `TestSleep { 10 }` write completes, read `helper.log`, find the entry with `cmd == "test-sleep"`, assert `args` matches what `status` reported as `data.in_flight.args` during the write. Locks the "status view ↔ audit log" agreement.
- `test_busy_error_includes_collision_pair` (Obs O1, updated for Obs option A) — strengthen `test_busy_rejection_is_audit_logged`: assert the audit entry's `args` contains both `rejected=<kind>` and `in_flight=<kind>` keyed strings so the collision pair is greppable as a single entry. The `error` field stays `None` for `rejected_busy` entries (option A puts collision identity in `args`, not `error`).
- `test_cancel_audit_args_name_target` (Obs O2) — strengthen `test_cancel_completed_is_audit_logged`: assert the audit entry's `args` contains `"target=test-sleep"` (or equivalent encoding) so `grep '"cmd":"cancel"' helper.log` answers "what got cancelled?" without joining against later entries.
- `test_sigkill_escalation_logged` (Obs O3 on Q3) — using a write stub that ignores SIGTERM (e.g., `/bin/sh -c 'trap "" TERM; sleep 30'`), spawn it, cancel after 200ms, wait for cancellation; assert the write's audit entry's `error` is the SIGKILL-escalated form (`"cancelled (sigterm timeout, escalated to sigkill)"`). Confirms operators can answer "did the child shut down cleanly or did we have to kill it?"
- `test_cancel_requires_auth_token` (CR9) — send `{"command":"cancel","token":"<wrong>"}` over the socket; assert `error_kind == "auth_failed"`. Confirms cancel is not exempt from the auth path.

**Implementation discipline (Test #6 — ADR-0002 no-panic in production):**

All new production code (write-state mutex, `cmd_cancel`, `WriteSlotGuard`, `ConnGuard`, `CancelSignal`) handles `PoisonError` via `.unwrap_or_else(|e| e.into_inner())` — never `.unwrap()` on a `Mutex::lock()`. Recovery semantics: a poisoned mutex means a previous holder panicked (vanishingly unlikely under workspace lints, but possible if `WriteSlotGuard::drop` fires during a panicking writer). The recovered inner state is the "last consistent" snapshot; the helper continues operating. Tests stay free to `.unwrap()` — that's idiomatic in `#[cfg(test)]` and not a production-path concern. Implementation Summary will explicitly call out poison-handling sites for Gate 3 verification.

### Open Questions — Resolved at Gate 1

1. **Test-only `HelperCommand::TestSleep` variant**: APPROVED (team-lead, operations, security). Gated behind `#[cfg(test)]`, never serialized over the wire — no `Request::parse_command` arm for it. **Mandatory regression test (per Security S6)**: `test_release_does_not_expose_test_sleep` constructs a `Request` from JSON `{"command":"test-sleep",...}` via `serde_json::from_str` then `parse_command()` and asserts the result is `Err(HelperError::InvalidCommand("test-sleep"))`. This proves the JSON parse path rejects "test-sleep" as an unknown command even with `#[cfg(test)]` enabled — the variant exists in the enum but has no parse arm.

2. **Max-concurrent-connections cap**: KEEP THE CAP at 32 (operations + security agree). Implementation: `Arc<AtomicUsize>` counter incremented BEFORE `thread::spawn`, decremented via `ConnGuard` RAII type owned by the spawned closure (panic-safe). Overflow path: close socket immediately, write a single rate-limited audit entry (`cmd: "rejected", error: "connection_cap_reached", outcome: "rejected"`, deduped to ≤1/sec via `AtomicI64` last-rejected timestamp).

3. **SIGTERM-then-SIGKILL on cancel** with `CANCEL_GRACEFUL_TIMEOUT = Duration::from_secs(2)`: APPROVED (team-lead, operations, security). Implemented inside `run_command_streaming` so the writer thread (which owns `Child`) is the only thing that signals the child — closes Security S3's PID-recycle TOCTOU. Process-wide shutdown path stays SIGKILL-immediate (unchanged). NO new cleanup logic — partial-Kind-cluster recovery relies entirely on the existing "cluster exists, reusing" branch in `cmd_setup` (commands.rs:237-243) and `infra/kind/scripts/setup.sh`. Test (e) verifies this path end-to-end. The constant lives near `MAX_LINE_LEN` for review visibility.

### Code-Reviewer Gate 1 Asks — Resolutions (CR7-CR13)

**CR7 — `state.write.lock().unwrap()` violates ADR-0002 no-panic**: FIXED. All `Mutex::lock()` callsites (in pseudocode and production code) use `.unwrap_or_else(|e| e.into_inner())`. Recovery is correct because the protected state is a plain `Option<InFlightOp>` with no torn invariants — a poisoned mutex means a previous holder panicked (vanishingly unlikely under workspace lints); recovering the inner state lets the helper continue. Helper macro/function: a small `lock_recovered<T>(m: &Mutex<T>) -> MutexGuard<'_, T>` is added to encapsulate the pattern at every callsite (DRY-friendly, single change point if we ever switch to `parking_lot::Mutex`). NOT taking the `parking_lot` dep — std + helper keeps the runtime-dep surface minimal.

**CR8 — `#[non_exhaustive]` on `HelperError`**: ADDED. `HelperError` enum gets `#[non_exhaustive]` so any future variant doesn't break downstream `match` sites. The `kind()` method's internal `match` is fine because it's the same module — but external matchers (none today inside this crate, but reviewers see the attribute and know the contract holds for future cross-crate use) must use `_` or recompile. Documented in §"Wire-Additive Protocol Changes" + the error.rs row of the classification table.

**CR9 — `Cancel` MUST go through `auth::validate_token`**: CONFIRMED. The flow is unchanged: `handle_connection` parses JSON → calls `auth::validate_token(&request.token, expected_token)` → THEN `request.parse_command()`. The `Cancel` variant is parsed AFTER auth, identical to every other command. No special-case fast path. Per Security S6's regression test pattern, I'll also add `test_cancel_requires_auth_token` that sends `{"command":"cancel"}` with a wrong token over the socket and asserts `error_kind == "auth_failed"`.

**CR10 — Max-concurrent-connections cap of 32**: KEEP IT. Already incorporated per @operations + @security. `Arc<AtomicUsize>` increment-at-spawn / decrement-via-`ConnGuard`-RAII / immediate-close-at-cap / ≤1-entry/sec rate-limited audit log. See §"Threading Model" + §"Open Questions" item 2.

**CR11 — Status `busy`/`in_flight` always emitted (no `skip_serializing_if`)**: ADJUSTED. The `data` JSON for status will literally include `"busy": false, "in_flight": null` when idle, so a newer client distinguishes "old helper, field absent" from "new helper, idle" deterministically. Client-side: `#[serde(default)]` on the deserializer for backward compat with old helpers (returns `false` / `None` when absent). Note: this contradicts my earlier line in §Tests that said either serialization is acceptable — overriding to "always emit". Updated `test_status_when_idle_busy_false` to assert literal presence of both keys (NOT absence).

**CR12 — SIGTERM-then-SIGKILL asymmetry documented**: WILL ADD CODE COMMENT. The asymmetry — cancel-token → SIGTERM-then-2s-then-SIGKILL; process-shutdown → SIGKILL-immediate — gets a `// IMPORTANT:` comment block in `run_command_streaming` explaining the rationale (cancel intends graceful tear-down of containerd children; shutdown is "we're going down, kind-recovery handles partial state"). Constant `CANCEL_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(2)` declared near `MAX_LINE_LEN` for review visibility (per Security S5).

**CR13 — `child.id() as pid_t` lossy cast**: FIXED. Use `i32::try_from(child.id()).map_err(|_| HelperError::CommandFailed { cmd: "kill".into(), detail: "PID exceeds i32".into() })?` then pass to `libc::kill`. Defensive; clippy will catch the lossy cast under workspace lints anyway. NOT taking the `nix` dep — single use site, `libc::kill` is fine.

**Code-Reviewer item 6 (one-line doc on `Response.data` dual role)**: ADDED. `Response.data` gets a `///` doc comment in protocol.rs reading: "Optional structured data. On success: command-specific result (e.g., port map on setup). On failure with `error_kind == \"busy\"`: a `{op, args}` object naming the in-flight write that blocked this request. Reviewers in 6 months: this is intentionally dual-role to keep the wire schema additive."

### Operations Gate 1 Asks — Resolutions

1. **Cancel UX (no-op case)** — `dev-cluster cancel` will print one of:
   - `=== cancel completed (exit 0, 0.0s) ===` followed by `No write in flight; nothing to cancel.` and exit 0, when the helper returns `data.cancelled == false`.
   - `=== cancel completed (exit 0, X.Xs) ===` followed by `Cancelled in-flight <op>; the operation will exit shortly.` and exit 0, when the helper returns `data.cancelled == true`.
   No silent zero-exit. (Surfaced in `### dev-cluster Client Changes` above.)

2. **ADR-0030 doc-update scope broadened** — the §"Helper API" rewrite will explicitly cover, in this order:
   1. **Three-state semantics** — read-anytime / write-mutex / cancel-anytime classification; the command table reflects the bucket each command belongs to.
   2. **Busy-hint shape** — the `data` field carries `{op, args}` on `error_kind="busy"` error responses, and `{busy: bool, in_flight: {op, args, started_at} | null}` on the `status` response's `data` field.
   3. **Error code shape** — new error kinds `"busy"` (write-while-write rejection) and `"cancelled"` (in-flight write that exited via cancel).
   4. **Cancel idempotency + return shape** — `cancel` always returns success; `data` contains `{cancelled: true, op, started_at}` when something was cancelled, `{cancelled: false, reason: "no-op"}` when idle.

3. **helper.log write-amplification** — addressed:
   - `cancel` and `status` ARE logged at the same verbosity as writes (one `log_command` entry per request), deliberately, for forensic visibility. Operations grep workflows for "what happened during the wedge?" require these entries.
   - `rejected_busy` entries are emitted at most once per rejected request — no per-poll multiplier because the rejection happens before any work and returns immediately. A poll storm of N busy requests produces N entries (1:1).
   - No log rotation today; entry size ~200 bytes JSON line. At sustained 100 status-polls/sec (extreme worst case) → ~20KB/sec → ~70MB/hour. Not a concern for normal devloop use (manual `dev-cluster status` invocations); IS a concern if devloop.sh ever adds a status poll loop. Action: add `docs/TODO.md` entry as known limitation with suggested mitigations (entry-rate cap, daily rotation, suppressed read-path logging). NOT blocking this PR per Operations.

4. **No retry-loop integration of `cancel`** — confirmed: `cancel` is human-driven only; nothing in `.claude/skills/devloop/SKILL.md` or its review-protocol invokes it. `Request::parse_command` arm for `cancel` is the only entry point.

### Audit-Event Schema Addendum (per team-lead + security + observability Gate 1 asks)

The audit log gains the following entries / fields. Schema lives in `logging.rs`; emission lives in `commands.rs` and `main.rs`. **Every** request and rejection MUST audit-log. Per Security S1/S2 + Operations + Observability O1-O5:

| Trigger | `cmd` | `args` | `error` field | `outcome` |
|---|---|---|---|---|
| Cancel arrives, write in flight | `"cancel"` | `["target=<op>", "target_args=<args>"]` (per Obs O2 — names what got cancelled, single-line greppable) | `"<in-flight op>"` | `"completed"` |
| Cancel arrives, no write in flight | `"cancel"` | `[]` | `"none"` | `"no-op"` |
| Write rejected because another write is in flight | `"rejected_busy"` | `["rejected=<kind>", "rejected_args=<args>", "in_flight=<kind>", "in_flight_args=<args>"]` (per Obs follow-up option **A** — both sides of the collision recorded as keyed strings; greppable via `grep '"cmd":"rejected_busy"' helper.log \| grep 'in_flight=setup'`) | `None` (`args` carries the full collision pair; `error` left empty) | `"rejected"` |
| Connection accepted but cap exceeded (32-thread limit) | `"rejected"` | `[]` | `"connection_cap_reached"` | `"rejected"` (rate-limited to ≤1 entry/sec) |
| Existing write completes normally | unchanged (`"setup"`/`"rebuild"`/etc.) | unchanged | unchanged | `"completed"` |
| Existing write fails normally | unchanged | unchanged | unchanged (existing `error` text) | `"error"` |
| Write returns `HelperError::Cancelled` (SIGTERM took it down within 2s) | unchanged | unchanged | `"cancelled by client request"` (per Obs O3 — must start with literal `"cancelled"` for prefix-grep) | `"cancelled"` |
| Write returns `HelperError::Cancelled` after SIGKILL escalation | unchanged | unchanged | `"cancelled (sigterm timeout, escalated to sigkill)"` (per Obs O3 + O3-on-Q3 — still prefix-`cancelled` so the same grep matches; suffix triages clean-vs-forced-shutdown) | `"cancelled"` |

The new `outcome: Option<&'a str>` field on `LogEntry` is `serde(skip_serializing_if = "Option::is_none")` so historical entries written by older binaries continue to parse cleanly. The two distinct `error` strings on `Cancelled` are produced by `HelperError::Cancelled`'s `Display` impl reading a `escalated: bool` flag set by `run_command_streaming` based on whether SIGKILL was needed — kept off the wire (the protocol-level `error_kind` stays `"cancelled"` regardless).

**Outcome value set locked** (per Obs follow-up): valid `outcome` values are `{"completed", "cancelled", "error", "rejected", "no-op"}` and ONLY these. Encoded as a `pub const OUTCOMES: &[&str] = &["completed", "cancelled", "error", "rejected", "no-op"]` in `logging.rs` so future emission sites can't typo a sixth value into existence. Unit test `test_outcome_round_trip` enumerates all five and asserts each round-trips through `LogEntry` serialization unchanged.

**`error` field cross-use comment** (per Obs follow-up): `logging.rs` near the `error` field definition gets a short doc comment: "`error` is also used by `cancel` events to carry the cancelled-op identifier — this is intentional cross-use to avoid schema growth. `rejected_busy` events instead encode the collision pair into `args` per Obs option A; their `error` is `None`."

**Cancelled-write `error` prefix invariant** (per Obs O3 option a): both Cancelled `Display` outputs MUST start with the literal string `"cancelled"`. A unit test `test_cancelled_display_prefix` asserts both `HelperError::Cancelled { escalated: false }.to_string().starts_with("cancelled")` and the `escalated: true` variant. This locks the greppability contract.

**Status/audit drift invariant test** (per Obs O4): in test (a), after the write completes, the test reads `helper.log` JSONL, finds the entry with `cmd == "test-sleep"`, and asserts `args` equals what `status` reported as `data.in_flight.args`. Locks the invariant "status's in-flight view and the audit log's record agree" against future drift.

@observability owns this taxonomy via the cross-boundary classification table and confirms before Gate 1 closes.

---

## Pre-Work

None.

---

## Human Review (Iteration 2)

**Feedback** (2026-05-04, after host-side manual validation):

> Cancel of an in-flight write currently takes ~60s instead of the designed ≤2s because `run_command_streaming` SIGTERM/SIGKILLs only the immediate child (setup.sh), but its grandchildren (`kubectl wait`, `kubectl apply`, etc.) inherit stdout/stderr and keep the pipes open after their parent dies. The reader-thread drain loop blocks on `read()` until the orphan grandchild closes its end naturally.
>
> **Two changes in scope:**
>
> **(a) Process-group cancel mechanics** — spawn write children in their own process group via `Command::process_group(0)` and signal `-pgid` (`libc::kill(-pgid, ...)`) on cancel so SIGTERM/SIGKILL reaches the whole tree. Maintains the Security S3-(a) shape: signaller stays on the writer thread's stack, child still owned, no PID/PGID crosses thread boundaries, called before any `wait()`. Update `CANCEL_GRACEFUL_TIMEOUT` comment block in `commands.rs` and ADR-0030 §"Helper API" "Cancel semantics" to correct the false claim about containerd/kubelet/etcd being immediate children of `setup.sh` — they're detached Kind components, not `setup.sh` descendants, and process-group signals don't (and shouldn't) reach them.
>
> **(b) `cancel_pending` in status response** — when `busy=true` and the in-flight `cancel_token` is set (during the SIGTERM grace window or any drain delay), include `cancel_pending: true` in the status `data`. dev-cluster client renders `[busy, cancelling] write in flight: ...` above the health summary so the user knows cancel was received and is pending.
>
> **Test additions:**
> - Process-group cancel: `TestSleepWithChild` stub spawning `bash -c 'sleep 30 & wait'` so a grandchild holds stdout/stderr; assert cancel completes within e.g. 5s (vs current 30s+).
> - `cancel_pending`: assert status during SIGTERM-grace window shows `cancel_pending=true`; assert it returns to false after the slot is released.
>
> **Edge case for security/operations to weigh at Gate 1**: `kind delete cluster` during teardown. With process-group SIGKILL, cancelling a teardown mid-flight orphans containerd/etcd. Probable answer: "cancelling teardown is the user's problem to recover from, same as cancelling setup" — but worth explicit confirmation. If the answer is "don't process-group teardown", we'd need a per-command policy.

The original "graceful teardown" framing in the iter-1 plan (CANCEL_GRACEFUL_TIMEOUT comment block citing kind/containerd/kubelet/etcd) was based on an incorrect mental model: `kind create cluster` does NOT keep those as immediate children — it returns once the cluster is bootstrapped, leaving them as detached Kind components managed by Kubernetes. Process-group signals never reached them in iter-1 either; the iter-1 design was pretending to deliver a cancel feature that didn't actually work as advertised.

---

## Implementation Summary

Reworked the devloop helper IPC concurrency model so read-only commands (`status`, `ports`, `version`) are never blocked by an in-flight write, and so `setup`/`deploy`/`rebuild`/`rebuild-all`/`teardown` serialize on a single per-helper write slot. Added a `cancel` command that flips a per-write `Arc<AtomicBool>` cancel token; the writer thread observes it, sends `SIGTERM` to its child, waits 2 s, then escalates to `SIGKILL`. Cancel is idempotent (no-op when idle, audit-logged with `outcome="no-op"`).

Switched the accept loop from one-client-at-a-time to thread-per-connection bounded at 32 concurrent connections (rejection rate-limited to one audit entry per second to prevent log spam under burst). Adopted `std::sync::Mutex` with a `lock_recovered` poison-recovery helper, RAII guards (`WriteSlotGuard`, `ConnGuard`) for panic-safe cleanup, and Security S3 option (a) — the writer thread is the sole signaller of its child, holding the `Child` on its own stack so PID-recycle TOCTOU is structurally impossible.

Wire schema is additive only: new `Cancel` command, new `Busy` and `Cancelled` `HelperError` variants, new `error_kind` field on `CommandResult`, new `outcome` field on JSONL audit entries, and `Response::err` reuses the existing `data` field to carry the `{op, args}` collision pair on busy rejections. `HelperError` is `#[non_exhaustive]` for forward-compat.

Audit log gains a locked `outcome` vocabulary (`completed`, `cancelled`, `error`, `rejected`, `no-op`); `rejected_busy` events encode the collision pair in `args` (Obs option A); `cancel` events carry the cancelled op identifier in `error` so `^"error":"cancelled` greps still work (Obs O3 invariant).

Client (`infra/devloop/dev-cluster`) gains the `cancel` subcommand, surfaces busy errors with the in-flight op + cancel hint, prints a `[busy] write in flight: <op>` banner above the status health summary when a write is in progress, and prints "No write in flight; nothing to cancel." for the cancel no-op path.

ADR-0030 §"Helper Process" gained the concurrency-model paragraph. §"Helper API" table grew the `class` column + `ports`/`version`/`cancel` rows + four numbered subsections (concurrency contract, busy rejection, cancel semantics, connection cap). §"Audit logging" gained the outcome-vocabulary sentence. `docs/TODO.md` gained a `helper.log` rotation entry.

**Iter-2** addressed the 60 s cancel-latency bug surfaced during host-side validation: writer children are now spawned with `Command::process_group(0)` and signalled via `libc::kill(-pgid, SIG)` so SIGTERM/SIGKILL reach grandchildren that today inherit stdout/stderr (`kubectl wait`, `kubectl apply`, etc.) and would otherwise hold the pipes open until natural exit. All four kill sites in `run_command_streaming` (cancel SIGTERM, cancel SIGKILL escalation, broken-pipe SIGKILL, process-shutdown SIGKILL) go through a single `signal_process_group` helper with `i32::try_from(child.id())` + `checked_neg()` defenses and a `child.kill()` fallback for pathological PIDs. Status response gains a top-level `cancel_pending: bool` (CR11 wire-deterministic) sourced from the in-flight op's `cancel_token` under the same mutex acquisition that snapshots `busy`/`in_flight`. The `dev-cluster status` banner shows `[busy, cancelling] write in flight: <op>` during the SIGTERM-grace window plus a hint line "Cancel sent; the in-flight write should exit within ~2s (SIGTERM) or ~4s (escalates to SIGKILL of the process group)." Iter-1's CANCEL_GRACEFUL_TIMEOUT comment block + ADR-0030 §"Helper API" §3 Cancel semantics are corrected: containerd/kubelet/etcd are detached components managed by Kind's PID 1, not descendants of `setup.sh` — they were never in our process group. The 2 s window is for the cooperative shell/kubectl tree only. Cancellation policy is uniform across writes: no per-command `is_cancellable()`; cancelling `teardown` follows the same partial-state-on-cancel posture as cancelling `setup`. Five new tests (155 total): `test_cancel_kills_grandchild_holding_pipes`, `test_cancel_kills_grandchild_with_sigkill_escalation`, `test_cancel_pending_visible_in_status`, `test_cancel_pending_false_when_busy_not_cancelling`, `test_cancel_pending_false_when_idle`; the existing `test_release_does_not_expose_test_sleep` regression test was renamed `test_release_does_not_expose_test_commands` and parameterized over the four cfg-test command names.

155 tests pass on `cargo test -p devloop-helper --bins`; `cargo clippy --workspace --all-targets -- -D warnings` is clean.

---

## Files Modified

**Helper crate** (`crates/devloop-helper/src/`):
- `error.rs` — added `Busy { op, args }` and `Cancelled { escalated }` variants, `format_busy_op` helper, `kind()` arms, `#[non_exhaustive]` attribute, tests for Display/kind invariants.
- `protocol.rs` — added `HelperCommand::Cancel` and `#[cfg(test)] HelperCommand::TestSleep`, `is_write()` classifier, `Request::parse_command` arm for `"cancel"` (no test-sleep arm — release-only check covered by test), `Response::err` populates `data: {op, args}` for `Busy` errors, `error_kind` field on `CommandResult`. **Iter-2**: added `#[cfg(test)] HelperCommand::TestSleepWithChild { seconds }` and `TestSleepWithChildIgnoringTerm { seconds }` (no parse arms — release-only check parameterized over four names in `test_release_does_not_expose_test_commands`).
- `logging.rs` — added `pub const OUTCOMES`, `outcome: Option<&str>` field on `LogEntry`, `outcome` parameter on `log_command`, round-trip + absent-serialization tests.
- `commands.rs` — `CANCEL_GRACEFUL_TIMEOUT` const, `lock_recovered<T>` helper, `CancelSignal { shutdown, cancel }` with `is_cancelled`/`cancel_set`/`shutdown_only`, `InFlightOp` (no `child_pid` per Security S3-a), `WriteState`, `WriteSlotGuard` RAII, `BusyHint` snapshot, `Context.write_state` field, `snapshot_busy()`, `run_with_write_slot`, `cmd_cancel`, `cmd_test_sleep` (cfg test, direct `sleep` invocation so SIGTERM propagates), all `cmd_*` signatures take `&CancelSignal`, `run_command_streaming` rewritten with SIGTERM-then-2s-SIGKILL escalation, `cmd_status` snapshots busy first. **Iter-2**: rewrote the `CANCEL_GRACEFUL_TIMEOUT` doc + cancel-vs-shutdown comment block to correct the false claim about containerd/etcd being children of setup.sh; added `signal_process_group(child, sig)` helper with `i32::try_from + checked_neg` defenses; `run_command_streaming` calls `cmd.process_group(0)` before spawn and signals `-pgid` at all four kill sites — cancel SIGTERM, cancel SIGKILL escalation, broken-pipe SIGKILL, process-shutdown SIGKILL — each with a `child.kill()` fallback for pathological-PID rejection; `BusyHint.cancel_pending: bool` populated under the same lock from `cancel_token.load()`; `cmd_status` emits top-level `cancel_pending` field; added `cmd_test_sleep_with_child` and `cmd_test_sleep_with_child_ignoring_term` (bash forking sleep grandchildren).
- `main.rs` — `MAX_CONCURRENT_CONNECTIONS = 32`, `REJECT_LOG_DEDUP_SECS = 1`, `Context` wrapped in `Arc` with `write_state`, thread-per-connection accept loop, `ConnGuard` RAII, `maybe_log_cap_reject` with `AtomicI64` dedup, `handle_connection` rewritten with audit-log shape per Obs schema (rejected_busy / cancel completed / cancel no-op / standard outcome), 12-test `concurrency_tests` module covering all required scenarios (a)–(e) plus drift invariant. **Iter-2**: added five tests (`test_cancel_kills_grandchild_holding_pipes`, `test_cancel_kills_grandchild_with_sigkill_escalation`, `test_cancel_pending_visible_in_status`, `test_cancel_pending_false_when_busy_not_cancelling`, `test_cancel_pending_false_when_idle`).

**Client** (`infra/devloop/`):
- `dev-cluster` — `cancel` subcommand, busy-error handler surfaces in-flight op + cancel hint, status post-command summary shows `[busy]` banner above health when a write is in flight, cancel post-command summary distinguishes "Sent cancel" vs "No write in flight; nothing to cancel." **Iter-2**: status banner reads top-level `data.cancel_pending` and renders `[busy, cancelling] write in flight: <op>` (with "Cancel sent; the in-flight write should exit within ~2s (SIGTERM) or ~4s (escalates to SIGKILL of the process group)." hint) when true; existing `[busy]` rendering retained when false.

**Docs**:
- `docs/decisions/adr-0030-host-side-cluster-helper.md` — §"Helper Process" concurrency-model paragraph (replaces old "one-client-at-a-time" sentence), §"Helper API" table gains `class` column + `ports`/`version`/`cancel` rows + four numbered subsections, §"Audit logging" gains outcome-vocabulary sentence. **Iter-2**: §"Helper Process" cancel paragraph now describes process-group signalling and the correct-child-tree story (no more containerd/kubelet/etcd claim); §"Helper API" §3 Cancel semantics rewritten with the process_group + 2s-window-is-for-cooperative-shell-tree clarification, the cancel_pending observability sentence, and the explicit "no per-command cancel policy; cancelling teardown follows the same partial-state posture as cancelling setup" recovery story.
- `docs/TODO.md` — added `helper.log` rotation tech-debt entry under §"Developer Experience".

---

## Devloop Verification Steps

**Operator-facing flow** (per Operations Gate 1 follow-up — discovery breadcrumbs for future debuggers):

1. **Long-running write in flight, status returns immediately**:
   ```
   dev-cluster setup &        # ~7 min
   dev-cluster status         # returns < 1s, shows "Helper busy: setup ..."
   ```
2. **Second write rejected with cancel hint**:
   ```
   dev-cluster setup          # in-flight
   dev-cluster rebuild ac     # immediate exit 1: "ERROR: helper busy with setup ...; run 'dev-cluster cancel' to abort it. (busy)"
   ```
3. **Cancel mid-flight + recovery**:
   ```
   dev-cluster setup          # stuck
   dev-cluster cancel         # exits 0: "Cancelled in-flight setup; the operation will exit shortly."
   dev-cluster setup          # restart — recovers via existing "cluster exists, reusing" branch (cmd_setup commands.rs:237-243)
   ```
3a. **Cancel didn't return within 5s? Check cancel_pending** (iter-2):
   ```
   dev-cluster cancel         # returns immediately (cancel is fire-and-forget)
   dev-cluster status         # cancel_pending=true means SIGTERM grace window in progress (≤2s normal)
   dev-cluster status         # cancel_pending=true after >5s means writer blocked on grandchild
                              # pipes despite process_group SIGKILL; bounce the helper
                              # (kill -9 $(pidof devloop-helper) + restart).
   ```
   Status banner during the cancel window: `[busy, cancelling] write in flight: <op>` plus the hint line "Cancel sent; the in-flight write should exit within ~2s (SIGTERM) or ~4s (escalates to SIGKILL of the process group)." The ≤4s ceiling is the operator-facing budget; if the banner persists beyond that, escalate per the runbook above.
4. **Cancel when idle is no-op (not silent)**:
   ```
   dev-cluster cancel         # exits 0: "No write in flight; nothing to cancel."
   ```
5. **Audit-log greppability for triage**:
   ```
   grep '"cmd":"cancel"' /tmp/devloop-{slug}/helper.log
   grep '"cmd":"rejected_busy"' /tmp/devloop-{slug}/helper.log                   # all busy-rejection events
   grep '"cmd":"rejected_busy"' /tmp/devloop-{slug}/helper.log | grep 'in_flight=setup'  # rejections caused by an in-flight setup
   grep -E '"error":"cancelled' /tmp/devloop-{slug}/helper.log
   grep '"cmd":"rejected"' /tmp/devloop-{slug}/helper.log    # connection-cap rejections
   ```

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED | 2 | 2 | 0 | F1 `test_connection_cap_rejects_at_32`, F2 `test_sigkill_escalation_logged` (Obs O3 escalation suffix). Both plan-promised, missing from initial implementation, added in-loop. |
| Test | RESOLVED | 3 | 3 | 0 | F1+F2 same as Security; F3 `test_cancel_during_teardown_propagates` for the semantic-guard cmd_teardown-swallow-cancel fix — landed via extracted `propagate_teardown_kind_result` pure helper + 2 unit tests. |
| Observability | RESOLVED | 2 | 2 | 0 | F1 `OUTCOMES` const → 5 named `OUTCOME_*` consts referenced by all 11 production emission sites (typo at callsite is now a compile error). F2 ADR-0030 §"Helper API" advertised `ports`/`version` rows that have no implementation — rows dropped. Hunk-ACK on logging.rs schema growth + ADR §"Audit logging" (Minor-judgment co-owner). |
| Code Quality | RESOLVED | 2 | 2 | 0 | Same F1+F2 as Security (independent flag, same fix). CR7-CR13 + Response.data dual-role doc all verified. ADR-0002 no-panic, ADR-0030 alignment confirmed. Ownership Lens: Minor-judgment hunk-ACK on `logging.rs` + ADR §"Audit logging". |
| DRY | RESOLVED | 3 | 3 | 0 | F1 `extract_op_and_args` (busy + cancel-completed branches in `handle_connection`). F2 `build_test_context` (3 callsites). F3 `read_all_ndjson` (4 callsites — initially scoped to 2, lifted to 4 on re-review). |
| Operations | RESOLVED | 1 | 1 | 0 | Cosmetic — cancel no-op message wording drifted from plan. Implementer aligned the plan/docs to the landed code (`Sent cancel to in-flight write: <op>` / `No write in flight; nothing to cancel.`). ADR-0030 §"Audit logging" Minor-judgment co-sign: ACK. |

**Layer-7 semantic-guard**: SAFE after one round-trip. Found one bug pre-Gate-3: `cmd_teardown` swallowed `HelperError::Cancelled` into `Ok(())`, masking cancellation as completion in the audit log and triggering port-deallocation under cancel. Fixed at `commands.rs:1041-1054`; later refactored into `propagate_teardown_kind_result` pure helper (test F3). Re-verify: SAFE.

**Final tally** (iter-1): 13 findings raised across all six reviewers + semantic-guard, all 13 fixed in-loop, 0 deferred, 0 escalated, 0 spun-out. 149/149 tests pass; clippy clean across workspace.

### Iteration 2 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | — | — | All three Gate-1 asks (S3-(a) PG continuity, teardown option-1 policy, cancel_pending audit shape) verified at code level. Re-confirmed CLEAR after operations F1 changed broken-pipe to PG-SIGKILL. Caught a self-deadlock liveness gap in their original "swept by next cancel/shutdown" framing. |
| Test | RESOLVED | 1 | 1 | 0 | T-1: `test_cancel_kills_grandchild_holding_pipes` was missing the Obs O3 prefix invariant assertion AND the negative-escalation assertion that pins clean PG-SIGTERM as the load-bearing path (vs SIGKILL-pgid fallback). Fixed in-loop. |
| Observability | CLEAR | 0 | — | — | `cancel_pending` wire-determinism (CR11) preserved, audit-log schema untouched (no new OUTCOMES/events), Obs O3 prefix invariant + Obs O4 drift invariant extended to cancel path. ADR §"Audit logging" hunk-ACK trivially holds (no edits). |
| Code Quality | CLEAR | 0 | — | — | All six Gate-1 checklist items verified. Implementer pre-adopted both code-quality plan-confirmation notes (Result-typed `signal_process_group`, restructured 1-2-3 comment block). ADR-0002 + ADR-0030 + CR11 all preserved. |
| DRY | RESOLVED | 0 | — | — | `signal_process_group` extraction collapses 4 pgid-kill sites cleanly. Four `cmd_test_sleep_*` stubs correctly NOT abstracted (parallel structure, semantically distinct payloads). One extraction opportunity logged to `docs/TODO.md` (`wait_for_busy` test-helper, 6 sites, low priority — non-blocking per ADR-0019). |
| Operations | RESOLVED | 2 | 2 | 0 | F1 broken-pipe path was re-introducing the iter-1 grandchild-pipe-wedge bug — replaced single-PID `child.kill()` with PG-SIGKILL (security re-verified CLEAR after the change). F2 doc drift between Implementation Summary and deployed banner phrasing — quoted active-voice ≤4s-ceiling phrasing, unified "four kill sites" count. ADR-0030 hunk-ACK recorded. |

**Layer-7 semantic-guard (iter-2)**: SAFE on first pass. Verified S3-(a) sole-signaller continuity through process-group signalling, ADR-0002 no-panic via `i32::try_from` + `checked_neg` defense, CR11 wire-determinism for `cancel_pending`, all four cfg(test) stub variants regression-covered, broken-pipe asymmetry note (which operations F1 then revisited and tightened to PG-SIGKILL).

**Adjudication**: `cancel_pending` placement (top-level vs nested) was disputed mid-review when security flip-flopped three times. Lead adjudicated **top-level** based on (1) explicit observability + operations Gate-1 plan-confirms, (2) idle-case cleanliness + transient race-window argument, (3) post-pushback ACK from test, (4) security CLEAR verdict already based on substantive invariants regardless of placement. Operations subsequently confirmed top-level on the merits (typo in their earlier message had been mis-read).

**Final tally** (iter-2): 3 findings (test T-1, operations F1, operations F2), all 3 fixed in-loop, 0 deferred, 0 escalated, 0 spun-out. 156/156 tests pass; clippy clean across workspace; 22/22 guards pass.

---

## Tech Debt References

No new TODO entries from review. The following entries were added/touched as part of the planned work:

- `docs/TODO.md` §"Developer Experience" — `helper.log` rotation (added by this devloop's plan, per Operations Gate 1 ask — write-amplification mitigation owed by the new audit-event taxonomy).

Pre-existing entries surfaced by reviewers but not actioned in scope (cited in DRY verdict):
- `kind get clusters | grep` cluster-existence check (3 cross-language locations)
- `ctx.host_gateway_ip.as_deref().unwrap_or(DEFAULT_HOST_GATEWAY_IP)` (3x)

**Pre-existing workspace `cargo audit` findings (not introduced by this devloop)**: 6 advisories on transitive deps via `quinn-proto`, `ring`, `rsa`, `rustls-webpki`, and 3 unmaintained-crate warnings (`rustls-pemfile`, `ring < 0.17`). Diff has zero `Cargo.toml`/`Cargo.lock` changes — these predate the devloop. Not a Gate-2 blocker for this PR; flagged here so future readers don't conflate them with this work.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `e46bf96b112de9d27f48d6278812348dec5764ea`
2. Review all changes: `git diff e46bf96b112de9d27f48d6278812348dec5764ea..HEAD`
3. Soft reset (preserves changes): `git reset --soft e46bf96b112de9d27f48d6278812348dec5764ea`
4. Hard reset (clean revert): `git reset --hard e46bf96b112de9d27f48d6278812348dec5764ea`

No schema, no infra manifests applied — straightforward git rollback.

---

## Issues Encountered & Resolutions

1. **Session interruption mid-implementation**: The original session was interrupted between implementation completion and Gate 2. Recovery (2026-05-04) re-validated the in-tree implementation with `cargo test -p devloop-helper --bins` (145 → 149 after Gate 3 test additions) and `cargo clippy --workspace -- -D warnings` (clean), then ran Gate 2 layers 1-7 + Gate 3. Layer 8 (env-tests) skipped — see below.

2. **Semantic-guard caught `cmd_teardown` cancel-swallow bug at Layer 7**: pre-existing helper had an "intentionally swallow kind-delete failure to allow port-deallocation to recover" pattern that also swallowed `HelperError::Cancelled`, producing `outcome="completed"` in the audit log for a cancelled teardown. Fix: pattern-match on the result; propagate `Cancelled` early before deallocation, swallow other errors as before. Then refactored to a pure `propagate_teardown_kind_result` helper (commands.rs:1085) backed by 2 unit tests locking the asymmetric invariant.

3. **`test_sigkill_escalation_logged` SIGTERM-ignoring stub took two attempts**: first attempt used a `sleep` child wrapped in `trap '' TERM` shell — when SIGKILL hit the shell, the orphaned `sleep` child kept the pipe FDs open and the test hung. Final approach uses an in-shell busy loop (`while [ $i -lt $max ]; do i=$(( i + 1 )); done`) so SIGKILL on the shell closes all FDs and the test resolves cleanly. Lesson: when testing process-group teardown, avoid grandchildren that outlive the shell holding the pipe.

4. **Layer 8 env-tests deferred — host-side helper restart not available from Claude Code container**: this devloop's diff is in `crates/devloop-helper/` (helper IPC plumbing) and `infra/devloop/dev-cluster` (client CLI). Exercising the new helper requires building the new binary AND restarting the host-side helper process, which cannot be done from inside the Claude Code container. Manual host-side validation via `dev-cluster status / setup / cancel` was used as the substitute for layer 8. This is acceptable because (a) the diff has zero impact on AC/GC/MC/MH service behavior — env-tests don't exercise the helper IPC path, (b) the in-crate test suite (149 tests) covers the new concurrency model end-to-end against realistic stubs (`TestSleep`, `TestSleepIgnoringTerm`), and (c) the original helper bug surfaced during R-35 image validation gives this devloop a real-world acceptance harness on first host-side use.

---

## Lessons Learned

1. **Per-write `Arc<AtomicBool>` cancel token + writer-thread-as-sole-signaller (Security S3 option a) is structurally TOCTOU-free for child-process termination.** The PID never crosses thread boundaries; the writer holds `Child` on its stack and signals via `libc::kill(child.id() as pid_t, SIGTERM)` BEFORE any `wait()` call. No PID-recycle race is possible. Cancel only flips an atomic; the writer observes it from inside `run_command_streaming`. This pattern is reusable for any "long-running child + interrupt request" shape.

2. **`#[non_exhaustive]` + `#[cfg(test)]`-gated enum variants + parse-arm-omission is a robust pattern for test-only protocol commands.** `HelperCommand::TestSleep` and `TestSleepIgnoringTerm` exist only in test builds, have no `Request::parse_command` arm, and are guarded by a regression test (`test_release_does_not_expose_test_sleep`) that constructs a Request from raw JSON and asserts `InvalidCommand`. The wire surface stays clean even when `cfg(test)` is enabled — proves the enum existing isn't enough; only the parse arm exposes a variant over the wire.

3. **Locking the audit-log outcome vocabulary as named consts (one per outcome) catches typos at compile time.** Originally landed as a single `OUTCOMES: &[&str]` slice that was `#[allow(dead_code)]` — production sites used string literals, so a typo would compile. Per @observability F1, refactored to `OUTCOME_COMPLETED`, `OUTCOME_CANCELLED`, `OUTCOME_ERROR`, `OUTCOME_REJECTED`, `OUTCOME_NO_OP` consts referenced by every emission site; the slice is now `#[cfg(test)]`-only and built from those consts. A typo at any callsite is a compile error; adding a sixth outcome forces three edits (const, slice, round-trip test) — the typing system is the gate, not test coverage.

4. **Defer the "two grace timeouts" decision into a named `Duration` constant near the top of the production file, not buried at the call site.** `CANCEL_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(2)` declared near `MAX_LINE_LEN` makes the SIGTERM-vs-SIGKILL deadline reviewer-discoverable; pairing that with an `// IMPORTANT:` comment block in `run_command_streaming` documenting the cancel-vs-shutdown asymmetry (cancel = graceful; shutdown = immediate SIGKILL) closes the per-Security-S5 / per-CR12 ask in one move.

5. **When a recovery session can't run the full validation pipeline, run the parts that DO exercise the changes and document why the rest is deferred.** Layer 8 env-tests were skipped because they don't exercise this devloop's code paths AND the helper restart they'd require is out-of-environment. The 149-test concurrency suite, `lock_recovered`-poison coverage, and `propagate_teardown_kind_result` unit tests collectively exercise every new path. Skipping layer 8 with this rationale is more honest than running it for ceremony — env-tests pass against this diff iff they pass against the start commit.
