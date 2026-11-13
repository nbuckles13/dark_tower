# ADR-0039: Media Handler Replacement and Migration

**Status**: Accepted (2026-10-31). Revised after the user's review (round 4,
`docs/debates/2026-10-28-mh-replacement-and-migration/round4-user-review.md`).

**Date**: 2026-10-28

**Deciders**: user; meeting-controller, media-handler, global-controller, protocol, client, infrastructure; security, test, observability, operations (mandatory cross-cutting)

**Supersedes**: ADR-0036 §8 "The mechanism" recovery model (periodic re-assert, restart detection) and §9's frozen per-meeting handler set; reverses ADR-0036 §11 "MH sheds media sessions on restart". Re-scopes story 4 of the ADR-0036 arc.

---

## Context

ADR-0036 §8 assumed a failed media handler (MH) comes back: "a re-assert remains the whole recovery
story". A handler lost to hardware, power or network never returns, deliberate moves (maintenance,
scale-down) were not covered, and a restarted MH registers under a new per-incarnation `handler_id`
outside the meeting's frozen handler set, so even same-pod recovery did not work. The periodic
re-assert itself never shipped (the runbooks say so in about a dozen places), so dropping it removes
a promise, not a mechanism.

The user set the framing before the debate (`docs/debates/2026-10-28-mh-replacement-and-migration/brief.md`):
MC continuously learns whether each MH is alive and keeping up; on death or impairment MC obtains a
new MH and moves the meeting there using the same logic as a participant joining; one mechanism
serves crash, deliberate transfer and (later) client network change; MC failure is entirely out of
scope with no stubs; **backward compatibility is not a concern** (no production, no old code
running; commits must still be self-contained).

Mid-debate the user added: **a new MH must never be alive on the same client-facing URL as an old
MH**, and MH must be spin-up-able on demand in Kind. Existing deploy shortcuts are in scope to fix.

## Decision

### 1. Liveness: MH→MC heartbeat, one per (meeting, handler)

- **One unary `MeetingHeartbeat` RPC per (meeting, handler)** on `MediaCoordinationService`. Each
  heartbeat goes to that meeting's MC meeting actor and gets that actor's verdict, so meetings stay
  independent: MH does no cross-meeting aggregation, and MC does no fan-out or fan-in. **Not
  retried**: the next tick is the retry. Each attempt's deadline is shorter than the send interval,
  so at most one heartbeat per meeting is outstanding and they arrive in order. The cost (about
  500 small RPC/s for 1,000 meetings on an MH at a 2 s interval, over one cached HTTP/2 channel
  per MC endpoint) is accepted. Batching per (MH, MC) may return later as a transport optimization
  only.
- MH's session actor runs one timer arm on the earliest per-meeting due instant, builds each
  heartbeat from its own state, and hands it to a sender task with a bounded `try_send` (full →
  dropped and counted). So a sent heartbeat implies a live actor, and the actor never blocks. A
  wedged actor yields missed heartbeats (Dead); a backed-up config apply yields a lagging
  `applied_generation` (Impaired). Each meeting's first deadline is jittered in [0, interval), since
  a migration registers many meetings together. **Heartbeats start at the registration upsert**,
  not at the first successful apply: a meeting whose first apply is refused must report
  `applied_generation = 0` plus its `apply_refusal`, not go silent. DRAINING resets every meeting's
  deadline to now.
- Request: `meeting_id`; `mc_id` (the meeting's registered MC, §6); `handler_id`;
  `ServingState {UNSPECIFIED (rejected), SERVING, DRAINING}`; `drain_remaining_ms` (set iff
  DRAINING; relative, so no clock skew; MH's grace period stays the only copy); `applied_generation`;
  `apply_refusal`; `connection_ids`. MC never looks up an incoming id: it walks its own live keys for
  that (meeting, handler) and retires the absent ones (§6), so no participant id is needed. The
  connection ids are those for which MH has begun `NotifyParticipantConnected` and not
  yet sent `Disconnected`. That is a superset including in-flight binds, which is safe for remove-only
  reconcile. Size is bounded by the per-meeting caps MC already enforces.
  (Amended 2026-11-10, story 4 MC-B1: MC's caps do not bound MH's begun-not-Disconnected superset,
  and MC has no per-meeting participant cap; the receiver bound is MC-wide. An over-cap or
  malformed `connection_ids` skips that heartbeat's reconcile and is counted, while the verdict and
  liveness still apply. Rule home: `MeetingHeartbeatRequest` header in `internal.proto`.)
- **`applied_generation` is the existing per-(meeting, handler) `policy_generation`**, reported
  from what MH has *applied*, never received (ADR-0036 §8 "Why the acknowledgement exists" still
  holds). No new counter.
- **Apply is all-or-none** (invariant, stated once at `RegisterMeetingResponse.applied_generation`;
  code: validation rejects in `crates/mh-service/src/routing/mod.rs` (`PolicyRejection::outcome`);
  stale, capacity and transient refusals and the single swap in `crates/mh-service/src/session/mod.rs`
  `handle_config_apply`). A policy apply installs the meeting's complete new
  snapshot (edges, source-stream declarations, dispositions) in one atomic swap, or installs
  nothing. On every refusal the previously installed snapshot **keeps forwarding unchanged**, and
  `applied_generation` (response and heartbeat) keeps reporting that installed generation, or 0 if
  none was ever installed for this registration. The refusals are:
  - stale (generation lower than installed);
  - validation reject (INVALID, TRANSPORT_INCONSISTENT, per-meeting bounds);
  - capacity (STREAM_CEILING, EDGE_CAP);
  - transient (mailbox full, timeout, actor unavailable).

  An equal generation is a no-op reported as applied. The registration record (mc_id, MC endpoint,
  dial public key, heartbeat interval) is upserted independently of the apply, and a refusal does
  not roll it back.
- **`apply_refusal`** (`UNSPECIFIED | INVALID | TRANSPORT_INCONSISTENT | STREAM_CEILING | EDGE_CAP`)
  is a 1:1 projection of MH's deterministic apply refusals: `rejected_invalid`,
  `rejected_transport_inconsistent`, `rejected_stream_ceiling` (`PolicyApplyOutcome`) and
  `ApplyFailure::EdgeCapExceeded` inside `apply_failed`. It reflects the latest apply and clears on
  success. `STREAM_CEILING` and `EDGE_CAP` are load-dependent and feed the replace path; `INVALID` and
  `TRANSPORT_INCONSISTENT` are MC contract faults: loud, no replacement. Transient failures surface
  as generation lag.
- **Config: MC owns one value, N** (`MC_MH_HEARTBEAT_TIMEOUT_MS`, default 6000 ms) and sends the
  send interval (N/3) to MH per meeting in `RegisterMeetingRequest.heartbeat_interval_ms` (0
  rejects). MH has no cadence config, so the two ends cannot drift.
- **The periodic re-assert cadence is dropped**, as is `process_start_epoch_ms`. gRPC channel
  connectivity events are **not** a liveness input (not deterministically testable).
  `RegisterMeetingResponse` keeps its synchronous `applied_generation` and per-stream
  `applied_source_streams` echo; the heartbeat adds applied-generation reporting for the async
  residual and lag.

### 2. Detection: one pure health function in MC

`assess(now, observations, previous_state, cfg) -> HandlerHealth` per (meeting, handler), with no
I/O and no timers inside. **The MC meeting actor owns it**: the actor keeps each handler's
last-receipt time, calls `assess` on heartbeat arrival and on a `sleep_until` arm, and acts only on
transitions. MC's gRPC handler routes each heartbeat to the meeting actor through the controller
(the same path as `Notify*`) with a oneshot reply. It stamps the receipt instant at gRPC arrival, so
mailbox delay cannot age an entry. The send into the actor waits for less than one interval and is
never dropped silently (a drop is counted). There is no process-wide liveness table. Inputs: last accepted entry time, applied-vs-sent generation and when the lag started,
`apply_refusal`, serving state, and client loss reports. An entry is accepted as liveness only if its
`applied_generation` is not lower than the last accepted one for that (meeting, handler), so a
stale or reordered heartbeat reads as lag, never as fresh liveness. This is NOT replay protection:
an equal-generation replay or a forged heartbeat still reads as live: MC authenticates only the
caller's service.write.mc token, never which handler sent it (handler_id is self-asserted,
S1b-extended), and the plaintext MH->MC channel exposes that bearer token to an on-path observer
(S6 residual). (Erratum 2026-11-06, story 4 task P1: the original text overclaimed replay.)

- **Dead** = entry age > N, the clock starting at the first confirmed `RegisterMeeting` for that pair.
  One-way: a removed handler is never re-added to that meeting.
- **Lagging** (reversible) → **Impaired** (one-way) when applied generation stays behind for longer
  than L (default 2N), or on a load-dependent `apply_refusal`. While Lagging, MC re-publishes the
  current snapshot to that handler's push worker (replacing the re-assert's self-correction on a live
  MH).
- **Draining** from `ServingState`.
- **Client connectivity is a second opinion only** in this story. It is an input to `assess` so a
  future story can promote it to a trigger by a policy change, not a restructure.
- Triggers share one proto enum (below): `HEARTBEAT_TIMEOUT`, `APPLY_LAG`, `PLANNED_TRANSFER`.

A false declaration (MC's path to MH broke, MH fine) is a **safe outcome**: an unnecessary migration
and a blip. It is counted on the MH side (§8):
`mh_media_handler_releases_total{trigger="heartbeat_timeout", clients="present"}`.

### 3. Replacement: removal first, then GC

- On a Dead/Impaired transition MC **removes the handler first** and re-renders edges on the
  surviving handlers; removal never waits on GC.
- MC then asks GC for one handler: **`AcquireMediaHandler {meeting_id, controller_id,
  exclude_handler_ids, trigger, replaced_handler_id}` → `MhAssignment`**.
  - `replaced_handler_id` is optional label context (`gc_view`) only; GC never treats it as a swap
    target.
  - `exclude_handler_ids` = current ∪ removed ∪ refused-at-admission, by id; unknown ids are
    ignored. GC owns the length cap
    and pushes it to MC in `RegisterMCResponse.max_exclude_handler_ids` (0/absent fails MC
    registration); MC derives its per-meeting attempt bound from it.
  - **GC alone chooses**, among healthy, fresh, under-ceiling candidates, with **the same
    weighted-random selection as initial allocation** (one function; the exclude list is applied
    inside the candidate query, before the least-loaded cut). That randomness is what spreads a burst
    of replacements from many MCs across handlers (user ruling, 2026-10-31); MH admission
    (`STREAM_CEILING`/`EDGE_CAP` refusals, §1) is the hard backstop. GC's own health view of the
    replaced handler is a metric label (`gc_view`), **never a veto** (it lags MC's detector by
    up to the MH staleness bound, k × the load-report interval, 30 s by default). GC persists nothing per meeting: **MC's versioned in-memory handler set is the sole
    authority**; Redis `MhAssignmentData` is GC's hand-off, read once when the meeting actor is
    created (later joins no longer read it; `mc_media_handler_set_divergence_total` is deleted). No
    region field (GC derives it from the ownership row).
  - Statuses: `OK`, `RESOURCE_EXHAUSTED` (no capacity), transport errors (GC unavailable),
    `FAILED_PRECONDITION` (caller not the meeting's MC — anti-misroute only; `controller_id` is
    self-asserted, recorded gap; not retried, loud), `INVALID_ARGUMENT` (over-length/malformed — MC bug, not retried).
  - No capacity / GC down: MC continues on survivors and retries with bounded, jittered backoff,
    bounded per meeting by a minimum interval between replacements and the attempt bound derived
    from `max_exclude_handler_ids`. There is no per-MC limiter (user ruling, 2026-10-31): it could not
    protect a shared MH or GC from other MCs and would only delay the tail of a crashed handler's
    meetings; a meeting at its bound stops acquiring (loud, counted). A meeting with zero
    handlers is an explicit, counted, paged "awaiting handler" state.
- **Register before token, everywhere, including first join.** Inside MC a handler is first
  *Registering*: it is in MC's set, its heartbeats get OWNED, and the binding gate admits it. It
  becomes *Active*, and enters the client-facing set with dial tokens, only once its
  `RegisterMeeting` is acked. The actor never blocks a turn on MH: a meeting's first join reply is
  parked in the actor, which keeps processing its mailbox, and is sent with tokens when the push
  worker reports the first ack. Later joins reply immediately. If the initial handler never acks
  (push retries exhausted), the `JoinResponse` goes out with the handlers that did ack, possibly
  none, and the unacked handler takes the normal replacement path. MC never issues a token for an
  unregistered handler, so MH declines any dial for a meeting it holds no key for at accept (counted
  as `failure_reason="no_meeting_key"`) instead of parking it, and its pending-before-registration
  path is deleted
  (`PendingConnection`, `meeting_notifiers`, `await_meeting_registration`,
  `MH_REGISTER_MEETING_TIMEOUT_SECONDS`, `mh_register_meeting_timeouts_total`).
- **Known gap, postponed: GC can keep selecting a bad MH for new meetings.** A dead MH is covered,
  slowly: its load reports go stale, and an MH whose `RegisterMeeting` fails (refused, unreachable or timed out) joins the meeting's
  exclude list. A *gray-failed* MH is not covered: GC can reach it, but MCs or clients can't, so it
  keeps reporting healthy and stays selectable. Each meeting placed on it heals through MC detection
  and `AcquireMediaHandler`, at the cost of one migration per bad placement.
  - Evidence is collected now, with no effect on selection: the existing
    `gc_mh_replacement_requests_total{trigger, outcome, gc_view}` (dashboard), plus one INFO event
    `gc.mh.replacement_report` {replaced handler_id, its webtransport_endpoint, controller_id,
    trigger, gc_view, outcome} per request carrying `replaced_handler_id`. Ids go in logs only. Only `heartbeat_timeout` and
    `apply_lag` reports count as failure evidence; `planned_transfer` reports do not.
  - GC keeps no in-memory corroboration window. GC runs 2+ load-balanced replicas, so a per-replica
    window would undercount exactly the cross-MC case. "Distinct MCs per handler" is a log query.
  - The discriminator is a handler reported dead by ≥ 2 distinct MCs that **keeps load-reporting
    healthy past the staleness bound**. `gc_view` at request time is not it: GC lags MC by design
    (MC declares at about N, GC goes stale at k × the load-report interval).
  - Until the automatic policy exists, the runbook's manual form applies: ≥ 2 distinct MCs AND the handler
    still load-reporting healthy past the staleness bound means drain it with `mh.sh drain`, one MC means suspect that MC's path.
  - The deferred automatic policy is a `docs/TODO.md` entry: quarantine on M ≥ 2 independent MC
    reports (`heartbeat_timeout`|`apply_lag`) within window W against a handler GC still sees fresh
    and healthy, released on expiry. It needs state shared across GC replicas and **authenticated MC
    identity first** (`controller_id` is self-asserted today).

### 4. Migration = the join path

- **Client signalling**: a full-snapshot `MediaHandlerSet {handler_set_version, handlers[{media_handler_url,
  handler_ref, media_dial_token}]}`, sent on `JoinResponse` and as a `ServerMessage` on every
  change and every token refresh (§5). `JoinResponse.media_servers` is deleted.
  `handler_set_version` is a membership epoch: it advances only when membership changes, a
  token-only refresh keeps it and the refs, and it is used to correlate loss reports, never to
  discard them. Only Active handlers are listed (§3), so the join-time set is empty only when the initial handler
  never acked: the client
  treats that as "media connecting" (no fault, no loss report), dials when the first non-empty set
  arrives, and raises one non-fatal, counted notice if none arrives within a clientConfig bound.
  `handler_ref` is MC-assigned, per incarnation,
  never reused within a meeting; `SendTarget`/`StreamAssignment` carry refs (URLs only in the set);
  every ref must be a current member (non-member counted + ignored). `MhConnectionStatus` echoes
  `handler_ref` + `handler_set_version` (`mh_url` deleted), is sent on every transition, and stays
  diagnostic-only.
- The client reconciles to the set, diffing on `handler_ref`. A set with the same members only
  replaces tokens: no dial, no close. It dials members it is not connected to. For a member that has
  left the set, one function, unit-tested, does all the cleanup:
  - close its transport as a directed removal (no fault, no loss report);
  - abort any in-flight dial and cancel any redial or park state;
  - drop its token and its egress lane;
  - end its read loops quietly and clear its per-transport hop-sequence monitor;
  - clear the reconnecting state on its slots.

  It redials lost in-set members with full-jitter
  backoff (first retry no earlier than one keepalive interval). It never dials anything outside the
  latest set.
- **Recovery is the join path**: routes that could not complete before complete on the new handler
  through normal §9 visibility and edge assignment, with no special case (the stranded A/B case:
  A and B only on the dead handler hear each other once both connect to the replacement).
- **Crash** = break-before-make: set without the dead handler (and with the replacement once
  granted). **Planned transfer** = make-before-break: add the new handler, wait until every routable
  participant is MH-reported Connected to it (or `drain_remaining − margin`, outcome `timed_out`),
  then remove the old handler: the set without it goes to clients first, then its push worker drains,
  then `EndMeeting(old)`. Edges move at that removal (ADR-0036 §9 edge stability keeps them on the old
  handler while both parties are on it). A draining
  handler with no grant makes the **survivors** the target; it is removed once every routable
  participant is on a survivor, or at `drain_remaining − margin` (outcome `timed_out` for
  stragglers). Sessions are shed only with no survivor and no grant. Because edges move only at
  the old handler's removal, a planned transfer in this story costs a small blip (one push, directive
  change and keyframe); ADR-0036 §9's multi-handler send (story 6) is the path to hitless later.
- **Slot states**: no new wire `SlotState`. The client renders a client-local *reconnecting* state
  for slots whose handler it observed lost or was told to drop, until the slot has a source again or
  a clientConfig cap expires — a client-observed fact, not inferred from silence (ADR-0036 §6).
- **Keys**: no KEK action (ADR-0036 §4: MH failover is not a trigger). The client's transmit-key
  manager survives a handler swap (a rebuild would reuse wrap nonces; pinned by a test).

### 5. Dial credential: MC-minted `media_dial` token

The join-time meeting JWT (900 s, deliberately not retained by the SDK) cannot dial a replacement
after minute 15. MC therefore mints a short-lived dial credential itself, with no AC call on join,
migration or redial.

- **Key**: an Ed25519 keypair **per meeting**, drawn from a CSPRNG when the MC meeting actor is
  created. It is held only in that actor's memory, beside and separate from the KEK. It is never
  persisted, logged or Debug-printed, and it dies with the meeting. It is never rotated (a
  live-memory compromise already yields the KEK).
  - Only the public half travels: `RegisterMeetingRequest.media_dial_public_key` (exactly 32 bytes,
    else rejected). MH can verify tokens but cannot mint them.
  - It is not symmetric. MC→MH gRPC is plaintext today (S6), so a shared secret there would let a
    *passive* observer mint tokens. It would also be exactly the secret-on-this-contract case that
    `internal-proto-no-key-material` exists to stop.
  - That guard gets one exact (message, field) allow-list entry, (RegisterMeetingRequest,
    media_dial_public_key), and is re-derived to "no secret or media-key material; one exactly
    allow-listed public verification key". Security owns the membership and infrastructure owns the
    construct. Entries match by exact equality on (fully qualified message name,
    field name), with no pattern or suppression path. Each entry must resolve to an existing `bytes`
    field; a missing, renamed or non-`bytes` entry fails the guard. The key is never
    string-encoded to dodge the guard.
  - MC's key is never in any JWKS.
- **Token**: a compact JWS, alg EdDSA, claims {`token_type=media_dial`, `sub` (from MC's session,
  never the request), `meeting_id`, `aud = handler_id`, `iat`, `exp`}. TTL
  `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS`: default 300, hard-fails outside [60, 900].
- **Delivery**: every `MediaHandlerSet` carries a token for **every** member.
  - MC re-pushes each participant's set every TTL/3, jittered per participant, through the existing
    per-turn coalesced flush. It also pushes on every membership change and
    on (re)join.
  - A token-only refresh keeps `handler_set_version` and the refs (§4).
  - A `MediaConnectionUpdate` FAILED for an in-set ref triggers an immediate, rate-limited re-push
    of that participant's set. That is a credential re-push to the sender only, never routing input.
  - No AC mint endpoint, scope or `MediaDialTokenRequest`/`MediaDialToken` exists (proposed in
    round 3, dropped in round 4 before implementation), so nothing on AC changes.
  - `MhConnectRequest.join_token` is replaced by `media_dial_token`.
- **MH verifies at accept, before holding anything**:
  - size cap; alg pinned to EdDSA; `kid`/`jwk` headers ignored;
  - the key is selected only by the `meeting_id` claim from that meeting's registration. An
    unregistered meeting is DECLINED at once;
  - then the signature, `token_type`, `exp`/`iat` with a **configurable** skew (small, about 30 s;
    today it is hardcoded to 300 s in `crates/mh-service/src/main.rs`), and a non-empty `sub`;
  - then `aud == own handler_id`, else `WRONG_HANDLER`. Every earlier failure is `DECLINED`.
- **Reuse**: a token is valid for any number of dials within its TTL; MH keeps no jti cache. MH caps
  concurrent connections awaiting their bind per (meeting, `sub`) at a small configured value, so one
  client cannot amplify MH→MC binding RPCs.
- **Client retention**: the client holds the latest token per in-set ref, only in its transport's
  set state. It is replaced per set and cleared on ref removal and session end, and it is never
  persisted, logged or stringified (a scoped R-23 amendment). Every dial (join, redial, retry) uses
  it. A `DECLINED` close means "token dead", not "handler dead": the client parks that ref, reports
  `MediaConnectionUpdate` FAILED for it (which triggers MC's re-push above), and makes exactly one
  dial when a set with a newer token arrives.
- **A participant who has left** but still holds a valid token needs no new mechanism: MH holds the
  connection awaiting its bind, MC answers sender_id 0, and MH closes with `DECLINED`.
- **Cross-meeting KEK guard, by construction**: MC's join path and GC verify only AC-signed tokens
  (AC JWKS, EdDSA pinned), so a `media_dial` token fails there. The rejection tests are re-pointed
  at MC-signed tokens.
- Rejected alternatives:
  - AC minting per dial: an AC round-trip per participant per handler at join, on migration and on
    redial, which puts AC on the migration path.
  - Client re-fetch via GC join: guests get a new `guest_id` per call, there is no `cnf`, and the
    user-token TTL applies.
  - A symmetric key (above).

### 6. Cleanup after removal (fencing)

**Invariant**: once MC removes a handler from a meeting, that handler stops serving the meeting, the
clients drop it, and MC forgets it. This must hold when the handler is a **zombie**: alive, but
declared dead because MC's path to it broke. Otherwise the zombie keeps forwarding to clients that
haven't moved, holds capacity it reports to GC, and MC leaks the bindings whose `Disconnected` will
never arrive. Each mechanism below covers one of those failures.

- **MC: forget it.** Drop the handler from the set and record it as removed for the meeting's life
  (one-way: never re-added). Push the set without it to clients. Release its bindings only after
  that. Send `EndMeeting(old, cause)` best-effort. A failed `EndMeeting` never blocks removal.
- **MC: refuse it.** The **binding gate**: MC answers sender_id 0 and records nothing for a
  `NotifyParticipantConnected` from an out-of-set or removed handler. Today `handle_media_connected`
  records before the membership check; that flips.
- **Client: drop it.** The set without the handler leads to the §4 cleanup function.
- **MH: stop serving it.** The heartbeat response is the retry for a lost `EndMeeting`:
  `verdict {OWNED, RELEASED}` plus a `ReleaseCause` on RELEASED. RELEASED means "not in my set", and
  MH ends the meeting through `handle_end_meeting`, closing its sessions with `HANDLER_RELEASED`
  and sending no `Disconnected`. OWNED does nothing. A verdict of `UNSPECIFIED` (proto zero) is
  malformed: MH counts it and takes no action. MH does not act on short MC silence; the only
  silence rule is the owner-silence release below.
  - Cause rules: a removed handler gets its removal trigger. A meeting with no MC actor gets
    `MEETING_ENDED` (answered by MC's router; no tombstone needed). A live meeting whose set never
    held this handler gets `UNSPECIFIED`, counted as a misroute.
  - The cause is what lets MH count false declarations (§8): the zombie case is exactly when
    `EndMeeting` is lost, so the verdict is the only message that reaches it.
  - **Owner check**: the heartbeat carries the meeting's registered `mc_id`. An MC that is not that MC
    answers `FAILED_PRECONDITION`, never RELEASED, and MH counts it and takes no action. This stops a
    stale MC endpoint whose IP was reused by *another* MC pod from releasing a live meeting.
  - **MC restart (incidental, not an MC-failover mechanism)**: MC's id is generated per process
    (`MC_ID` is unset in our deploy), so a restarted MC has a new id and answers its predecessor's
    meetings' heartbeats with `FAILED_PRECONDITION`. Those meetings are already lost (MC failure is out
    of scope); MH holds them until the owner-silence release below reclaims the capacity.
  - **Owner-silence release** (user ruling, 2026-10-31): MH ends a meeting when it has had no
    communication from the meeting's owning MC for `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS`
    (default 900). Communication means a `RegisterMeeting` or `EndMeeting` from that MC, or a
    heartbeat answered with a verdict by it; transport errors and `FAILED_PRECONDITION` do not count.
    Sessions close with `HANDLER_RELEASED`, and the release is counted with its own cause. It is safe
    because the bound is far beyond N: an MC that cannot reach MH for that long has already declared
    it dead and moved off, and an MH that stops heartbeating a meeting is declared dead by a live MC.
    MH validates the value at startup against a floor well above the largest heartbeat interval.
- **One enum**: `ReleaseCause {UNSPECIFIED, HEARTBEAT_TIMEOUT, APPLY_LAG, PLANNED_TRANSFER,
  MEETING_ENDED}` serves `AcquireMediaHandler.trigger` (`MEETING_ENDED` and `UNSPECIFIED` are `INVALID_ARGUMENT` there),
  `EndMeetingRequest.cause` (new; rejects `UNSPECIFIED`) and the RELEASED verdict (where `UNSPECIFIED` is
  only the documented misroute case). Every service projects its `trigger`
  label from it. On a release, connections still awaiting their bind are closed with
  `HANDLER_RELEASED` for removal causes, and left to the bind outcome on `MEETING_ENDED`.
- **Lost `Disconnected`: connected-set reconcile, remove-only.** A heartbeat never adds connectivity
  (the sender_id bind stays the only add path). MC retires a key absent from a heartbeat only if it
  recorded the bind before the *previous* heartbeat for that (meeting, handler) arrived: one instant
  per pair in the actor, valid because per-meeting heartbeats arrive in order. The same rule
  releases `HandedOutBindings` entries, closing the lost-`Disconnected` leak.
- `connection_id` is **required**; the empty-means-legacy mode and the `unreleasable` set are deleted.

### 7. MH addressing and provisioning (user ruling)

- **Every MH incarnation's client-facing address belongs to that incarnation; no two live MH
  processes ever share one.** Sequential reuse after the previous holder has exited is allowed (any
  fixed address pool requires it); `handler_ref` and the `media_dial` `aud` make it safe.
- **Kind**: the MH WebTransport port declares a **hostPort slot**. The scheduler admits at most one
  pod per hostPort per node and a terminating (draining) pod holds it until gone, so surge, eviction
  and delete-replace all wait for the old process to exit — uniqueness by construction. K = 6 slots,
  one home in `crates/devloop-helper/src/ports.rs`, with kind-config `extraPortMappings`, per-slot
  ConfigMaps (`MH_WEBTRANSPORT_ADVERTISE_ADDRESS`) and GC's port allowlist derived from it (drift
  guard). The per-instance NodePort Services and the duplicated mh-0/mh-1 Deployments are deleted;
  one MH template remains. Draining MHs stay reachable by construction (hostPort DNAT, no Service).
- **On demand**: `infra/kind/scripts/mh.sh up | drain <k> | upgrade | list`, using the caller's
  kubeconfig (no helper verb, no operator). `up` takes the next free slot after the highest occupied
  one (next-fit, wrapping), fails loudly when none is free, and returns only when the MH is GC-selectable. `drain` refuses the last serving MH without
  `--force`, and refuses (and `list` flags) while any MH pod is stuck terminating past its grace.
  `upgrade` is serial and resumable: new MH up → Available → drain old → gone; deploy.sh renders the
  template once in the environment-root render (stored in-cluster as ConfigMap `mh-template`, which
  `mh.sh` instantiates; the root apply excludes the MH workload), calls `upgrade`, and tops up to
  baseline B (default 2; one home in the Kind overlay, distinct from GC's per-meeting initial
  handler count). Rollback = converge from the previous checkout. Crash = in-place container restart
  (same slot, same pod IP, new `handler_id`).
- **Callable by automated tests**, from the host or a devloop container (both already have a
  kubeconfig). The contract is in `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt`:
  - stdout is JSON only (`--json`); stderr ends with exactly one
    `MH_RESULT STATUS=OK|FAIL REASON=<token>` line. The token list has one home in `mh.sh`, and
    exit codes are grouped (0 OK, 1 FAIL, 2 PRECONDITION, 3 CAPACITY, 4 REFUSED, 5 BUSY);
  - `list --json` is schema-versioned and additive-only, covering every slot (address, pod, state,
    revision, owner, pod_uid, restart count; no `handler_id`, and never a GC or database read). "GC-selectable"
    means the Deployment is Available, which means MH `/ready`;
  - `up --slot k --owner ID` and `drain` are idempotent;
  - an owner label (`baseline`, `manual`, or the Layer-7 run token) applies, and `drain` refuses
    another owner's MH unless `--force`;
  - a short `mh-pool` Lease serialises only the decide-and-create/delete step;
  - the leak check is per owner;
  - a destructive leg on a shared cluster fails loudly with `foreign-mh-activity`;
  - the fixture is `crates/env-tests/src/fixtures/mh_pool.rs`, owned by test;
  - the hermetic self-test is `infra/kind/scripts/mh.test.sh`, in Layer 3.

  CI runs no Kind or Layer 7 today. That is an existing gap, recorded and not solved here.
- **Template invariants** (dt-guard over the template): `app=mh-service` + `component=media`, ports
  8083/50053/4434-UDP-with-hostPort, MC 50052 egress, the shared `mh-service-tls` dev-cert Secret.
  `shareProcessNamespace: true` is **Kind-only** (overlay patch, listed as test instrumentation in the
  ADR-0038 parity allowlist; guarded present in Kind, absent from base and every non-Kind render).
- MH keeps reading its advertised address from injected config; no topology in Rust.
- **GC registration rule** (stated once above `RegisterMHRequest`): both endpoints pass per-zone,
  config-driven allowlists (client-facing: `https` + `GC_MH_WT_ALLOWED_HOSTS` + `GC_MH_WT_ALLOWED_PORTS`;
  internal: `GC_MH_GRPC_ALLOWED_SCHEMES` = `grpc` today — cleartext h2c, unauthenticated — switched by
  config only to a TLS scheme when internal mTLS lands, plus `GC_MH_GRPC_ALLOWED_CIDRS` and
  `GC_MH_GRPC_ALLOWED_PORTS`; Kind values derived from the slot table and kind-config podSubnet,
  never retyped) else `INVALID_ARGUMENT`; endpoints are fixed for an
  incarnation's life (changed endpoint for an existing id → `FAILED_PRECONDITION`); against another
  id's row: both endpoints match → supersede; one matches and that row is draining/unhealthy/stale →
  supersede; one matches and that row is fresh and serving → reject `address_conflict` (MH retries
  until it goes stale). **A live serving MH is never displaced. Superseded rows affect only new
  selection; MC never reacts to GC registry rows.** Full text:
  `docs/debates/2026-10-28-mh-replacement-and-migration/gc-register-rule.txt`.
- **Drain on SIGTERM** (reverses ADR-0036 §11 "MH sheds media sessions on restart"): MH sends an
  immediate `HEALTH_STATUS_DRAINING` load report to GC (and keeps reporting it until exit), flags
  DRAINING in its heartbeat, keeps forwarding, and exits when it holds no meetings or at grace −
  margin. MH also sends a load report immediately after every registration (no reliance on the first
  timer tick).
- **Close codes**: `MhCloseCode {WRONG_HANDLER, BINDING_CONFLICT, MC_UNAVAILABLE, HANDLER_RELEASED,
  DECLINED}` in `signaling.proto`, numeric only, sent only after token validation (every token
  failure is `DECLINED`), only about the caller's own meeting. A close with no code means transport
  loss. The enum's zero value (`MH_CLOSE_CODE_UNSPECIFIED`) is never sent; the client treats a received
  0, like any unknown code, as a contract fault; MH closes every declined, released or drain-deadline-shed session with a code. MH's QUIC `max_idle_timeout` becomes config (≈ N) so a silent MH crash surfaces at the client
  natively; no app-level keepalive.

### 8. Telemetry

Full contract: `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md`. Key points:

- **The user-required metric**: at each Dead or Impaired declaration (trigger `heartbeat_timeout` or
  `apply_lag`; never `planned_transfer`), the count and
  0–1 share of the clients MC believed connected to that handler whose latest status for that
  `handler_ref` was a loss received while the ref was in the set, plus a lead-time histogram. Counted
  at MC receipt time, never client time. **It is interpretable only when client detection time
  (MH QUIC idle timeout) ≈ N; both are published as config gauges.** A companion
  `mc_media_handler_client_loss_disagreements_total` counts the opposite case (most clients report
  loss while heartbeats look healthy) — the false-negative half of the evidence. It detects a
  forged heartbeat only when the forger echoes the live `connection_ids`: an empty or wrong list
  retires the believed-connected set through the remove-only reconcile, emptying its denominator
  (story 4 MC-E finding). Zero disagreements is NOT evidence of no forgery.
- One `trigger` label from `ReleaseCause`. No meeting, participant or `handler_id` labels on MC/GC
  series (per-incarnation ids churn; the `internal.proto` comment allowing `handler_id` as a label is
  narrowed).
- False declarations are counted on MH: `mh_media_handler_releases_total{trigger="heartbeat_timeout",
  clients="present"}` (apply_lag+present is a correct removal; planned_transfer+present is expected).
  The release counter covers removals only (`MEETING_ENDED` stays on
  `mh_media_meeting_teardowns_total{outcome="released"}`). MH's exit-time counters are best-effort;
  the drain-shed alert keys on MC's planned-transfer outcome and MH logs the shed count on exit.
- Page: a meeting without a handler beyond a config-derived bound (`MCMediaMeetingsAwaitingHandler`);
  sustained GC supersede rate together with `unknown_handler` load reports
  (`GCMhRegistrationPingPong` — dead processes never report, which separates ping-pong from
  crash-restart churn); fleet/region-level `MHDown` (zero healthy non-draining MHs; single-MH death is routine);
  `MCMediaGenerationDivergence` (retained, re-assert text removed; the
  `mc_media_generation_divergence` gauge is deleted). Warnings: migration failures with a surviving
  handler (including `timed_out`); any `gc_mh_load_reports_total{outcome=unknown_handler}` increase.
  The client-before-MC metric is dashboard-only.
- Round-4 changes:
  - Detection series are per (meeting, handler), observed by the MC meeting actor. The required
    metric's ratio histogram is meeting-weighted.
  - MC's own-path warning counts **handlers**, not meetings:
    `mc_media_handler_first_declarations_total{trigger}` (once per handler, from a bounded MC-wide
    set of recently declared handlers) against an `mc_media_handlers_in_use` gauge. It fires when one
    MC declares ≥ 2 distinct handlers within a window and they are a high share of its in-use
    handlers, so one MH death cannot fire it.
  - `mc_media_handler_heartbeats_total{outcome=owned|released_removed|released_ended|released_unassigned|owner_mismatch}`.
    `released_ended` is the lost-`EndMeeting` retry rate. `released_unassigned` and `owner_mismatch` are
    expected to be zero, and any increase is a WARNING.
  - The census behind `first_declarations_total` is one MC-wide `HandlerCensus` on the
    `MutedSourceCensus` pattern, pruned on the controller health walk. Handler ids are never labels.
  - Every UNKNOWN series is deleted (none of them, nor the round-3 AC mint counter and
    `token_unavailable` outcome, was ever implemented). `mh_register_meeting_timeouts_total` is deleted.
  - New `mc_media_handler_set_pushes_total{reason=join|change|token_refresh|failed_report}`; the
    token TTL and the derived refresh interval (TTL/3) are published as config gauges.
  - MH's `mh_jwt_validations_total` uses `token_type="media_dial"`, and its client-path
    `failure_reason` splits into `expired|signature_invalid|aud_mismatch|wrong_type|no_meeting_key|malformed`.
    Clients still see only `DECLINED`. A sustained `expired` count is a WARNING: it is the only sign
    that the refresh push is broken. Sustained `no_meeting_key` (an ordering bug) and any `wrong_type`
    are WARNINGs. Pending-cap refusals are a decline outcome on `mh_media_session_starts_total`.
  - Heartbeats are logged on state transitions only, never per heartbeat.

### 9. Verification

Per-service gates: `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md`. Highlights:
`assess` is table-tested with an injected clock and the actor with paused tokio time; the stranded
A/B case in MC Tier-1b, the Rust env-test and a browser spec; Layer-7 env-tests for crash (process
`kill -KILL` from a debug container, asserting a new `handler_ref` and a restart-count increment on an
unchanged pod (`mh.sh list --json`) — the address may be reused by the in-place restart; env-tests do
not observe `handler_id` — never `kubectl delete pod --force --grace-period=0`, which
defeats the slot lock and is guarded against), planned drain (`mh up` + `mh drain`, with and without
a target), and zombie (SIGSTOP/SIGCONT under `shareProcessNamespace: true`, set ONLY in a Kind overlay listed as
test instrumentation in the ADR-0038 parity allowlist; the kustomize guard asserts it is absent from
the base and every non-Kind render and present in the Kind render; with a preflight pod-spec check, a freeze budget derived from the live MH liveness probe, and an
assertion that the freeze took effect with restartCount unchanged; deterministic-tests-only fallback). Bounds are failure-only ceilings derived from deployed
config. The manual browser plan includes a planned transfer in a meeting older than max(join JWT
TTL, `media_dial` TTL). Round-4 gates:
- one meeting's hung heartbeat does not delay another's;
- **all-or-none apply**, parameterised over the refusal kinds and asserted on forwarded frames: a
  generation carrying one valid and one refusing change leaves the previous generation forwarding
  and reports it;
- a first apply that is refused still heartbeats, reporting 0 plus its refusal;
- a replacement's first heartbeat, arriving before MC processes its `RegisterMeeting` response,
  gets OWNED;
- OWNED and RELEASED arms, plus the `mc_id`-mismatch error;
- a token-only refresh causes no reconnect;
- DECLINED parks the ref until a newer token arrives, then exactly one dial;
- MH rejects tokens that are cross-meeting, wrong-aud, expired, wrong-type or for an unregistered
  meeting, and enforces the pending cap;
- MC join and GC reject MC-signed tokens;
- an empty join-time set followed by a set gives normal media, and past the bound exactly one notice
  with the session kept;
- GC selection is identical with and without replacement reports (seeded RNG).

Resilience legs use `mh.sh` owner-scoped teardown (`drain --owner <run> --all`) and the per-owner
leak check. The fixture waits for Prometheus to scrape a new MH before taking metric baselines.

### 10. Operations

Runbook and alert changes: `docs/debates/2026-10-28-mh-replacement-and-migration/runbook-alert-changes.md`
(rollout/rollback rewritten around `mh.sh`, node maintenance, new MC/MH/GC scenarios, address-pool
exhaustion, force-delete ban). The MH load-report interval becomes GC config
(`GC_MH_LOAD_REPORT_INTERVAL_MS`, pushed in `RegisterMHResponse`, published as a gauge), and the MH
staleness bound is derived from it (k × interval) — replacing the hardcoded 30 s and borrowed 60 s. The stale sweep stops skipping
DRAINING rows; old per-incarnation rows are deleted after a configured retention.
GC's hardcoded initial "up to 2" handler count becomes config (default 2). The image-prune keep set
for MH = images of every live MH pod ∪ the previous revision (per-slot Deployments have no rollout
history); ConfigMap generations referenced by a live MH pod are never pruned.
`MC_MAX_MEETINGS`/`MC_MAX_PARTICIPANTS` hard-fail on a bad value.

## Implementation Guidance

- **First**: infrastructure spike — Calico portmap with UDP hostPort plus NetworkPolicy in our Kind,
  proven by an env-test that a disallowed source is dropped. Fallback: StatefulSet-per-slot behind
  per-slot NodePort Services with `publishNotReadyAddresses`.
- Specialists: meeting-controller (heartbeat intake, `assess`, mutable versioned set, replacement,
  fence, bindings, dial-token serving), media-handler (heartbeat, verdicts, drain, close codes,
  idle timeout), global-controller (`AcquireMediaHandler`, registration rule, allowlists, staleness
  config, registry hygiene), protocol (internal + signaling contracts; deletions carry `reserved` lines
  only to satisfy the `buf breaking` gate), client
  (handler-set reconcile, dial tokens, close codes, metrics), infrastructure (slots, template,
  `mh.sh`, deploy.sh, guards), observability, operations, test.
- Ordering: contracts before consumers within self-contained commits; the empty-`connection_id`
  retirement and binding gate land before bulk binding release; MC-minted `media_dial` (with the
  guard allow-list and MH's pending-path deletion) lands before the
  handler-set change starts adding handlers mid-meeting. The env-test fixture migration off fixed
  `mh-0`/`mh-1` and `ENV_TEST_MH_{0,1}_*` (fixtures `mh_grpc`, `media`, `kube`, `metrics`,
  `egress_admission`, `deployment_config`; tests 01, 02, 29; layer7.sh port-forwards) to per-test
  slot-label resolution lands in the same commit as the slot/template change, or Layer 7 breaks.

## Consequences

### Positive

- A meeting survives permanent loss of a handler, and the same mechanism performs planned transfers
  (rolling upgrades, scale-down) with a small blip.
- Lost `Disconnected` notifications and leaked sender-id bindings self-correct within N.
- Two live MHs can no longer share a client URL; MH can be provisioned on demand.
- Several open TODO entries close or become moot (§ below).

### Negative

- A heartbeat per (meeting, handler) runs continuously, and one MH death produces one Dead
  declaration per meeting and one replacement request each, spread across handlers by GC's
  weighted-random selection (the same load shape as a top-of-hour join spike).
- MC re-pushes every participant's handler set every TTL/3 to refresh dial tokens.
- A meeting's first join waits one internal `RegisterMeeting` round-trip before its reply.
- A gray-failed MH stays GC-selectable (known gap, §3).
- A meeting whose MC endpoint now answers as a different `mc_id` (an MC restart) is held by MH, since
  MH never acts on `FAILED_PRECONDITION`, until the owner-silence release (default 15 min). It is
  visible on `mh_media_meetings_without_clients`.
- A planned transfer costs a small blip until multi-handler send lands.
- Kind's address pool is fixed at cluster creation (changing K recreates the cluster).

### Recorded residual risks (security)

- **S1b extended**: another MH pod can forge heartbeats for a dead one and suppress failover;
  detected by `mc_media_handler_client_loss_disagreements_total` only when the forger echoes live
  `connection_ids`; an empty-list forgery also empties the disagreement denominator (story 4 MC-E
  finding). A denominator the suspect MH cannot erase is a `docs/TODO.md` §Media Path Obligations
  item.
- **S6**: MH's bearer now crosses the plaintext MH→MC channel periodically (the TODO entry is
  updated in the allowlist commit). GC's per-zone endpoint allowlist is the registration-side bound
  on MC-bearer harvest via a rogue `grpc_endpoint`; it is not peer authentication and does not close
  S6.
- A stolen `media_dial` token is usable for TTL + skew (about 5.5 min). The impact is unchanged:
  ciphertext, or a binding-conflict DoS, because frames need the identity key.
- A scope holder that re-registers a meeting on an MH replaces its dial public key and binding
  endpoint there. That is the existing "MH does not verify meeting ownership" gap, and it yields
  ciphertext only.
- S6 now carries a public verification key, not a minting secret. Cross-meeting minting via AC is
  gone, because AC is off the dial path.
- GC's `controller_id` check is anti-misroute, not authentication.

### Neutral / out of scope

- MC failure (no stubs). MC's mc-0/mc-1 have the same surge-shares-URL bug → a `docs/TODO.md` entry.
- Production provisioning (per-incarnation DNS, MH-only wildcard cert, record teardown, provisioner,
  node firewalling under hostNetwork) belongs to the cloud-deploy debate; this design does not
  depend on it.
- Client connectivity as a primary trigger; dynamic capacity (adaptive-capacity story).

### TODO dispositions

Close in this story: "Missing alert: MC RegisterMeeting Coordination Failures"; "MC cannot see a
lost `NotifyParticipantDisconnected` on a LIVE MH"; the MH half of "Connectivity repair after an MH
or MC restart". Moot: "MC's `policy_generation` resets to 1…" and "`EndMeeting` has no fence against
a late `RegisterMeeting`" (no periodic re-push; the RELEASED fence also covers late registrations). Superseded: "`handler_id` is a per-INCARNATION token…" (stable handler id) — per-incarnation
`handler_id` is now the intended identity; rewrite the entry. Narrowed: "A meeting whose MC never sends
`EndMeeting` is never reclaimed" — the failed/exhausted-`EndMeeting` arm is reclaimed via the
RELEASED(MEETING_ENDED) for a meeting with no MC actor, and the MC-crash arm by the owner-silence
release (§6); MC failover itself stays out of scope.

## Participants

Round-4 scores (after the user's review):

- meeting-controller (93): per-meeting heartbeat owned by the meeting actor; Registering→Active ordering; MC-minted tokens.
- media-handler (95): heartbeat from registration upsert; all-or-none apply invariant; pending path deleted.
- global-controller (94): replacement evidence as counter + log event; gray-failure gap and TODO.
- protocol (94): `MeetingHeartbeat`, membership-epoch `handler_set_version`, public-key field.
- client (96): token per ref, refresh without reconnect, removal cleanup, empty join set.
- infrastructure (94): guard allow-list construct; `mh.sh` test contract; no new Secret.
- security (95): per-meeting Ed25519, register-before-token, `mc_id` owner check; residuals above.
- test (93): merged `mh.sh` contract and fixture; round-4 gates.
- observability (96): handler-level own-path warning; cause on RELEASED keeps false declarations countable.
- operations (95): restart-release wording; gray-failure manual runbook.

## Debate Reference

`docs/debates/2026-10-28-mh-replacement-and-migration/` — `brief.md`, `debate.md`, and the
supporting files named above.
