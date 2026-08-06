# Devloop Output: Reduce/Bound Roster Leave Latency (MC disconnect-detection)

**Date**: 2026-08-06
**Task**: Make tab-close roster removal fast and bounded; diagnose MC disconnect-detection path, handle transport close-event promptly, make the crash/network-loss idle timeout config-driven, and make disconnect reason observable.
**Specialist**: meeting-controller
**Mode**: Agent Teams (full, 8 teammates)
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3077160ebe4b70bd070b12a2957cc6d6b2033592` |
| Branch | `feature/user-story-run-test` |
| Headless | yes (run-story task #64) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` (Gate 2 PASSED attempt 2; Gate 3 all CLEAR/RESOLVED-FIXED; Gate 2 re-confirmed after review fixes — all 7 layers green) |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

---

## Task Overview

### Objective
After a participant closes their browser tab, the other participant's roster takes
noticeably long to update (manual test 2026-08-05). It does eventually update, so the
`ParticipantLeft` broadcast path works — the problem is *latency of detection/removal*.
Make tab-close roster removal fast (target: a few seconds), bounded, and documented.

### Scope
- **Service(s)**: mc-service (WebTransport connection, meeting/participant actors, config, metrics, ops docs)
- **Schema**: No
- **Cross-cutting**: Observability (new/extended metric), Operations (config value + ops-doc latency budget), possibly Infrastructure (env var in deployment manifest)

### Debate Decision
NOT NEEDED — this is a bounded implementation within existing MC boundaries. The
grace-period-vs-immediate-leave behavior interacts with ADR-0023 session recovery, but
the design (clean close → prompt Left; abrupt loss → bounded idle-timeout + grace) is
within the meeting-controller's domain. If the implementer concludes a wire-contract
(proto) change is required, escalate — that would pull in Protocol as a conditional
reviewer and touch a Guarded Shared Area.

---

## Diagnostic context (Lead pre-work — verify, don't take on faith)

The Lead's read of the current code before spawning. The implementer must independently
confirm each point:

1. **Disconnect detection.** `run_bridge_loop` (`webtransport/connection.rs`) only
   `select!`s over the outbound channel, the cancel token, and `read_framed_message`
   on the **recv stream**. It does NOT watch the session-level
   `wtransport::Connection::closed()` event. On a clean tab close the recv-stream read
   *should* error promptly; on crash/network-loss only the QUIC **max_idle_timeout**
   surfaces the departure — and `webtransport/server.rs::bind()` never sets
   `max_idle_timeout`/`keep_alive_interval`, so today the crash case falls back to the
   quinn library default (not config-driven, per-conventions a violation).
   `wtransport 0.7.1` exposes `ServerConfigBuilder::max_idle_timeout(Option<Duration>)`
   and `keep_alive_interval(Option<Duration>)`; `Connection::closed()` resolves with the
   close reason (already used client-side in `env-tests/26_mh_quic.rs`).

2. **Grace period gates the roster removal.** Even once disconnect IS detected,
   `meeting.rs::handle_disconnect` marks the participant `Disconnected` + starts a 30 s
   grace period (`DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS`, ADR-0023 reconnection) and
   broadcasts `ParticipantStateUpdate::Disconnected` — which `participant.rs::handle_update`
   does **not** serialize to the wire (only `Joined`/`Left` reach the client). The client
   roster is only actually removed when the grace timer fires `LeaveReason::Timeout` →
   `Left`. So the observed latency is plausibly dominated by the **30 s grace period**,
   not (or in addition to) the QUIC idle timeout. Confirm which dominates; the fix likely
   needs to make a **clean transport close skip the grace period** and broadcast
   `ParticipantLeft` promptly with an appropriate `LeaveReason`, while the ambiguous
   crash case keeps the grace period (bounded by the new config idle timeout).

3. **`LeaveReason` already has the values we need** (`signaling.proto`): `VOLUNTARY=0`,
   `CONNECTION_LOST=2`, `TIMEOUT=4`. Prefer reusing these over a wire change.

4. **Disconnect reason is not observable today.** `mc_connections_active` is a plain
   gauge (up on accept, down on close — no reason). `mc_mh_notifications_received_total`
   is the MH→MC path, unrelated to client tab-close. There is no counter for
   participant leave/disconnect *cause*. The task asks to add/extend a metric so
   clean-close vs idle-timeout vs grace-timeout departures are distinguishable.

---

## Cross-Boundary Classification

<!-- Filled by implementer at planning; Lead runs classification-sanity guard at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/webtransport/server.rs` | Mine | MC — config-driven `max_idle_timeout`/`keep_alive_interval` on `ServerConfig` |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | MC — watch `Connection::closed()`, classify `ConnectionError`, thread `DisconnectCause` |
| `crates/mc-service/src/config.rs` | Mine | MC — new `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` (fail-loud parse) |
| `crates/mc-service/src/actors/messages.rs` | Mine | MC — new `DisconnectCause` enum; `ConnectionDisconnected` gains `cause` |
| `crates/mc-service/src/actors/meeting.rs` | Mine | MC — skip-grace-on-clean-close in `handle_disconnect`; leave-metric emission |
| `crates/mc-service/src/actors/participant.rs` | Mine | MC — carry `DisconnectCause` through exit notify |
| `crates/mc-service/src/observability/metrics.rs` | Mine | MC — TWO new counters: `mc_participant_leaves_total{reason}` (4) + `mc_participant_disconnects_total{cause}` (3); exhaustive enum→label maps. (Observability = ADR-0011/0032 convention author/co-reviewer, NOT cross-boundary) |
| `docs/observability/metrics/mc-service.md` | Mine | MC — catalog entries for BOTH new counters + cardinality rows; update `mc_webtransport_connections_total` entry (`error`=establishment-only) |
| `crates/mc-service/src/main.rs` | Mine (Mechanical) | MC — threads `config.quic_max_idle_timeout_seconds` into `WebTransportServer::new` |
| `crates/mc-service/src/grpc/gc_client.rs` | Mine (Mechanical) | MC — test-only `Config { .. }` literals gain the new `quic_max_idle_timeout_seconds` field |
| `crates/mc-service/tests/gc_integration.rs` | Mine (Mechanical) | MC — test `Config { .. }` literal gains the new `quic_max_idle_timeout_seconds` field |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Mine (Mechanical) | MC — test `Config { .. }` literal gains the new `quic_max_idle_timeout_seconds` field |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | Mine (Mechanical) | MC — test rig passes the new `WebTransportServer::new` idle-timeout arg |
| `docs/observability/metrics/mc-service.md` | Mine | MC — catalog entries for BOTH new counters + cardinality rows + PromQL; update `mc_webtransport_connections_total` entry (`error`=establishment-only) |
| `infra/grafana/dashboards/mc-overview.json` | Mine | MC (service-owned dashboard, ADR-0031) — "Participant Leaves by Reason" + "Participant Disconnects by Cause" panels; exprs reused from the runbook PromQL (SSoT) |
| `docs/runbooks/mc-incident-response.md` | Mine | MC — worst-case roster-remove latency budget (SSoT-derived) + Scenario 14 |
| `crates/mc-service/tests/disconnect_latency_integration.rs` | Mine | MC — new production-path component test (counters + wire frames) |
| `crates/mc-service/tests/join_tests.rs` | Mine | MC — `join_keep_open` helper so the first participant stays connected across a second join |

**Scope — deliberately NOT changed** (kept out of the table above, which lists only touched
paths; these are intentional non-changes, preserved here as prose):
- `infra/services/mc-service/mc-0-configmap.yaml` / `mc-1-configmap.yaml` — the
  `DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS = 10` default applies (the disconnect grace period
  is also default-only today), so no manifest override is added. The env var + default are
  documented in the runbook for operators. If an override is later wanted, that is an
  infrastructure-owned edit (Minor-judgment per ADR-0024 §6 — a new manifest env var, not a
  structure-preserving change).
- `proto/**` — Guarded Shared Area; NOT edited. The existing `LeaveReason` `VOLUNTARY` /
  `TIMEOUT` wire values are reused (mapped in `handler.rs`), so no wire-contract change is
  required. Owner `protocol` if a change ever becomes necessary (escalate first).

---

## Planning

### Diagnosis (confirmed against code, not taken on faith)

Two independent latency contributors; the SECOND dominates the observed tab-close case.

1. **Detection.** `run_bridge_loop` (`connection.rs:536`) `select!`s over cancel token,
   `outbound_rx`, and `read_framed_message(recv_stream)` only. It does NOT watch the
   session-level `Connection::closed()`. `server.rs::bind()` (line 127) builds
   `ServerConfig` with NO `max_idle_timeout`/`keep_alive_interval` → falls back to the
   quinn library default (conventions violation: not config-driven, and the crash-case
   bound is undocumented). On a clean browser tab-close the recv-stream read *usually*
   errors promptly, so detection is not the dominant contributor for the reported case —
   but the crash/network-loss case has NO bounded detector today.

2. **Grace period dominates (the reported symptom).** Once disconnect is detected, the
   participant actor's exit notifies `meeting.rs::handle_disconnect` (line 663), which
   marks the participant `Disconnected`, starts a **30s** grace period
   (`DISCONNECT_GRACE_PERIOD`), and broadcasts `ParticipantStateUpdate::Disconnected`.
   `handler.rs::encode_participant_update` does NOT serialize `Disconnected` to the wire
   (only `Joined`/`Left`). So the OTHER client's roster is unchanged until the grace timer
   fires (checked on a 5s interval) → `Left{Timeout}`. **A clean tab-close therefore waits
   the full ~30s grace + up to 5s interval before the roster updates.** This is the
   observed latency. Confirmed.

3. `LeaveReason` (internal enum, messages.rs) already maps to proto `VOLUNTARY`/`TIMEOUT`
   in `handler.rs` — no proto change needed.

### Design

**(a) Clean transport close → skip grace, prompt `ParticipantLeft`.**
Thread the transport close cause from the bridge loop into the disconnect decision:

- Add `Connection::closed()` as a `select!` arm in `run_bridge_loop` (pass `&connection`,
  which already lives for the whole `handle_connection` scope). It resolves with a
  `ConnectionError` — the **transport-authenticated** session-close signal (NOT a
  client payload; cannot be forged by a crafted client frame — addresses @security #1).
- Classify `ConnectionError` → new `DisconnectCause` (messages.rs):
  - `ApplicationClosed | ConnectionClosed` → `ClientClosed` (clean peer/app close)
  - `TimedOut | QuicProto | LocalH3Error | CidsExhausted` → `ConnectionLost` (abrupt)
  - `LocallyClosed` → `ServerInitiated` (we closed it — cancel/drain/explicit-leave)
- On the recv-stream-read `Err` arm (and post-join write `Err`): the stream ended; resolve
  the authoritative cause via a **bounded** `Connection::closed()` (short internal micro-
  bound, ~2s — on any real close it resolves in ~0ms; NOT a policy knob). If it can't
  resolve in the micro-bound (half-closed stream, session lingering — does not occur on a
  browser tab close) → classify `ConnectionLost` (safe: keeps grace). No busy-loop; each
  arm cleanly `return`s (addresses @code-reviewer #5).
- Carry the cause to the meeting: `ParticipantActorHandle` + `ParticipantActor` share a
  small `Arc<AtomicU8>` cause cell. `handle_connection` calls
  `participant_handle.set_disconnect_cause(cause)` (Release store) BEFORE `cancel()`; the
  actor's run-loop tail reads it (Acquire load) and passes it in
  `connection_disconnected(conn_id, part_id, cause)`. Race-free (shared-memory happens-
  before cancel; no mailbox-vs-cancel ordering dependency).
- `meeting.rs::handle_disconnect(conn_id, part_id, cause)`:
  - `ClientClosed` → **remove immediately**, broadcast `Left{Voluntary}` (skip grace).
  - `ConnectionLost | ServerInitiated` → **existing behavior**: mark `Disconnected`, start
    grace period, broadcast `Disconnected` → preserves ADR-0023 reconnection
    (correlation_id + binding token) for the genuine transient case (addresses
    @code-reviewer #2). `ServerInitiated` no-ops when the participant was already removed
    by `handle_leave`/`handle_end_meeting` (existing `get_mut` guard).

`LeaveReason` for the clean close = **`Voluntary`**: a browser closing the WebTransport
session is a deliberate departure with no reconnection intent (binding-token reconnect is
driven by transient loss, which surfaces as `TimedOut`/reset → `ConnectionLost`, NOT a
clean app close). Wire-maps to proto `VOLUNTARY` (existing).

**(b) Crash/network-loss → config-driven, bounded idle timeout.**
- New config: `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS`, `DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS = 10`.
  **Fail-loud parse** (mirrors the OTel `map_err(...)?` pattern, NOT the lenient
  `unwrap_or(DEFAULT)` used by the other `_SECONDS` vars): non-numeric → `ConfigError`;
  value `0` → reject (0/None = infinite idle, which the wtransport docs warn can hang
  futures). Config tests mirror the OTel ones (valid / default / non-numeric→fail /
  zero→fail). (addresses @security #2, @code-reviewer #3.)
- `server.rs::bind()` sets `.max_idle_timeout(Some(Duration::from_secs(cfg)))?` (mapping
  wtransport `InvalidIdleTimeout` to a fail-loud bind error) and
  `.keep_alive_interval(Some(idle/3))` — keep-alive < idle keeps a healthy-but-quiet
  connection alive, so a live participant is never spuriously ejected (mitigates
  @security "too-low → spurious ejection"; posture is fail-safe: healthy sessions survive,
  only truly dead ones time out). 10s idle is a defensible "few seconds" choice: fast
  enough for crash detection, with keep-alive preventing false positives; the true idle
  timeout is `min(ours, peer)` so our value governs vs the browser's ~30s.

**(c) Observability — TWO new bounded counters (per @observability primary finding).**

Both facades in `observability/metrics.rs`; both map from INTERNAL enums via **exhaustive
match, NO `_` wildcard** (a future enum variant → COMPILE error / fail-loud on drift; the
`_ => unspecified` catch-all is ONLY for proto-derived enums carrying unknown wire ints —
not these). Neither label is ever a client string / `mh_url` / participant-id / raw
close-reason text.

- NEW counter 1 `mc_participant_leaves_total{reason}` (`record_participant_leave`). Emitted
  once per **roster removal** (each `Left` broadcast), centralized in the one
  `remove_and_broadcast_left(participant_id, reason)` choke-point. `reason` exhaustive-mapped
  from internal `LeaveReason`:
  - `voluntary`  — clean transport close (skip-grace) OR explicit leave
  - `timeout`    — grace period expired (abrupt/idle-loss that did not reconnect)
  - `removed`    — host-removed
  - `meeting_ended` — meeting ended
  Cardinality = **4**.

- NEW counter 2 `mc_participant_disconnects_total{cause}` (`record_participant_disconnect`).
  Emitted at the **transport-disconnect moment** — `handle_disconnect` entry, BEFORE the
  immediate-vs-grace branch. `cause` exhaustive-mapped from `DisconnectCause`:
  - `client_closed`    — clean close (→ skip-grace path)
  - `connection_lost`  — idle-timeout/abrupt (→ grace path)
  - `server_initiated` — LocallyClosed/cancel/drain
  Cardinality = **3**. This is a DIFFERENT event from a terminal leave, so it does NOT
  double-count leaves. It makes the grace-vs-immediate decision observable and, critically,
  lets ops **verify the new config `max_idle_timeout` path is actually firing** (hidden
  inside `leaves{reason=timeout}` otherwise) and derive reconnect rate
  (`disconnects{connection_lost} − leaves{timeout} ≈ reconnected`).

ADR-0032 for BOTH: `MetricAssertion` per-value delta + sibling adjacency in the metrics.rs
test module AND production-path emission asserted in the integration test; catalog entries +
cardinality-table rows in the metrics doc.

Catalog condition (@observability): the existing `mc_webtransport_connections_total` entry
is updated to state `error` = accept/handshake/**establishment** errors ONLY (post-join
disconnects now live on the leave/disconnect counters), so the doc doesn't drift from the
reclassified behavior. Verified no MC alert/dashboard keys off `status="error"` for
mid-session drops — `mc-alerts.yaml` alerts on `{status="rejected"}`, `mc-overview.json`
uses `{status="rejected"}` + a `sum by(status)` breakdown panel (no error-count alert). No
regression.

**Worst-case roster-remove latency bound (documented in runbook, SSoT-derived — NOT a
hardcoded prose number):**
- Clean tab-close: ≈ network RTT (sub-second) — `Connection::closed()` fires immediately,
  grace skipped. This is the "few seconds" acceptance target and what task #60's teardown
  E2E asserts.
- Crash / network-loss: `max_idle_timeout` + `disconnect_grace_period` + grace-check
  interval (5s) = `10 + 30 + 5 = 45s` worst case, derived from the two config values.
  Grace is intentionally preserved here for ADR-0023 reconnection. (addresses
  @observability #4, @operations SSoT.)

### Note on a minor metric-semantics change
Post-join transport write/read failures currently make `handle_connection` return `Err`,
bumping `mc_webtransport_connections_total{status="error"}`. Under the new design they are
classified as normal disconnects (`ConnectionLost`) and no longer counted as connection
"error"s — more correct (a peer going away mid-session is not a server error). Flagging for
@observability/@code-reviewer sign-off.

### Testing (addresses @test)
- meeting.rs actor tests (tokio `start_paused`, no real sleeps): (1) `ClientClosed` →
  immediate `Left{Voluntary}`, grace SKIPPED (participant gone before grace elapses);
  (2) `ConnectionLost` → `Disconnected`, grace PRESERVED, reconnect-within-grace still
  works (ADR-0023 regression guard).
- config.rs tests: valid / default / non-numeric→fail-loud / zero→fail-loud.
- metrics.rs: `record_participant_leave` per-value delta + adjacency (ADR-0032).
- New integration test `tests/disconnect_latency_integration.rs`: production-path emission
  of `mc_participant_leaves_total` for voluntary + timeout, and a deterministic assertion
  that the clean-close bound is config-derived (feeds task #60's E2E).
- `classify_connection_error` unit tests over each `ConnectionError` variant.

---

## Implementation Notes (deviations from plan, for Gate 2)

Implementation complete. One honest deviation from the plan text, consistent with the plan's
INTENT:

1. **`join_tests::test_second_participant_does_not_trigger_register_meeting` updated.** Its
   helper `join_and_read_response` dropped the first participant's connection right after the
   response; the OLD 30s grace masked that (Alice lingered), so Bob was "second". Under the
   new clean-close path Alice is removed immediately → Bob looked like the first participant
   and RegisterMeeting fired twice. Fixed by adding a `join_keep_open` helper that holds
   Alice's live connection/streams while Bob joins — which is what "a meeting that already
   has a participant" actually means. This is the correct expression of the test's intent, not
   a workaround; no production behavior was reverted.

**Counter assertions ARE on the production path (earlier carve-out withdrawn).** An initial
draft asserted only roster behavior + wire frames because a first attempt at the metric
delta read 0. On @observability's prompt I re-probed: `MetricAssertion` DOES capture spawned
`MeetingActor` emissions under `flavor = "current_thread"` — the delta-0 was a
synchronization bug, not a framework limit. Fix: assert counters only AFTER a
`get_state().await` round-trip that follows the disconnect send (FIFO mailbox ⇒ the
disconnect is fully processed and its metric emitted before `get_state` returns).
`tests/disconnect_latency_integration.rs` now asserts BOTH the real production-path counter
deltas (`disconnects{client_closed|connection_lost}`, `leaves{voluntary|timeout}`, with
sibling adjacency) AND the decoded `ParticipantLeft` wire frames.

**Gate 2 guard fixes (validation attempt 1):**
- `validate-application-metrics`: added two panels to `infra/grafana/dashboards/mc-overview.json`
  ("Participant Leaves by Reason", "Participant Disconnects by Cause"), exprs reused from the
  runbook PromQL.
- `validate-cross-boundary-scope`: added the 3 mechanical inbound caller rows + the dashboard
  row to the Cross-Boundary table, and moved the two deliberate non-changes (configmap,
  `proto/**`) out of the table into the "Scope — deliberately NOT changed" prose above.

**Verification run locally:** `cargo test -p mc-service` → 282 lib + all integration tests
pass (incl. the counter-delta integration test); `cargo clippy -p mc-service --all-targets`
clean; `cargo fmt --check` clean; `cargo check --workspace` clean. (Full
`./scripts/layer-all.sh` is the Lead's Gate 2.)

**Remaining host-side action:** none in-tree. Infra configmap intentionally NOT edited (the
`DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS = 10` default applies, mirroring how the grace period
is default-only today); the env var + default are documented in the runbook for operator
override.

---

## Plan Confirmations (Gate 1)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed (T1–T4 folded in) |
| Observability | confirmed (added 2nd counter `mc_participant_disconnects_total{cause}`) |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

**Gate 1 classification-sanity guard**: `STATUS=OK` (after correcting the `proto/**` row Owner to the manifest owner `protocol`). Plan approved by Lead → implementation.

---

## Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | close-reason log leak → `error_variant` `&'static str` discriminant (connection.rs) |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | clean-close real-transport coverage gap → new `join_tests` end-to-end test |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | disconnect double-count guard (counter-only, removal unconditional) + `removed`-label RESERVED honesty note |
| Code Quality | CLEAR | 0 | 0 | 0 | verified cause-cell ordering, exhaustive maps, ADR-0002/0023 compliance |
| DRY | CLEAR | 0 | 0 | 0 | config positive-int parse logged as non-blocking extraction opportunity (ADR-0019 exception) |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | runbook Scenario 9 drift (mid-session drops → `disconnects{connection_lost}`) |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | same close-reason PII/log-injection leak (corroborates Security) |

**Gate 3 outcome**: all 7 reviewers CLEAR or RESOLVED-FIXED. **Zero accepted deferrals, zero escalations.** 6 review findings total, all fixed in-diff. Re-validation (Gate 2 confirm) in progress after the review fixes touched production code.

---

## Accepted Deferrals

- **(none)** — all 6 review findings were fixed in-diff. Zero findings remain in the diff; zero escalations.

## Follow-ups (forward-looking, nothing left in the diff)

These are tracked in `docs/TODO.md`, not deferrals — the current diff is complete:
- `docs/TODO.md` §Cross-Service Duplication → From DRY Reviewer: the fail-loud "parse env var → positive integer, reject 0" config block now repeats across GC (×4) + MC (×1); extraction opportunity per the ADR-0019 DRY Reviewer Exception (non-blocking). Owner: dry-reviewer to propose; common home.
- `docs/TODO.md` §Code Quality: MH `webtransport/server.rs` has the SAME unbounded QUIC disconnect-detection gap MC just closed (no `max_idle_timeout`/`keep_alive_interval`). Parallel MH-owned fix (not a shared extraction; MC's `keep_alive_for` is N=1). Owner: media-handler + operations.
- `docs/observability/metrics/mc-service.md`: `LeaveReason::Removed` (`removed` label) is RESERVED — enum + wire-map + label-map ready, no production emission site yet. Documentation-honesty note added (ADR-0032 pattern); not a deferral, a real reserved-for-future state.
