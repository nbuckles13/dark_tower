# Devloop Output: MH-3 — MH planned drain on SIGTERM, owner-silence release, load-report hygiene, QUIC idle timeout as config

**Date**: 2026-11-11
**Task**: Story 4 task MH-3 (run-story task #20) — see task prompt; story docs/user-stories/2026-10-31-survive-handler-loss.md
**Specialist**: media-handler (paired: operations, test)
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h40m (approximate total time; incl. two Gate-2 env-test runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b384097a7ff7119b5c8993a3bfab5dcec9e34d98` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (ab422aa4cf0d7b926)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security (a569c881e95aebbe0)` |
| Test | `test (a526af61384e864b5) — paired` |
| Observability | `observability (a8fe530b7ba30fee7)` |
| Code Quality | `code-reviewer (a59ec466d0889f139)` |
| DRY | `dry-reviewer (a527ac5c5516e5d87)` |
| Operations | `operations (a4132b8a59ce94a1b) — paired` |
| Semantic Guard | `semantic-guard (a8fa52d9d53c63ea1)` |

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
MH half of planned transfer (ADR-0039 §7): drain on SIGTERM (readiness latched off, DRAINING to GC and every owning MC with drain_remaining_ms, exit when empty or at grace − margin, shed HANDLER_RELEASED at the deadline); owner-silence release (§6); immediate load report after every (re-)registration; QUIC idle timeout as config (MH_MAX_IDLE_TIMEOUT_MS = MC's N).

### Scope
- **Service(s)**: mh-service (primary); common (one const hoist), mc-service (const derives from common), sdk-core (keepalive floor literal), infra/docs/dashboards
- **Schema**: No
- **Cross-cutting**: Yes (value-neutral const hoist; client keepalive floor guard-pinned to MH keepalive)

### Debate Decision
NOT NEEDED - governed by ADR-0039 §6/§7/§8/§10 and `docs/debates/2026-10-28-mh-replacement-and-migration/`.

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
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/lib.rs` | Mine | — |
| `crates/mh-service/src/drain.rs` (new) | Mine | — |
| `crates/mh-service/src/grpc/gc_task.rs` (new) | Mine | — |
| `crates/mh-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mh-service/src/grpc/mod.rs` | Mine | — |
| `crates/mh-service/src/grpc/heartbeat_sender.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/session/heartbeat.rs` | Mine | — |
| `crates/mh-service/src/session/owner_silence.rs` (new) | Mine | — |
| `crates/mh-service/src/session/release.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/webtransport/server.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` (comment only: `Shutdown` doc) | Mine | — |
| `crates/mh-service/src/observability/health.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/tests/gc_task_integration.rs` (new) | Mine | — |
| `crates/mh-service/tests/drain_integration.rs` (new) | Mine | — |
| `crates/mh-service/tests/owner_silence_integration.rs` (new) | Mine | — |
| `crates/mh-service/tests/mc_heartbeat_integration.rs` | Mine | — |
| `crates/mh-service/tests/gc_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_close_code_integration.rs` | Mine | — |
| `crates/mh-service/tests/media_session_binding_integration.rs` (comment citing the retired constant) | Mine | — |
| `crates/mh-service/tests/common/**` | Mine | — |
| `infra/services/mh-service/config.env` | Mine | — |
| `infra/services/mh-service/deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` (shed/drain triage pointers, if any stale text) | Not mine, Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md` (the "MH sheds on restart" pointer → drain; operations Gate-3 F2) | Not mine, Minor-judgment | operations |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` | Not mine, Minor-judgment | observability |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/44_gc_mh_registration.rs` (doc comment: `run_gc_task` moved to `grpc/gc_task.rs`) | Not mine, Mechanical | test |
| `crates/mh-test-utils/src/admission.rs` (`fixture_quic_transport` gains the idle field; deployed keepalive) | Mine | — |
| `packages/sdk-core/src/config/clientConfig.ts` (`MH_KEEPALIVE_FLOOR_MS` 10_000 → 2_000, guard-pinned equal to `MH_KEEPALIVE_INTERVAL_MS`) | Not mine, Minor-judgment | client |
| `packages/web-app/README.md` (the floor's documented value) | Not mine, Mechanical | client |
| `docs/DEVELOPMENT.md` (example `MH_KEEPALIVE_INTERVAL_MS` + the two new keys) | Not mine, Mechanical | operations |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `crates/mh-service/src/auth/mod.rs` (test only: re-sample `now` for the boundary twin, a flake under Layer-4 load) | Mine | — |
| `crates/common/src/config.rs` (new `MH_HEARTBEAT_TIMEOUT_CEILING_MS`, value unchanged) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/config.rs` (`MAX_MH_HEARTBEAT_TIMEOUT_MS` defined from the common const; doc text) | Not mine, Minor-judgment | meeting-controller |

No GSA path is touched: `proto/**` and `crates/media-protocol/**` are NOT edited (`HealthStatus::DRAINING = 4`, `ServingState::DRAINING`, `drain_remaining_ms`, `MH_CLOSE_CODE_HANDLER_RELEASED` all exist). ADR-0036 §11 already carries its "Reversed by ADR-0039" note, so no ADR edit.

---

## Planning

### Restated as mechanisms (not instances)

1. **Process state "draining" with ONE deadline** read by every component that reports state outward (GC load report, MC heartbeat, readiness). The wider class: every outward "am I serving" signal — `/ready`, GC `health`, heartbeat `ServingState`. All three are covered; `/health` (liveness) is deliberately NOT part of the class (stays 200, ops #4).
2. **"Report immediately after every registration"** — one helper used by both register paths; the class has exactly those two sites plus the drain transition.
3. **Owner contact** — one reset point; sites are an allow-list.
4. **Retired constants** — every comment/test/log citing `MAX_IDLE_TIMEOUT_SECONDS` / `SHUTDOWN_SETTLE_TARGET_SECONDS` / `DrainWindowSource` / "settle" goes (grep shows zero hits left in `crates/`, `infra/`, `docs/runbooks/`).

### A. Drain on SIGTERM (R-3 MH half, R-22)

**Config.** `Config.drain_window = grace − SHUTDOWN_MARGIN_SECONDS` (derived ONCE; the strict grace parse and the `grace <= margin` refusal and `saturating_sub` are kept). `SHUTDOWN_SETTLE_TARGET_SECONDS`, `DrainWindowSource` and the `drain_window_source`/`shutdown_settle_target_seconds` startup-log fields are deleted outright; `drain_window_seconds` stays on the startup line. The doc comment obliging an ADR-0036 §11 amendment becomes a pointer to ADR-0039 §7 (already recorded in ADR-0036). `CLOSE_SESSION_LINGER < SHUTDOWN_MARGIN_SECONDS` const-assert kept.

**One typed state** (`crate::drain`): `DrainState = watch::Sender<Option<DrainDeadline>>`, `DrainDeadline { deadline: tokio::time::Instant }` with `remaining_ms(now) -> u32` = `deadline.saturating_duration_since(now)` clamped into `u32` (the proto width) with `u32::try_from(..).unwrap_or(u32::MAX)` — no casts (security S5). Readers: GC task, session actor, health (via the GC task's readiness guard). The deadline is set ONCE at signal receipt: `t_signal + drain_window` (ops #1). Nobody recomputes it.

**Coordinator** `drain::run(session, drain_tx, window) -> DrainOutcome { exit: Empty | Deadline, shed: usize, elapsed: Duration }` (lib, paused-time testable; main only wires and logs):
1. `health.set_not_ready()` (main, before calling).
2. `drain_tx.send(Some(deadline))` → GC task sends an immediate DRAINING report; the session actor resets every held meeting's beat due to `now`.
3. INFO "drain started" (`meetings_held`, `drain_deadline_seconds`).
4. `select!` on `held.wait_for(|n| *n == 0)` (a `watch::Receiver<usize>` the actor updates at every registered-count change — no polling of the actor) vs `sleep_until(deadline)`. Exit condition depends on neither GC nor MC reachability (ops #3).
5. Deadline arm: `session.shed_all()` — ONE actor turn releasing every held meeting through `handle_end_meeting` with the MH-local `ReleaseKind::DrainShed` → every session's per-connection close token cancels → `ConnectionEnd::MeetingReleased` → `HANDLER_RELEASED` capsule (MH-CLOSE mapping, only that meeting's sessions; security S8). Returns the count.
6. Metrics: `mh_drain_duration_seconds` observed once (start = signal receipt; obs (b)); `mh_drain_meetings_shed_total += shed`; `mh_drain_meetings_remaining` recompute-and-set from the held watch during the drain (0 when not draining).
7. Back in main: `sleep(CLOSE_SESSION_LINGER)` (so the coded closes from the shed — or from the last release on the early-exit path — leave the process; bounded and < margin), THEN `shutdown_token.cancel()`, THEN TokenManager abort / OTel flush. The shutdown token is cancelled only after the shed closes are issued — load-bearing because `connection.rs` makes the shutdown-token path code-less (test D3).
8. Final INFO line (`exit_reason = empty|deadline`, `meetings_shed`, `drain_duration_seconds`) — counts only, no ids (security S7 / semantic-guard 1). This is the runbook evidence; exit-time counters may never be scraped.

**Servers keep running** during the drain: gRPC, WebTransport accept loop, media, heartbeat sender, GC task, health server all stay on the (not-yet-cancelled) shutdown token. `/health` stays 200.

**Decisions asked for:**
- **New RegisterMeeting / new dials while draining: ACCEPTED, unchanged** (ADR-0039 §7 "keeps serving"; a takeover or an in-flight MC selection can still land). No admission check is relaxed or added (security S4): `media_dial` verification, the pending cap, the binding gate and RegisterMeeting validation are untouched. A meeting registered during the drain counts as held (delays the early exit, bounded by the deadline) and its FIRST beat is due immediately (no jitter) carrying DRAINING. Tested (test (c)).
- **Second SIGTERM/SIGINT during the drain: ignored** (one INFO line); SIGKILL at grace is the backstop (ops #7).
- **Signal before initial GC registration completes:** the GC task keeps trying to register (and sends DRAINING right after it succeeds), but readiness can never come back: `HealthState::begin_drain()` clears ready AND sets a one-way latch, and `set_ready()` is a no-op once latched (ops #2; pinned by a test: register succeeding after the latch leaves `/ready` 503).

**Session actor changes.** New `select!` arm `drain_rx.changed()` → `schedule.due_all_now(now)` + transition log. `build_beat` reads the deadline: `ServingState::Draining` + `remaining_ms(now)`, else `Serving` + 0. New `SessionMessage::ShedAll`. `HeartbeatSetup` gains `drain: watch::Receiver<Option<DrainDeadline>>` and `owner_silence_timeout: Duration` (both are "MC liveness" inputs; an actor without a heartbeat sink gets no owner-silence either — correct, since without beats no OWNED could ever reset it). A single `publish_held()` replaces each `publish_registered_meetings` call and also sends the held-count watch. No `.await` is added in the actor loop (semantic-guard 2).

### B. GC load-report hygiene (R-22, ADR §7/§10)

`run_gc_task` moves to `crate::grpc::gc_task::run(gc: impl GcRegistry, health, drain_rx, cancel)`, generic over a small seam trait `GcRegistry { register(), attempt_reregistration(), send_load_report(HealthStatus), load_report_interval() }` implemented by `GcClient` (paused time over real tonic sockets auto-advances the clock — the MH-2 rig's reason, reused). Loop:
- register (retry forever while not ready; existing behaviour incl. `FAILED_PRECONDITION address_conflict`; the 5 s literal becomes a named const `REGISTRATION_RETRY_DELAY`);
- on EVERY successful register/re-register → `register_then_report`-style single helper `report_now()` sends ONE report immediately, then the ticker restarts with first tick at `now + interval` (`interval_at`), re-reading GC's pushed interval (kept);
- health for every report comes from ONE function `health_of(&drain_rx)` → `HEALTHY` or `DRAINING` (no literal at any send site; DRAINING survives a re-registration — security S3);
- `drain_rx.changed()` arm → immediate out-of-cycle report;
- GC failures during drain never block exit (the task is cancelled by main after the drain; each RPC is bounded by the existing `GC_RPC_TIMEOUT`).
`GcClient::send_load_report` takes the health parameter; the hardcoded `health: 1` goes.

### C. QUIC idle timeout as config (R-21)

`MH_MAX_IDLE_TIMEOUT_MS` REQUIRED, in `QuicTransportParams.max_idle_timeout_ms`; range `MAX_IDLE_TIMEOUT_MS_RANGE = (1_000, 30_000)` via the existing `parse_in_range` (security S6: floor + cap, hard fail; 0 is out of range — QUIC 0 means "no idle timeout"). Cap 30 000 ms = MC's N ceiling (`MC_MH_HEARTBEAT_TIMEOUT_MS ≤ 30 000`): "≈ N" can never mean more than the largest N; also the old constant, so no deployment is pushed past today's behaviour. Floor 1 000 ms is OUR choice (ADR says only "≈ 3–6 s"); below it a single RTT spike would idle out healthy sessions — reviewers may move it. The keepalive ratio check (`MIN_KEEPALIVE_TO_IDLE_RATIO = 3`) reads the configured value. `build_transport_config` reads the field; `main.rs` logs it; the "live wall" doc comment is rewritten against the two KEYS. Deployed: **idle 6000 (= MC's N), keepalive 2000** (ratio exactly 3, boundary-exact as today) — the keepalive must drop in the same commit or every MH crash-loops. That moves the client's `MH_KEEPALIVE_FLOOR_MS` to 2000 (guard-pinned equality; client row above). A Layer-4 test parses the real `config.env` values. Rollback: the previous image ignores the new keys and keepalive 2000 passes its 30 s / 3 check, so an image-only rollback is safe (runbook note). `MAX_IDLE_TIMEOUT_SECONDS` is a constant, not a key: deleted, NOT a `RETIRED_KEYS` entry.

### D. Owner-silence release (R-11; ADR §6)

`session/owner_silence.rs`: `OwnerSilence { timeout, heard: HashMap<meeting, (tokio::time::Instant, gen)>, queue: BTreeMap<(Instant, seq), (meeting, gen)> }` — monotonic tokio `Instant` only (security S1), lazy deadlines (a reset is an O(1) `Instant` overwrite; on pop, a not-yet-expired entry is re-queued at `heard + timeout`; a released/re-created meeting's stale entry is dropped by `gen`). ONE reset method `note_owner_contact(meeting, now)`.

**Reset allow-list (every call site, all AFTER validation passed):**
1. `handle_register_meeting`, after the record upsert succeeds — includes a takeover from a different `mc_id` (the existing takeover rule, unchanged: still requires a record-valid RegisterMeeting; security S2) and a record-valid registration whose POLICY was refused (record validation passed; it is MC communication — answers test (b)). A record-invalid request (e.g. `heartbeat_interval_ms = 0`) is refused in `mh_service.rs` before the actor and cannot reach it. Cap-refused NEW ids hold nothing.
2. New `SessionMessage::HeartbeatOwned { meeting_id, fence }`, relayed by the sender task ONLY for `HeartbeatVerdict::Owned`, applied only when `fence` equals the meeting's current fence (the beat was built for the current registration's `mc_id` + endpoint). Relayed with `try_send` (never blocks a beat task, so it cannot cause `skipped_in_flight`); a dropped relay needs ≥ 30 consecutive drops (300 s / ≤ 10 s) to matter, which is a wedged actor that is already visible as missed beats.
3. Owner `EndMeeting` and a fence-matching RELEASED verdict END the meeting; the silence entry is removed with it, which supersedes a reset (answers test (a): observable only as "released by the message, never by silence").
Never reset by: transport errors, timeouts, `FAILED_PRECONDITION`, `UNSPECIFIED`/unknown verdicts, stale RELEASED, non-owner `EndMeeting`, record-invalid registrations — none of those paths calls the method (each pinned by a test arm).

**Expiry**: actor `select!` arm `sleep_until(owner_silence.earliest())` → one INFO (`meeting_id`, owner `mc_id`, `silence_seconds`; no struct dumps) → `handle_end_meeting(meeting, registered mc_id, ReleaseKind::OwnerSilence)` — THE release path, so sessions close `HANDLER_RELEASED`, no Disconnected, awaiting-bind connections closed. Code comment states: capacity reclamation, NOT authz (`mc_id` is self-asserted) and NOT an MC-failover mechanism. Active during drain too (item 5: draining never self-fences; owner silence is the only silence rule). Config: `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` REQUIRED (no code default; 900 lives in `config.env`), `MEETING_OWNER_SILENCE_TIMEOUT_SECONDS_RANGE = (300, 3600)` through `parse_in_range`; config reference states MC's N ≤ 30 s ⇒ interval ≤ 10 s ⇒ 300 s ≥ 30 intervals; no per-registration check.

`ReleaseKind` gains MH-local `OwnerSilence` and `DrainShed` (never produced by `ReleaseKind::of`; both close awaiting-bind connections). Counting: `OwnerSilence` → `mh_media_handler_releases_total{trigger="owner_silence"}`; `DrainShed` → counted in bulk by the coordinator on `mh_drain_meetings_shed_total` (one home per event).

### E. Telemetry (obligation c) — names per observability

- `mh_media_handler_releases_total`: MH-local `ReleaseTrigger { Removal(RemovalTrigger), OwnerSilence }` with `ALL` in `metrics.rs` (NOT added to `common`'s `RemovalTrigger`); zero-init grows 6 → 8; `clients` from the same predicate.
- `mh_media_meeting_owner_silence_timeout_seconds` and `mh_media_transport_idle_timeout_seconds` (`key_custody="operator"`), published at startup from the SAME config fields the timer / `build_transport_config` read.
- `mh_drain_duration_seconds` (histogram, explicit buckets `[0.5,1,2,5,10,15,20,25,30,45,60]`; a unit test pins the top bucket above the deployed drain window), `mh_drain_meetings_remaining` (gauge, zero-init 0), `mh_drain_meetings_shed_total` (counter, zero-init) — no labels, no `key_custody` (not media-path).
- Panels on `mh-media.json` (drain row; owner-silence release split; two config stats), catalog entries, ADR-0032 tests. No alert rules (OBS-ALERTS).

### F. Tests (obligation d; all paused time, no sleeps; each negative paired with a positive)

Taking @test's list as the contract: L1–L5 (`gc_task_integration.rs`, fake `GcRegistry`), D1–D5 (`drain_integration.rs`: SessionManager + HeartbeatSender + FakeMc + `drain::run`), D6 one unpaused real-client arm in `webtransport_close_code_integration.rs` asserting the `HANDLER_RELEASED` capsule from a deadline shed, C1–C3 + owner-silence range (config.rs unit), O1–O3 (`owner_silence_integration.rs`, real 300 s timeout under paused time), the real-`config.env` keepalive/idle test, readiness-latch test, register-during-drain test, metric zero-init/recorder tests, and one unpaused `gc_integration.rs` test asserting `health = 4` on the wire. Env-test 01: `WIRED_KEYS` (Hard) + `CONFIG_GAUGES` rows for both keys and `assert_deployed_relationships` (idle ≥ 3 × keepalive; silence ∈ range).

### G. Docs / manifests (obligation a) — same commit

`config.env` + `deployment.yaml` (two required `configMapKeyRef`s; grace comments rewritten: the grace IS the drain budget, lengthening it lengthens every drain and how long a terminating pod holds its hostPort slot). `mh-deployment.md`: rows for both new keys, updated keepalive + grace rows, the "MH sheds media sessions on restart" section rewritten to "MH drains on SIGTERM" — stating honestly that until MC-F lands MC does not move meetings off a draining MH, so a roll still sheds every held meeting, now at grace − margin (~30 s) instead of ~2 s, and an empty MH exits at once — plus the other stale shed mentions; drain-shed triage points at MC's `planned_transfer`/`timed_out` + MH's final log line.

### Gate-1 refinements (folded in after reviewer replies)

- **(test 1)** The linger + cancel move behind the lib boundary: `drain::run(session, drain_tx, window, shutdown)` itself sleeps `CLOSE_SESSION_LINGER` after the shed (or after the last release on the early-exit path) and THEN cancels `shutdown`. main.rs no longer touches the order. D3 asserts in paused time: at the shed instant every close token is cancelled and `shutdown` is not; `shutdown` is cancelled at exactly shed + linger. D4/D5 do the same with last-release + linger. No other fixed sleep exists on any path (ops 3: an empty MH exits in about 1 s).
- **(test 2)** C3 adds the range boundaries: 999 and 30001 refuse, 1000 and 30000 accept, alongside the ratio boundary.
- **(observability)** `ReleaseKind::DrainShed` has an explicit no-count arm in `handle_end_meeting`. A shed is counted ONLY on `mh_drain_meetings_shed_total`, which the coordinator increments by the returned count. `mh_media_handler_releases_total` and `mh_media_meeting_teardowns_total` are left untouched. `ReleaseTrigger` stays `{Removal, OwnerSilence}`. D3 asserts both of those series are unobserved and that the shed delta == N.
- **(security i/ii)** The RegisterMeeting reset fires only after the upsert, so it touches the meeting whose record now holds the caller's `mc_id`. An OWNED verdict whose beat was built before a takeover carries the OLD fence and is ignored. Test arm: hold an OWNED beat in flight, run the takeover, release the verdict, and the new owner's timer is NOT refreshed (it fires at takeover + T).
- **(operations 1)** The runbook rollback note says that a mixed window (new client floor of 2 s, old MH keepalive of 10 s) only means an earlier first redial, which is harmless. **(ops 2)** The histogram's top-bucket test derives the drain window from the parsed real `config.env` / grace, not a hand-typed value.

- **(code-reviewer)** The `OwnerSilence` queue stays, because the actor recomputes `earliest()` every loop turn and an O(n) min at the 8192-meeting cap would cost too much per message. It is fully encapsulated (no pub fields), and its re-queue and stale-gen arms are unit-tested directly. The expiry arm is `Option`-guarded and never sleeps on a sentinel. A `HeartbeatOwned` relay dropped on a full mailbox is logged at WARN, not silently. `GcRegistry` mirrors `HeartbeatTransport`: a minimal seam trait using native `impl Future + Send`, with no `async_trait`. MH-local kinds are doc-commented, and `ReleaseKind::of` stays exhaustive with no wildcard.

- **(dry A)** The 30 000 ms heartbeat-timeout ceiling is hoisted into `crates/common/src/config.rs` as `MH_HEARTBEAT_TIMEOUT_CEILING_MS`, the one home. Three places read it. MC's `MAX_MH_HEARTBEAT_TIMEOUT_MS` is defined from it, and its validation is unchanged. MH's `MAX_IDLE_TIMEOUT_MS_RANGE.1` is the ceiling itself. MH's existing `mh_mc_heartbeat_latency` top-bucket ANCHOR comment becomes a const-assert (top bucket ≥ ceiling/3 s). The owner-silence relationship is a const-assert in MH config instead of prose: `MEETING_OWNER_SILENCE_TIMEOUT_SECONDS_RANGE.0 * 1000 >= 30 * (CEILING / 3)`.
- **(dry B)** Deployed relationship, stated once: **MH idle == MC's N**. A Layer-4 MH config test parses both `infra/services/mh-service/config.env` and `infra/services/mc-service/config.env` and asserts `MH_MAX_IDLE_TIMEOUT_MS == MC_MH_HEARTBEAT_TIMEOUT_MS`, plus idle ≥ 3 × keepalive. Env-test 01 `assert_deployed_relationships` asserts the same equality against the live `mc-service-config` ConfigMap.

### Cross-task obligations
(a) two config keys with manifest + runbook rows; (b) retirements: the two constants + `DrainWindowSource` + their log fields (constants, so no `RETIRED_KEYS` entry); (c) five new series + one new label value, each with panel, catalog, zero-init, ADR-0032 test; (d) test-gates.md MH load-report and drain bullets + owner-silence gates; (e) no id labels anywhere.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations (paired) | confirmed |
| Semantic Guard | confirmed |

Lead: `validate-cross-boundary-classification.sh` → STATUS=OK. No GSA, no Domain-judgment rows. Client (`clientConfig.ts` keepalive floor, guard-pinned to MH keepalive) and meeting-controller (`crates/common/src/config.rs` ceiling hoist, value-neutral) rows are Minor-judgment → review-only by the standard panel per §6.3 (common/** outside GSA: DRY + code-reviewer on panel). Plan approved.

## Pre-Work

None

---

## Implementation Summary

### Drain on SIGTERM (R-3 MH half, R-22)
| Item | Before | After |
|------|--------|-------|
| Shutdown | not-ready → cancel → sleep min(2 s, grace−5) → exit | `HealthState::begin_drain` (latched) → `drain::run`: DRAINING to GC + every MC at once → keep serving → exit on held==0 or at grace−5 → shed remainder `HANDLER_RELEASED` → linger → cancel token |
| Drain window | `min(SHUTDOWN_SETTLE_TARGET_SECONDS, grace−margin)` + `DrainWindowSource` | `grace − SHUTDOWN_MARGIN_SECONDS`, no cap (both constants/enum deleted) |
| Heartbeat state | always SERVING, `drain_remaining_ms` 0 | DRAINING + `remaining_ms` from the one `DrainDeadline` at beat time; every due reset to now at the transition; meetings registered mid-drain beat at once |
| Final evidence | none | INFO `mh.drain` "Drain finished" (`exit_reason`, `meetings_shed`, `drain_duration_seconds`) |

### GC load-report hygiene (R-22)
`run_gc_task` moved from main.rs to `grpc/gc_task.rs::run` over a `GcRegistry` seam: report immediately after every (re-)registration, then the ticker on GC's pushed interval; health from one `health_of(drain)` (DRAINING survives re-registration); drain transition sends an out-of-cycle report; rejected registration retried every `REGISTRATION_RETRY_DELAY` while not ready.

### Owner-silence release (R-11)
`session/owner_silence.rs` (lazy monotonic timers, one reset); reset sites: record upsert, fence-matched OWNED (relayed via non-blocking `VerdictSink::owned`); expiry releases through `handle_end_meeting(ReleaseKind::OwnerSilence)`; `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` [300, 3600], const-asserted ≥ 30 MC intervals at the shared ceiling.

### QUIC idle timeout (R-21)
`MH_MAX_IDLE_TIMEOUT_MS` [1000, 30000] replaces `MAX_IDLE_TIMEOUT_SECONDS`; deployed idle 6000 (= MC's N), keepalive 10000 → 2000 (client `MH_KEEPALIVE_FLOOR_MS` with it). MC's N ceiling hoisted to `common::config::MH_HEARTBEAT_TIMEOUT_CEILING_MS`.

### Telemetry
`mh_drain_duration_seconds`, `mh_drain_meetings_remaining`, `mh_drain_meetings_shed_total`, `mh_media_meeting_owner_silence_timeout_seconds`, `mh_media_transport_idle_timeout_seconds`, `trigger="owner_silence"` (MH-local `ReleaseTrigger`); panels in mh-media.json row "Planned Drain and Owner Silence", catalog entries, zero-init, ADR-0032 tests.

### Additional Changes
- The MH-2 fake-MC rig was extracted to `tests/common/heartbeat_rig.rs` (shared by three binaries).
- Real bug found by the new tests: a drain beginning mid-registration made the GC task send a duplicate DRAINING report (fixed: the drain change is marked seen before the post-registration report).

---

## Files Modified

```
 46 files changed, 4547 insertions(+), 767 deletions(-)
```

Key changes by file: see § Implementation Summary and the Cross-Boundary Classification table.

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → L1 OK 7s, L2 OK 5s, L3 OK 121s, L4 OK 132s, L5 OK 2s, L6 OK 5s, L7 OK 1533s; TOTAL_RESULT=OK (third run; see Issue 1).

### Layer 1: cargo check
**Status**: PASS
**Duration**: see Gate 2 summary
**Output**: See Gate 2 summary above.

### Layer 2: cargo fmt
**Status**: PASS
**Duration**: see Gate 2 summary
**Output**: See Gate 2 summary above.

### Layer 3: Simple Guards
**Status**: ALL PASS
**Duration**: see Gate 2 summary

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

See Gate 2 summary above.

### Layer 4: Unit Tests
**Status**: PASS
**Duration**: see Gate 2 summary
**Output**: See Gate 2 summary above.

### Layer 5: All Tests (Integration)
**Status**: PASS
**Duration**: see Gate 2 summary
**Tests**: see Gate 2 summary

See Gate 2 summary above.

### Layer 6: Clippy
**Status**: PASS
**Duration**: see Gate 2 summary
**Output**: See Gate 2 summary above.

### Layer 7: Env-tests
**Status**: PASS
**Duration**: see Gate 2 summary
**Output**: See Gate 2 summary above.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | grace ceiling 3600 + checked_add deadline |
| Test (paired) | RESOLVED-FIXED | 5 | 5 | 0 | O1 bound session, D6 no-Disconnected, bounded step_until, L2 re-pushed interval, log-capture flake fixed |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | drain-duration panel query |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | shed loop, idle fallback error!, fail-closed checked_after |
| DRY | RESOLVED-FIXED | 6 | 6 | 0 | extraction opportunities in docs/TODO.md |
| Operations (paired) | RESOLVED-FIXED | 2 | 2 | 0 | SHED_BUDGET bound; stale runbook text |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

F1: unbounded MH_TERMINATION_GRACE_SECONDS → Instant overflow panic at SIGTERM. Fixed: MAX_TERMINATION_GRACE_SECONDS=3600 (startup hard-fail, edge tests) + checked_add deadline.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 5 fixed, 0 deferred

F1 bound-session owner-silence arm; F2 D6 no-Disconnected + timing control; F3 bounded step_until; F4 re-pushed GC interval honoured; F5 log-capture flake fixed in-loop (7/20 → 0/20).

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

F1: Drain Duration panel structurally empty; histogram zero-initialised, panel uses increase(_sum)/increase(_count) per pod.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

F1 shed loop out of filter predicate; F2 idle-timeout fallback logs error! + range-edge test; F3 owner-silence uses shared fail-closed heartbeat::checked_after.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow): F1 shared QUIC fixture pinned to config.env; F2 one strict env reader; F3 shared checked_after (drift removed); F4 ReleaseTrigger::ALL ↔ RemovalTrigger::ALL const-assert; F5 one drain_window_for(); F6 shared tests/common/paused_time.rs — all fixed.

**Extraction opportunities** (appended to `docs/TODO.md`):
- `docs/TODO.md` §Cross-Service Duplication "From DRY Reviewer — story 4 MH-3 (2026-11-11)" — MC run_gc_task quirk-dependent shape
- same subsection — strict config.env parser in five test homes
- same subsection — OwnerSilence/HeartbeatSchedule deadline-queue kernel (extract at N=3)

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

F1: unbounded deadline shed wait → bounded by SHED_BUDGET (2 s), const-asserted inside the margin with the linger; final log line written before linger. F2: stale shed-on-restart/settle text in client-dev-local.md and mh-incident-response.md rewritten.

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found, 0 fixed, 0 deferred

No findings (credential-leak, client-credential-lifetime, actor-blocking, error-context-preservation, metrics-path-completeness all clean).

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `b384097a7ff7119b5c8993a3bfab5dcec9e34d98`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit` — MH_MAX_IDLE_TIMEOUT_MS=6000 and MH_KEEPALIVE_INTERVAL_MS=2000 must move together (3:1 ratio check would crash-loop MH), and the client floor is guard-pinned to the keepalive; image-only rollback with the new config.env is safe per runbook. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Gate-2 run killed by harness background limit; leaked MH pool slot
**Problem**: The first `layer-all.sh` run was killed by the Lead harness's 10-min background limit mid-Layer-7, leaking MhPool slot 2 (owner `e2e-926673fe1f2c4ef3`). The second run then failed Layer 7 with `foreign-mh-activity` (mh_pool tests) and two 35_mc_server_mute_teardown misses pinned to that leaked pod (mh-s2).
**Resolution**: Reclaimed with `mh.sh drain --all --owner e2e-926673fe1f2c4ef3` (pool back to baseline 2) and re-ran `layer-all.sh` with a 2 h background budget: TOTAL_RESULT=OK, all 7 layers. Treated as an infra/precondition contamination (Lead-caused), not an implementation failure.

---

### Pre-existing flakes in mh-service unit tests (found while running the full suite)
**Problem**: two unit tests failed intermittently, on the unmodified base `b384097a` as well (1 in 6 runs each), and MH-3's added tests raised the rate: `session::tests::a_same_mc_key_change_warns_and_a_same_key_reregistration_does_not` failed 7 of 20 runs of `cargo test -p mh-service -p mh-test-utils --lib`. A second test, `auth::tests::every_reason_has_its_input_and_the_valid_twin_passes`, also failed intermittently.
**Resolution**:
- auth: the boundary twin re-samples `now` (4 s of slack against a sample taken at the top of a long test).
- session: tracing's callsite interest cache could store `never` from a subscriber-less thread. A once-per-process global dispatcher that is interested in every callsite "sometimes" (and enabled for none) removes the race.
- Verified with **20 runs, 0 failures** (before: 7 failures in 20).

## Lessons Learned

1. Run the Gate-2 `layer-all.sh` with `run_in_background` and a long timeout from the start; a foreground timeout migrating to a 10-min background cap kills Layer 7 and leaks MhPool slots.
2. Tightening the QUIC idle timeout drags the keepalive (3:1 ratio) and the client floor guard with it — size such changes as a coupled set at planning.

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
