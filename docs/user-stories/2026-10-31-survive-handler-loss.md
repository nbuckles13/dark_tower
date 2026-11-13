# User Story: Survive a Handler Loss

**Date**: 2026-10-31
**Status**: Ready
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, client, database, protocol, infrastructure, security, test, observability, operations

## Story

As a **participant**, I want **my meeting's media to keep flowing when a media handler crashes, hangs or is taken down for maintenance, because the meeting moves to another handler on its own** so that **losing a handler costs a short blip instead of the meeting, and operators can upgrade or drain handlers without ending anyone's call**.

**Governing design**: `docs/decisions/adr-0039-mh-replacement-and-migration.md` (Accepted 2026-10-31), which supersedes ADR-0036 §8's re-assert recovery model and §9's frozen handler set. This is story 4 of the ADR-0036 arc (*Addendum — Planned Story Arc*), re-scoped from "Survive a handler restart" by `docs/debates/2026-10-28-mh-replacement-and-migration/brief.md`. Builds on story 3 (`docs/user-stories/2026-10-07-see-each-other.md`).

**Settled before planning** (brief + ADR-0039; not re-litigated here): backward compatibility is not a concern (commits still self-contained); MC failure is out of scope with no stubs; a new MH is never alive on the same client-facing URL as an old one, and MH is provisionable on demand in Kind; a planned transfer in this story costs a small blip (hitless is story 6's multi-handler send).

## Requirements

**Confirmed by the user 2026-10-31** after one review round. Synthesized from all 12 specialists' proposals; every item traces to ADR-0039 (section in brackets). Mechanism details the proposals named (series names, file paths, lock strategy) are carried into task descriptions, not dropped.

**User rulings in review (2026-10-31)**, amending ADR-0039 in the same change:
- *Owner-silence release* (R-11, ADR §6): MH ends a meeting after a long configurable period (default 15 min) with no communication from its owning MC. Closes the MC-restart capacity leak (MC ids are per process, so a restarted MC answers its predecessor's meetings with owner-mismatch). The leak until then is accepted.
- *Client connection and disruption telemetry* (R-42, ADR §8): dial outcomes and timing for every dial, disconnects by cause, disruption timers.
- *No per-MC limiter* (R-6, ADR §3/§5): it could not protect a shared MH or GC from other MCs and only delayed a crashed handler's tail meetings. Per-meeting minimum interval and attempt bound stay.
- *Replacement selection = initial selection* (R-23, ADR §3): GC's existing weighted-random, load-ratio selection, exclude list applied before the least-loaded cut. GC's load view is streams only and lags by one load-report interval; MH admission is the hard backstop. Accepted as for top-of-hour join spikes.
- *No mass-reconnect jitter mechanism* (ADR-0036 arc line): GC's randomized selection and per-client dial jitter spread the burst; the crash env-test and R-42 timers measure it; keyframe jitter only if data shows a need.

**Headline: the meeting survives**
- [ ] R-1: **Crash recovery** [§1–§4]. When a handler serving a meeting is killed, loses its node, or hangs so it stops heartbeating, MC declares it dead within a bound derived from the heartbeat timeout N, removes it, and every participant's media resumes on a surviving or replacement handler without anyone rejoining or reloading. Each participant's final set holds a handler not present before. The meeting never ends because a handler died. *(meeting-controller, client, test)*
- [ ] R-2: **Stranded routes complete** [§4]. Participants reachable only through the failed handler hear and see each other again once both reach the replacement, with no special case — the same as a fresh join. *(meeting-controller, client, test)*
- [ ] R-3: **Planned transfer** [§4, §7]. Draining a handler (`mh.sh drain`, upgrade, scale-down, SIGTERM) moves each of its meetings make-before-break: the set adds the new handler, then drops the old one; clients are told to drop it before it ends the meeting. A small blip only: no "reconnecting", no transport fault, no session shed, and no observation on the R-33 metric. With no grant, participants move to surviving handlers. Stragglers at the drain deadline are cut over and counted timed out. Sessions are shed only when there is neither a survivor nor a grant. *(meeting-controller, media-handler, operations, test)*
- [ ] R-4: **Impaired handlers** [§1, §2]. A handler that keeps heartbeating but stays behind on the meeting's policy past a configured lag bound, or refuses it for capacity, is replaced the same way. A contract-fault refusal (invalid policy, transport mismatch) is loud and counted but not replaced. A handler that catches up before the bound is not replaced. *(meeting-controller)*
- [ ] R-5: **Calls of any length** [§5]. Connecting to a handler — at join, when one is added, on redial — works at any point in a meeting, including calls older than both the join-token and dial-credential lifetimes, for signed-in users and guests, without user action. *(security, client, meeting-controller)*
- [ ] R-6: **Removal never waits on GC; replacement is bounded** [§3]. With a surviving handler, media continues on it even when GC is down or out of capacity. A meeting left with no handler is visibly "awaiting handler" (counted, paged past a config-derived bound) and recovers by itself when GC later grants one. Retries are bounded and jittered; a "not this meeting's MC" or malformed-request answer is never retried and is loud. Replacement activity is bounded per meeting (minimum interval, attempt bound derived from GC's advertised exclusion cap), so a flapping detector cannot churn a meeting or flood GC; there is no per-MC limiter (user, 2026-10-31), so a crashed handler's meetings all start migrating at once. *(meeting-controller)*

**Liveness and cleanup**
- [ ] R-7: **Heartbeat** [§1]. For every meeting it holds, MH sends that meeting's MC one heartbeat per MC-declared interval, starting at registration even before any policy applies, reporting the applied policy generation (0 if none), the latest deterministic apply refusal, serving state (with remaining drain time), the meeting's connections and the registered MC identity. Heartbeats are not retried; one meeting's slow heartbeat never delays another's; a wedged MH stops heartbeating. A zero declared interval refuses the registration; MH has no cadence setting of its own. gRPC channel events never cause a declaration. *(media-handler, protocol, meeting-controller)*
- [ ] R-8: **All-or-none apply** [§1]. A policy refused for any reason (stale, invalid, transport-inconsistent, per-meeting bound, stream ceiling, edge cap, transient) leaves the previously applied policy forwarding unchanged, and both the registration response and heartbeats keep reporting that generation. A refused first policy reports 0 plus the refusal. *(media-handler)*
- [ ] R-9: **Heartbeats only correct, never add** [§1, §2, §6]. A heartbeat never adds a meeting, handler, address or connectivity to MC's state, and a stale or replayed heartbeat never counts as proof of life. A lost disconnect notification self-corrects (connectivity and sender binding) within about two intervals, and a heartbeat never removes a connection MC confirmed after the previous heartbeat. *(meeting-controller, security)*
- [ ] R-10: **A removed handler is forgotten and fenced** [§6]. Once removed from a meeting a handler is never re-added; any connection it later reports is refused with no binding recorded; its bindings are released only after clients were told to drop it; it is sent `EndMeeting` with the cause best-effort, and a failure never blocks removal. Its heartbeats are answered RELEASED with the cause, and MH then stops serving that meeting within one interval, closing sessions with `HANDLER_RELEASED`, sending no disconnects and reclaiming capacity. This holds for a zombie (alive but cut off) once its path heals. A heartbeat for a meeting MC has no record of gets RELEASED (meeting ended); one for a live meeting that never held the handler is counted as a misroute. *(meeting-controller, media-handler, security, test)*
- [ ] R-11: **Only the owner releases** [§6]. A heartbeat answered by any MC other than the meeting's registered one gets an error and never ends the meeting; short MC silence or unreachability never ends a meeting. **Owner-silence release** (user, 2026-10-31; amends ADR-0039 §6): MH ends a meeting after a configurable period (default 15 min) with no communication from its owning MC — a registration, teardown or heartbeat verdict from that MC; errors and owner-mismatch answers don't count — closing sessions with `HANDLER_RELEASED` and counting the release under its own cause. This reclaims capacity after an MC restart (a restarted MC has a new per-process id, so it answers its predecessor's meetings with owner-mismatch). Not an MC-failover mechanism. *(media-handler, meeting-controller)*
- [ ] R-12: **Register before anyone dials** [§3]. No client is told about, or given a credential for, a handler before it has confirmed the meeting's registration — first join included: the first join's reply arrives after the initial handler confirms, without stalling anyone else's traffic. If no initial handler confirms, the join still succeeds with an empty set and recovers through R-6. MH never holds a connection for a meeting it is not registered for. *(meeting-controller, media-handler, security)*

**Dial credentials**
- [ ] R-13: **Admission at the handler** [§5]. MH admits a client only on a current MC-issued credential for that meeting and that handler. A credential for another handler is refused with `WRONG_HANDLER`, only after full validation. Every other failure — expired beyond a small configurable skew (≈30 s), bad signature, wrong type, cross-meeting or unregistered meeting, malformed, oversized, an AC meeting token — is refused at once with `DECLINED`, nothing held. A credential is reusable within its lifetime. Connections awaiting MC's bind answer are capped per (meeting, participant). A participant who has left but holds a valid credential is refused with `DECLINED` and receives no media. *(media-handler, security)*
- [ ] R-14: **Delivery and refresh** [§5]. Every handler-set message carries a credential for every member, refreshed before expiry (lifetime configurable, default 300 s; MC refuses to start outside [60, 900] s). A refresh causes no reconnect. A participant's dial-failure report makes MC promptly re-send that participant's set, with no routing effect. After `DECLINED` the client makes no further dial to that handler until a fresher credential arrives, then exactly one. *(meeting-controller, client)*
- [ ] R-15: **Credential and key isolation** [§5]. MC join and GC user auth refuse a dial credential, and AC tokens keep working there. The signing key is per meeting, from a CSPRNG, held only in MC memory, never persisted or logged, and dies with the meeting; only its public verification key reaches the handler. The MC→MH contract carries no secret or media-key material: validation fails on any key-material field other than the one allow-listed public key, and on a stale, renamed or non-bytes allow-list entry. Credentials never appear in logs, metrics, traces, errors, serialized state or the UI; the client holds only the latest per in-set handler, cleared when it leaves the set and at session end. *(security, meeting-controller, global-controller, infrastructure, client)*
- [ ] R-16: **Media continuity across a swap** [§4]. A handler change causes no meeting-key action. A publisher's key ids keep increasing across the swap (never reused under one KEK); video on the new handler resumes from a keyframe; a frame arriving via two handlers during an overlap is played once. *(client, security)*

**Client signalling and behaviour**
- [ ] R-17: **Handler set** [§4]. Clients receive the meeting's handler set as a complete snapshot at join and on every membership change or credential refresh. Its version changes only when membership changes. Each handler has an MC-assigned reference never reused within the meeting, even when a later handler reuses the address. Send directives, slot assignments and client connection reports name handlers by reference only; addresses reach clients only from MC's set as granted by GC, and a client-supplied address is never a target. A later change to GC's hand-off record does not alter a live meeting's handlers. *(protocol, meeting-controller, security)*
- [ ] R-18: **Client reconcile** [§4]. A join with an empty set shows "connecting media" (no error) and media starts without user action once a handler is listed; if none is listed within a configured bound, exactly one non-fatal notice and the session stays up. An added handler is connected without interrupting media on the others. A removed handler's connection is closed with no fault and no loss report, even if the server left it open. A new incarnation at a reused address gets a real reconnect. Losing an in-set handler is reported to MC at that transition and redialled with jittered backoff; the client never dials outside the latest set. *(client)*
- [ ] R-19: **Close codes** [§7]. Every intentional MH session close (decline, release, drain-deadline shed) carries exactly one numeric close code; zero is never sent; codes are sent only after credential validation (every credential failure is `DECLINED`) and only about the caller's own meeting; no reason strings reach the client; a close without a code means transport loss. The client treats wrong-handler / binding-conflict / MC-unavailable as counted and retried (not loss), released as expected, declined per R-14, no code as loss, and an unknown or zero code as a counted contract fault. *(media-handler, client, protocol)*
- [ ] R-20: **What users see** [§4]. Tiles whose handler was lost show "reconnecting" — never a frozen live frame, never a vanished participant — until the slot has a source again or a configured cap expires. A planned transfer shows no "reconnecting". The debug panel and the e2e test bus show the set version and each handler's reference and address, never credentials. *(client)*
- [ ] R-21: **A silent MH death is noticed by clients** [§7] within MH's QUIC idle timeout, which becomes configuration sized near N, with no app-level keepalive. *(media-handler)*

**Media handler lifecycle and GC**
- [ ] R-22: **MH drain and registration** [§7, §10]. On SIGTERM MH immediately reports DRAINING to GC (and keeps reporting it) and in every meeting's heartbeat with remaining drain time, keeps forwarding and stays reachable, and exits when it holds no meetings or at grace minus margin, logging how many it shed; it no longer sheds sessions at shutdown. MH sends a load report immediately after every GC registration, retries a rejected registration (including an address conflict) while not ready, and uses GC's pushed load-report interval. Each MH process is its own handler identity with no process-start epoch, and reads its advertised address only from injected config. *(media-handler)*
- [ ] R-23: **Replacement grant** [§3]. On request from a meeting's MC, GC returns one registered handler that is healthy, recently heard from, under capacity and not excluded, or an explicit "no capacity". The grant uses the same weighted-random, load-ratio selection as initial allocation (one function, user 2026-10-31), with the exclude list applied before the least-loaded cut, so a burst of replacements from many MCs spreads across handlers. A caller that is not the meeting's MC is refused (anti-misroute; the id is self-asserted); a malformed or over-length request is refused as invalid; unknown excluded ids are ignored. A grant is issued even when GC still sees the reported handler as healthy, and reports never change which handler GC selects. Every MC learns GC's exclusion cap at registration; a zero cap fails MC registration. *(global-controller, meeting-controller, protocol)*
- [ ] R-24: **Registry freshness** [§7, §10]. A handler is never selected once it reports draining. One that stops reporting, draining or not, drops out within a single staleness bound derived from the configured load-report interval, which governs the sweep, selection and the registration rule alike. Entries older than a configured retention are deleted, and a live MH whose entry was deleted is counted as unknown. GC refuses to start if retention ≤ the staleness bound. *(global-controller, database)*
- [ ] R-25: **Registration rule** [§7]. GC refuses an MH registration whose client-facing or internal address is outside per-zone configured allowlists, or that changes an existing incarnation's addresses. A new incarnation reusing both addresses of an existing entry supersedes it whatever its state (an in-place restart: two live processes cannot hold one pod IP and port). One sharing a single address supersedes an entry that is draining, unhealthy or stale, and is refused with a distinguishable reason if that entry is fresh and serving — a live serving handler is never displaced by a one-sided match. Concurrent overlapping registrations never leave two entries sharing an address, and supersede is atomic. No schema migration. *(global-controller, database, security)*
- [ ] R-26: **GC configuration** [§10]. The load-report interval (pushed to MHs), the derived staleness bound, the initial handlers per meeting (default 2) and the registry retention are configuration, visible in-cluster. *(global-controller)*

**Kind provisioning**
- [ ] R-27: **Gate zero** [Implementation Guidance]. Before any slot change lands, an env-test proves a host client reaches an MH over a UDP host-port slot end to end, and that the MH network policy drops a disallowed source on it. If either fails, the fallback (StatefulSet per slot behind per-slot NodePorts) is adopted and recorded; no unpolicied UDP ingress ships. *(infrastructure, test, security)*
- [ ] R-28: **Address uniqueness** [§7, §9]. In Kind no two live MH processes ever share a client-facing address — not during upgrade, replace, eviction or crash restart; asserted after every resilience leg. Validation fails on any tooling, test or demo that force-deletes a pod with zero grace. *(infrastructure, test, security)*
- [ ] R-29: **`mh.sh`** [§7, `mh-sh-contract.txt`]. `up` returns only when the new MH is GC-selectable and fails at once with a capacity reason when no slot is free. `drain` keeps the MH serving on its own address until it exits, frees the slot only then, and refuses the last serving MH (without `--force`), another owner's MH, and any MH stuck terminating. `upgrade` is start-new-then-drain-old, one at a time, resumable, refuses before touching any old MH when no slot is free, and never drops below baseline. The machine contract holds from host and devloop container alike (JSON stdout, one result line, grouped exit codes, idempotent, owner-scoped teardown and leak check, concurrent runs cannot remove the last serving MH), proven by a hermetic Layer-3 self-test. *(infrastructure)*
- [ ] R-30: **Cluster baseline and rollback** [§7, §10]. A fresh Kind cluster comes up with baseline B serving MHs (default 2, one configurable home); K = 6 slots fixed at creation. Rollback is the upgrade run from the previous checkout. Every live MH, including an old revision mid-upgrade, can crash-restart in place because its image and config generation survive pruning. The first deploy (and any rollback across it) requires a cluster recreate. *(infrastructure, operations)*
- [ ] R-31: **Deploy guards** [§7]. Validation fails if any MH workload loses the properties that make it scraped, reachable by MC, policy-covered and browser-trusted; if the process-namespace test instrumentation appears outside the Kind render or is missing from it; or on drift between the slot table, host port mappings, slot addresses and GC's allowlists. Every new config value is a required reference (missing fails at startup), and retired keys are absent everywhere. *(infrastructure, security)*
- [ ] R-32: **Dev preflight** [§7, retirement of fixed instances]. `pnpm dev`'s WebTransport preflight checks the live MH pool: it fails loudly when fewer than baseline handlers are live, treats empty slots as normal and never assumes fixed instances. *(client — see clarification 2)*

**Telemetry**
- [ ] R-33: **The user-required metric** [§8]. At every dead or impaired declaration (never a planned transfer) an operator can read how many of the clients MC believed connected to that handler had already reported losing it, as a count and a 0–1 share, plus their lead time — counted at MC receipt, no identities. MH's client idle timeout and N are published side by side as config gauges. A companion counts "most clients report loss while heartbeats look healthy". *(meeting-controller, observability)*
- [ ] R-34: **Operators can see the mechanism** [§8, `telemetry-contract.md`]. Per the contract and its single `trigger` vocabulary: declarations, near-misses, detection time, migration duration and outcome, meetings with no handler, MC's own-path signature, heartbeat verdicts and lag, connected-set corrections, false declarations on MH, drain, credential rejections by reason, handler-set pushes, the client media-dark window and lost-handler count, GC replacement/registration/load-report/availability series and the per-report log event, and config gauges — each on its owning dashboard. No meeting, participant, handler or connection id label anywhere; heartbeats logged on transitions only; credentials and keys never logged. *(observability, all service specialists)*
- [ ] R-35: **Alerting posture** [§8]. A single handler death never pages by itself. Pages: meetings awaiting a handler past the bound, registration ping-pong, fleet/region-level `MHDown`, generation divergence (re-assert text removed). Warnings per the contract. Every added or changed alert's runbook anchor resolves in the same commit. *(observability, operations)*
- [ ] R-42: **Client-side connection and disruption telemetry** (user, 2026-10-31; amends ADR-0039 §8 and the telemetry contract's Client section). Through the existing SDK export path, with bounded labels and no identities, operators can read: every MH dial attempt (join, added handler, redial — not only the first) by outcome (connected, each close code, transport failure, timeout, aborted by removal) and its time to connect; every MH disconnect by cause (transport loss, each close code, directed removal); media-disruption timers — the per-slot media-dark window and the per-handler time from loss to reconnected or abandoned; and the lost-handler count. Existing first-connection series are extended, not duplicated. Client proposes the full shape in design, expanding where it makes sense. *(client, observability)*

**Retirements, operations, verification**
- [ ] R-36: **Retired surfaces are gone** [§1, §3–§6, §8]. Periodic re-assert and the process-start epoch; empty-connection-id legacy mode (an empty id is refused); the join-time server list; handler addresses on send targets, slot assignments and client reports; the meeting JWT on MH connect; MH's pending-before-registration path, its timeout config and metric; the set-divergence counter and generation-divergence gauge. Each leaves wire and code together with its catalog entries, panels, alert and runbook references, and generated client code proves its absence. *(protocol, meeting-controller, media-handler, observability)*
- [ ] R-37: **Runbooks and configuration reference** [§10, `runbook-alert-changes.md`]. Every scenario in the changes list exists with its procedure before the story closes (rollout/rollback around `mh.sh`, node maintenance, pool exhausted, upgrade stuck, declared dead/impaired, awaiting handler, migration timed out, one MC declaring several handlers, drain shed, meetings held without clients, MC rollout releasing meetings, two live handlers on one address, gray-failure manual procedure, force-delete ban), retired items removed. Every new or changed key is documented with default and range; MC refuses to start on a missing or invalid heartbeat timeout or meeting/participant cap. *(operations, meeting-controller)*
- [ ] R-38: **Recorded decisions and gaps** [§3, Consequences, TODO dispositions]. `docs/TODO.md` closes or marks moot the entries ADR-0039 names, adds the gray-failure quarantine (with its authenticated-MC-identity precondition), the MC rollout shared-URL bug and a reference to CI running no Kind/Layer 7, and updates the S6 and MH-ownership residuals. The SLO register replaces the applied-generation-echo line with migration completion and media-dark window, target open. *(operations, observability, security, global-controller)*
- [ ] R-39: **Resilience env-tests** [§9, `test-gates.md`]. Live-cluster legs for crash (process kill), stranded pair (Rust and browser), planned transfer with a target (Rust and browser) and without, zombie (freeze/resume; the injector proves the freeze took effect or fails loudly; deterministic-only fallback), left participant, and the address invariant — bounds are failure-only ceilings from deployed config. Each leg restores the baseline pool, leaks no owned MH, fails loudly on foreign pool activity, and takes metric baselines only after Prometheus scrapes a new MH. Existing env-tests find MHs by slot and pass in any order, landing with the slot change. *(test, infrastructure, client)*
- [ ] R-40: **Deterministic coverage** [§9]. Health decisions, heartbeat deadlines, credential refresh and drain deadlines are tested on injected or paused clocks with no sleeps; every negative test has its positive arm; metric assertions use ADR-0032 semantics. Each service's `test-gates.md` section is implemented in that service's own task, reviewed by test. *(test)*
- [ ] R-41: **Manual test plan** (`2026-10-31-survive-handler-loss-manual-test-plan.md`), run before `/close-story`: crash by process kill; `mh.sh up` + drain with and without a target; upgrade with live meetings; freeze/resume; stranded pair; a planned transfer in a call older than both credential lifetimes; pool exhaustion refused with the old handler still serving; an MC rollout releasing its meetings. *(test, operations)*

---

## Architecture Validation

**Result**: PASS — all twelve specialists. Every contract change is a new RPC on an existing service (`MeetingHeartbeat` on MH→MC `MediaCoordinationService`; `AcquireMediaHandler` on MC→GC `GlobalControllerService`, which MC's existing `service.write.gc` scope already passes), or new/changed messages and fields on existing channels. The MC→AC dial-token path was dropped in ADR-0039 round 4 and never existed in code; auth-controller opts out (verified: no dial endpoint, scope or counter exists; R-5, R-13, R-15, R-23, R-36 interface-checked correct). database opts out of owning a task (no migration, no index; recommends against unique endpoint indexes because rollback is code-only) and reviews GC-1, GC-2 and GC-3.

---

## Design

### Meeting Controller

**What changes.** ADR-0036 §9's frozen per-meeting handler set becomes an MC-owned, versioned, mutable set, and the MC meeting actor becomes the single owner of each handler's liveness, health decision and removal (ADR-0039 §2–§6).
- **Handler set** (`media_routing/placement.rs`):
  - Versioned (membership epoch), possibly empty.
  - Per-incarnation `handler_ref`, never reused in a meeting.
  - Registering → Active per handler, and a one-way removed record.
  - Redis `MhAssignmentData` is read once at actor creation; later joins stop reading it and the divergence counter is deleted.
  - Clients receive `MediaHandlerSet` snapshots listing Active handlers only.
  - The first join's reply is parked, non-blocking, until the initial RegisterMeeting ack (MC-A).
- **Heartbeat intake** (`grpc/media_coordination.rs` → controller → meeting actor, oneshot verdict):
  - Receipt stamped at the gRPC edge.
  - Verdicts: OWNED (including Registering); RELEASED with cause (removed → trigger, no actor → MEETING_ENDED, never in set → UNSPECIFIED). An mc_id mismatch gets FAILED_PRECONDITION.
  - Remove-only reconcile with the previous-receipt rule, which also releases bindings.
  - The binding gate is flipped (record only after membership), and the empty-connection_id legacy mode is deleted (MC-B1).
- **Detection** (`media_routing/handler_health.rs`, pure `assess`):
  - Dead = age > N from the first confirmed RegisterMeeting. Lagging is reversible and re-publishes the snapshot. Impaired comes after L = 2N or a capacity refusal.
  - Contract-fault refusals are loud, never replaced.
  - Removal order: set update → bulk binding release → push-worker drain → `EndMeeting(cause)` best-effort. A meeting left with no handler shows on the awaiting-handler gauge (MC-B2).
- **Dial credential**: a per-meeting Ed25519 keypair in actor memory, with the public half in RegisterMeeting. A kid-less EdDSA JWS is minted for every set member and refreshed every TTL/3. Client status is keyed by handler_ref, and a FAILED report triggers a per-session-limited re-push (MC-D).
- **Replacement**: GC `AcquireMediaHandler` from a version-tagged spawned task, with exclude = current ∪ removed ∪ refused.
  - Per-meeting minimum interval and an attempt bound derived from GC's cap; no per-MC limiter.
  - Recovery from awaiting-handler on a later grant.
  - (MC-C covers crash/impaired replacement.)
- **Planned transfer** on DRAINING: make-before-break through MC-C's acquisition path, with the survivor target, cutover at `drain_remaining_ms − MC_PLANNED_TRANSFER_MARGIN_MS` (`timed_out`) and shed only with neither (MC-F).
- **Telemetry**: the required loss metric at Dead/Impaired declarations, the client-loss disagreement counter, and a HandlerCensus (MutedSourceCensus pattern) for first declarations and handlers in use (MC-E). Each task emits its own `mc_media_handler_*` series per `telemetry-contract.md`.

**Tasks.**
- MC-A ← P2.
- MC-B1 ← P1, MC-A (lands before MH-2).
- MC-D ← SEC-A, P1, MC-A (before C1 and MH-1).
- MC-B2 ← MC-B1, MH-2, T-1 (crash-survivor and zombie legs).
- MC-C ← GC-3, MC-D, C1, MH-1, T-1, MC-B2.
- MC-F ← MC-C, MH-3.
- MC-E ← C1, MC-B2 (light).

**Interfaces.**
- FROM protocol: P1/P2 additive. The `rpc MeetingHeartbeat` line rides MC-B1. P3 deletes the legacy client fields and MC's population lines.
- FROM media-handler:
  - MH-2 deletes `process_start_epoch_ms` and MC's log reads.
  - MH refuses `heartbeat_interval_ms = 0` only after MC-B1 populates it.
  - MH-3 supplies DRAINING and `drain_remaining_ms` (consumed by MC-F).
- FROM global-controller: GC-3 owns the AcquireMediaHandler slice and populates `max_exclude_handler_ids` (default 16, ≈ 14 lifetime replacements per meeting, then `rate_limited`).
- FROM client: C1 follows sets by ref, sends MhConnectionStatus with ref + version, and dials with media_dial.
- FROM test: T-1's pool fixture and injector.

**Config.**
- `MC_MH_HEARTBEAT_TIMEOUT_MS` (N): default 6000, range (0, 30000]. Interval N/3 is sent per meeting; L = 2N.
- `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS`: default 300, range [60, 900], refresh TTL/3.
- `MC_MAX_MEETINGS` / `MC_MAX_PARTICIPANTS` hard-fail on a bad value.
- `MC_PLANNED_TRANSFER_MARGIN_MS`, `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` and `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS`: see Assumptions.

**Assumptions (Lead rulings, for user review).**
- Planned-transfer cutover margin: `MC_PLANNED_TRANSFER_MARGIN_MS`, default 5000, range [1000, 30000], published as a config gauge. Cutover happens at `drain_remaining_ms − margin` (MC-F).
- Client-loss disagreement: `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION`, default 0.5, range (0, 1]; `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS`, default 3, ≥ 1. Both are published as config gauges (MC-E).
- Lost-Disconnected self-correction (R-9) cannot be injected in Kind (a live MH always sends Disconnected), so it is covered at Tier 1b only.

### Media Handler

**What changes**
- **Liveness (MH-2).**
  - One `MeetingHeartbeat` per held meeting per MC-declared interval, starting at the registration upsert (before any apply). The schedule is a timer arm in the session actor, with the first deadline jittered in [0, interval).
  - Each beat is built inside the actor, so a sent beat proves the actor is alive. It reports the applied generation (from the routing snapshot), the latest deterministic apply refusal, serving state and the connection set.
  - Beats go out from a sender task on a cached channel per MC endpoint: at most one outstanding per meeting, deadline below the interval, no retry.
- **Fencing (MH-2).**
  - RELEASED(cause) ends the meeting through `handle_end_meeting`: sessions close `HANDLER_RELEASED` and no `Disconnected` is sent.
  - OWNED does nothing. Owner mismatch, unspecified verdicts and transport errors are counted, with no action.
- **All-or-none apply (MH-2).** Already true in code. This story pins it with a frame-level, parameterised test and states it once at `RegisterMeetingResponse.applied_generation`.
- **Admission (MH-1).**
  - Only MC-minted `media_dial` tokens, verified against the meeting's registered Ed25519 public key (exactly 32 bytes, installed by the registration upsert).
  - Fixed order: size cap; alg EdDSA (key-locating headers ignored, no key fetch); registration lookup; signature; claims with a configurable skew (default 30 s, ≤ 60); then aud, which fails with `WRONG_HANDLER`. Every earlier failure is `DECLINED`.
  - Tokens are reusable within their TTL.
  - Connections awaiting the bind are capped per (meeting, participant).
- **Pending path deleted (MH-1).** No client is held for an unregistered meeting. `PendingConnection`, the registration timeout and its metric go.
- **Close codes (MH-CLOSE).** Every intentional close carries one numeric `MhCloseCode`. A close without a code means transport loss.
- **Drain (MH-3).** On SIGTERM MH reports `HEALTH_STATUS_DRAINING` to GC immediately and keeps reporting it, sends every meeting an immediate DRAINING beat with the remaining time, and keeps forwarding. It exits when empty or at grace − margin, logging the shed count. The 2 s settle cap goes. This reverses ADR-0036 §11.
- **GC registration (MH-3).** A load report goes out immediately after every successful registration; rejected registrations are retried while not ready.
- **QUIC idle timeout (MH-3).** Becomes config, sized near N.
- **Owner-silence release (MH-3).** A per-meeting monotonic timer reset only by validation-passing owner messages or answered verdicts (which MH-2 provides). `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS`, default 900, validated to [300, 3600]. Counted as `trigger="owner_silence"`. It is not an MC-failover mechanism.
- **Identity (MH-2).** `process_start_epoch_ms` and `process.rs` are deleted; each process is its own handler.

**Interfaces**
- From protocol:
  - P1: heartbeat messages, `ReleaseCause`, the key and interval fields, and `EndMeeting` cause;
  - P2: `MhCloseCode`.
  Deletions ride consumers: the epoch goes in MH-2; `join_token` goes in P3 after MH-1.
- From MC:
  - MC-B1 serves the heartbeat rpc with verdicts and a non-zero interval. It must land before MH-2 (the rpc must be served and the interval populated), and so before MH-3's owner-silence release, which an UNIMPLEMENTED answer would never reset;
  - MC-B2 detection lands after MH-2;
  - MC-D minting and MC-A register-before-token land before MH-1.
- From client: C1 populates `media_dial_token` alongside `join_token` before MH-1. MH never accepts both credential types.
- From infrastructure: I-2 moves MH's two `include_str!` manifest tests to the single template and keeps one `configMapKeyRef` per key. MH reads only its injected advertise addresses, with no Rust change for slots.
- To MC: the heartbeat per R-7, drain state, and `drain_remaining_ms` for planned transfer (MC-F).
- To observability: the MH series in the telemetry contract. `owner_silence` is the one MH-local trigger value, and it warns only with clients present.

**Assumptions**
- New MH keys (names proposed, all required refs): `MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS` (30), `MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT` (2), `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (900), `MH_MAX_IDLE_TIMEOUT_MS` (≈ N). Retired: `MH_REGISTER_MEETING_TIMEOUT_SECONDS`, the constant `MAX_IDLE_TIMEOUT_SECONDS`.
- There is no per-registration interval check (lead ruling). The config reference states the relationship: MC's N ≤ 30 s, so its interval is ≤ 10 s, and silence ≥ 300 s is ≥ 30 intervals.
- A RELEASED burst after an MC restart is bounded at one per meeting per interval, spread by the jitter. Each release costs one O(meetings) routing-snapshot rebuild, which is acceptable at deployed sizes.
- The zombie env-test leg is MC-B2's; MH reviews it. The left-participant leg is in MH-1, which also switches the Rust env-test MH connect to media_dial_token (depends on T-1). C1 and MH-1 depend on MH-CLOSE (P2 only).

### Global Controller

**What changes.** GC stays a stateless placer: it persists nothing per meeting, and MC's versioned handler set is the only authority (ADR-0039 §3). Three tasks:

- **GC-1: registry freshness + config** (R-24, R-26). One MH staleness bound = `GC_MH_STALENESS_MULTIPLIER` (k, default 3) × `GC_MH_LOAD_REPORT_INTERVAL_MS` (default 10000, pushed to MH in `RegisterMHResponse`). It replaces the hardcoded 30 s candidate window and the borrowed 60 s MC threshold. It drives selection, the stale sweep (which stops skipping DRAINING rows) and `gc_mh_available`. Rows past `GC_MH_REGISTRY_RETENTION_SECONDS` (default 3600) are deleted; startup refuses retention ≤ the bound. The initial handler count becomes `GC_MH_INITIAL_HANDLERS_PER_MEETING` (default 2). New series: `gc_mh_load_reports_total{outcome=accepted|unknown_handler}`, `gc_mh_available{region,state}`, interval and staleness config gauges. Also rewrites `docs/DATABASE_SCHEMA.md` §8.
- **GC-2: registration rule** (R-25). Per-zone allowlists (`GC_MH_WT_ALLOWED_HOSTS/_PORTS`, https; `GC_MH_GRPC_ALLOWED_SCHEMES` = `grpc` cleartext h2c today, `_CIDRS`, `_PORTS`) with Kind values rendered from I-2's slot table. Then one transaction under `pg_advisory_xact_lock(hashtext('gc_mh_registry'))`:
  - an existing id with changed endpoints → `endpoint_changed`;
  - both endpoints match another row → supersede;
  - one-sided match against a draining/unhealthy/stale row → supersede;
  - one-sided match against a fresh serving row → `address_conflict`.

  No migration and no unique index. It writes the RegisterMH rule comment block in `internal.proto`, adds `gc_mh_registrations_total{outcome, match}`, and adds the S6 TODO line.
- **GC-3: AcquireMediaHandler** (R-23). It adds the rpc, request/response and `RegisterMCResponse.max_exclude_handler_ids` (`GC_MH_MAX_EXCLUDE_HANDLER_IDS`, default 16). One weighted-random selection function serves initial allocation and replacement, with the exclude list applied in the candidate query before the LIMIT.
  - Ownership: the active `meeting_assignments` row supplies the region.
  - Statuses: OK / RESOURCE_EXHAUSTED / FAILED_PRECONDITION (not owner) / INVALID_ARGUMENT; unknown ids ignored.
  - `gc_view` is a label, never a veto.
  - Series: `gc_mh_replacement_requests_total{trigger,outcome,gc_view}` plus duration, and the `gc.mh.replacement_report` INFO event.
  - Also the R-15 token pin and the gray-failure TODO entry.

**Interfaces (agreed directly).**
- protocol: P1 (P1) supplies `ReleaseCause` only. GC writes its own proto slices, paired with protocol, so the rpc and its impl land together with no UNIMPLEMENTED stub.
- infrastructure: I-2's single slot-table source function in deploy.sh feeds both the per-slot MH ConfigMaps and GC's allowlist values. I-3's drift guard depends on GC-2.
- meeting-controller: MC-C depends on GC-3. MC's zero-cap registration check lands there. Attempt bound = cap − current set size, about 14 lifetime replacements per meeting at the default, then `rate_limited`.
- database reviews all three tasks: numeric ms interval binds, the single registry lock, the §8 rewrite, and the retention-vs-staleness startup check.
- observability: each GC task lands its panels and catalog entries (dt-guard application-metrics checks 5/6). Alert rules are OBS-ALERTS's.
- security: the three-arm token pin (no kid; live AC kid with a foreign signer; HS256) plus an AC-token positive arm. The allowlists are the registration-side bound on MC-credential harvest, not peer authentication.
- test: GC-2's env-test asserts that a fresh deploy registers every MH with no `invalid_endpoint`/`address_conflict`. Supersede on crash-in-place and replacement grants ride in the crash legs of T-1/MC-B2/MC-C.

**Assumptions made (flag if wrong).**
- Defaults and ranges not fixed by the ADR: interval [1000, 60000] ms; k default 3, [2, 10]; retention default 3600 s; initial handlers [1, 4]; cap [4, 64], and startup refuses cap ≤ initial count.
- `unregistered` is the `gc_view` value when no replaced_handler_id is given or no row exists.
- No GC env-test calls AcquireMediaHandler directly, since that would need MC credentials. Its cluster coverage is MC's crash leg.
- `meeting_assignments.rs`'s MC staleness constant is untouched (MC rows, out of scope).

### Client

**What changes**
- The single-use MH connect at join becomes **reconcile-to-`MediaHandlerSet`**, diffing on `handler_ref`. Join and migration share one path.
  - The client dials members, closes non-members as a directed removal (no fault, no loss report, even if the server left the transport open), and redials lost in-set members with full-jitter backoff (first retry ≥ one keepalive interval).
  - It never dials outside the latest set.
  - A token-only refresh (same refs) causes no dial and no close. A new ref at a reused URL gets a real reconnect.
  - An empty join-time set means "media connecting", with one bounded non-fatal notice.
- **Send and receive resolve handlers by ref.** A non-member ref is counted and ignored. The SDK stops reading `media_servers`, the URL fields and `mh_url`; P3 deletes them.
- **Dial credential**:
  - The client holds the latest `media_dial` token per in-set ref, only in transport set state (scoped R-23 amendment). It is cleared on removal and at session end, never logged, and opaque.
  - C1 populates `MhConnectRequest.media_dial_token` alongside `join_token`. MH-1 flips MH to media_dial only, and P3 deletes `join_token`.
  - `DECLINED` parks the ref, reports FAILED (which triggers MC's re-push), and makes exactly one dial on a newer token.
- **Close codes**:
  - wrong-handler, binding-conflict and MC-unavailable: counted, retried, not loss;
  - released: expected (grace, then loss, if the ref is still in the set);
  - declined: as above;
  - no code: loss;
  - zero or unknown: a counted contract fault.
- **Loss reports**: `MediaConnectionUpdate` on every per-ref transition, with `handler_ref` + `handler_set_version`, instead of once at join.
- **UX**: a client-local *reconnecting* slot state (no wire SlotState): set on observed loss or removal, cleared on a source or a config cap, never shown on a planned transfer. A "connecting media" state. The debug panel shows the set version, refs and URLs, never tokens.
- **Continuity**: SessionMedia and TransmitKeyManager are never rebuilt; key ids strictly increase; a new target keys a group (keyframe); overlap duplicates are dropped once.
- **Telemetry (R-42, all in C2 after the 2026-11-01 resize)**, names frozen in `telemetry-contract.md` §Client:
  - `dt_client_mh_connection_total{phase, outcome}` (every dial)
  - `dt_client_time_to_first_mh_connected_ms`
  - `dt_client_mh_dial_duration_ms`
  - `dt_client_mh_disconnects_total{cause}` (lost = transport_loss | handler_released)
  - `dt_client_mh_reconnect_duration_ms`
  - `dt_client_media_resume_ms`
  - a replay cross-handler reason token
  The two existing series leave the grandfathered roster: `meeting_id_hash` dropped, labels via `mediaMetricLabels`, exported, and `dt_client_mh_` added to the media-path prefixes block.
- **e2e**: the bus gains a `handlerSets` history ({version, [{ref, url}]}) plus overlay and transportFault events. Existing specs move off `joined.mediaServers`. New `mh-replacement.spec.ts` (crash, planned, stranded).
- **Dev preflight (R-32)**: inside infra's I-2 (reads `mh.sh list --json`), with client reviewing.

**Interfaces**
- FROM protocol (P2): additive signaling fields and `MhCloseCode`.
- FROM MC: the set with refs (MC-A); a token on every member, TTL/3 refresh, ref-keyed status and FAILED re-push (MC-D); no mid-meeting adds before C1 (MC-C depends on C1); planned transfer (MC-F).
- FROM MH: close codes (MH-CLOSE); media_dial-only admission (MH-1, after C1); QUIC idle timeout ≈ N (MH-3).
- FROM test: `e2e/mhPool.ts` and the injector (T-1).
- TO MC: per-transition `MhConnectionStatus{handler_ref, handler_set_version, state}`.

**Tasks**: C1 (transport, credentials, close codes, loss reports, e2e migration; keeps the two existing series unchanged), C2 (UX, continuity, all R-42 series and their export path, resume_ms, replay token), C3 (browser resilience spec; depends on T-1, MC-C, MC-F, C2).

**Assumptions**
- The client never parses the token: freshness comes from MC's TTL/3 push on the reliable signaling stream, and a defect surfaces as `DECLINED`.
- Until MH-1 lands, the held tokens are sent but ignored by MH.
- Browser-trust for on-demand MHs relies on one shared dev cert across incarnations (security E-3; guarded in I-3).

### Database Changes

N/A — no migration and no index (database, verified against `migrations/20260124000001_mh_registry.sql`). Supersede deletes the superseded row inside one registration transaction under `pg_advisory_xact_lock(hashtext('gc_mh_registry'))`; nothing references `media_handlers` by foreign key. Query-pattern changes (one ms-bound staleness value, sweep includes draining, retention DELETE, exclude list before LIMIT, numeric interval binds) and the `docs/DATABASE_SCHEMA.md` §8 rewrite land in GC-1, GC-2 and GC-3, reviewed by database. Unique endpoint indexes are rejected: a previous-checkout GC (rollback) upserts on `handler_id` only and would fail every in-place MH restart.

### Protocol Changes

Governing design: ADR-0039 §1, §4–§8. Protocol owns P1, P2 and P3. Every other proto edit rides the consumer task that makes it true, paired with protocol, because `proto/` is a Guarded Shared Area (`internal.proto` also pulls in auth-controller and security).

#### Sequencing rule (self-contained commits)
- **Additive first** (P1, P2): new messages, enums and fields, with every rule written once at its field. Superseded fields stay alongside their replacements.
- **No `rpc` line lands without its server impl**, because a tonic server trait has no default methods:
  - `rpc MeetingHeartbeat` lands in MC-B1;
  - `rpc AcquireMediaHandler`, its messages and `RegisterMCResponse.max_exclude_handler_ids` land in GC-3;
  - the RegisterMH endpoint-rule comment block lands in GC-2, with its enforcement.
- **A rejection lands with or after its producer.**
  - MH's `heartbeat_interval_ms = 0` rejection (MH-2) comes after MC emits a non-zero value (MC-B1).
  - MC's zero-exclude-cap failure (MC-C) comes after GC populates it (GC-3).
  - `EndMeetingRequest.cause` UNSPECIFIED rejection (MH-2) is safe because P1 sets `MEETING_ENDED` at today's only teardown caller.
- **One retirement task** (P3) deletes the superseded signaling/connect surfaces after every consumer has moved: commit sequencing, never a runtime mode, flag or "legacy" path.
- **media_dial cut-over is atomic on MH** (MH-1). C1 populates `MhConnectRequest.media_dial_token` alongside `join_token` beforehand, so MH never accepts both credentials.

#### Internal contract (P1)
- `ReleaseCause {UNSPECIFIED, HEARTBEAT_TIMEOUT, APPLY_LAG, PLANNED_TRANSFER, MEETING_ENDED}`. It is the single vocabulary for Acquire's trigger, EndMeeting's cause and the RELEASED verdict, and every `trigger` label is projected from it.
- `ServingState`, `Verdict {OWNED, RELEASED}` and `ApplyRefusal` (a 1:1 projection of MH's deterministic refusals).
- `MeetingHeartbeatRequest {meeting_id, mc_id, handler_id, serving_state, drain_remaining_ms, applied_generation, apply_refusal, connection_ids}` and `MeetingHeartbeatResponse {verdict, cause}`. The text covers:
  - start at the registration upsert; no retry; jittered first send;
  - remove-only reconcile with the previous-receipt rule;
  - the owner check (FAILED_PRECONDITION, never RELEASED);
  - the cause rules, including the misroute UNSPECIFIED exception;
  - owner-silence release;
  - the S6 "not an authorization control" note.
- `RegisterMeetingRequest.heartbeat_interval_ms` (0 rejects) and `media_dial_public_key` (32-byte Ed25519 public key), with the single no-key-material allow-list entry in the same commit (construct from I-4).
- `EndMeetingRequest.cause`.
- Comment changes:
  - the all-or-none apply invariant is stated once at `applied_generation`;
  - `handler_id` is never a metric label on MC/GC series;
  - re-assert framing removed;
  - EndMeeting race (C) and the "ROLLOUT (R-24)" paragraph deleted;
  - `policy_generation` ORDERING text untouched.

#### Signaling contract (P2)
- `MediaHandlerSet {handler_set_version, handlers}` on JoinResponse and as a ServerMessage. The version is a membership epoch used for correlation only, and the join-time set may be empty.
- `MediaServerInfo.handler_ref` (per incarnation, never reused within a meeting) and `media_dial_token` (credential, redacting Debug).
- `handler_ref` on SendTarget and StreamAssignment; `MhConnectRequest.media_dial_token`; `MhConnectionStatus.handler_ref` + `handler_set_version`.
- `MhCloseCode` with security's three conditions. Zero is never sent, and "no code" means transport loss.
- MediaConnectionUpdate stays diagnostic. A FAILED report may trigger a credential re-push to its sender only.

#### Retirement (P3)
- Deletes `JoinResponse.media_servers`, `SendTarget`/`StreamAssignment.media_handler_url`, `MhConnectionStatus.mh_url` and `MhConnectRequest.join_token`, with `reserved` lines.
- In the same commit: TS regen, verify-codegen absence checks, and removal of dead MC population, SDK and env-fixture lines.
- `JoinRequest.join_token` stays.

#### Assumptions
- `reserved` lines on deletions satisfy the `buf breaking` WIRE_JSON gate; they are not compatibility affordances.
- Additive tasks make only compile-required consumer edits (explicit zero/empty fields at Rust struct literals; an ignore arm for the new ServerMessage variant if a dispatcher is exhaustive).
- `process_start_epoch_ms` is deleted in MH-2 and the connection_id legacy text in MC-B1, not by protocol.

---

## Cross-Cutting Requirements

### Security

Governing design: ADR-0039 §3–§7 and Consequences "Recorded residual risks". Security owns SEC-A (policy lens and residual) and is the mandatory reviewer on every task touching R-9..R-15, R-17, R-19, R-25, R-27, R-28 and R-31.

#### Checklist by surface

| Surface | Authentication | Authorization | Input validation | Errors / data |
|---|---|---|---|---|
| Client→MH WebTransport dial (R-13) | MC-signed `media_dial` JWS, EdDSA pinned, key only from the meeting's registration; kid/jwk/jku/x5u/x5c ignored, never fetched | `meeting_id` + `aud` = own handler_id, then MC's binding answer for the token's `sub` | size cap, required claims, exp/iat with config skew (≈30 s); unregistered meeting declined at once, nothing held; pending per (meeting, sub) capped | one generic `DECLINED`; `WRONG_HANDLER` only after full validation; reasons to metrics only |
| MC join / GC user auth (R-15) | AC JWKS only | unchanged | kid-less dial token fails at `extract_kid` (MissingKid) before any JWKS lookup | tests: kid-less, forged served-kid, HS256 arms + AC positive |
| MH→MC `MeetingHeartbeat` (R-7, R-9..R-11) | existing McAuthLayer (service.write.mc, service_type media-handler) | exact match against the meeting's current set; registered `mc_id` checked, mismatch = FAILED_PRECONDITION, never RELEASED | ids via validate_id_field, per-meeting caps; non-decreasing applied_generation for liveness; remove-only reconcile | handler identity self-asserted (S1b residual) |
| MC→MH `RegisterMeeting` (R-15) | MhAuthLayer | `mc_id` self-asserted (ownership residual) | `media_dial_public_key` exactly 32 bytes | the only allow-listed key bytes; no secret/media key material (guard) |
| MC→GC `AcquireMediaHandler` (R-23) | GC auth layer, service_type meeting-controller | anti-misroute vs assignment row (self-asserted) | exclude list ≤ `GC_MH_MAX_EXCLUDE_HANDLER_IDS` (16) else INVALID_ARGUMENT; unknown ids ignored | GC alone chooses; reports never change selection |
| MH→GC `RegisterMH` (R-25) | existing | per-zone allowlists on both endpoints; endpoints fixed per incarnation; live serving MH never displaced | https + host list + port set (client); CIDR + port set, scheme `grpc` = cleartext until S6 (internal) | supersede counted, sustained rate pages |
| MC→client `MediaHandlerSet` (R-14, R-17) | authenticated MC signalling session | token per member for this participant's own `sub`; refs only from MC's set | URLs only from GC grants; client-supplied URL never a target | credential-leak treatment; no key material, no cert hashes |
| `mh.sh` (R-29) | caller's existing kubeconfig | no new privilege | — | output never echoes kubeconfig, env or Secret contents; no force-delete (R-28) |

#### Cryptography
- Dial signing key: per-meeting Ed25519 from `ring::rand::SystemRandom`, held only in the MC meeting actor (`dial_signing_key`, redacting wrapper), never persisted/logged, no rotation, dies with the meeting. Never in any JWKS. Asymmetric chosen because MC→MH is plaintext h2c (S6): a symmetric key there would let a passive observer mint.
- Token: header exactly `{"alg":"EdDSA","typ":"JWT"}` (no kid); claims token_type=media_dial, sub, meeting_id, aud=handler_id, iat, exp; `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` default 300, [60, 900]; refresh TTL/3; reusable within TTL (no jti cache).
- No KEK action on a handler change; transmit-key generation/sequence strictly continue across a swap (R-16).

#### Fencing
- Removed handler: never re-added, sender_id 0 with nothing recorded, bindings released only after clients were told, RELEASED on its heartbeats (R-10).
- Owner-silence release (R-11): timer per meeting, monotonic, reset only by a successful message from the owner; safe by config ranges — MH silence [300, 3600] s (default 900) ≥ 10 × MC N ≤ 30 s.

#### Deployment
- Gate zero (R-27): canary selected by the real MH policy; allowed port echoes AND non-allowed port never arrives, via the same hostPort DNAT; nothing received on either = failed gate.
- `shareProcessNamespace` only in the Kind render (guard asserts absent elsewhere, present in Kind) (R-31).

#### Recorded residuals (ADR-0039 Consequences)
S1b extended (forged heartbeat suppresses failover; the disagreement counter detects only a forger echoing live connection_ids — see docs/TODO.md §Media Path Obligations); S6 (periodic MH bearer; GC allowlist is not peer authentication — line lands in GC-2); stolen token usable TTL + skew ≈ 5.5 min, ciphertext or binding-conflict DoS only; re-registration replaces a meeting's dial key and binding endpoint (ownership residual, SEC-A); GC `controller_id` check is anti-misroute only.

### Observability

**Source of truth**: ADR-0039 §8 and `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md`. Per-task series obligations are in `sections/observability-obligations.md`, and each owner carries them into their prompt.

**How the work is cut.**
- dt-guard application-metrics (checks 3–6) rejects a metric without a panel and catalog entry, and a panel or rule naming an absent metric. Every series therefore lands in the task that emits it, with its panel, catalog entry, zero-init and ADR-0032 test.
- Every retirement removes its references in the same commit.
- Only the alert rules, the alert inventory, the SLO line and two cross-service panels form a separate task: OBS-ALERTS. It comes after all series and after OPS has created the runbook anchors.

**Metrics (business, not transport).** Everything uses one `trigger` label (heartbeat_timeout | apply_lag | planned_transfer), projected from `ReleaseCause`.
- MC detection and migration. Per (meeting, handler):
  - declarations, suspicions, detection_seconds;
  - migrations_total{trigger, outcome} and migration_duration_seconds{trigger, phase};
  - meetings_without_handlers;
  - heartbeat verdicts, interval and applied lag;
  - connected-set corrections.
  Per handler: first_declarations_total and handlers_in_use (HandlerCensus).
- The user-required metric (R-33). loss_clients_total{reported}, loss_prior_report_ratio and loss_client_lead_seconds, snapshotted at the Dead/Impaired transition and never for planned_transfer, plus client_loss_disagreements_total. It is interpretable only when MH's QUIC idle timeout is about N, so both are config gauges shown side by side.
- MH:
  - heartbeats and latency;
  - releases_total{trigger, clients}. heartbeat_timeout+present is the false-declaration count; owner_silence is the one MH-local value;
  - meetings_without_clients;
  - media_dial rejections by reason;
  - pending-cap declines and close codes;
  - drain series (best-effort; MC's planned_transfer outcome is authoritative).
- GC: replacement requests by trigger, outcome and gc_view; registrations by outcome (match on superseded); load reports (unknown_handler); availability by region and state; the load-report interval and the derived staleness bound.
- Client (R-42, user ruling: extend, don't duplicate):
  - dt_client_mh_connection_total{phase, outcome} and dt_client_time_to_first_mh_connected_ms are extended in place. They leave the grandfathered roster, drop meeting_id_hash and move to mediaMetricLabels, and are exported for the first time.
  - New: dt_client_mh_dial_duration_ms, dt_client_mh_disconnects_total{cause} (lost = transport_loss|handler_released) and dt_client_mh_reconnect_duration_ms.
  - Also: dt_client_media_resume_ms, the per-slot media-dark window.
  - `dt_client_mh_` joins the media-path-prefixes block.

**Labels and logs (ADR-0036 §11).**
- No meeting, participant, connection, handler_id, handler_ref, slot or revision label anywhere. key_custody=operator applies on all media-path series, and env-test 32 enforces both.
- Logs carry the ids instead:
  - MC: health transitions; migration start and terminal outcome.
  - MH: releases, including owner-silence; the drain exit line with the shed count.
  - GC: the `gc.mh.replacement_report` event (the corroboration source for the gray-failure gap, counted by log query) and registration rejects.
- Never logged: individual heartbeats, dial tokens, the signing key.

**Dashboards.**
- mc-media: detection, migration, the R-33 panel, handler-set pushes, and N beside the QUIC idle timeout.
- mh-media: heartbeats, releases, drain, credential rejections.
- client-media: dial, disconnect and reconnect timing, resume.
- gc-overview: replacement, registration, load reports, availability. The gc_view panel is described as lagging MC by design.

**Alerts.** Thresholds and derivations are in the contract and OBS-ALERTS. No single MH death pages.
- PAGE: MCMediaMeetingsAwaitingHandler (bound from a published gauge); GCMhRegistrationPingPong (supersede rate AND unknown_handler); MHDown re-scoped to fleet/region (zero healthy MHs); MCMediaGenerationDivergence (kept, re-assert text removed).
- WARNING:
  - migration failures while a handler survives; drain shed (keyed on MC);
  - the MC own-path signature; the per-handler crash rate;
  - client-loss disagreement; sustained rate_limited; sustained corrections;
  - misroute or owner mismatch;
  - false declarations; owner-silence with clients present; meetings without clients past the owner-silence timeout;
  - media_dial expired, no_meeting_key, wrong_type, aud_mismatch;
  - unknown_handler; persistent address_conflict; endpoint faults; no_capacity;
  - MCRegisterMeetingFailureRate.
- Every rule resolves to a section-J runbook anchor and has a byte-identical alerts.md inventory entry.

**SLO.** slos.md replaces the applied-generation-echo item with migration completion and the media-dark window. The target stays OPEN: observed, never gated.

**Retirements, each with its references in the same commit.**
- The mc_media_generation_divergence gauge, the handler_id_mismatch outcome and the empty-connection_id counters (MC-B1).
- mc_media_handler_set_divergence_total (MC-A).
- mh_register_meeting_timeouts_total (MH-1).

**Assumptions.**
- The MH-local owner_silence value on releases_total is the single documented exception to the ReleaseCause projection.
- The "MC RegisterMeeting coordination failures" TODO entry closes in OBS-ALERTS, because the rule lands there; OPS handles the other dispositions.

#### Per-task telemetry obligations

Source of truth: `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md` (ADR-0039 §8). Tasks are named by tag, as in the Implementation Plan below.

Every series listed here lands **in the task that emits it**, in the same commit as:
- its panel on the owning board;
- its catalog entry in `docs/observability/metrics/<service>.md`;
- zero-initialisation of every label combination;
- an ADR-0032 `MetricAssertion` test.

dt-guard application-metrics checks 3–6 fail the commit otherwise.

Every retirement removes its panels, catalog entry, alert-expression and runbook references in the same commit.

Hygiene for every series: no meeting, participant, connection, handler_id, handler_ref, slot or revision label. `key_custody="operator"` goes on every `mc_media_*`, `mh_media_*` and `dt_client_media_*` series, and env-test 32 must stay green. `trigger` is projected from `ReleaseCause` (heartbeat_timeout, apply_lag, planned_transfer). Health transitions are logged, never individual heartbeats. Dial tokens and the signing key are never logged, labelled or traced.

Alert rules are NOT in these tasks. They all land in OBS-ALERTS. The one exception is noted under MC-B1.

#### MC (board: `infra/grafana/dashboards/mc-media.json`; story-4 handler rows on `mc-media-handlers.json` since MC-F)

#### MC-A
Retire `mc_media_handler_set_divergence_total`: non-first joins stop reading Redis.

#### MC-B1
Add:
- `mc_media_handler_heartbeats_total{outcome=owned|released_removed|released_ended|released_unassigned|owner_mismatch}`;
- `mc_media_handler_heartbeat_interval_seconds`, buckets around N/3, N/2 and N;
- `mc_media_handler_applied_sequence_lag`;
- `mc_media_handler_connected_set_corrections_total`;
- config gauge `mc_media_handler_heartbeat_deadline_seconds`, from `MC_MH_HEARTBEAT_TIMEOUT_MS`;
- `mc_media_handler_heartbeat_anomalies_total{reason=connection_ids_over_cap|connection_id_malformed|applied_ahead_of_sent}` (added at MC-B1 Gate 1, observability-approved; expected zero, a WARNING candidate for OBS-ALERTS).

Retire:
- the `mc_media_generation_divergence` gauge, including its step in the `docs/observability/alerts.md` ladder and in mc-incident-response Scenario 15;
- the `handler_id_mismatch` outcome of `mc_media_policy_pushes_total`, together with all three of its "one decision" sites: the `MCMediaGenerationDivergence` expression in `infra/docker/prometheus/rules/mc-alerts.yaml`, the catalog entry, and the `mc-deployment.md` checklist;
- the empty-`connection_id` counters (`mc_mh_notifications_without_connection_id_total` and the degraded-mode counter).

The binding gate reuses `mc_mh_notifications_unapplied_total{reason="handler_not_in_set"}`; it needs no new series.

#### MC-B2
Add:
- `mc_media_handler_declarations_total{trigger}`, one per (meeting, handler);
- `mc_media_handler_suspicions_total{outcome=declared|cleared}`;
- `mc_media_handler_detection_seconds{trigger}`;
- `mc_media_meetings_without_handlers`, a recompute-and-set gauge.

Log one INFO per health transition: meeting_id, handler_id, trigger, signals.

#### MC-C (crash replacement)
Add, zero-initialised for trigger heartbeat_timeout and apply_lag:
- `mc_media_handler_migrations_total{trigger, outcome=completed|no_capacity|gc_unavailable|admission_refused|rate_limited|timed_out|abandoned}`, where `rate_limited` means only the per-meeting minimum interval or attempt bound;
- `mc_media_handler_migration_duration_seconds{trigger, phase=replacement|registration|reconnect}`;
- a published config gauge for the awaiting-handler page bound (the replacement retry budget), because the page must not use a literal.

Log the migration start and its terminal outcome: meeting_id, old and new handler_id.

#### MC-F (planned transfer)
- Add the `trigger="planned_transfer"` combinations of `mc_media_handler_migrations_total` and `mc_media_handler_migration_duration_seconds`, zero-initialised. `timed_out` here is the authoritative drain-shed count.
- Add a published config gauge for `MC_PLANNED_TRANSFER_MARGIN_MS`.
- Add the planned-transfer split to the mc-media migration panels.
- Assert that a planned drain produces NO observation on the R-33 loss metric (ADR-0032 unobserved semantics).

#### MC-D
Add:
- `mc_media_handler_set_pushes_total{reason=join|change|token_refresh|failed_report}`;
- a new `payload_kind` on `mc_participant_outbound_messages_dropped_total` for the handler set;
- config gauges `mc_media_dial_token_ttl_seconds` and `mc_media_dial_token_refresh_interval_seconds` (TTL/3).

#### MC-E
Add:
- `mc_media_handler_loss_clients_total{reported=before_declaration|not_reported}`;
- `mc_media_handler_loss_prior_report_ratio`, buckets 0, .1, .25, .5, .75, .9, 1;
- `mc_media_handler_loss_client_lead_seconds`;
- `mc_media_handler_client_loss_disagreements_total`, with its fraction and minimum published as config;
- `mc_media_handler_first_declarations_total{trigger}`;
- `mc_media_handlers_in_use`, from HandlerCensus (MutedSourceCensus pattern).

Rules for the loss metric:
- Snapshot at the Dead/Impaired transition.
- Not observed for planned_transfer; assert this as unobserved.
- Counted at MC receipt time.

#### MH (board: `infra/grafana/dashboards/mh-media.json`)

#### MH-2
Add:
- `mh_mc_heartbeats_total{outcome=success|error|timeout|skipped_in_flight|dropped_queue_full}`;
- `mh_mc_heartbeat_latency_seconds`;
- `mh_media_handler_releases_total{trigger, clients=present|none}`, covering removals only (the ReleaseCause projection);
- `mh_media_meetings_without_clients`.

#### MH-1
- `mh_jwt_validations_total`: token_type becomes `media_dial`, and the client-path failure_reason splits into expired, signature_invalid, aud_mismatch, wrong_type, no_meeting_key and malformed. Clients only ever see DECLINED.
- Add a pending-cap decline outcome on `mh_media_session_starts_total`.
- Add a config gauge for the exp skew.
- Retire `mh_register_meeting_timeouts_total` and its mh-overview panels.

#### MH-3
- Add the drain duration histogram and the meetings-remaining gauge. Both are best-effort, because the authoritative outcome is MC's `migrations_total{trigger="planned_transfer"}`.
- Write the shed count in the exit log line.
- Add a config gauge for the QUIC `max_idle_timeout`.
- Owner-silence release: add the MH-local `trigger="owner_silence"` value on `mh_media_handler_releases_total` (zero-initialised with clients=present|none, documented in the enum and the catalog) and the config gauge `mh_media_meeting_owner_silence_timeout_seconds`. Log one INFO per release: meeting_id, owner mc_id, duration.

#### MH-CLOSE
Add a session-close counter by `MhCloseCode` (one per intentional close, zero-initialised for every code).

#### GC (board: `infra/grafana/dashboards/gc-overview.json`)

#### GC-1
Add:
- `gc_mh_load_reports_total{outcome=accepted|unknown_handler}`;
- `gc_mh_available{region, state}`;
- `gc_mh_load_report_interval_seconds`;
- `gc_mh_staleness_threshold_seconds`, derived.

#### GC-2
Add `gc_mh_registrations_total{outcome=ok|superseded|address_conflict|endpoint_changed|invalid_endpoint, match}`. `match` appears on superseded only; zero-initialise all seven series.

#### GC-3
- Add `gc_mh_replacement_requests_total{trigger, outcome, gc_view}` and `gc_mh_replacement_duration_seconds{outcome}`.
- Emit the INFO event `gc.mh.replacement_report` {handler_id, webtransport_endpoint, controller_id, trigger, gc_view, outcome}.
- Describe the gc_view panel as "GC's view at request time, which lags MC by design".

#### Client (board: `infra/grafana/dashboards/client-media.json`; export path: collector allowlist, GC telemetry filter, client-metrics-export guard)

#### C1
Keep `dt_client_mh_connection_total` and `dt_client_time_to_first_mh_connected_ms` compiling and passing unchanged; C2 extends them. C1 adds no R-42 series.

#### C2
Two existing series are extended in place:
- `dt_client_mh_connection_total{phase=join|added|redial, outcome}`;
- `dt_client_time_to_first_mh_connected_ms`.

Both leave the grandfathered roster, drop meeting_id_hash, status and mh_index_bucket, switch to `mediaMetricLabels`, and are exported.

Add:
- `dt_client_mh_dial_duration_ms{phase, outcome}`;
- `dt_client_mh_disconnects_total{cause}`, where lost = cause=~"transport_loss|handler_released";
- `dt_client_mh_reconnect_duration_ms{outcome}`.

Add `dt_client_mh_` to the label-taxonomy media-path-prefixes block together with the hygiene-kernel const. Flip the `MediaTransport.ts` spread comment and the test pin. Also in C2: the collector allowlist, the GC telemetry filter, the export guard and catalog entries for all of these.

Also add `dt_client_media_resume_ms{media_kind, content_kind, outcome}` and the cross-transport replay-drop reason token. The abandonment caps are published clientConfig.

#### OBS-ALERTS
Holds every new or changed alert rule, the `alerts.md` inventory, the `slos.md` line, and the cross-service panels (N beside the QUIC idle timeout, plus the R-33 interpretability panel).

### Test

**Coverage contract.** `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md` is the per-service gate list. Each service task implements its own section, paired with test. Unit and integration tests stay in the service tasks (ADR-0028 §7).

Rules for every task:
- All deadlines (heartbeat, `assess`, token refresh, drain, owner silence) run on `tokio::time::Instant` or an injected clock. They are tested with `start_paused` + `advance`, never sleeps.
- Jitter uses a seedable RNG.
- Every negative test has its positive arm in the same test.
- Metric assertions use ADR-0032 `MetricAssertion`. Planned-transfer negatives use "never observed" semantics.
- Env-test bounds are failure-only ceilings derived from deployed config, never latency targets (ADR-0036 §10).

**Test-owned tasks**
- T-0 (before I-2): env-tests find MH pods by label. The gRPC port-forwards move from layer7.sh Phase 1i into the fixture, on local port 0 and never a helper port-map number. `ENV_TEST_MH_{0,1}_*` retires, and there is no fixed MH count.
- T-1 (after I-5, I-2 and MC-A):
  - `fixtures/mh_pool.rs` over `mh-sh-contract.txt`: owner-scoped teardown, `assert_baseline` (`foreign-mh-activity`), a Prometheus scrape-discovery wait, and the address-invariant helper;
  - the process injector: `kubectl debug --target` as uid 65532, a `shareProcessNamespace` preflight, KILL with in-place restart asserted, and STOP/CONT with a liveness-probe-derived budget and restart-count assertion;
  - participant-fixture access to MediaHandlerSet;
  - `packages/web-app/e2e/mhPool.ts`.
- T-2: the manual test plan, written and run (R-41).

**Env-test legs and their owning tasks** (all paired with test; all but gate zero use T-1's fixture)

| Leg | Requirement | Task |
|---|---|---|
| mh.sh contract: hermetic self-test (no cluster) | R-29 | I-5 |
| Gate zero: UDP hostPort reachable; NetworkPolicy drops a disallowed source. Never-ready canary in its own binary, positive control on both arms | R-27 | I-1 |
| Address invariant after every leg (inline, then hoisted into the T-1 helper) | R-28 | I-2, T-1 |
| Existing suites green on slots: tests 01/02 and deployment_config on slot ConfigMaps | R-39 | I-2 |
| Zombie: freeze, then declared dead, then resume → RELEASED on first heartbeat, no media, release counted | R-10 | MC-B2 |
| Crash to survivor | R-1 | MC-B2 |
| Crash with replacement (two-handler meeting): final set holds a new ref | R-1 | MC-C |
| Stranded pair (Rust) | R-2 | MC-C |
| Planned transfer with a target (`mh up` + `mh drain`), and without | R-3 | MC-F |
| Admission matrix (WRONG_HANDLER, DECLINED cases) and left participant | R-13 | MH-1 |
| Browser: crash, stranded pair (blockHandlers), planned (no overlay, no transportFault, ordered sets) | R-1–R-3, R-20 | C3 |
| Deployment-config coverage of new and retired keys | R-31, R-37 | MH-2, MH-1, MH-3 and others |

**Integration-test highlights** (full list in test-gates.md):
- MC Tier 1b:
  - stranded A/B: RegisterMeeting(Y) precedes the set;
  - every GC outcome arm, including awaiting-handler then a later grant;
  - binding gate: sender 0 and the ledger unchanged;
  - Registering race → OWNED;
  - EndMeeting failure → the next heartbeat gets RELEASED;
  - remove-only reconcile, both arms;
  - survivor target, three arms;
  - loss metric k-of-n with its positive arm.
- MH rigs:
  - per-meeting heartbeat independence;
  - all-or-none apply over 4 refusal kinds plus the first-refused arm, asserted on frames;
  - verdict arms;
  - pending cap at cap and cap+1;
  - owner-silence (errors don't reset it);
  - drain, three arms.
- GC: status table, a predicate test proving the SQL matches it, and selection invariance with reports (seeded).
- Client: a token-only refresh does not reconnect; DECLINED park, then exactly one dial; same-URL/new-ref reconnect.

**Assumptions.**
- Concurrent Layer-7 runs on one cluster stay unsupported (layer7.sh), but they fail loudly.
- CI runs no Kind or Layer 7 (a recorded existing gap).
- If the Kind freeze injector proves impractical, the zombie leg falls back to deterministic tests only, and that is recorded.

### Deployment

**What changes**
- **Gate zero (I-1).** Before any slot change, an env-test proves UDP hostPort delivery and MH NetworkPolicy enforcement on the hostPort DNAT path. The canary carries the real policy's labels and never becomes ready. It exposes allowed 4434 and non-allowed 9999 on node hostPorts 30830/30530, with positive controls on both arms, so no cluster recreate is needed. On failure, the fallback (StatefulSet-per-slot behind per-slot NodePort Services with publishNotReadyAddresses) is recorded and the Lead decides before I-2.
  - **Gate zero RESULT (2026-11-06): PASS for policy enforcement on the hostPort DNAT path. I-2 proceeds on hostPort slots; the fallback is NOT adopted.** Test: `crates/env-tests/tests/41_gate_zero_hostport_policy.rs` (own binary, `resilience` feature), run on the devloop Kind cluster (Kind v1.27.3 node, Calico with `portmap` in the CNI chain). The canary's labels came from the live `networkpolicy/mh-service` podSelector, and `mh-service` was the only policy selecting it. A crossover ran two phases (A: hostPort 30830->4434, 30530->9999; B: swapped). In each phase, UDP 4434 was echoed and logged (RTT about 12 ms) and stayed reachable after the drop window (bracketed). Five UDP 9999 datagrams over 5 s were neither echoed nor logged, while the 9999 listener's loopback control was logged. So each hostPort both delivered and dropped, and the policy is the only variable. Evidence line (post-review run, `cargo test -p env-tests --features resilience --test 41_gate_zero_hostport_policy`): `GATE0 PASS policy-on-DNAT: PASS (phases A+B; ...); publish-layer UDP leg: NOT EXERCISED (I-2 obligation)`; verbatim run record in `docs/devloop-outputs/2026-11-06-gate-zero-hostport-policy/main.md`. Attribution: `mh-service` was the only k8s NetworkPolicy selecting the canary, and no Calico-native (Global)NetworkPolicy existed.
  - **Scope of the evidence (Lead decision, Option A).** The task assumed today's Kind mappings for containerPorts 30830/30530 could carry UDP. Both are `protocol: TCP` in `kind-config.yaml.tmpl` (the static `kind-config.yaml` has neither), so UDP from the host gateway never reaches the node. The gate therefore sends to the node IP (`.status.hostIP`) on the hostPort. That datagram goes through the same node PREROUTING, CNI-HOSTPORT-DNAT (portmap) and Calico policy path as a host-published one. The Kind/podman host-publish UDP leg (host gateway -> node) is NOT exercised here. It is the same under hostPort slots and under the fallback, so it does not bear on this decision. Today it is covered by composition: MH WebTransport UDP through the 30434 mapping (`26_mh_quic.rs`). The runner->node UDP leg is not independently controlled; only TCP reachability is probed.
  - **Obligation inherited by I-2:** I-2's slot env-test MUST send a UDP datagram from the env-test runner to the host gateway on a slot's UDP extraPortMapping, and get it delivered end to end to the slot's MH. That closes the host-publish leg gate zero did not exercise.
- **Slot pool (I-2).**
  - Each MH incarnation owns a hostPort slot. The scheduler admits one pod per hostPort per node, and a Terminating pod holds it until gone, so no two live MHs share an address by construction.
  - K = 6 slots, with one home in `crates/devloop-helper/src/ports.rs` (offsets +20..+25; the dead MH health/gRPC mappings go). kind-config mappings, per-slot advertise-address ConfigMaps and GC's allowlists derive from it.
  - The per-instance NodePort Services and the mh-0/mh-1 Deployments are deleted, and one MH template remains.
  - A crash is an in-place container restart: same slot, same pod IP, new handler_id.
- **Provisioning tool (I-5, before I-2).**
  - `infra/kind/scripts/mh.sh up|drain|upgrade|list` implements the merged machine contract (`mh-sh-contract.txt`): JSON stdout, one `MH_RESULT` line, grouped exit codes, next-fit, owner labels, an `mh-pool` Lease around decide-plus-create/delete, idempotent retry and drain. Its hermetic self-test runs in Layer 3.
  - It ships uncalled. Its script header documents the cluster-side interface I-2 must produce: the `mh-template` ConfigMap and per-slot ConfigMaps labelled `dark-tower.io/mh-slot`.
- **Deploy wiring (I-2).**
  - deploy.sh renders the template once (stored as ConfigMap `mh-template`, excluded from the root apply), runs `upgrade` (start-new-then-drain-old, serial, resumable) and tops up to baseline B = 2 (one home in the Kind overlay).
  - Image prune keeps every live MH pod's image plus the previous revision.
- **Guards (I-3, I-4).**
  - MH template invariants: labels, 8083/50053/4434+hostPort, the shared dev-cert Secret, MC egress coverage.
  - `shareProcessNamespace` present only in the Kind render.
  - No MH workload in the root apply set.
  - Slot-table drift across ports.rs, kind-config, slot ConfigMaps and GC allowlists.
  - The force-delete ban.
  - An exact (message FQN, field) allow-list construct for the key-material guard, with vacuity (the entry exists and is `bytes`). It lands empty; P1 adds the single entry.

**Interfaces**
- TO test:
  - T-0 lands first (label discovery; port-forwards on local port 0, never the helper MH ports that I-2 repurposes).
  - I-2 exports `DT_MH_SH` from layer7.sh, carries the R-28 invariant env-test inline, and moves test 01/02 plus the deployment_config fixture.
  - T-1 builds the mh_pool fixture on the contract.
- TO global-controller: I-2 exposes one slot-table source in deploy.sh. GC-2 renders its five GC_MH_* allowlist keys from it in the same render_env_overlay pass; I-3's drift guard follows GC-2.
- TO client: I-2 switches dev-web.sh's MH preflight to `mh.sh list --json` (R-32), with client reviewing.
- TO media-handler: I-2 moves the MH crate's include_str! tests onto the template. New MH keys are wired by whichever task lands second; the template keeps one configMapKeyRef per key.
- TO operations: I-2 fixes only the runbook commands that break and adds K/B config rows. OPS writes the procedures.
- TO protocol/security: P1 depends on I-4. Security owns the allow-list membership; infrastructure owns the construct.

**Assumptions**
- Calico's CNI chain in our Kind includes portmap. I-1 verifies this and reports a hostPort failure distinctly from a policy failure. Verified by I-1 (gate zero PASS, above).
- test-gates.md's "lowest-free-slot" wording is superseded by ADR-0039 §7 next-fit.

**Remaining host-side actions (not devloop steps)**
- After I-2 merges (not after I-5, which changes no cluster): rebuild devloop-helper on the host (ports.rs changed), then recreate every existing Kind cluster to get the six slot mappings. The first deploy, and any rollback across I-2, requires a recreate (R-30).
- I-1, I-3, I-4 and I-5 need neither.

### Operations

**What changes for operators.** MH upgrades, scale-down and node maintenance become routine and keep calls up. SIGTERM drains a handler: meetings move make-before-break to a GC grant or onto survivors, and the MH exits when empty or at grace minus margin. This reverses ADR-0036 §11 "MH sheds". A single MH death is healed automatically and does not page. All MH lifecycle actions go through `infra/kind/scripts/mh.sh` (up / drain / upgrade / list) and deploy.sh. `kubectl rollout undo` no longer applies to MHs.

**Runbooks (one task, OPS; full list in `runbook-alert-changes.md` §I–J).**
- mh-deployment.md:
  - drain-based rollout and rollback (upgrade from the previous checkout);
  - node maintenance;
  - address pool exhausted (exit 3 `mh-no-free-slot`, old MH keeps serving);
  - upgrade stuck (`not-available-timeout`, check GC);
  - every mh.sh refusal (last-serving-mh, stuck-terminating, foreign-owner, mh-pool-busy, drain-timeout);
  - the zero-grace force-delete ban;
  - scale-to-zero;
  - the replacement attempt budget (≈ 14 lifetime replacements per meeting at the default exclusion cap of 16);
  - the 24-hour checklist.
- mc-deployment.md: an MC rollout leaves its meetings on MHs until the owner-silence release (~15 min) reclaims them. That is incidental, not failover.
- Incident scenarios, anchors fixed before OBS-ALERTS' alert rules (OBS-ALERTS depends on OPS):
  - MC 24–29: declared dead/impaired, awaiting handler, migration timed out / drain shed, several handlers declared dead by one MC, client-loss disagreement, misroute/owner mismatch;
  - MH 24–26: credential rejections, meetings held without clients, owner-silence release with clients present;
  - MH 1 re-scoped to fleet-level MHDown;
  - GC 14–15: two live MHs on one address, gray-failure manual procedure;
  - existing scenarios cleaned of re-assert and legacy text.
- Retired-surface runbook text leaves with the retiring task (MH Scenario 13 with MH-1). Each new key's Configuration Reference row lands with the task that adds the key. I-2 fixes runbook commands that break when mh-0/mh-1 disappear.

**Configuration relationships documented.**
- Owner silence `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` [300, 3600], default 900, against heartbeat timeout N `MC_MH_HEARTBEAT_TIMEOUT_MS` (0, 30000] ms, default 6000. The floor is ≥ 10× the largest N, so a live MC always moves off a silent MH long before MH releases on owner silence.
- `GC_MH_MAX_EXCLUDE_HANDLER_IDS` 16 → ≈ 14 replacements per meeting, then `rate_limited`.
- The drain deadline derives from terminationGracePeriodSeconds (the one value to change).
- The staleness bound derives from `GC_MH_LOAD_REPORT_INTERVAL_MS`.
- QUIC idle timeout ≈ N.
- K = 6 slots (changing it means a cluster recreate); baseline B = 2.

**Alerting posture (rules in OBS-ALERTS).** One MH death never pages by itself. Pages:
- meetings awaiting a handler past the bound;
- registration ping-pong;
- fleet/region `MHDown`;
- generation divergence.
Warnings follow the telemetry contract. Raw declaration volume is never alerted on (it counts meetings, not handlers). The MC-own-path signature counts handlers.

**Rollback.** Code, config and Kind topology only; no data or schema change. MH rollback = the upgrade run from the previous checkout, with meetings migrating normally. Images and ConfigMap generations of live MH pods plus the previous revision survive pruning. The first deploy changes Kind's host-port mappings to hostPort slots, so it, and any rollback across it, requires a cluster recreate.

**24-hour watch.**
- migrations timed_out / no_capacity unobserved;
- planned_transfer completions ≈ meetings on rolled MHs;
- meetings without handlers = 0;
- GC address_conflict flat (superseded only at crash restarts);
- client declined/wrong_handler near zero;
- owner-silence releases only ~15 min after an MC rollout;
- `mh.sh list --json` ≥ B serving and ≥ 1 free slot.

**Assumptions.**
- Metric and log names are taken verbatim from `telemetry-contract.md`.
- mh.sh tokens and exit codes come from `mh-sh-contract.txt`.
- OPS lands before OBS-ALERTS, so every alert `runbook_url` resolves at its commit.
- CI runs no Kind or Layer 7 (existing gap, referenced in docs/TODO.md), so operator procedures are exercised in the manual test plan (T-2).

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | Owner-silence timeout validated at MH startup to [300, 3600] s (default 900); MC's `MC_MH_HEARTBEAT_TIMEOUT_MS` bounded to (0, 30000] ms, so silence ≥ 30× the heartbeat interval by config ranges alone; no per-registration check; both config references state the relationship | team-lead (security accepted) | Preserves R-11's safety intent without coupling at registration |
| 2 | The `media_dial` JWS carries no `kid` (header exactly alg EdDSA, typ JWT), so a stray token at MC join or GC is rejected at `extract_kid` with no AC JWKS fetch | security (auth-controller proposed) | MH ignores key-locating headers anyway (ADR-0039 §5) |
| 3 | GC refuses to start if registry retention ≤ the staleness bound | database | Otherwise a fresh row could be deleted, breaking the address-conflict guarantee |
| 4 | GC defaults: load-report interval 10 s [1, 60]; staleness multiplier 3 [2, 10]; retention 1 h; initial handlers 2 [1, 4]; exclude cap 16 [4, 64], above the initial count | global-controller | ADR-0039 names the values, not all defaults/ranges |
| 5 | MC defaults: `MC_PLANNED_TRANSFER_MARGIN_MS` 5000 [1000, 30000]; `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` 0.5 (0, 1]; `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` 3 (≥ 1) | team-lead | ADR-0039 and the telemetry contract name these without values |
| 6 | Exclude cap 16 allows ≈14 replacements per meeting lifetime (removed handlers stay excluded), then `rate_limited`; the runbook says so | global-controller, meeting-controller | Consequence of ADR-0039 §3, documented rather than changed |
| 7 | R-9 (lost-Disconnected self-correction) is covered by deterministic Tier-1b tests only for now; a timed MH→MC partition test is possible but under discussion with the user | meeting-controller, test | Not reproducible in Kind without fault injection the ADR does not specify |
| 8 | Gate zero proves NetworkPolicy enforcement by allowed (4434) vs non-allowed (9999) port through the same hostPort DNAT path on a never-ready pod carrying the real MH policy labels, with positive controls | infrastructure (security, test accepted) | The MH policy admits UDP 4434 from any source by design, so a disallowed source IP cannot be the test |
| 9 | R-42 extends `dt_client_mh_connection_total` and `dt_client_time_to_first_mh_connected_ms` in place: they leave the grandfathered roster, drop `meeting_id_hash`, and are exported; cost: browser-console log↔metric correlation by meeting is lost on these series | client, observability | Required for them to be exported at all (ADR-0036 §11 hygiene) |
| 10 | Contract changes land additively (P1, P2); consumers move; P3 alone deletes the superseded surfaces. No flags, no runtime switches; MH never accepts both credentials (C1 sends both tokens, MH-1 switches MH to `media_dial` only) | team-lead | Keeps every commit green without a legacy mode |
| 11 | P1 sets `EndMeetingRequest.cause = MEETING_ENDED` at MC's existing teardown caller, so MH-2's UNSPECIFIED rejection never refuses a real teardown before MC-B2 populates removal causes | protocol | Rejection must land after its producer |
| 12 | The GC fleet capacity-headroom alert proposed in `runbook-alert-changes.md` §E is dropped: no ADR-0039 trace and no source series | team-lead | Carry, don't invent |
| 13 | T-0 moves env-test MH discovery to labels BEFORE I-2, stronger than the ADR's "same commit as the slot change" | test, infrastructure | Keeps Layer 7 green across both commits |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|
| 1 | What is the MC-restart scenario, and how does MH know which MC answered? | user | Answered | MH doesn't authenticate the responder; the receiving MC compares the `mc_id`. MC ids are per process, so a restarted MC answers owner-mismatch; owner-silence release (R-11) reclaims within 15 min |
| 2 | Resume/keyframe jitter after a mass reconnect: in story 4? | team-lead | Answered | No mechanism; GC's randomized selection and per-client dial jitter spread the burst; measured by the crash env-test and R-42; ADR-0036 arc line updated |
| 3 | Is the per-MC limiter worth keeping? | user | Answered | Dropped (it cannot protect a shared MH or GC from other MCs); per-meeting bounds stay |
| 4 | Should replacement selection match initial selection? | user | Answered | Yes: same weighted-random function, exclude list before the least-loaded cut |
| 5 | Client-side MH connection/disruption metrics? | user | Answered | Added as R-42 |
| 6 | Is the `pnpm dev` preflight (R-32) in scope? | client | Answered | Yes (it reads files I-2 deletes); lands inside I-2 |
| 7 | Omit `kid` from the `media_dial` JWS? | auth-controller | Answered | Yes (security) |

---

## Implementation Plan

Human-readable intent only. Status and devloop output live in the manifest below (ADR-0035 §4). Tasks are named by tag; manifest ids follow this order. Resized 2026-11-01 (user): OPS-DEPLOY + OPS-INCIDENT merged into OPS; `mh.sh` split out of I-2 as I-5; planned transfer split out of MC-C as MC-F; R-42 client telemetry moved C1 → C2; owner-silence release moved MH-2 → MH-3.

| # | Tag | Task | Specialist | Dependencies | Covers | Env-tests |
|---|-----|------|-----------|--------------|--------|-----------|
| 1 | I-1 | Gate zero: UDP hostPort + NetworkPolicy enforcement in Kind (decides the slot design or its fallback) | infrastructure | — | env-tests | Gate zero: UDP delivery via a slot port; policy drops a non-allowed port (positive controls) |
| 2 | I-4 | Key-material guard: exact (message, field) allow-list construct, empty | infrastructure | — | guard code, unit-tests | None (guard fixtures) |
| 3 | SEC-A | Security policy content for MC-minted dial credentials (credential-leak and client-lifetime checks, MH-ownership residual) | security | — | docs, guard policy | None |
| 4 | T-0 | Env-test MH discovery by label; per-test port-forwards | test | — | test infra | None new: existing MH suites re-run green via label-resolved forwards |
| 5 | P1 | Internal contract, additive (heartbeat, causes, dial public key + allow-list entry, EndMeeting cause) | protocol | I-4 | proto, unit-tests | None (additive contract) |
| 6 | P2 | Signaling contract, additive (MediaHandlerSet, refs, tokens, close codes) + TS codegen | protocol | — | proto, codegen, unit-tests | None (additive contract) |
| 7 | GC-1 | GC registry freshness, staleness/retention, GC config | global-controller | — | code, tests, dashboards, deploy, docs | GC config gauges match deployed config; healthy count ≥ live MHs |
| 8 | I-5 | `mh.sh` (up/drain/upgrade/list, contract) + hermetic Layer-3 self-test; not yet called | infrastructure | I-1 | code, unit-tests | None (hermetic self-test) |
| 9 | I-2 | MH slot pool, single template, deploy via `mh.sh`, dev preflight, per-instance test moves, runbook command fixes | infrastructure | I-1, T-0, I-5 | code, deploy, env-tests | Unique slot addresses (R-28); per-instance config tests on slot ConfigMaps; existing MH env-tests green on slots |
| 10 | GC-2 | GC registration rule: allowlists, supersede/reject, atomic transaction | global-controller | GC-1, I-2 | code, tests, env-tests, dashboards, deploy, docs | Fresh deploy registers every MH ok; zero invalid_endpoint / address_conflict |
| 11 | GC-3 | GC `AcquireMediaHandler` (+ rpc slice), shared weighted-random selection | global-controller | P1, GC-1 | proto, code, tests, dashboards, docs | None own (granted assertion rides MC-C) |
| 12 | MC-A | MC-owned versioned ref-addressed handler set; register-before-token; first-join parking | meeting-controller | P2 | code, tests, env-tests, dashboards | A real join's set carries refs and names only registered handlers |
| 13 | MC-B1 | Heartbeat intake, verdicts, remove-only reconcile, binding gate, connection_id required, N config | meeting-controller | P1, MC-A | proto (rpc), code, tests, dashboards | None (heartbeat check rides MC-B2; R-9 deterministic, under discussion) |
| 14 | MH-CLOSE | MH numeric close codes on every intentional close | media-handler | P2 | code, tests, env-tests, dashboards | Release observed as HANDLER_RELEASED; invalid credential as DECLINED |
| 15 | MH-2 | MH per-meeting heartbeat, verdicts, owner check, release causes, all-or-none pin, process-start epoch deleted | media-handler | MC-B1, MH-CLOSE | proto, code, tests, env-tests, dashboards | Heartbeats flow per meeting against the real MC; no releases while owned |
| 16 | MC-D | MC-minted `media_dial` (per-meeting Ed25519), delivery/refresh, ref-keyed status, FAILED re-push | meeting-controller | SEC-A, P1, MC-A | code, tests, env-tests, dashboards | A real join's set carries a verifiable token per member |
| 17 | C1 | SDK reconcile-to-set transport, per-ref tokens, close-code table, per-transition reports, e2e migration | client | P2, MC-A, MC-D, MH-CLOSE | code, tests, env-tests | Existing browser specs migrated to bus handler sets |
| 18 | T-1 | MH pool fixture, process injector, resilience hygiene, `mhPool.ts`, participant set exposure | test | I-5, I-2, MC-A | test infra, env-tests | Fixture smoke (up/drain/baseline); injector smoke (freeze/resume, kill) |
| 19 | MH-1 | MH `media_dial`-only admission, pending-path deletion, pending cap, env-test client switch | media-handler | SEC-A, MH-CLOSE, MC-D, C1, T-1 | code, tests, env-tests, deploy, dashboards | Token connects; WRONG_HANDLER; AC token / unheld meeting DECLINED; dial after join-JWT lifetime; left participant DECLINED |
| 20 | MH-3 | MH drain on SIGTERM, owner-silence release, immediate load report, QUIC idle timeout as config | media-handler | MH-2 | code, tests, env-tests, deploy, dashboards | Config surface (drain and owner-silence deterministic; Kind drain legs in MC-F) |
| 21 | MC-B2 | Pure `assess`, Dead/Impaired declaration, removal | meeting-controller | MC-B1, MH-2, T-1 | code, tests, env-tests, dashboards | Kill one MH of two → media continues on the survivor; zombie freeze/resume → released |
| 22 | P3 | Retire superseded signaling/connect surfaces (media_servers, url fields, mh_url, join_token) | protocol | MC-A, C1, MH-1, T-1 | proto, codegen, code, tests | None new (Layer 7 stays green) |
| 23 | MC-C | Crash/impaired replacement via GC; awaiting-handler; per-meeting bounds | meeting-controller | GC-3, MC-D, C1, MH-1, T-1, MC-B2 | code, tests, env-tests, dashboards | Crash with replacement (new ref, GC granted); stranded pair (Rust) |
| 24 | MC-F | Planned transfer: make-before-break, survivor target, deadline cutover | meeting-controller | MC-C, MH-3 | code, tests, env-tests, dashboards | Planned drain with a target; planned drain without a target |
| 25 | MC-E | Required loss metric, client-loss disagreement, HandlerCensus | meeting-controller | C1, MC-B2 | code, tests, env-tests, dashboards | Extends MC-B2's crash leg: loss-metric observation rises |
| 26 | C2 | R-42 client series + export path, reconnecting state, connecting-media notice, debug panel, resume_ms, swap continuity tests | client | C1 | code, tests, telemetry export | None (unit/component; exercised live by C3) |
| 27 | I-3 | Deploy guards: template invariants, Kind-only instrumentation, slot/allowlist drift, force-delete ban | infrastructure | I-2, GC-2 | guard code, unit-tests | None (guard fixtures) |
| 28 | OPS | Runbooks: deployment procedures around the slot pool, incident scenarios (13 anchors), TODO dispositions | operations | I-2, MH-1, MH-2, MH-3, GC-2, GC-3, MC-C, MC-F, MC-E | runbooks, TODO | None (procedures exercised in T-2) |
| 29 | OBS-ALERTS | Alert rules (MC/MH/GC), inventory, SLO line, cross-service panels | observability | GC-1, GC-2, GC-3, MC-D, MH-1, MH-CLOSE, MH-3, MC-C, MC-F, MC-E, C2, OPS | alerts, dashboards, docs, env-tests | Every new/changed rule loads; alert `for:` durations match config gauges; no firing legs |
| 30 | C3 | Browser resilience spec `mh-replacement.spec.ts` | client | T-1, MC-C, MC-F, C2 | env-tests | Browser crash, planned transfer with target, stranded pair |
| 31 | T-2 | Manual test plan, written and run, with resilience evidence | test | P3, I-3, OPS, OBS-ALERTS, C3 | manual test plan | None (manual run) |

### Requirements Coverage

| Req | Covered By Tasks |
|-----|-----------------|
| R-1 | MC-B2, MC-C, C3 |
| R-2 | MC-C, C3 |
| R-3 | MC-F, MH-3, C3 |
| R-4 | MC-B2 |
| R-5 | MC-D, C1, MH-1 |
| R-6 | MC-C |
| R-7 | P1, MC-B1, MH-2 |
| R-8 | MH-2 |
| R-9 | MC-B1 |
| R-10 | MC-B1, MC-B2, MH-2 |
| R-11 | P1, MC-B1, MH-2 (owner check), MH-3 (owner-silence release) |
| R-12 | MC-A, MH-1 |
| R-13 | MH-1 |
| R-14 | MC-D, C1 |
| R-15 | I-4, SEC-A, P1, MC-D, GC-3, C1 |
| R-16 | C2 |
| R-17 | P2, MC-A, C1, P3 |
| R-18 | C1, C2 |
| R-19 | P2, MH-CLOSE, MH-1, C1 |
| R-20 | C2 |
| R-21 | MH-3 |
| R-22 | MH-3 |
| R-23 | GC-3, MC-C |
| R-24 | GC-1 |
| R-25 | GC-2 |
| R-26 | GC-1 |
| R-27 | I-1, I-2 (host-publish UDP leg) |
| R-28 | I-2, I-3, T-1 |
| R-29 | I-5, I-2 |
| R-30 | I-2, OPS |
| R-31 | I-2, I-3 |
| R-32 | I-2 |
| R-33 | MC-E, OBS-ALERTS |
| R-34 | every feature task (per-task obligations), OBS-ALERTS |
| R-35 | OPS, OBS-ALERTS |
| R-36 | P1, MC-A, MC-B1, MH-1, MH-2, P3 |
| R-37 | OPS, MC-A (cap hard-fail), config rows in each key-introducing task |
| R-38 | SEC-A, GC-2, GC-3, OPS, OBS-ALERTS |
| R-39 | T-0, T-1, I-2, MC-B2, MC-C, MC-F, MH-1, C3 |
| R-40 | every service task (its `test-gates.md` section), reviewed by test |
| R-41 | T-2 |
| R-42 | C2 |

### Aspect Coverage

| Aspect | Covered By Tasks | N/A? |
|--------|-----------------|------|
| Code | P1, P2, P3, GC-1–3, MC-A, MC-B1, MC-B2, MC-C, MC-D, MC-E, MC-F, MH-CLOSE, MH-1–3, C1, C2, I-2, I-3, I-4, I-5 | |
| Database | GC-1, GC-2, GC-3 (query patterns, schema doc) | No migration (database) |
| Tests | every code task (unit/integration per `test-gates.md`); env-tests per the column above; fixtures T-0, T-1 | |
| Observability | series + panels + catalog in each feature task; rules in OBS-ALERTS | |
| Deployment | I-2, GC-1, GC-2, MH-1, MH-3 (config keys), I-3 | |
| Operations | OPS, T-2 | |

**Remaining host-side actions (not devloop steps)**: after I-2 merges, rebuild devloop-helper on the host and recreate every existing Kind cluster to get the 6 slot mappings; the first deploy, and any rollback across I-2, requires a cluster recreate.

---

## Task Metadata (dt-story manifest v1)

The **only** home for per-task machine state (ADR-0035 §4). Emitted with `dt-story add-task`; do not hand-edit.

```yaml
# task-metadata (dt-story manifest v1)
story: 2026-10-31-survive-handler-loss
tasks:
- id: 1
  status: completed
  specialist: infrastructure
  prompt: |-
    Story 4 task I-1 (infrastructure): Gate zero — prove UDP hostPort reachability and NetworkPolicy enforcement on the hostPort DNAT path in Kind.

    GOAL
    ADR-0039 (docs/decisions/adr-0039-mh-replacement-and-migration.md) Implementation Guidance "First" and §7: before any MH slot change lands, an env-test must prove two things on the live Kind cluster. (1) A host-side client reaches a pod end to end over a UDP hostPort. (2) The real MH NetworkPolicy (infra/services/mh-service/network-policy.yaml) is enforced on that hostPort/portmap DNAT path. If either fails, the fallback is adopted and recorded: StatefulSet-per-slot behind per-slot NodePort Services with publishNotReadyAddresses. No unpolicied UDP ingress ever ships. This task changes no product behaviour; it is the gate for task I-2.

    Requirement satisfied: R-27 (docs/user-stories/2026-10-31-survive-handler-loss.md).

    WHY THE EVIDENCE IS PORT-BASED (security ruling)
    The MH policy admits UDP 4434 from any source by design (browsers), so a "disallowed source" cannot be a client IP. The gate proves the policy is EVALUATED on the DNAT path: an allowed port is delivered and a non-allowed port is dropped, both through the same hostPort mechanism the slots will use.

    WHAT TO BUILD
    A new env-test in crates/env-tests/, in its OWN test binary (e.g. a new tests/<nn>_gate_zero_hostport_policy.rs) so no other test runs alongside it.
    - Canary pod in namespace dark-tower:
      - carries EXACTLY the MH NetworkPolicy podSelector labels (app=mh-service, component=media), so the real policy object selects it, not a copy;
      - has a readiness probe that never passes, so it is never an endpoint of the mh-service ClusterIP Service (which selects the same labels), while label-based policy and hostPort DNAT still apply;
      - exposes no 8083 or 50053;
      - listens on UDP 4434 (policy-allowed) and UDP 9999 (not in the policy) and logs every datagram it receives;
      - uses the busybox image already used by crates/env-tests/src/canary.rs (busybox:1.36). Extend or reuse CanaryPod where it fits; do not add a new image without checking preload_third_party_images in infra/kind/scripts/deploy.sh.
    - hostPort exposure uses two EXISTING, currently dead kind mappings, so no cluster recreate is needed. infra/kind/kind-config.yaml.tmpl maps containerPort 30830 (MH-0 health, nothing listens) and 30530 (MH-0 gRPC, nothing listens) to helper-allocated host ports; the static infra/kind/kind-config.yaml must be checked for the same and the test must fail loudly if a mapping it needs is absent.
      - The canary declares hostPort 30830 -> containerPort 4434/UDP and hostPort 30530 -> containerPort 9999/UDP.
      - The test reads the host-side port numbers from the existing port-map environment (DT_PORT_MAP / helper port map for devloop clusters; the static values for host mode), never hardcoded.
    - The client sends datagrams to the host-gateway address the slots will use (DT_HOST_GATEWAY_IP in a devloop cluster, 127.0.0.1 in host mode), from where env-tests run.
    - Positive controls on BOTH arms, before any external datagram counts:
      - each listener proves itself with a local self-send logged as received;
      - allowed arm: an external datagram to the 4434 path is echoed back AND appears in the canary log;
      - drop arm: an external datagram to the 9999 path is never echoed and never appears in the canary log within a bound.
      - Silence on either arm before its control passes FAILS the gate; it is never a pass.
    - Cleanup is explicit and asserted. A Drop backstop prints the kubectl delete command for the canary. It must never use `kubectl delete pod --force --grace-period=0`.

    OUTCOME HANDLING
    - PASS: the gate result goes in the story file's Design section (docs/user-stories/2026-10-31-survive-handler-loss.md), and I-2 proceeds on hostPort slots.
    - FAIL on either arm: do not work around it. Record the failure and the adopted fallback (StatefulSet-per-slot behind per-slot NodePort Services with publishNotReadyAddresses) in the story file, and escalate to the Lead before I-2 starts. The fallback changes I-2's design.
    - Calico's CNI chain must include the portmap plugin for hostPort to work at all. If hostPort delivery itself fails, report that distinctly from a policy failure.

    PAIRING AND REVIEW
    Paired with test (fixture discipline, own binary, positive controls). Security reviews the evidence shape (same policy object, both arms positive, same DNAT mechanism as the slots).

    DEPENDENCIES
    None. This is the first task of the story; I-2 depends on it.

    TIER: full (decides the architectural fallback and the MH network-policy posture).

    CROSS-TASK OBLIGATIONS
    (a) No new config keys. (b) No retirements. (c) No new metric series. (d) test-gates.md "Gate zero" line is implemented here. (e) The canary's labels never become metric labels.

    COVERS: env-tests.
  slug: 2026-11-06-gate-zero-hostport-policy
  tag: story-2026-10-31-survive-handler-loss-task-I-1
- id: 2
  status: completed
  specialist: infrastructure
  prompt: |-
    Story 4 task I-4 (infrastructure): an exact allow-list construct for the internal-proto key-material guard, landing with an EMPTY list.

    GOAL
    ADR-0039 §5 (docs/decisions/adr-0039-mh-replacement-and-migration.md): the MC→MH contract will carry exactly one public verification key, `RegisterMeetingRequest.media_dial_public_key` (32 bytes, Ed25519 public half). Check 1 of the `internal-proto-no-key-material` guard (crates/dt-guard/src/internal_proto_no_key_material.rs) currently rejects every `bytes` field with no per-field allowance. This task changes the guard MACHINERY so check 1 accepts an exact, enumerated allow-list. The list lands EMPTY. The single entry is policy content owned by security and lands in task P1 in the same commit as the field.

    Requirement satisfied: R-15, guard half ("validation fails on any key-material field other than the one allow-listed public key, and on a stale, renamed or non-bytes allow-list entry").

    OWNERSHIP (CLAUDE.md "Guard-crate ownership")
    Construct shape and evaluation are infrastructure (machinery). List membership is security (content). This task changes the construct, so it is machinery; it adds no membership.

    THE CONSTRUCT
    - An allow-list of (fully qualified message name, field name) pairs, e.g. the form (dark_tower.internal.v1.SomeMessage, some_field).
    - Matching is exact string equality on BOTH components. No patterns, globs, prefixes or regexes, no type-based exemption, no suppression comment or attribute path, no environment or CLI override.
    - Only check 1 consults the list. Every other check in the guard is unchanged.
    - Vacuity (fail loudly). Every entry must resolve, via the guard's existing proto lexer (crates/dt-guard/src/common/proto_lex.rs and common/proto_files.rs), to an existing field on that exact message whose type is `bytes`. Each of these fails the guard with a message naming the entry:
      - an entry whose message does not exist;
      - an entry whose field does not exist on that message (renamed or deleted);
      - an entry whose field is not `bytes`.
    - The list has one home in the guard module, with a header comment stating: membership is security-owned content; the shape and evaluation are infrastructure machinery; never string-encode a key to dodge the guard (ADR-0039 §5).
    - The guard's rationale text is re-derived to: "no secret or media-key material on the internal contract; exactly allow-listed public verification keys only".

    TESTS (in the crate, fixture-proven, every negative with its positive arm)
    Fixtures use a test-local list, so the shipped list stays empty:
    - a bytes field on a listed (message, field) passes check 1;
    - the same field name on a DIFFERENT message still fails;
    - a listed entry whose field was renamed fails (vacuity);
    - a listed entry whose message does not exist fails;
    - a listed entry whose field is `string` or another non-bytes type fails;
    - with the shipped (empty) list, the repo passes exactly as today.
    Keep the existing e2e (crates/dt-guard/tests/proto_ports_e2e.rs, `--root` flag) green and add a case for the allow-list there if it exercises check 1.

    MUST NOT
    - Add any entry. P1 adds (dark_tower.internal.v1.RegisterMeetingRequest, media_dial_public_key) with the field.
    - Touch proto/.

    DEPENDENCIES AND PAIRING
    None. P1 depends on this task. Security reviews (construct semantics, no bypass path).

    TIER: full (secrets-policy guard).

    CROSS-TASK OBLIGATIONS
    (a) No config keys. (b) No retirements. (c) No metric series. (d) No service test-gates section; the guard's own fixtures are the gate. (e) N/A.

    COVERS: code, unit-tests.
  slug: 2026-11-06-internal-proto-key-allowlist
  tag: story-2026-10-31-survive-handler-loss-task-I-4
- id: 3
  status: completed
  specialist: security
  prompt: |-
    SEC-A: Security policy content for MC-minted dial credentials

    Specialist: security. Tier: full (auth/crypto/secrets policy content). Dependencies: none. Covers: guard-lens and TODO documentation; no production code, no env-tests.
    Pairing: semantic-guard reviews the checks.md edits as the lens's consumer. meeting-controller, media-handler and client review the parts that name their emission sites.
    Requirements satisfied: R-15 (credential and key isolation, the review lens that enforces it) and R-38 (the MH-ownership residual update).

    GOAL
    Story 4 (docs/user-stories/2026-10-31-survive-handler-loss.md) makes MC the issuer of a short-lived `media_dial` credential. ADR-0039 §5 specifies it. MC signs a compact JWS with a per-meeting Ed25519 key. MH verifies it with that meeting's public key, which arrives on `RegisterMeetingRequest.media_dial_public_key`. The client holds the latest token per in-set handler to redial.
    This task updates the security review lenses so the implementing tasks are reviewed against the right rules, and records the changed residual. It must land BEFORE MC-D (MC-minted media_dial) and MH-1 (MH media_dial-only admission), so their Gate reviews apply the updated lens. It does not block any other task.

    GOVERNING TEXT
    - docs/decisions/adr-0039-mh-replacement-and-migration.md §5 (dial credential), §6 (owner check, owner-silence release), Consequences "Recorded residual risks (security)".
    - ADR-0036 §4 "MH stays keyless, and that is a guard, not a convention".
    Carry these; invent nothing.

    CHANGE 1: scripts/guards/semantic/checks.md, §Check: Credential Leak, "Rust — MC key custody (items 11-13)"
    Extend the scope from "the meeting KEK and transmit keys" to also cover the per-meeting media_dial SIGNING key: the private half of the Ed25519 keypair the MC meeting actor generates (ADR-0039 §5). Add it to the emphasis list of emission sites:
    - the keypair's generation in the MC meeting actor, next to and separate from the KEK. The implementing task names the private half `dial_signing_key` inside a redacting wrapper with no Debug output;
    - token minting;
    - the MediaHandlerSet encode that carries tokens;
    - the RegisterMeeting push that carries the PUBLIC half.
    Item 11 stays absolute for KEK and transmit keys. State explicitly that the ONLY permitted key bytes on the MC→MH contract are the 32-byte PUBLIC verification key on the single field allow-listed by dt-guard internal-proto-no-key-material (that allow-list construct is I-4; its one entry lands in P1). The private dial signing key, or any symmetric key, crossing that contract is an item-11 finding. Item 12 sinks apply to the private dial signing key and to minted tokens. Item 13 (redaction defeated) applies to the redacting wrapper around the private half.
    Add one short note for MH: a minted token or the raw connect payload reaching a tracing macro, span field, metric label, error string or gRPC Status message on MH's verify path is an item-12-class finding. Credential-failure reasons go to metrics only (MH's validation counter, per the telemetry contract) and never to the client: one generic DECLINED (ADR-0039 §7 close codes).

    CHANGE 2: scripts/guards/semantic/checks.md, §Check: Client Credential Lifetime
    Add a scoped, stated exception: the media_dial dial credential is NOT a "credential retained after the exchange that made it unnecessary". The client needs it for every dial (join, redial, retry) within its lifetime (ADR-0039 §5 "Client retention"; the scoped R-23 amendment). It is SAFE only when ALL of these hold:
    - held only in the media transport's current-set state, latest token per in-set handler ref;
    - replaced wholesale by each new MediaHandlerSet;
    - cleared when its ref leaves the set and at session end;
    - passed only into the connect request.
    Add UNSAFE examples, each a finding:
    - the token stored anywhere outside that set state: a store, a prop, session object, localStorage or sessionStorage;
    - kept after its ref left the set;
    - reaching any §Credential Leak client sink (items 5-10: metric label, span attribute, console, error or cause chain, JSON.stringify or structured clone, DOM render, debug panel, e2e test bus);
    - a fabricated request path to obtain a token. MediaDialTokenRequest does not exist (ADR-0039 §5); after DECLINED the client waits for the next set.
    Add one SAFE example mirroring the bullets above. Do not restate the PII word list. The mechanical guard (dt-guard ts-no-retained-credentials) classifies `token` as a session token, not a credential, so this lens is the only enforcement; say so in the "Known blind spots this lens OWNS" list.

    CHANGE 3: docs/TODO.md entry "MH does not verify meeting ownership" (§Media Path Obligations)
    Append, without restating the entry, a dated paragraph (2026-10-31, story 4, ADR-0039) recording two things.
    - A validation-passing RegisterMeeting under the self-asserted `mc_id` now also REPLACES that meeting's media_dial public key and the endpoint MH asks for sender bindings, on that MH. A scope holder that re-registers a meeting therefore controls dial admission and binding answers for it on that MH. That yields ciphertext only (MH keyless; frames need the AC-attested identity key).
    - The same registration resets MH's owner-silence timer (ADR-0039 §6, `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS`), so a takeover also keeps the meeting from being released for silence.
    The "whatever binds caller identity to mc_id MUST cover BOTH RegisterMeeting AND EndMeeting" sentence now also covers the dial key. Keep the entry OPEN.

    NOT IN THIS TASK (self-containment; say so in the devloop output)
    - The internal-proto-no-key-material allow-list ENTRY lands with the field in P1, after I-4.
    - The S6 TODO line (periodic MH bearer; GC allowlist is not peer authentication) lands in GC-2 with the allowlist.
    - No code, config or metrics change.

    ACCEPTANCE
    - checks.md renders the three additions inside the existing sections, with no new top-level check.
    - Every claim cites ADR-0039 or ADR-0036 by section.
    - No fenced code blocks are introduced.
    - Layer 3 is green, including validate-knowledge-index and any docs guards.
    - The docs/TODO.md entry is still open, and the new paragraph does not duplicate existing text.

    CROSS-TASK OBLIGATIONS (a)-(e): none applies. There are no config keys, retirements or series, this is not a service task, and there are no labels. State this in the devloop output.
  slug: 2026-11-06-sec-a-media-dial-lens
  tag: story-2026-10-31-survive-handler-loss-task-SEC-A
- id: 4
  status: completed
  specialist: test
  prompt: |-
    T-0: Env-test MH discovery by label

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md (story 4 of the ADR-0036 arc). Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md, section 7 (MH addressing and provisioning) and Implementation Guidance (ordering: the env-test fixture migration off mh-0/mh-1 must keep Layer 7 green across the slot change). Test gates: docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md, section "Round 3", item "Fixture migration".

    Requirements satisfied: R-39 (existing env-tests find MHs by slot-independent discovery and pass in any order; this task does the runtime-discovery half).

    GOAL
    Env-tests and Layer 7 stop assuming two fixed MH instances named mh-0 and mh-1 for RUNTIME access (gRPC, pod IP, Prometheus instance pinning). After this task, every MH a test talks to is found by label from the live pods, and its gRPC port-forward is owned by the test fixture, not by layer7.sh. It must be green against today's cluster (the mh-0/mh-1 Deployments). It lands BEFORE I-2 (infrastructure: MH slot pool, single template, mh.sh), which repurposes the host-port offsets that today's forwards use. This task is a precursor of I-2; I-2 depends on it.

    WHAT CHANGES
    1. layer7.sh Phase 1i goes away. Today scripts/layer7.sh starts one kubectl port-forward per MH pod and exports ENV_TEST_MH_{0,1}_GRPC_URL and ENV_TEST_MH_{0,1}_POD_IP (the forward helpers around scripts/layer7.sh:520-680 and the Phase 1i call site near :1600). Delete the step, its preconditions, its REASON tokens (mh-grpc-forward-not-ready, mh-grpc-port-occupied, mh-grpc-port-unallocated) and its step-duration name (mh-grpc-forwards). Update scripts/layer7.test.sh for the removed step, and remove the Phase 1i rows from docs/runbooks/devloop-validation.md (around :737-739). Retire ENV_TEST_MH_{0,1}_* everywhere, including crates/env-tests/README.md prerequisite 4.
    2. A fixture-owned forward. In crates/env-tests/src/fixtures/mh_grpc.rs, `handlers()` resolves every live, Ready, non-terminating pod with label `app=mh-service` in namespace dark-tower (reuse the pod-listing helpers in crates/env-tests/src/fixtures/kube.rs, `live_pods_json`, rather than a new kubectl call shape). For each it starts a per-test `kubectl port-forward pod/<pod> :50053` (LOCAL PORT 0), reads the bound local port back from kubectl's "Forwarding from 127.0.0.1:<port>" line, waits until it accepts, and stops the child process on Drop. HARD CONSTRAINT (agreed with infrastructure): the forward must NEVER bind a port number from the devloop helper's port map (the ENV_TEST_MH_{0,1}_GRPC / HOST_PORT_MH_*_GRPC offsets), because I-2 turns those offsets into MH slots. `Handler` keeps `grpc_url` and `pod_ip`; its `name` becomes the pod name (no instance assumption). A forward that cannot be established is a loud PRECONDITION failure naming the pod and the kubectl output; never skip.
    3. No fixed count. crates/env-tests/src/fixtures/injection.rs `both_handlers()` asserts exactly two pods; replace it with "all live MH pods" (rename to reflect that), asserting at least one and that no MH pod is Terminating at resolution time (a Terminating pod mid-suite is a loud PRECONDITION, not a silent subset). Every caller (tests 26, 28, 29, 37, 38, 39 via `injectable_handlers`, `assert_mc_programmed_a_carrying_pod`, `install_all`) keeps its meaning: injections go to every live MH.
    4. Prometheus pinning stays per pod IP (crates/env-tests/src/fixtures/metrics.rs `instance_for_pod_ip`, `distinct_pod_ips`); only the source of the IPs changes.
    5. Tests and fixtures that name mh-0/mh-1 for RUNTIME purposes stop doing so: mh_grpc, injection, media, metrics, egress_admission fixtures; tests 26, 28, 29, 37, 38, 39. Comments that describe today's two-instance Kind layout as an assumption are updated to describe discovery.

    OUT OF SCOPE (stays in I-2, inherently per-instance until slots exist)
    - crates/env-tests/src/fixtures/kube.rs `MH_INSTANCES` and `PROFILE_READERS` and every `configmap_name_for_all(MH_INSTANCES, ...)` read (tests 01, 02, 28, 29, 38) and the deployment_config fixture's mh-0/mh-1 config names: these read per-instance ConfigMaps, which I-2 replaces with slot ConfigMaps.
    - Anything about slots, mh.sh or the MH pool fixture (I-5, I-2, T-1).

    SAME COMMIT
    The layer7.sh deletion, the fixture forward and every consumer migration land together; Layer 1 to 7 green after the commit. Run Layer 7 against a live cluster, not only the hermetic self-tests.

    TESTS
    - Hermetic unit tests (in-module, like the existing kube.rs/metrics.rs tests): parsing kubectl's forwarding line (including a malformed line, which fails loudly); label-based pod selection excludes Terminating and not-Ready pods; zero pods is a PRECONDITION failure.
    - scripts/layer7.test.sh: updated so the removed phase is absent and no test still expects ENV_TEST_MH_* exports.
    - Env-tests: the existing MH suites (26, 28, 29, 37, 38, 39) pass unchanged in assertion content through the new discovery.

    CROSS-TASK OBLIGATIONS
    (a) No new config key. (b) This task retires ENV_TEST_MH_{0,1}_* and Phase 1i: remove every reference (README, runbook REASON list, layer7.test.sh) in the same commit. (c) No new metric series. (d) This IS test's section for the fixture migration. (e) No metric labels involved.

    PAIRING AND DEPENDENCIES
    Dependencies: none. I-2 (infrastructure) depends on this task. Pair with infrastructure (layer7.sh lane semantics and the port-map constraint). Tier: full (Layer 7 lane change plus a shared env-test fixture seam).
  slug: 2026-11-06-env-test-mh-discovery-by-label
  tag: story-2026-10-31-survive-handler-loss-task-T-0
- id: 5
  status: completed
  specialist: protocol
  deps:
  - 2
  prompt: |-
    Task P1: Story-4 internal contract, additive, plus the media_dial public-key allow-list entry

    Specialist: protocol. Tier: full (proto/wire, Guarded Shared Area, key-material guard).
    Pairing: pair with security. Security owns the allow-list MEMBERSHIP, and internal.proto is a GSA owned by protocol, auth-controller and security, so auth-controller reviews as well.
    Depends on: I-4, which lands the exact-equality (message FQN, field) allow-list construct for check 1 of `dt-guard internal-proto-no-key-material`, with an EMPTY list. Do not start until I-4 has landed.
    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md (Accepted), sections 1, 5, 6 and 8, plus Consequences and TODO dispositions.
    Requirements satisfied (contract parts only; behaviour lands in the consumer tasks named below): R-7, R-9, R-10, R-11, R-15, R-36 (comment retirements).

    GOAL
    Add every new internal.proto type and field that story 4's MC and MH tasks consume. Write each rule ONCE, at its field, so consumers implement against one text. The change is ADDITIVE: no field is deleted here, and no `rpc` line is added. A tonic server trait has no default methods, so an rpc without its server impl fails compile. `rpc MeetingHeartbeat` therefore lands in MC-B1 with MC's server impl. The AcquireMediaHandler slice (rpc, request/response, RegisterMCResponse.max_exclude_handler_ids) lands in GC-3, and the RegisterMH endpoint-rule comment block lands in GC-2. Do NOT add those here.

    CHANGES (all in proto/dark_tower/internal/v1/internal.proto unless stated; fresh tags only, never a reserved one; follow docs/protocol/CONVENTIONS.md: buf STANDARD, ENUM_UNSPECIFIED zero values, bare request/response names)

    1. enum ReleaseCause { UNSPECIFIED; HEARTBEAT_TIMEOUT; APPLY_LAG; PLANNED_TRANSFER; MEETING_ENDED; } (ADR §6 "One enum").
       - It is the single vocabulary for AcquireMediaHandlerRequest.trigger (GC-3), EndMeetingRequest.cause and the RELEASED heartbeat verdict.
       - Every service projects its `trigger` metric label from it (like KindLabels::from_proto).
       - State: MEETING_ENDED and UNSPECIFIED are INVALID_ARGUMENT on AcquireMediaHandler. EndMeetingRequest.cause rejects UNSPECIFIED. On a RELEASED verdict, UNSPECIFIED is ONLY the documented misroute case.
    2. enum ServingState { UNSPECIFIED; SERVING; DRAINING; }. UNSPECIFIED is rejected (ADR §1).
    3. enum Verdict (name per CONVENTIONS) { UNSPECIFIED; OWNED; RELEASED; }. A received UNSPECIFIED verdict is malformed: MH counts it and takes no action (ADR §6).
    4. enum ApplyRefusal { UNSPECIFIED (= no refusal); INVALID; TRANSPORT_INCONSISTENT; STREAM_CEILING; EDGE_CAP; } (ADR §1).
       - It is a 1:1 projection of MH's deterministic refusals: rejected_invalid, rejected_transport_inconsistent, rejected_stream_ceiling (PolicyApplyOutcome, crates/mh-service/src/observability/metrics.rs), and ApplyFailure::EdgeCapExceeded inside apply_failed (crates/mh-service/src/session/mod.rs).
       - It reflects the latest apply and clears on success.
       - STREAM_CEILING and EDGE_CAP are load-dependent. INVALID and TRANSPORT_INCONSISTENT are MC contract faults. Transient failures surface as generation lag and never appear here.
    5. message MeetingHeartbeatRequest { meeting_id; mc_id; handler_id; ServingState serving_state; uint32 drain_remaining_ms; uint64 applied_generation; ApplyRefusal apply_refusal; repeated string connection_ids; }
       message MeetingHeartbeatResponse { Verdict verdict; ReleaseCause cause; }
       State once, at these messages (ADR §1, §2, §6):
       - One unary heartbeat per (meeting, handler) to that meeting's MC meeting actor.
       - The interval comes from that meeting's RegisterMeetingRequest.heartbeat_interval_ms.
       - Heartbeats start at the registration UPSERT, not at the first successful apply. A refused first apply reports applied_generation 0 plus its apply_refusal.
       - Each meeting's first send is jittered in [0, interval). DRAINING resets every deadline to now.
       - Not retried. The deadline is shorter than the interval, so at most one heartbeat per meeting is outstanding and they arrive in order.
       - drain_remaining_ms is set iff DRAINING, relative, and MH's grace is the only copy.
       - applied_generation REFERENCES RegisterMeetingResponse.applied_generation and does not restate it. An entry with a lower applied_generation than the last accepted one is lag, never liveness.
       - connection_ids are the ids for which MH has begun NotifyParticipantConnected and not yet sent Disconnected (a superset including in-flight binds).
       - Reconcile is REMOVE-ONLY. A heartbeat never adds connectivity: the sender_id bind is the only add path, and MC never looks up an incoming id. MC retires a key absent from a heartbeat only if it recorded the bind before the PREVIOUS heartbeat for that (meeting, handler) arrived.
       - mc_id is the meeting's REGISTERED MC. An MC that is not that MC answers FAILED_PRECONDITION, never RELEASED, and MH counts it and takes no action.
       - Verdict: OWNED does nothing. RELEASED means "not in my set": MH ends the meeting through the EndMeeting path, closing sessions with HANDLER_RELEASED and sending no Disconnected.
       - Cause rules on RELEASED: a removed handler gets its removal trigger; no MC meeting actor gets MEETING_ENDED (answered by MC's router, no tombstone); a live meeting whose set never held the handler gets UNSPECIFIED (misroute, counted).
       - MH never self-fences on short MC silence. Owner-silence release: MH ends a meeting after MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS (default 900, range [300, 3600]) with no communication from its owning MC. Communication means a RegisterMeeting or EndMeeting from it, or a heartbeat verdict from it; errors and FAILED_PRECONDITION don't count. MC ids are per process. This is not an MC-failover mechanism.
       - Size is bounded by the per-meeting caps MC enforces. Ids and counts are validated (validate_id_field pattern, crates/mc-service/src/grpc/media_coordination.rs).
       - Comment: this rides the plaintext, MC-unauthenticated MH->MC channel (the S6 residual), so it is not an authorization control.
       - Observability: meeting_id, mc_id, handler_id, connection_ids and generation values are never metric labels.
    6. RegisterMeetingRequest gains:
       - `uint32 heartbeat_interval_ms`: MC-derived (N/3) from its one value MC_MH_HEARTBEAT_TIMEOUT_MS (default 6000, range (0, 30000] ms). 0 rejects the registration. Enforcement owner: MH-2, which lands after MC-B1 makes MC emit a non-zero value (the same ordering trap the policy_generation comment documents). MH has no cadence config.
       - `bytes media_dial_public_key`. Use security's field comment verbatim: "PUBLIC Ed25519 key, exactly 32 bytes, else the registration is rejected; populated only from the meeting's dial keypair; the private half never leaves MC's meeting actor; NOT media key material (ADR-0036 §4)". MH selects it by the token's meeting_id claim (ADR §5). Populated by MC-D and consumed by MH-1.
       IN THE SAME COMMIT, add the single allow-list entry (dark_tower.internal.v1.RegisterMeetingRequest, media_dial_public_key) to the I-4 construct in crates/dt-guard/src/internal_proto_no_key_material.rs. This is security-reviewed content, and the guard stays red on any other bytes field. Never string-encode the key to dodge the guard.
    7. EndMeetingRequest gains `ReleaseCause cause` (UNSPECIFIED rejected). A removal-cause release closes connections still awaiting their bind with HANDLER_RELEASED; MEETING_ENDED leaves them to the bind outcome (ADR §6). Populated by MC-B2.
    8. Comment edits in this file:
       - RegisterMeetingResponse.applied_generation: state the all-or-none invariant once (ADR §1, R4-2). On every refusal (stale, INVALID, TRANSPORT_INCONSISTENT, per-meeting bounds, STREAM_CEILING, EDGE_CAP, transient) the previously installed snapshot keeps forwarding unchanged, and applied_generation (response AND heartbeat) keeps reporting it, or 0 if none. An equal generation is a no-op reported as applied. The registration record is upserted independently of the apply, and a refusal does not roll it back.
       - RegisterMeetingResponse.handler_id: narrow "acceptable as a metric label" to "never a metric label on MC/GC series; per-handler views come from the MH pod's own series" (ADR §8).
       - MediaHandlerService header and RegisterMeetingRequest: remove the periodic re-assert / "re-assert remains the whole recovery story" framing (ADR §1: the cadence is dropped). Registration stays the full-snapshot forwarding policy.
       - EndMeetingRequest: delete race (C) "Story 4's periodic re-assert" (moot, TODO dispositions), and DELETE the "ROLLOUT (R-24)" paragraph (dead older-MH compatibility text; Lead ruling). Do NOT touch the policy_generation ORDERING CONSTRAINT text.

    DO NOT (other tasks own these)
    - Add any rpc line (MC-B1, GC-3). Add the AcquireMediaHandler messages or max_exclude_handler_ids (GC-3), or the RegisterMH rule block (GC-2).
    - Delete process_start_epoch_ms or the connection_id legacy text (consumer tasks).
    - Change service behaviour.

    MINIMUM CONSUMER EDITS THAT MUST LAND IN THIS COMMIT (prost struct literals fail to compile when a message gains a field)
    - Every Rust construction site of RegisterMeetingRequest (crates/mc-service/src/grpc/mh_client.rs and tests, mh-service tests): set heartbeat_interval_ms 0 and an empty media_dial_public_key explicitly. MH does not enforce either until MH-2 and MH-1, and MC populates them in MC-B1 and MC-D.
    - Every construction site of EndMeetingRequest: today's only production caller is meeting teardown (crates/mc-service/src/grpc/mh_client.rs end_meeting, plus media_routing/teardown.rs), so set cause = MEETING_ENDED there. That is its true meaning, and it lets MH-2 enforce "UNSPECIFIED rejected" without an ordering trap. MC-B2 adds the removal causes. Test fixtures set an explicit cause.
    - No other behaviour changes.

    TESTS
    - crates/proto-gen/tests/internal_roundtrip.rs: a wire-shape roundtrip for every new message and field, keeping its "what these do NOT assert" header honest.
    - Guard: the entry passes; a second bytes field still reds; renaming or retyping the allowed field reds (I-4's vacuity).
    - Implement the Protocol section of docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md for the internal messages, paired with test (cross-task obligation (d)).
    - Layer 1–7 green: buf lint/breaking (additive only), the Rust workspace, and TS codegen if internal_pb.ts is generated (packages/proto-gen/scripts/codegen-paths.sh).

    CROSS-TASK OBLIGATIONS
    (a) No new config key is read here. The keys are named only as contract references; their owning tasks add the runbook rows.
    (b) The comment retirements remove only proto text.
    (c) No series are emitted here.
    (d) Test-gates Protocol section, as above.
    (e) Label hygiene: every comment that names an id states it is never a label.
  slug: 2026-11-06-story4-internal-contract-p1
  tag: story-2026-10-31-survive-handler-loss-task-P1
- id: 6
  status: completed
  specialist: protocol
  prompt: |-
    Task P2: Story-4 signaling contract, additive, plus TS codegen

    Specialist: protocol. Tier: full (wire contract, credential-bearing fields).
    Pairing: pair with client (the SDK consumes the generated TS). proto/ is a GSA owned by protocol; security reviews the credential fields.
    Depends on: none. It may run in parallel with P1.
    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md (Accepted), sections 4, 5 and 7.
    Requirements satisfied (contract parts; behaviour lands in MC-A, MC-D, C1, MH-CLOSE, MH-1 and P3): R-13, R-14, R-17, R-19.

    GOAL
    Add every new signaling.proto type and field that story 4's MC, MH and client tasks consume, with each rule written ONCE at its field. The change is ADDITIVE: the superseded fields (JoinResponse.media_servers, SendTarget.media_handler_url, StreamAssignment.media_handler_url, MhConnectionStatus.mh_url, MhConnectRequest.join_token) STAY. They are deleted in P3 once every consumer has moved. The new ref/token fields sit ALONGSIDE them. This is commit sequencing, not a runtime mode: no flag, no switch, no "legacy" naming.

    CHANGES (proto/dark_tower/signaling/v1/signaling.proto; fresh tags only, never a reserved one; docs/protocol/CONVENTIONS.md applies)

    1. message MediaHandlerSet { uint64 handler_set_version; repeated MediaServerInfo handlers; }. State once (ADR §4, §5):
       - It is a full snapshot with replace semantics, sent on JoinResponse and as a ServerMessage on every membership change and every token refresh.
       - handler_set_version is a MEMBERSHIP EPOCH. It advances only when membership changes. A token-only refresh keeps it and the refs, and the client then only replaces tokens: no dial, no close. It is used to correlate loss reports, never to discard anything, and never for ordering (all ServerMessages ride one ordered stream).
       - A handler is listed only after MC has confirmed its RegisterMeeting, so the JoinResponse set may be empty, with a set message following.
       - No two entries in one set share a media_handler_url. Addresses reach clients only from this set, and the client never dials outside the latest set.
       - Move JoinResponse.media_servers' NON-AUTHORITATIVE-for-send-placement text here, as the set's statement, and leave a pointer at media_servers until P3 deletes it.
    2. MediaServerInfo gains:
       - `uint32 handler_ref`: MC-assigned, one per handler INCARNATION, never reused within a meeting even when a later handler reuses the address. Zero is invalid. The client diffs on ref: a new ref at an old url means close and redial.
       - `string media_dial_token`: a CREDENTIAL. A compact JWS, alg EdDSA, claims {token_type=media_dial, sub, meeting_id, aud=handler_id, iat, exp}, signed by MC's per-meeting key. It is present on EVERY entry of EVERY set. TTL is MC_MEDIA_DIAL_TOKEN_TTL_SECONDS (default 300, MC refuses to start outside [60, 900]), refreshed at TTL/3. It is reusable within its lifetime. It is never logged, never in a metric, trace, error or serialized state, and never in the UI. The client retains only the latest token per in-set ref and clears it on removal and at session end (scoped R-23 amendment, ADR §5).
       - Point media_handler_url's comment at the per-incarnation address rule stated above RegisterMHRequest in internal.proto (GC-2, GC-2).
    3. SendTarget and StreamAssignment each gain `uint32 handler_ref`, alongside media_handler_url until P3. Every ref MUST be a member of the client's current set. The client counts and ignores a non-member ref.
    4. JoinResponse gains `MediaHandlerSet media_handler_set`. The ServerMessage oneof gains `MediaHandlerSet media_handler_set` (variant tags stay below 20).
    5. MhConnectRequest gains `string media_dial_token` (credential, as above), alongside join_token. C1 populates it, MH-1 makes MH accept it ONLY, and P3 deletes join_token.
    6. MhConnectionStatus gains `uint32 handler_ref` and `uint64 handler_set_version`, alongside mh_url until P3. MediaConnectionUpdate stays DIAGNOSTIC ONLY under its existing prohibition. Add two permitted uses, both still never routing, reachability or placement input:
       - the input to the "clients detected the loss before MC" metric (ADR §8);
       - "a FAILED report for an in-set ref MAY trigger a credential re-push of the set to its sender only".
       Reports are sent on every transition.
    7. enum MhCloseCode (emitted by MH-CLOSE) { UNSPECIFIED; WRONG_HANDLER; BINDING_CONFLICT; MC_UNAVAILABLE; HANDLER_RELEASED; DECLINED; }, beside MhServerMessage. It is carried as the WebTransport application close code, numeric only. Comment (security's three conditions plus ADR §7):
       - Every credential failure (expired beyond skew, signature, type, cross-meeting or unregistered meeting, malformed, oversized, an AC meeting token) is the single generic DECLINED. Specific codes are sent only after full validation.
       - Every code concerns only the caller's own token-validated meeting.
       - No reason strings. Numbers are defined once here.
       - UNSPECIFIED (0) is never sent, and a received 0 or unknown code is a counted client contract fault.
       - Every intentional MH close (decline, release, drain-deadline shed) carries exactly one code. A close with no code means transport loss.
       - DECLINED means "token dead", not "handler dead": the client waits for the next set carrying a token for that handler, then makes exactly one dial.
       - WRONG_HANDLER, BINDING_CONFLICT and MC_UNAVAILABLE are retried, not loss. HANDLER_RELEASED is expected after a set change.
    8. Redaction: add MediaServerInfo and MediaHandlerSet to the redacting skip_debug set in crates/proto-gen/build.rs (+ src/lib.rs impls), on the same mechanism as MhConnectRequest/JoinResponse. Update build.rs's MhConnectRequest note, which today says it carries only join_token.
    9. Regenerate TS (packages/proto-gen/scripts/codegen.sh, codegen-paths.sh) into packages/sdk-core/src/proto.

    MINIMUM CONSUMER EDITS THAT MUST LAND IN THIS COMMIT (compile only, no behaviour)
    - Rust struct literals of the changed messages in crates/mc-service (e.g. webtransport/connection.rs JoinResponse, media_signaling/directive.rs SendTarget, media_signaling/assignments.rs StreamAssignment) and in tests: new fields set explicitly to zero/empty. MC populates them in MC-A and MC-D.
    - If any SDK or MC dispatcher matches the ServerMessage oneof exhaustively, give the new variant an arm with the existing unknown/ignored handling. C1 implements it.
    - Nothing else.

    TESTS
    - crates/proto-gen/tests/signaling_roundtrip.rs: a roundtrip for every new message and field, plus redaction tests (a Debug of MediaServerInfo, MediaHandlerSet and MhConnectRequest never prints a token).
    - packages/proto-gen/scripts/verify-codegen.sh: PRESENCE checks for every new message, field and enum member. The absence checks belong to P3.
    - Implement the signaling parts of the Protocol section of docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md, paired with test.
    - Layer 1–7 green (buf breaking passes, because nothing is deleted).

    SAME-COMMIT NOTE: the credential-leak semantic-check surfaces (scripts/guards/semantic/checks.md) and `dt-guard ts-no-retained-credentials` coverage for these fields are security policy content, owned by SEC-A. If SEC-A has landed, make sure its named fields match the names here; if it hasn't, tell security the final names.

    CROSS-TASK OBLIGATIONS
    (a) No new config key is read here. MC_MEDIA_DIAL_TOKEN_TTL_SECONDS is a contract reference; MC-D owns its runbook row.
    (b) No retirement here.
    (c) No series emitted.
    (d) Test-gates Protocol section, as above.
    (e) handler_ref, handler_set_version and tokens are never metric labels, span attributes or per-frame log fields. State that at each field.
  slug: 2026-11-07-story4-signaling-contract
  tag: story-2026-10-31-survive-handler-loss-task-P2
- id: 7
  status: completed
  specialist: global-controller
  prompt: |-
    Story 4 task GC-1: GC registry freshness and configuration

    Specialist: global-controller. Reviewer: database (reviews every registry query). Pair with test for the env-test and the test-gates section. Tier: light (GC only; no proto or wire change, since RegisterMHResponse.load_report_interval_ms already exists; no auth, crypto or GSA).
    Dependencies: none. GC-2 and GC-3 depend on this one: both reuse the single staleness bound and the rebound candidate query established here.
    Requirements satisfied: R-24 (registry freshness), R-26 (GC configuration), and the GC series of R-34.

    Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md §10 (Operations: interval becomes GC config, staleness derived, sweep stops skipping DRAINING, retention, initial handler count) and §7 (drain: a DRAINING MH is never selected). Telemetry: docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md, GC section. Test gates: docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md, GC section (the parts listed below; the AcquireMediaHandler and registration-rule gates belong to GC-3 and 9).

    Goal: one staleness bound, derived from one configured load-report interval, governs every freshness decision GC makes about media handlers. A handler that stops reporting, draining or not, drops out of selection within that bound. Old per-incarnation registry rows are deleted after a configured retention. GC-owned timing and sizing are configuration and visible in-cluster.

    Configuration (crates/gc-service/src/config.rs, Config::from_vars; every key a REQUIRED reference set in infra/services/gc-service/config.env; a missing or out-of-range value fails startup with a ConfigError naming the key):
    - GC_MH_LOAD_REPORT_INTERVAL_MS, default 10000, range [1000, 60000]. It replaces the hardcoded DEFAULT_LOAD_REPORT_INTERVAL_MS in crates/gc-service/src/grpc/mh_service.rs (pushed to MH in RegisterMHResponse.load_report_interval_ms). GC is the single source; MH's own constant in crates/mh-service/src/grpc/gc_client.rs is only its pre-registration fallback and is not touched here.
    - GC_MH_STALENESS_MULTIPLIER (k), default 3, range [2, 10]. The MH staleness bound = k × GC_MH_LOAD_REPORT_INTERVAL_MS, computed once at startup. There is no other MH staleness value anywhere in GC.
    - GC_MH_REGISTRY_RETENTION_SECONDS, default 3600. Startup refuses retention ≤ the staleness bound (compare in milliseconds).
    - GC_MH_INITIAL_HANDLERS_PER_MEETING, default 2, range [1, 4]. It replaces the hardcoded "up to 2" in crates/gc-service/src/services/mh_selection.rs (select_mhs_for_meeting). Keep the selection algorithm otherwise unchanged; GC-3 refactors it into the shared exclude-aware function.

    Registry changes (crates/gc-service/src/repositories/media_handlers.rs, crates/gc-service/src/tasks/mh_health_checker.rs, crates/gc-service/src/main.rs):
    - Delete DEFAULT_HEARTBEAT_STALENESS_SECONDS (= 30) from media_handlers.rs. get_candidate_mhs takes the bound as a parameter, bound as an i64 millisecond value `$n * INTERVAL '1 millisecond'`, replacing the string concatenation `($2 || ' seconds')::INTERVAL`.
    - mark_stale_handlers_unhealthy uses the same bound and drops `AND health_status != 'draining'`: a draining MH that exited must go unhealthy, not stay 'draining' forever. A live draining MH keeps reporting DRAINING, so it is never swept.
    - main.rs stops passing the MC staleness threshold to the MH health checker (`let mh_staleness_threshold = mc_staleness_threshold`); it passes the derived MH bound. Leave meeting_assignments.rs's MC staleness constant and MC_STALENESS_THRESHOLD_SECONDS alone (MC rows, out of scope).
    - Retention: on the MH health-checker tick, `DELETE FROM media_handlers WHERE last_heartbeat_at < NOW() - $1 * INTERVAL '1 millisecond'`, with its own DB-query metric label. A load report for a deleted row stays an UPDATE returning zero rows → the existing NOT_FOUND "Handler not registered"; never recreate the row.
    - Add the new DB-query labels to the zero-init label list in crates/gc-service/src/observability/metrics.rs.

    Telemetry (each series: zero-initialised, catalog entry in docs/observability/metrics/gc-service.md, panel in infra/grafana/dashboards/gc-overview.json, ADR-0032 integration test; dt-guard application-metrics checks 3–6 must pass). No handler_id, meeting or connection label (ADR-0036 §11, ADR-0039 §8).
    - gc_mh_load_reports_total{outcome=accepted|unknown_handler}. unknown_handler is the NOT_FOUND path in send_load_report: a process GC has no row for is alive.
    - gc_mh_available{region, state=healthy|degraded|draining|pending|unhealthy}, a recompute-and-set gauge from the registry on the health-checker tick, counting rows within the staleness bound (unhealthy counted as such). This is the input to the fleet-level MHDown, whose rule is OBS-ALERTS's.
    - Config gauges gc_mh_load_report_interval_seconds and gc_mh_staleness_threshold_seconds.
    Alert rules are NOT in this task (OBS-ALERTS, OBS-ALERTS).

    Docs:
    - docs/DATABASE_SCHEMA.md §8 only: rewrite it to match migrations/20260124000001_mh_registry.sql exactly (columns, CHECK, both partial indexes). State that handler_id is per incarnation, and how staleness and retention are derived. Keep the max_streams DEFAULT note, but re-cite its last sentence to ADR-0039 R-25 ("no schema migration") instead of "the ADR-0036 arc forbids one".
    - docs/runbooks/gc-deployment.md Configuration Reference: one row per new key, with default and range (cross-task obligation a).
    - The docs/specialist-knowledge/global-controller/INDEX.md pointer to the MH registry stays accurate.

    Tests:
    - Update every test hardcoding 30 s or 60 s MH staleness (crates/gc-service/tests/mh_registry_tests.rs, crates/gc-service/src/tasks/mh_health_checker.rs tests, crates/gc-service/tests/db_metrics_integration.rs).
    - Sweep: a stale draining row goes unhealthy, and a fresh draining row stays draining (positive arm).
    - Selection: a draining row and a stale row are never candidates; a fresh healthy row is.
    - Retention: a row older than retention is deleted, and a younger one is kept; a load report after deletion → NOT_FOUND plus unknown_handler +1, and no row is recreated.
    - Config: each key missing or out of range fails startup; retention ≤ bound fails; the boundary values are accepted.
    - Interval push: RegisterMHResponse carries the configured interval.
    - Env-test (crates/env-tests, paired with test): in the live cluster the two config gauges equal the deployed config.env values, and gc_mh_available{state="healthy"} is at least the number of live MHs. Derive expectations from deployed config, never retype them.

    Same commit: config keys + config.env values + runbook rows; every series with its panel, catalog entry and test; the deleted constants and their test updates.
    Cross-task obligations: (a) config reference rows; (b) no retirements here beyond the deleted constants; (c) panel + catalog + zero-init + ADR-0032 test per series; (d) implement the GC freshness gates of test-gates.md, paired with test; (e) R4 label hygiene: no id labels.
  slug: 2026-11-07-gc-mh-registry-freshness
  tag: story-2026-10-31-survive-handler-loss-task-GC-1
  tier: light
  tier_reason: 'GC-only registry/config change: no proto or wire change (load_report_interval_ms already exists), no auth/crypto, no GSA, one service'
- id: 8
  status: completed
  specialist: infrastructure
  deps:
  - 1
  prompt: |-
    Story 4 task I-5 (infrastructure): infra/kind/scripts/mh.sh, the MH pool tool, plus its hermetic self-test.

    GOAL
    ADR-0039 §7 "On demand" and "Callable by automated tests" (docs/decisions/adr-0039-mh-replacement-and-migration.md). Implement the merged machine contract docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt EXACTLY: one script by which humans, env-tests and deploy.sh provision, drain, upgrade and list MH incarnations on hostPort slots. This task ships the script and its hermetic self-test ONLY. Nothing calls mh.sh yet and no cluster behaviour changes. I-2 introduces the slot pool, the template and the deploy.sh call in a later commit.

    Requirement satisfied: R-29, tool half (docs/user-stories/2026-10-31-survive-handler-loss.md). Its live-cluster half is proven when I-2 wires the script in.

    DEPENDENCY
    I-1 (gate zero). If I-1 PASSED, slots are hostPort slots as below. If I-1 recorded the fallback (StatefulSet-per-slot behind per-slot NodePort Services), the per-slot workload kind and the "slot occupied" test change accordingly. Read the recorded outcome in the story file before starting, and escalate if it is ambiguous.

    CLUSTER-SIDE INTERFACE mh.sh consumes (I-2 must produce exactly this; state it in the script header as the interface)
    - ConfigMap `mh-template` in namespace dark-tower. It holds the fully rendered MH Deployment (hash-named ConfigMap refs and content-tagged image already resolved by deploy.sh's root render), the template revision (hash), and the baseline B.
    - Per-slot ConfigMaps labelled dark-tower.io/mh-slot=<k>. Each carries key MH_WEBTRANSPORT_ADVERTISE_ADDRESS and the slot's host port. They are found by label, never by a hardcoded name list.
    - mh.sh instantiates the template as Deployment mh-s<k> (replicas 1) and sets four things on it:
      - labels dark-tower.io/mh-slot=<k>, dark-tower.io/mh-revision=<hash>, dark-tower.io/mh-owner=<owner>;
      - the webtransport container port's hostPort = the slot's node port;
      - the MH_WEBTRANSPORT_ADVERTISE_ADDRESS configMapKeyRef name = slot k's ConfigMap;
      - the replaces=<slot> annotation during upgrade.
      It never computes an address. Addresses come only from the slot ConfigMaps.

    THE CONTRACT (mh-sh-contract.txt governs; summary)
    - Invocation: host or devloop container with the caller's kubeconfig. The cluster is selected via infra/kind/scripts/lib/common.sh dt_init_cluster_env (DT_CLUSTER_NAME -> kind-<name>), never the current context. Dependencies: kubectl and jq only. Never touches GC or its database.
    - Streams: stdout carries one JSON object per invocation, with --json only. stderr carries human text plus exactly ONE final line, `MH_RESULT STATUS=OK|FAIL REASON=<token> [SLOT=k] [DETAIL=…]`. With --json, failures also print {"ok":false,"reason":…} on stdout.
    - Exit codes and tokens. The tokens live in ONE array in mh.sh.
      - 0 OK
      - 1 FAIL: mh-rollout-failed, drain-timeout, not-available-timeout
      - 2 PRECONDITION: usage, no-kubeconfig, no-template
      - 3 CAPACITY: mh-no-free-slot
      - 4 REFUSED: last-serving-mh, stuck-terminating, foreign-owner
      - 5 BUSY: mh-pool-busy (DETAIL=holder=<owner>)
    - `list --json`, schema_version 1, additive-only, every slot including free:
      - top level: template_revision, baseline;
      - per slot: slot, host_port, address, pod_uid, state free|starting|serving|draining|terminating, terminating_past_grace, deployment, pod, revision, stale (revision != template_revision), owner, restart_count;
      - no handler_id;
      - list takes no lock and never fails because the pool is empty or full.
    - `up [--slot k] [--owner ID] [--timeout S] --json`:
      - Slot choice is next-fit: the next free slot after the highest occupied, wrapping. Not lowest-free.
      - The create is atomic: AlreadyExists -> next slot. The hostPort is the platform backstop.
      - Returns only when the Deployment is Available (= MH /ready = RegisterMH accepted = GC-selectable). The default timeout is derived from the template's probes, not retyped.
      - Idempotent: with --slot k and a live current-revision MH of the same owner already there, it exits 0 with "created": false.
      - On not-available-timeout it leaves the pod in place for diagnosis and prints the drain command in DETAIL.
    - `drain <k> [--owner ID] [--no-wait] [--timeout S] [--force]`:
      - foreground delete; waits until the pod and object are gone, bounded by the pod's own terminationGracePeriodSeconds plus a margin (else drain-timeout);
      - --no-wait returns once SIGTERM has been delivered;
      - refuses: last-serving-mh (unless --force), foreign-owner (unless --force), stuck-terminating (any MH pod Terminating past its grace: "investigate, don't force");
      - an empty slot exits 0 with "drained": false; a draining slot joins the wait;
      - `drain --owner ID --all` drains everything that owner holds (teardown);
      - mh.sh NEVER issues `delete pod --force --grace-period=0` (the one way to defeat the slot lock).
    - `upgrade`: for each live MH whose revision != template_revision, serially: up (annotated replaces=<old slot>) -> Available -> drain old -> gone.
      - Resumable: on rerun, a Ready replacement with replaces=k only triggers drain k.
      - With no free slot it refuses (mh-no-free-slot) BEFORE touching any old MH, which keeps serving.
    - Owners: `baseline` (deploy.sh's MHs), `manual` (default), or the Layer-7 run token.
    - Lock: a coordination.k8s.io Lease `mh-pool` in dark-tower serialises ONLY the decide-plus-create/delete step of up, drain and each upgrade step.
      - Atomic create with holder and expiry; reclaimed only after expiry.
      - The Ready/gone waits run outside it.
      - Busy -> exit 5 at once.
      - It closes the concurrent-drain race that could remove the last serving MH.

    SELF-TEST
    infra/kind/scripts/mh.test.sh: hermetic, PATH-stub kubectl in the scripts/layer7.test.sh style, using the shared helpers in scripts/lang/_test_helpers.sh. Wire it into scripts/layer3.sh, the hermetic script self-test layer. It covers:
    - a golden of the `list --json` key set;
    - every token with its exit code, with the token array complete in BOTH directions (every listed token emitted by some case, every emitted token listed);
    - next-fit including wrap, and the AlreadyExists race moving to the next slot;
    - idempotent up --slot/--owner, drain of an empty slot, drain joining a draining slot;
    - foreign-owner, last-serving-mh, stuck-terminating;
    - Lease busy and expired-Lease reclaim;
    - the --no-wait return point;
    - upgrade resume via replaces=, and upgrade refusing with no free slot before touching an old MH;
    - that the script never emits a force/zero-grace delete.
    Every negative case has its positive arm.

    PAIRING AND REVIEW
    Paired with test, who consumes this contract in T-1's mh_pool fixture. Operations reviews the REASON tokens that its runbook branches on.

    TIER: full (the provisioning contract for a stateful service; consumed by deploy, tests and runbooks).

    CROSS-TASK OBLIGATIONS
    (a) No service config keys. B and K rows land with I-2. (b) No retirements. (c) No metric series; mh.sh is Prometheus-free. (d) Implements test-gates.md's mh.sh contract and self-test lines. test-gates.md's "lowest-free-slot" wording is superseded by ADR-0039 §7 next-fit. (e) Slot, revision and owner labels are pod labels only, never metric labels.

    INDEX: add a pointer for mh.sh and mh.test.sh in docs/specialist-knowledge/infrastructure/INDEX.md and run scripts/guards/simple/validate-knowledge-index.sh.

    COVERS: code, unit-tests (hermetic self-test). No env-tests: the script is not deployed or called until I-2.
  slug: 2026-11-07-mh-sh-pool-tool
  tag: story-2026-10-31-survive-handler-loss-task-I-5
- id: 9
  status: completed
  specialist: infrastructure
  deps:
  - 1
  - 4
  - 8
  prompt: |-
    Story 4 task I-2 (infrastructure): MH hostPort slot pool and single MH template, wired to infra/kind/scripts/mh.sh (built by I-5).

    GOAL
    ADR-0039 §7 and §10 (docs/decisions/adr-0039-mh-replacement-and-migration.md) plus the merged contract docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt.
    - Every MH incarnation in Kind owns a unique client-facing address for its whole life. No two live MH processes ever share one, during upgrade, delete-replace, eviction or crash restart.
    - MH is provisionable and drainable on demand by humans and automated tests through one script.
    - Upgrades are start-new-then-drain-old.
    The broken shortcut being removed: each MH is a replicas:1 RollingUpdate Deployment (infra/services/mh-service/mh-0-deployment.yaml, mh-1-deployment.yaml) behind a per-instance NodePort Service (service.yaml mh-service-0/1), so a surge or replacement pod shares the old pod's URL.

    Requirements satisfied: R-28 (address invariant env-test; the force-delete ban guard is I-3), R-29 (live-cluster half; the tool and its self-test are I-5), R-30, R-32 (docs/user-stories/2026-10-31-survive-handler-loss.md).

    PRECONDITIONS (dependencies)
    - I-5 has landed: infra/kind/scripts/mh.sh and its hermetic self-test exist, not yet called by anything.
    - I-1 (gate zero) PASSED on hostPort. If I-1 recorded the fallback, STOP and follow the recorded fallback design instead (StatefulSet-per-slot behind per-slot NodePort Services with publishNotReadyAddresses); escalate if it is not recorded.
    - T-0 has landed: env-tests already discover MHs by label (app=mh-service), and the MH gRPC port-forwards are per-test on local port 0. layer7.sh Phase 1i and ENV_TEST_MH_{0,1}_* are gone. If any consumer of the helper's MH health/gRPC ports remains, migrate it here.

    1. SLOT POOL (ADR §7 "Kind")
    - K = 6 slots with ONE home: crates/devloop-helper/src/ports.rs.
      - Replace MH_0_*/MH_1_* offsets 20..25 with six MH WebTransport slot offsets in that range. The four MH health/gRPC host mappings are dead ("NOTHING LISTENS") and go.
      - infra/kind/scripts/setup.sh's MH health port-forward (MH_HEALTH_PORT, to svc/mh-service) still needs a helper-allocated local port: give it its own offset outside the slot range, never a slot offset. Keep the "offset 104 retired, never reuse" rule.
      - Update commands.rs port-map writers (write_port_map_shell, cmd_status) and the template env generator, and their tests.
    - infra/kind/kind-config.yaml.tmpl and the static infra/kind/kind-config.yaml: one UDP extraPortMapping per slot, host port -> node containerPort = the pod hostPort.
      - Keep listenAddress ${HOST_GATEWAY_IP} and today's WSL2 address-family behaviour unchanged (docs/runbooks/client-dev-local.md §5).
      - The MH block is derived from K, not hand-listed six times, if the helper renders it. Otherwise a drift test in the helper proves template == table. The cross-artifact drift guard is I-3.
    - Changing K means recreating the cluster. State it in the ports.rs comment and the runbook config reference.

    2. SINGLE TEMPLATE (ADR §7 "Template invariants")
    - Replace mh-0-deployment.yaml and mh-1-deployment.yaml with ONE Deployment template.
      - Keep one configMapKeyRef per key (dt-guard env-config rules apply unchanged), every existing hard/optional ref decision, the downward-API MH_POD_MEMORY_LIMIT_BYTES, POD_IP-derived MH_GRPC_ADVERTISE_ADDRESS, labels app=mh-service + component=media, ports 8083/TCP, 50053/TCP, 4434/UDP, the mh-service-tls Secret mount, securityContext and probes.
      - restartPolicy stays Always: a crash is an in-place container restart, same slot, same pod IP, new handler_id.
    - The webtransport containerPort declares hostPort = the slot's node port. mh.sh sets it per instance.
    - MH_WEBTRANSPORT_ADVERTISE_ADDRESS comes from a per-slot ConfigMap generator (mh-slot-<k>-config, key == env name), replacing mh-0-config.env/mh-1-config.env.
    - The kustomize `replacements:` block deriving MH_TERMINATION_GRACE_SECONDS from terminationGracePeriodSeconds shrinks to ONE entry. terminationGracePeriodSeconds stays the drain SSoT.
    - Delete service.yaml's per-instance NodePort Services. Keep the ClusterIP mh-service (health/gRPC). Keep the PDB (it gates evictions). Update network-policy.yaml comments that mention NodePort 30434.
    - shareProcessNamespace: true is set ONLY by a Kind overlay patch in infra/kubernetes/overlays/kind/services/mh-service/, listed as test instrumentation in the ADR-0038 parity allowlist. It is never in the base.
    - Pod labels: dark-tower.io/mh-slot=<k>, dark-tower.io/mh-revision=<template hash>, dark-tower.io/mh-owner=<owner>. These are never metric labels (telemetry-contract.md §MH).

    3. DEPLOY (ADR §7 "On demand", §10)
    - infra/kind/scripts/deploy.sh:
      - renders the template in the ONE environment-root render, excludes the MH workload from the root apply and from env_root_workloads()/wait_for_env_root(), and stores the rendered template in-cluster as ConfigMap mh-template (data, not pod-consumed);
      - then runs `mh.sh upgrade`, then tops up to baseline B.
    - B: default 2, one home in the Kind overlay (an mh-fleet env source rendered into mh-template). It is distinct from GC's per-meeting initial handler count.
    - One slot-table source function in deploy.sh (helper port table -> slot -> host port -> advertise address, plus the kind-config podSubnet). It feeds the per-slot ConfigMaps through render_env_overlay. advertise_instances() keeps MC's per-instance glob; MH moves to the slot table. GC-2 consumes this same source for GC's allowlists in the same pass. Expose it so GC-2 can call it.
    - Failure tokens: DEPLOY_FAILED REASON=mh-no-free-slot | mh-rollout-failed. The old MH keeps serving on failure.
    - Image prune (prune_superseded_images): the MH keep set = images of every LIVE MH pod ∪ the previous revision. Per-slot Deployments have no rollout history, so this does not come from previous_generation_refs. Add a comment that ConfigMap generations referenced by a live MH pod are never pruned (none are pruned today).

    4. USE mh.sh (built by I-5; infra/kind/scripts/mh.sh, contract docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt)
    - Produce EXACTLY the cluster-side interface the I-5 script header documents: ConfigMap mh-template (the rendered Deployment, template revision, baseline B) and per-slot ConfigMaps labelled dark-tower.io/mh-slot=<k> carrying MH_WEBTRANSPORT_ADVERTISE_ADDRESS and the slot's host port. If the interface needs to change, change mh.sh and mh.test.sh in this commit too; never fork the contract.
    - deploy.sh's baseline MHs use owner `baseline`. deploy.sh calls `mh.sh upgrade`, then tops up with `mh.sh up --owner baseline` until B serve, mapping mh.sh failures to DEPLOY_FAILED REASON=mh-no-free-slot | mh-rollout-failed.
    - scripts/layer7.sh exports DT_MH_SH (absolute path to mh.sh) for env-tests (consumed by T-1).

    5. SAME-COMMIT CONSUMERS of fixed instances (cut by invariant: "no fixed MH instances anywhere")
    - env-tests: test 01 (crates/env-tests/tests/01_mh_deployment_config.rs), test 02 where it reads MH, and crates/env-tests/src/fixtures/deployment_config.rs move from per-instance mh-0/mh-1 ConfigMaps/Deployments to the template and slot ConfigMaps. KIND_SIZING_RESTORE_CMD is updated if it names mh-N.
    - NEW R-28 env-test, paired with test: every live MH pod's advertised address is unique and equals its slot ConfigMap value. Resolve pods via `mh.sh list --json`. T-1 later hoists it into the mh_pool fixture.
    - NEW R-27 publish-leg env-test, paired with test (obligation inherited from I-1; see the Gate zero RESULT and Scope bullets in the Deployment section): send a UDP datagram from the env-test runner to the host gateway on a slot's UDP extraPortMapping and prove it is delivered end to end to that slot's MH. Gate zero sent node-direct because the old 30830/30530 mappings were TCP-only, so the Kind/podman host-publish UDP leg is unproven until this test passes. A failure here is a slot-design failure: escalate, do not work around it.
    - scripts/dev-web.sh (R-32, client reviews): the MH preflight stops globbing mh-[0-9]*-config.env and reads `mh.sh list --json`.
      - Hard-fail when fewer than `baseline` MHs are live and listening.
      - Run today's per-port listener and address-family check on each occupied slot; empty slots are normal.
      - Update the loopback comment block near lines 90-96 and scripts/dev-web.test.sh.
      - The MC half keeps its glob.
    - MH crate (media-handler reviews): crates/mh-service/src/config.rs (the_shipped_base_and_kind_configurations_pass_the_memory_check, ~line 4806, loops over mh-0/mh-1) and crates/mh-service/tests/common/otel_capture.rs include_str! the template instead.
    - dt-guard fixtures and tests naming mh-0/mh-1: crates/dt-guard/src/kustomize.rs (~960-1016), env_config.rs fixtures where they model real files, and kustomize_resource_field_refs.rs only if it reads real files.
    - scripts/setup.test.sh: the MH_0/1_WEBTRANSPORT_PORT cases move to slot-port cases.
    - Runbooks (operations writes procedures later in OPS): fix ONLY commands that become wrong (deployment/mh-N, MH `rollout undo`, mh-N port-forward recipes) in docs/runbooks/mh-deployment.md, mh-incident-response.md and devloop-validation.md, one line each, with the mh.sh / label-selector equivalent. Add K and B rows to mh-deployment.md's Configuration Reference.
    - docs/specialist-knowledge/infrastructure/INDEX.md: update the pointers for moved or renamed files.

    PAIRING: paired with test (env-test changes, R-28). Client reviews dev-web.sh; media-handler reviews the MH crate test moves; operations reviews the runbook command fixes.

    TIER: full (architectural seam, deploy path, more than one area).

    CROSS-TASK OBLIGATIONS
    (a) K and B get Configuration Reference rows.
    (b) Retired per-instance files, Services and helper offsets leave every reference in the same commit.
    (c) No new metric series.
    (d) Implements test-gates.md §infra lines: the slot pool, the invariant env-test, the fixture migration remainder. The mh.sh contract and self-test are I-5's.
    (e) Slot, revision and owner pod labels never become metric labels; dashboards select by app=mh-service.

    REMAINING HOST-SIDE ACTIONS (not devloop steps; state them in the devloop summary): rebuild devloop-helper on the host (ports.rs changed), then recreate every existing Kind cluster to get the 6 slot mappings. The first deploy, and any rollback across this commit, needs a recreate (R-30).

    COVERS: code, deploy, unit-tests, env-tests (R-28 address invariant; R-27 host-publish UDP leg; existing MH env-tests green on slots).
  slug: 2026-11-07-mh-hostport-slot-pool
  tag: story-2026-10-31-survive-handler-loss-task-I-2
- id: 10
  status: completed
  specialist: global-controller
  deps:
  - 7
  - 9
  prompt: |-
    Story 4 task GC-2: GC registration rule (endpoint allowlists, supersede/reject, atomic registration)

    Specialist: global-controller. Pairing: infrastructure, for the deploy.sh render of GC's allowlist values; protocol, for the RegisterMHRequest comment block (internal.proto is a GSA owned by protocol + auth-controller + security). Reviewers: database (transaction and queries), security (allowlists are the registration-side bound on MC-credential harvest). Pair with test for the test-gates section. Tier: full (security control, a GSA proto edit, and a deploy render shared with infrastructure).
    Dependencies: GC-1, which supplies the single MH staleness bound used by the rule's "stale" check (landing before it would reintroduce a second staleness value), and I-2, which supplies the slot-table render in deploy.sh that GC's Kind values derive from. I-3 depends on this task: it adds the drift guard between slot table, host mappings, slot ConfigMaps and these allowlist values.
    Requirements satisfied: R-25 (registration rule), the GC part of R-31 (required keys), the GC S6 line of R-38, and the GC registration series of R-34.

    Governing design: ADR-0039 §7 ("GC registration rule", stated once above RegisterMHRequest) and the full rule text docs/debates/2026-10-28-mh-replacement-and-migration/gc-register-rule.txt (carry it exactly). Recorded residuals: ADR-0039 Consequences (S6; controller_id anti-misroute). Telemetry: telemetry-contract.md GC section. Test gates: test-gates.md GC section, registration parts.

    Goal: no two LIVE MH incarnations ever share an endpoint in GC's registry; sequential reuse after an incarnation is gone is permitted; a live serving MH is never displaced; endpoints come only from per-zone allowlists.

    Configuration (crates/gc-service/src/config.rs; all REQUIRED, missing or malformed = startup failure naming the key):
    - GC_MH_WT_ALLOWED_HOSTS: exact hosts or ".suffix" entries. GC_MH_WT_ALLOWED_PORTS: port set. The client-facing scheme is https only.
    - GC_MH_GRPC_ALLOWED_SCHEMES: `grpc` today, with a code and config comment stating it means cleartext h2c, unauthenticated. It flips by config only to a TLS scheme when S6 mTLS lands; the spelling is decided then.
    - GC_MH_GRPC_ALLOWED_CIDRS (CIDR set) and GC_MH_GRPC_ALLOWED_PORTS (port set; Kind {50053}, matching MH_GRPC_ADVERTISE_ADDRESS grpc://$(POD_IP):50053).
    - Kind values, host-mode and devloop: rendered in infra/kind/scripts/deploy.sh render_env_overlay (the gc-service config patch) from I-2's single slot-table source function (helper port table → slot → host port → advertise address) and the kind-config networking.podSubnet. Never retyped. This deploy.sh edit is in this task, paired with infrastructure.
    - Rows in docs/runbooks/gc-deployment.md Configuration Reference (obligation a).

    Rule (crates/gc-service/src/grpc/mh_service.rs register_mh + crates/gc-service/src/repositories/media_handlers.rs):
    0. Allowlist first: both endpoints validated against their zone (scheme, host/CIDR, port). Failure = INVALID_ARGUMENT, outcome=invalid_endpoint. This extends validate_endpoint, which today only checks the scheme prefix.
    Then ONE transaction whose first statement is `SELECT pg_advisory_xact_lock(hashtext('gc_mh_registry'))` (single registry-wide key, the idiom of crates/ac-service/src/handlers/admin_handler.rs; registration is rare; per-endpoint keys would need ordered double-locking and still miss a new row overlapping two existing rows):
    1. Select by handler_id. If it exists with either endpoint changed → FAILED_PRECONDITION, outcome=endpoint_changed, rollback, the row unchanged. This replaces the silent `ON CONFLICT (handler_id) DO UPDATE SET webtransport_endpoint=…, grpc_endpoint=…` rewrite.
    2. Select other handler_ids' rows where webtransport_endpoint = $wt OR grpc_endpoint = $grpc, and apply the rule in Rust:
       a. both endpoints match → supersede (in-place container restart); match=both.
       b. exactly one matches and that row is draining, unhealthy, or stale (heartbeat older than GC-1's bound) → supersede; match=one_sided.
       c. exactly one matches and that row is fresh and pending/healthy/degraded → FAILED_PRECONDITION with the distinct message "address_conflict", outcome=address_conflict, rollback with no writes.
    3. DELETE the superseded rows, then the upsert. The ON CONFLICT path only re-registers the same id with identical endpoints, keeping today's reset to health_status='pending' and last_heartbeat_at=NOW() (MH's immediate post-register load report flips it).
    4. COMMIT. One DB-query metric over the whole transaction under label "register_mh_txn" (replacing "register_mh" in the label list in observability/metrics.rs).
    No schema migration, no unique index (it would break converge-from-previous-checkout rollback), and no endpoint index (the table is tens of rows).
    Superseded rows affect only new selection; MC never reacts to GC registry rows.

    Proto (paired with protocol): write the RegisterMH endpoint-rule comment block ONCE above RegisterMHRequest in proto/dark_tower/internal/v1/internal.proto: arms 0–3 as in gc-register-rule.txt plus the outcome vocabulary. webtransport_endpoint and grpc_endpoint each carry a one-line pointer to that block. Include the sentence "MC never reacts to GC registry rows". The comment lands in the same commit as the enforcement.

    Telemetry (zero-init all series, catalog entry in docs/observability/metrics/gc-service.md, panel in infra/grafana/dashboards/gc-overview.json, ADR-0032 integration test): gc_mh_registrations_total{outcome=ok|superseded|address_conflict|endpoint_changed|invalid_endpoint, match=both|one_sided}, where match is carried only on superseded. No handler_id label; log lines may carry ids. Alert rules (ping-pong page, address_conflict persistent, endpoint_changed/invalid_endpoint warnings) are OBS-ALERTS's.

    docs/TODO.md: add to the existing S6 entry (§Media Path Obligations, "The MC→MH control channel that now carries a ROUTING AUTHORITY is plaintext"): "GC's RegisterMH endpoint allowlist (per-zone, config-driven) is the registration-side bound on MC-credential harvest via a rogue grpc_endpoint; it is NOT peer authentication and does not close S6."

    Tests (database and security will look for these):
    - one test per arm: 0 per zone with a cross-zone distinguishing input (an address valid in one zone rejected in the other), 1 (the row is unchanged after the reject), 2a, 2b × {draining, unhealthy, stale}, 2c, plus re-registering the same id with identical endpoints (accepted, reset to pending).
    - concurrency: two tasks registering overlapping endpoints at once, joined; assert at most one row per endpoint, and that each outcome follows the rule.
    - each negative arm has its positive arm, and metric assertions use ADR-0032 semantics.
    - Env-test (crates/env-tests, paired with test): after deploy, every live MH registered with outcome=ok, and the cluster shows zero invalid_endpoint and zero address_conflict. This proves the rendered Kind allowlists match the real slot addresses. (The crash-in-place supersede assertion, superseded{match=both} +1 with unknown_handler unchanged, rides in T-1's or MC-B2's crash leg.)

    Same commit: config keys + render + runbook rows; enforcement + proto comment block; the series with panel, catalog entry and test; the S6 TODO line.
    Cross-task obligations: (a) config rows; (b) the old silent endpoint rewrite removed; (c) panel + catalog + zero-init + ADR-0032 test; (d) GC registration gates of test-gates.md, paired with test; (e) no id labels.
  slug: 2026-11-09-gc-mh-registration-rule
  tag: story-2026-10-31-survive-handler-loss-task-GC-2
- id: 11
  status: completed
  specialist: global-controller
  deps:
  - 5
  - 7
  prompt: |-
    Story 4 task GC-3: GC AcquireMediaHandler, with its proto slice and rpc

    Specialist: global-controller. Pairing: protocol, for the internal.proto slice (a GSA owned by protocol + auth-controller + security). Reviewers: security (ownership check, token pin), database (candidate query). Pair with test for the test-gates section. Tier: full (proto/wire in a GSA, the MC↔GC seam, an auth test pin).
    Dependencies: P1, for the ReleaseCause enum shared with MC and MH, and GC-1, which rebinds get_candidate_mhs and supplies the single staleness bound and GC_MH_INITIAL_HANDLERS_PER_MEETING (this task edits the same query). MC-C depends on this one; MC's "zero/absent cap fails registration" check lands there, after this task starts populating the cap.
    Requirements satisfied: R-23 (replacement grant), the GC part of R-15 (GC user auth refuses dial credentials), the GC replacement series and log event of R-34, and the GC gray-failure entry of R-38.

    Governing design: ADR-0039 §3 (Replacement: removal first, then GC; statuses; known gap), §5 (cross-meeting guard by construction), §9 (GC selection identical with and without reports). Telemetry: telemetry-contract.md GC section. Test gates: test-gates.md GC section (AcquireMediaHandler and token lines).

    Proto (proto/dark_tower/internal/v1/internal.proto, paired with protocol):
    - `rpc AcquireMediaHandler(AcquireMediaHandlerRequest) returns (AcquireMediaHandlerResponse);` on GlobalControllerService. It is already routed to service_type meeting-controller by crates/gc-service/src/grpc/auth_layer.rs, so no auth-layer change.
    - AcquireMediaHandlerRequest { meeting_id; controller_id; repeated exclude_handler_ids; ReleaseCause trigger; replaced_handler_id (optional, label and log context only, never a swap target) }. There is NO region field: GC derives it from the ownership row. The comment states: exclude = current ∪ removed ∪ refused-at-admission, by id; unknown ids ignored; length capped by RegisterMCResponse.max_exclude_handler_ids (pointer); MEETING_ENDED and UNSPECIFIED triggers are invalid here; statuses as below; GC's health view is never a veto.
    - AcquireMediaHandlerResponse { MhAssignment assignment } (reuse the existing MhAssignment).
    - RegisterMCResponse gains `uint32 max_exclude_handler_ids`, the same pattern as fast_heartbeat_interval_ms (a value GC owns, pushed to MC). Zero/absent fails MC registration (enforced in MC-C).

    Configuration (crates/gc-service/src/config.rs, REQUIRED, set in infra/services/gc-service/config.env, row in docs/runbooks/gc-deployment.md Configuration Reference): GC_MH_MAX_EXCLUDE_HANDLER_IDS, default 16, range [4, 64]. Startup refuses a value ≤ GC_MH_INITIAL_HANDLERS_PER_MEETING (from GC-1). Runbook row note: MC derives its per-meeting attempt bound as cap − current set size; removed handlers stay excluded for the meeting's life, so about 14 lifetime replacements at the default, then rate_limited; raising it needs MC re-registration to take effect.

    Selection (crates/gc-service/src/services/mh_selection.rs, mc_assignment.rs, repositories/media_handlers.rs). User ruling R-23: ONE function for initial allocation and replacement.
    - Refactor select_mhs_for_meeting into select(region, count, exclude). Initial assignment calls it with count = GC_MH_INITIAL_HANDLERS_PER_MEETING and an empty exclude; AcquireMediaHandler with count 1 and the request's exclude list. The weighted-random, load-ratio algorithm is unchanged.
    - get_candidate_mhs gains `AND NOT (handler_id = ANY($n::text[]))` before ORDER BY/LIMIT, bound as a Vec<String>; an empty array for initial selection.
    - Rewrite the DRY anchor in mc_assignment.rs (assign_meeting_with_mh: "this is the ONLY select_mhs_for_meeting callsite … 1:1 with new assignments"). There are now two callers, and gc_mh_selections_total stays 1:1 with NEW assignments, with replacements counted by their own series.

    RPC behaviour (crates/gc-service/src/grpc/mc_service.rs):
    - Validate meeting_id, controller_id and each id (existing validators). A length > cap, a malformed id, or a MEETING_ENDED/UNSPECIFIED trigger → INVALID_ARGUMENT (outcome invalid_request).
    - Ownership: the active meeting_assignments row for (meeting_id, meeting_controller_id = controller_id, ended_at IS NULL) supplies the region. A miss → FAILED_PRECONDITION (outcome not_owner). This is anti-misroute only; controller_id is self-asserted (a recorded gap).
    - select(region, 1, exclude): none → RESOURCE_EXHAUSTED (outcome no_capacity); otherwise OK with the MhAssignment (outcome granted). A DB error → INTERNAL (outcome error).
    - gc_view: a pure function classify(health_status, heartbeat_age, staleness_bound) over the replaced handler's row → healthy|degraded|unhealthy|draining|stale|unregistered (no row, or no replaced_handler_id given). It is a label only; selection never reads it, nor trigger, nor replaced_handler_id.
    - Populate RegisterMCResponse.max_exclude_handler_ids in register_mc from the config.

    Telemetry (zero-init, catalog entry in docs/observability/metrics/gc-service.md, panels in infra/grafana/dashboards/gc-overview.json, ADR-0032 integration tests; no handler_id, meeting or controller labels):
    - gc_mh_replacement_requests_total{trigger=heartbeat_timeout|apply_lag|planned_transfer (projected from ReleaseCause), outcome=granted|no_capacity|not_owner|invalid_request|error, gc_view}.
    - gc_mh_replacement_duration_seconds{outcome}.
    - One INFO event, target `gc.mh.replacement_report`, per request carrying replaced_handler_id: replaced handler_id, its webtransport_endpoint from GC's row (omitted if no row), controller_id, trigger, gc_view, outcome. Ids appear in logs only. This event is the source of truth for the gray-failure log query; GC keeps NO in-memory corroboration window (2+ load-balanced replicas would undercount).
    Alert rules (the no_capacity warning) are OBS-ALERTS's.

    R-15 GC pin (crates/gc-service/tests/auth_tests.rs, with the jwt_fixtures helpers): GC user auth rejects a JWT carrying media_dial claims {token_type=media_dial, sub, meeting_id, aud, iat, exp}: (1) with no kid → rejected at kid extraction (MissingKid); (2) carrying a live AC key's kid but signed by a different Ed25519 key → signature failure; (3) HS256-signed → rejected by the EdDSA pin. Positive arm: an AC-signed user token is accepted.

    docs/TODO.md: add the gray-failure entry (owner global-controller; filed by ADR-0039 §3 known gap, R4-5), using the agreed text. GC's view is MH→GC load reports, so a gray-failed MH stays selectable; dead MHs are covered (staleness, plus RegisterMeeting-failure exclusion). Evidence: gc_mh_replacement_requests_total and the gc.mh.replacement_report event. Discriminator: reported dead (heartbeat_timeout|apply_lag) by ≥ 2 distinct MCs while still load-reporting healthy past the staleness bound; gc_view at request time is not it. Manual mitigation: the runbook's gray-failure scenario. Fix when acted on: quarantine on M ≥ 2 independent MC reports within W against a handler GC still sees fresh and healthy, released on expiry; it needs state shared across GC replicas and authenticated per-instance MC identity first (controller_id is self-asserted today). Defer trigger: evidence of multi-MC agreement on a GC-healthy handler, or an incident with repeated migrations off one bad MH.

    Tests:
    - {A,B,C} excluding {A,B} → C; excluding all → RESOURCE_EXHAUSTED; a draining MH is never granted; unknown excluded ids are ignored; length cap (cap accepted, cap+1 rejected); not_owner (an ended assignment, and another controller_id).
    - exclude-before-LIMIT: more than 5 healthy rows with the 5 least-loaded excluded → still granted.
    - MC declares X failed while GC sees X healthy → granted anyway, gc_view=healthy.
    - seeded RNG: selection identical with and without replaced_handler_id/trigger (evidence only, ADR-0039 §9).
    - the initial-assignment path behaves as before with the empty exclude list (existing mc_assignment_rpc_tests.rs and meeting_assignment_tests.rs stay green).
    - register_mc returns the configured cap.
    - the INFO event fields, and the absence of any id label on the counters.
    Env coverage: AcquireMediaHandler is reachable only from MC, so its cluster-level assertions (gc_mh_replacement_requests_total rising with outcome=granted on a crash) ride in MC-C's crash leg, paired with test. No GC env-test needs MC credentials.

    Same commit: proto slice + rpc impl + cap population + config + runbook row; series with panels, catalog entries and tests; DRY anchor rewrite; TODO entry.
    Cross-task obligations: (a) config row; (b) the DRY anchor text updated; (c) panel + catalog + zero-init + ADR-0032 test; (d) GC AcquireMediaHandler and token gates of test-gates.md, paired with test; (e) no id labels.
  slug: 2026-11-10-gc-acquire-media-handler
  tag: story-2026-10-31-survive-handler-loss-task-GC-3
- id: 12
  status: completed
  specialist: meeting-controller
  deps:
  - 6
  prompt: |-
    MC-A: MC-owned, versioned, ref-addressed handler set; register-before-token

    Specialist: meeting-controller. Pair with test (test-gates review) and protocol (signalling consumer review).
    Depends on: P2 (signaling contract additive: MediaHandlerSet, handler_ref on MediaServerInfo/SendTarget/StreamAssignment, the JoinResponse and ServerMessage set fields).
    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md §3 (authority, register-before-token), §4 (client signalling, handler_ref, version semantics). Supersedes ADR-0036 §9's frozen handler set.
    Requirements satisfied: R-12, R-17 (MC side), R-36 (set-divergence counter), R-37 (MC cap hard-fail).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (its section for this task is reproduced below). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    Replace the meeting's frozen handler set with an MC-owned, versioned, mutable set whose members clients learn only by reference and only once each handler has confirmed the meeting's registration. No handler is added or removed mid-meeting yet (detection and replacement are MC-B2 and MC-C); this task builds the set, its states and the client-facing snapshot, so later tasks only call add/remove.

    CONCRETE CHANGES
    1. crates/mc-service/src/media_routing/placement.rs: MeetingHandlers stops being "frozen, non-empty". It carries:
       - a handler_set_version (u64, monotonic per meeting) that advances ONLY on a membership change (§4: membership epoch; never advanced by a token refresh);
       - per handler an MC-assigned handler_ref (u32, non-zero, allocated per incarnation, never reused within the meeting even if a later handler reuses the address);
       - per handler a state: Registering (in MC's set; heartbeats will get OWNED and the binding gate admits it — MC-B1/MC-B2) then Active (its push worker's first RegisterMeeting was acked);
       - a removed record (handler ids removed for the meeting's life; one-way, never re-added). This task only adds the structure plus add/remove/mark-active methods that bump the version; removal triggers come later.
       - May be empty. Keep the existing invariants: duplicate handler id rejected, S1a exact-match resolve, urls server-derived only. Update the module doc (it describes the frozen set and the story-4 hazard).
    2. Redis MhAssignmentData is GC's hand-off, read ONCE when the meeting actor is created (§3). crates/mc-service/src/webtransport/connection.rs:read_meeting_handlers is called on every join today; later joins must no longer read it, and a later change to the Redis record must not alter a live meeting's handlers. Delete actors/meeting_media.rs install()'s divergence branch and mc_media_handler_set_divergence_total, with its catalog entry (docs/observability/metrics/mc-service.md), panels and any alert/runbook references (obligation b).
    3. Register before token (§3, L4-2): a handler enters the client-facing set only when Active. The push worker (media_routing/pusher.rs) reports its first confirmed generation for a handler back to the meeting actor (a mailbox message; the actor never awaits MH inside a turn).
       - At a meeting's FIRST join the actor parks that join's reply and keeps serving its mailbox; it sends the JoinResponse once the initial handlers' RegisterMeeting is acked. Later joins reply at once.
       - If no initial handler ever acks (the pusher's retries exhausted), the JoinResponse carries an empty set; the unacked handler is left for the replacement path (MC-C). The join itself never fails because the initial MH is dead.
    4. Client signalling (§4): send MediaHandlerSet {handler_set_version, handlers[{media_handler_url, handler_ref}]} on JoinResponse and as a ServerMessage on every membership change, listing Active handlers only. Every url still comes from MeetingHandlers::url_of. Put handler_ref on every SendTarget and StreamAssignment (media_signaling/directive.rs, media_signaling/assignments.rs). KEEP populating the legacy JoinResponse.media_servers and the url fields for now: sequential double-population until C1 switches the SDK and P3 deletes the legacy fields. This is commit sequencing, not a runtime mode. media_dial_token is filled by MC-D; leave it empty here.
    5. A client-supplied address never reaches a target: extend tests/media_client_signaling_integration.rs a_client_supplied_handler_url_never_reaches_a_send_target to the new message and to refs.
    6. crates/mc-service/src/config.rs:565-573: MC_MAX_MEETINGS and MC_MAX_PARTICIPANTS parse with .ok().unwrap_or(), which masks typos; make both hard-fail on an unparseable value (R-37).

    TESTS (test-gates.md §MC, items that belong here; paused time, no sleeps; every negative gate has its positive arm)
    - No MediaHandlerSet (including the first JoinResponse) names a handler before its RegisterMeeting ack; positive arm: after the ack it is listed.
    - First join waits for the ack without stalling another participant's traffic in the same meeting (paused-time Tier 1b with a mock MH that delays the ack).
    - Initial handler never acks: join succeeds with an empty set.
    - handler_ref never reused within a meeting; version advances only on membership change.
    - A later Redis assignment change does not alter a live meeting's handlers.
    - Config: unparseable MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS fail startup; valid values pass.
    - Env-test (pair with test): a real join's set carries non-zero refs and names only registered handlers (extend crates/env-tests/tests/27_mc_slot_placement.rs or the MC session fixture). MH discovery by label comes from T-0.

    TELEMETRY
    - Retire mc_media_handler_set_divergence_total (catalog, panels, references).
    - No new series required here (set pushes are counted in MC-D).

    SAME-COMMIT OBLIGATIONS
    (a) Any new or changed config key gets its row in docs/runbooks/mc-deployment.md §Configuration Reference (the cap hard-fail changes their documented behaviour: say so).
    (b) Every retirement removes its catalog/panel/alert/runbook references in this commit.
    (c) Any emitted series lands with panel + catalog + zero-init + ADR-0032 test.
    (d) Implement the MC test-gates.md items above, paired with test.
    (e) R4 label hygiene: no meeting, participant or handler-id labels.
    Update docs/specialist-knowledge/meeting-controller/INDEX.md pointers for anything moved.

    Tier: full (wire consumer, architectural seam).
  slug: 2026-11-10-mc-versioned-handler-set
  tag: story-2026-10-31-survive-handler-loss-task-MC-A
- id: 13
  status: completed
  specialist: meeting-controller
  deps:
  - 5
  - 12
  prompt: |-
    MC-B1: Heartbeat intake, verdicts, remove-only reconcile, binding gate, connection_id required

    Specialist: meeting-controller. Pair with protocol (internal.proto is a Guarded Shared Area; this task adds the rpc line) and test.
    Depends on: P1 (internal contract additive: MeetingHeartbeatRequest/Response, Verdict, ReleaseCause, ServingState, ApplyRefusal, RegisterMeetingRequest.heartbeat_interval_ms), MC-A (versioned set with Registering/Active and removed record).
    Must land BEFORE MH-2: MH's heartbeat sender and its zero-interval rejection need this task's served rpc and non-zero heartbeat_interval_ms, otherwise MH's owner-silence release would end every meeting older than its timeout against an UNIMPLEMENTED MC.
    Governing design: ADR-0039 §1 (heartbeat), §2 (intake ownership), §6 (verdicts, owner check, reconcile, binding gate, connection_id required).
    Requirements satisfied: R-7 (MC side), R-9, R-10 (verdicts, binding gate), R-11 (owner check), R-36 (legacy connection_id mode).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (its section for this task is reproduced below). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    MC serves the per-(meeting, handler) heartbeat, answers it from the owning meeting actor's state, and uses it to correct (never add) connectivity. No declaration happens yet: detection and removal are MC-B2. Liveness is recorded so MC-B2 only adds assess().

    CONCRETE CHANGES
    1. proto/dark_tower/internal/v1/internal.proto: add `rpc MeetingHeartbeat(MeetingHeartbeatRequest) returns (MeetingHeartbeatResponse)` to MediaCoordinationService together with the server impl (tonic traits have no default methods, so the line and impl land together).
    2. crates/mc-service/src/grpc/media_coordination.rs: bound-check ids (validate_id_field; connection_ids count bounded by the per-meeting caps MC already enforces), stamp the receipt tokio Instant at gRPC arrival, route to the meeting actor through the controller (same path as Notify*) with a oneshot reply. The send into the actor waits less than one heartbeat interval; a drop is counted, never silent. No meeting actor → answer RELEASED(MEETING_ENDED) from the router (no ended-meeting tombstone).
    3. Verdicts from the meeting actor's own state (§6, L4-6, L4-7):
       - handler in the set, including Registering → OWNED;
       - handler in the removed record → RELEASED(its removal trigger) (the record is written by MC-B2; the branch and its test land here using the MC-A structure);
       - live meeting whose set never held this handler → RELEASED(UNSPECIFIED), counted as a misroute;
       - request mc_id is not this MC's id → FAILED_PRECONDITION, never RELEASED.
       There is no UNKNOWN verdict.
    4. The actor stores per (meeting, handler): last accepted receipt instant, previous receipt instant, last accepted applied_generation, apply_refusal, serving_state, drain_remaining_ms. A heartbeat is accepted as liveness only if applied_generation is not lower than the last accepted one (a stale/replayed heartbeat reads as lag, never as fresh liveness). It never adds a meeting, handler, address or connectivity.
    5. Remove-only connected-set reconcile (§6): MC never looks anything up by an incoming connection id. It walks its own live keys for that (meeting, handler) and retires a key absent from the heartbeat only if MC recorded its bind before the PREVIOUS heartbeat for that pair arrived (one instant per pair; valid because per-meeting heartbeats arrive in order). The same rule releases HandedOutBindings entries (media_admission/epoch.rs), closing the lost-Disconnected leak. A retirement recomputes routing through the normal sync path.
    6. Binding gate (§6; ADR ordering: before any bulk binding release): actors/meeting.rs:handle_media_connected records the binding BEFORE the membership check today. Flip it: an out-of-set or removed handler gets sender_id 0 and NOTHING is recorded (reuse mc_mh_notifications_unapplied_total{reason="handler_not_in_set"}). A Disconnected from a removed handler is a counted no-op that never touches another handler's connectivity.
    7. connection_id is REQUIRED (§6): an empty connection_id is refused (sender_id 0, counted). Delete the legacy branch in media_routing/connectivity.rs, the `unreleasable` set in media_admission/epoch.rs, mc_mh_notifications_without_connection_id_total, and flip internal.proto's "EMPTY MEANS LEGACY" text to REQUIRED, all in this commit.
    8. Config (§1, Lead ruling): MC_MH_HEARTBEAT_TIMEOUT_MS = N, default 6000, startup-validated to (0, 30000] ms (hard-fail outside). Send N/3 per meeting in RegisterMeetingRequest.heartbeat_interval_ms (crates/mc-service/src/grpc/mh_client.rs). Required configMapKeyRef in infra/services/mc-service/config.env and deployment.
    9. Delete the re-assert/restart-detection framing in MC docs: media_routing/mod.rs:28, pusher.rs:52 and the "story-4 periodic re-assert" paragraphs, teardown.rs:28-38 STORY-4 NOTE, generation.rs registry doc. process_start_epoch_ms itself is deleted by MH-2 together with the reads in grpc/mh_client.rs; this task must not add any dependency on it.

    TESTS (test-gates.md §MC; paused time; positive arm for every negative)
    - Verdicts: all five arms in one test (OWNED in set, OWNED while Registering, RELEASED removed with its trigger, RELEASED(MEETING_ENDED) no actor, RELEASED(UNSPECIFIED) never in set, FAILED_PRECONDITION on mc_id mismatch).
    - Race gate: a handler's first heartbeat arriving before MC processed its RegisterMeeting ack is OWNED.
    - Reconcile: a key recorded after the previous heartbeat's receipt and absent now is kept; one recorded before it and absent is retired, its binding released; heartbeats never add a key.
    - Stale heartbeat (lower applied_generation) not accepted as liveness.
    - Binding gate: Connected from an out-of-set handler → sender 0 AND the ledger is unchanged; positive arm in-set binds. Empty connection_id refused.
    - Config: N=0 and N>30000 rejected; interval N/3 sent in RegisterMeeting.
    - Env: no declaration exists yet, so the cluster check is in MC-B2.

    TELEMETRY (telemetry-contract.md §MC)
    - mc_media_handler_heartbeats_total{outcome=owned|released_removed|released_ended|released_unassigned|owner_mismatch}
    - mc_media_handler_connected_set_corrections_total
    - mc_media_handler_heartbeat_interval_seconds (buckets around N/3, N/2, N)
    - mc_media_handler_applied_sequence_lag — replaces the mc_media_generation_divergence gauge (crates/mc-service/src/observability/metrics.rs:461-495).
    - RETIRE in this commit (OBS-ALERTS only verifies): the mc_media_generation_divergence gauge, including its catalog entry, panels, its step in the docs/observability/alerts.md ladder and in docs/runbooks/mc-incident-response.md Scenario 15; and the handler_id_mismatch outcome of mc_media_policy_pushes_total together with all three of its "one decision" sites: the MCMediaGenerationDivergence expression in infra/docker/prometheus/rules/mc-alerts.yaml (this task edits that rule expression, the one alert-rule exception in story 4), the catalog entry, and the docs/runbooks/mc-deployment.md checklist. MCMediaGenerationDivergence keeps paging on its remaining arms with re-assert text removed.
    - mc_media_handler_heartbeat_deadline_seconds config gauge (N).
    - Retire mc_mh_notifications_without_connection_id_total and the empty-connection_id degraded-mode counter.
    - Heartbeats logged on state transitions only, never per heartbeat.

    SAME-COMMIT OBLIGATIONS: (a) MC_MH_HEARTBEAT_TIMEOUT_MS row in docs/runbooks/mc-deployment.md §Configuration Reference with default, range and its relationship to MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS ([300, 3600]); (b) retirements remove catalog/panel/alert/runbook refs here; (c) each series with panel (infra/grafana/dashboards/mc-media.json) + catalog + zero-init + ADR-0032 test; (d) test-gates items above; (e) no meeting/participant/handler/connection id labels.

    Tier: full (proto GSA, architectural seam).
  slug: 2026-11-10-mc-heartbeat-intake
  tag: story-2026-10-31-survive-handler-loss-task-MC-B1
- id: 14
  status: completed
  specialist: media-handler
  deps:
  - 6
  prompt: |-
    Task MH-CLOSE: MH numeric close codes on every intentional client-session close.

    GOAL
    Clients must be able to tell "retry", "expected", "wrong incarnation" and "terminal" apart from "the handler died". So MH closes every client session it ends ON PURPOSE with exactly one numeric MhCloseCode. A close with no code then means transport loss: a crash, or the QUIC idle timeout. Today every declined session closes by drop with no code or reason (crates/mh-service/src/webtransport/connection.rs close_declined_connection; "close by drop, no reason string"). Story: docs/user-stories/2026-10-31-survive-handler-loss.md.

    GOVERNING DESIGN
    docs/decisions/adr-0039-mh-replacement-and-migration.md §7 (Close codes: numeric only; sent only after credential validation; every credential failure is DECLINED; only about the caller's own meeting; a close with no code means transport loss; MH closes every declined, released or drain-deadline-shed session with a code), and security's three conditions recorded there. Carry these; invent nothing.

    DEPENDENCIES
    P2 (MhCloseCode in proto/dark_tower/signaling/v1/signaling.proto, beside MhServerMessage: UNSPECIFIED, WRONG_HANDLER, BINDING_CONFLICT, MC_UNAVAILABLE, HANDLER_RELEASED, DECLINED). C1 and MH-1 depend on THIS task. MH-2 and MH-3 use HANDLER_RELEASED: whichever of MH-2 / MH-3 and this task lands second wires its release or shed path to the code.

    PAIRING
    media-handler; client reviews (the SDK's handling contract in ADR §7 / R-19); security reviews (no oracle). Requirements: R-19 (MH half), R-34 (MH rows), R-40.

    CHANGES (crates/mh-service)
    1. One mapping, one home: a total function from MH's existing decline outcome (metrics::MediaSessionStartOutcome) and release reasons to MhCloseCode. An exhaustive match with no wildcard, so a new outcome cannot compile without a code. Never emit UNSPECIFIED / zero. The mapping:
     - DECLINED: every credential-validation failure (today the AC meeting-token path; MH-1 later replaces it with media_dial, and every (a)–(e) failure stays DECLINED), declined_no_sender_binding, declined_sender_binding_out_of_range, declined_mc_auth_rejected, declined_mc_endpoint_unknown, and the current register-meeting timeout close (that path is deleted by MH-1);
     - BINDING_CONFLICT: declined_sender_binding_conflict (the client retries after its old session ages out);
     - MC_UNAVAILABLE: declined_mc_unavailable. Keep the existing close jitter (MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS);
     - HANDLER_RELEASED: a meeting release, i.e. EndMeeting today (ConnectionEnd::MeetingReleased via the close token); MH-2 adds the RELEASED verdict and owner-silence; MH-3 adds the drain-deadline shed;
     - WRONG_HANDLER: reserved for MH-1's post-validation aud mismatch. Present in the mapping with no emitter yet.
    2. Emit the code as the WebTransport session close's application error code on every one of those paths, instead of closing by drop. No reason strings ever reach the client.
    3. Leave code-less: process crash, QUIC idle timeout, client-initiated close, and server shutdown cancellation before MH-3's drain lands.
    4. Security conditions:
     - a code other than DECLINED is sent only after the credential fully validated (true by construction: every pre-validation exit is DECLINED);
     - every code concerns only the caller's own token-validated meeting;
     - nothing distinguishes token failure kinds to the peer. The failure kinds stay metric-only on mh_jwt_validations_total.

    TELEMETRY (obligation (c))
    No new series is required. The decline outcomes are already counted on mh_media_session_starts_total. If you add a per-code close counter, it must carry the full zero-init, catalog entry, panel and ADR-0032 test, with the code as a bounded label and no ids. Otherwise do not add one.

    TESTS (obligation (d); Tier 1b rigs with the transport shim; positive and negative arms)
    - The mapping is exhaustive over MediaSessionStartOutcome and release reasons, and no arm yields zero.
    - Each decline path closes with its code (shim-observed): a credential failure gets DECLINED; a sender-binding conflict gets BINDING_CONFLICT; MC unreachable gets MC_UNAVAILABLE (jitter preserved); an EndMeeting release of an active session gets HANDLER_RELEASED.
    - A client-initiated close and a shutdown cancellation carry no code.

    ENV-TESTS (Covers: env-tests; paired with test)
    - Extend crates/env-tests/tests/29_mh_meeting_teardown.rs (or the teardown fixture) so a Rust WebTransport client observes HANDLER_RELEASED when its meeting is released by EndMeeting.
    - A dial with an invalid credential observes DECLINED.
    Use the slot-label MH discovery from T-0 if it has landed.

    SAME COMMIT
    The mapping together with every emission site, so no intentional close path is left code-less.

    CROSS-TASK OBLIGATIONS
    (a) no new config key; (b) nothing retired; (c) as above; (d) test-gates "MH" close-code coverage, paired with test; (e) no id labels. Tier: full (client-facing wire contract).
  slug: 2026-11-10-mh-close-codes
  tag: story-2026-10-31-survive-handler-loss-task-MH-CLOSE
- id: 15
  status: completed
  specialist: media-handler
  deps:
  - 13
  - 14
  prompt: |-
    Task MH-2: MH per-meeting heartbeat, verdict handling, release causes, all-or-none pinned.

    GOAL
    For every meeting it holds, MH tells that meeting's MC, once per MC-declared interval, that it is alive and what it has APPLIED. It obeys MC's verdict, releasing the meeting when MC says the handler is not in its set. This is the MH half of liveness and fencing in story 4 (docs/user-stories/2026-10-31-survive-handler-loss.md).

    GOVERNING DESIGN
    docs/decisions/adr-0039-mh-replacement-and-migration.md §1 (Liveness), §2 (what MC does with it), §6 (Cleanup after removal: verdicts, cause rules, owner check), §8 (telemetry); docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md (MH rows) and test-gates.md (section "MH"). Carry these details; invent nothing beyond them.

    DEPENDENCIES (must have landed)
    - MC-B1. It serves the MeetingHeartbeat rpc (the rpc line lands there), answers OWNED / RELEASED(cause) / FAILED_PRECONDITION, and populates a non-zero RegisterMeetingRequest.heartbeat_interval_ms.
    - P1 (via MC-B1). It adds MeetingHeartbeatRequest/Response, ServingState, ApplyRefusal, Verdict, ReleaseCause, RegisterMeetingRequest.heartbeat_interval_ms and EndMeetingRequest.cause.
    - MH-CLOSE. It owns the MhCloseCode mapping used by the release path (HANDLER_RELEASED).
    - ORDERING REASON: this task must not land before MC-B1. Without the served rpc every heartbeat fails, and the 0-interval rejection would refuse every registration until MC populates the interval. MH-3 adds the owner-silence release on top of this task.
    - MC-B2 (detection) depends on THIS task. MH-3 builds on it and adds the owner-silence release.

    PAIRING
    Paired with protocol (GSA: internal.proto edits below) and test (the test-gates.md MH section). Requirements: R-7, R-8, R-10 (MH half), R-11 (owner check only; owner-silence is MH-3), R-36 (process-start epoch), R-34 (MH rows), R-40.

    CHANGES (crates/mh-service unless noted)
    1. Heartbeat timer in the session actor (crates/mh-service/src/session/mod.rs, SessionManagerActor::run). Add ONE select arm driven by the earliest per-meeting due instant (tokio::time, never std Instant).
     - A meeting's schedule starts at the REGISTRATION UPSERT (handle_register_meeting), not at the first successful apply. A meeting whose first apply is refused must still report applied_generation 0 plus its apply_refusal.
     - The first deadline is jittered uniformly in [0, interval). Use a seedable RNG so tests are deterministic.
     - The interval comes from that meeting's RegisterMeetingRequest.heartbeat_interval_ms. The latest value applies on re-register.
     - The schedule is removed on release.
     - MH has NO cadence config of its own.
    2. A registration with heartbeat_interval_ms == 0 is refused as invalid. Enforce this only now that MC-B1 populates it.
    3. Build each beat INSIDE the actor (so a sent beat implies a live actor; a wedged lifecycle mailbox means no beats):
     - meeting_id; the meeting's registered mc_id; handler_id;
     - ServingState (SERVING; DRAINING is wired by MH-3);
     - drain_remaining_ms (only with DRAINING);
     - applied_generation, read from the routing snapshot via generation_for, NEVER from the received request;
     - apply_refusal: the latest apply attempt's deterministic refusal, cleared on the next success. INVALID / TRANSPORT_INCONSISTENT / STREAM_CEILING map from PolicyApplyOutcome; EDGE_CAP maps from ApplyFailure::EdgeCapExceeded inside apply_failed. Transient failures (mailbox full, timeout, actor unavailable) are NOT refusals; they show as lag;
     - the connection set in P1's final shape. It holds connections for which MH has begun NotifyParticipantConnected and not yet sent Disconnected; the actor's active_connections is that superset.
     Hand the beat to a separate sender task with a bounded try_send. When the queue is full the beat is dropped and counted; the actor never blocks.
    4. Sender task. Keep a cached tonic Channel per MC endpoint (today McClient builds a channel per call, crates/mh-service/src/grpc/mc_client.rs). At most ONE outstanding beat per meeting: a due beat whose predecessor is still in flight is skipped and counted. The per-attempt deadline is shorter than the interval, there is NO retry, and one meeting's hung call never delays another's.
    5. Verdicts:
     - RELEASED(cause): release through handle_end_meeting with the registered mc_id (Ownership check kept). Close every active session with MhCloseCode HANDLER_RELEASED (MH-CLOSE owns the code mapping and lands before this task, a hard dependency). Send NO NotifyParticipantDisconnected (ConnectionEnd::MeetingReleased already suppresses it).
     - OWNED: nothing.
     - FAILED_PRECONDITION (owner mismatch), an UNSPECIFIED verdict, or a transport error: count it and take NO action.
     - MH never ends a meeting because MC is unreachable. (MH-3 adds the one silence rule, the owner-silence release.)
    6. EndMeetingRequest.cause is honoured and recorded. An EndMeeting with cause UNSPECIFIED is refused INVALID_ARGUMENT and counted. This is safe to enforce now: P1 already sets cause = MEETING_ENDED at MC's existing teardown caller, so no live caller sends UNSPECIFIED. On a removal cause (HEARTBEAT_TIMEOUT, APPLY_LAG, PLANNED_TRANSFER), connections still awaiting their bind are closed with HANDLER_RELEASED. On MEETING_ENDED they are left to the bind outcome.
    7. All-or-none apply (R-8). The behaviour already holds (handle_config_apply; validation rejects in crates/mh-service/src/routing/mod.rs PolicyRejection::outcome). Pin it with tests (below). With protocol, state the invariant ONCE at RegisterMeetingResponse.applied_generation in proto/dark_tower/internal/v1/internal.proto, if it is not already worded as ADR §1 words it: on any refusal the previously installed snapshot keeps forwarding and applied_generation keeps reporting it, or 0 if none. The registration record (mc_id, endpoint, dial public key, heartbeat interval) is upserted independently and is NOT rolled back by a refusal.
    8. Retire the process-start epoch (R-36), paired with protocol. Delete RegisterMeetingResponse.process_start_epoch_ms (reserve the tag and name), crates/mh-service/src/process.rs and its plumbing (main.rs, grpc/mh_service.rs, and the test process_start_epoch_is_identical_across_two_calls_in_one_process). Also delete MC's mechanical log reads of it in crates/mc-service/src/grpc/mh_client.rs, and add the field to the verify-codegen absence check so generated code proves it is gone. Remove the obsolete STORY-4 re-assert NOTE in handle_end_meeting and any remaining "re-assert cadence" premise in MH comments touched here.

    TELEMETRY (crates/mh-service/src/observability/metrics.rs; obligation (c): each series lands with its mh dashboard panel, its docs/observability/metrics/mh-service.md catalog entry, zero-init, and an ADR-0032 test)
    - mh_mc_heartbeats_total{outcome=success|error|timeout|skipped_in_flight|dropped_queue_full}
    - mh_mc_heartbeat_latency_seconds (histogram)
    - mh_media_handler_releases_total{trigger, clients=present|none}. Removal triggers are projected from ReleaseCause (heartbeat_timeout, apply_lag, planned_transfer). MH-3 adds the one MH-local value owner_silence. MEETING_ENDED stays on mh_media_meeting_teardowns_total{outcome="released"}. clients=present means at least one active connection at release.
    - Counters for the verdict outcomes: owner_mismatch, unspecified_verdict, misroute (RELEASED with cause UNSPECIFIED, which still ends the meeting).
    - mh_media_meetings_without_clients (gauge, set on the actor tick)
    - Heartbeats are logged on state transitions only, never per beat. No meeting, participant, handler or connection id labels.

    CONFIG (obligation (a))
    No new config key in this task (the heartbeat interval comes from MC per meeting).

    TESTS (obligation (d): test-gates.md "MH", Tier 1b rigs, paused time, no sleeps; every negative test has its positive arm)
    - Each registered meeting heartbeats independently to its own MC and carries the registered mc_id. A hung RPC for M1 does not delay M2's tick.
    - A full config-apply mailbox shows as lag. A wedged lifecycle mailbox produces no heartbeat.
    - The connection set excludes pre-binding sessions and reflects a disconnect whose notify exhausted MAX_RETRY_ATTEMPTS.
    - No retries; the per-attempt timeout is below the interval.
    - Verdicts in one test:
      - RELEASED ends the meeting, closes sessions and sends no Disconnected;
      - OWNED does nothing;
      - FAILED_PRECONDITION and an UNSPECIFIED verdict do nothing and the meeting stays held.
    - All-or-none, parameterised over INVALID / TRANSPORT_INCONSISTENT / STREAM_CEILING / EDGE_CAP:
      - generation 4 is applied and forwarding A→B;
      - generation 5 carries one valid change plus one refusing change. Asserted on frames through the transport shim: the valid change does not take effect and A→B keeps forwarding. The heartbeat reports applied_generation=4 with apply_refusal=<kind>;
      - generation 6 (valid) applies and clears the refusal;
      - extra arm: a first-ever apply refused still heartbeats, with 0 plus the refusal.
    - heartbeat_interval_ms 0 is refused.

    ENV-TEST (Covers: env-tests; paired with test)
    In crates/env-tests, against the real MC: mh_mc_heartbeats_total{outcome="success"} rises for a live meeting on each MH that holds it, and mh_media_handler_releases_total stays flat while the meeting is owned. The zombie leg belongs to MC-B2, which this task's owner reviews.

    SAME COMMIT
    The process_start_epoch_ms proto deletion together with its MH and MC readers and its verify-codegen absence check. Every new series together with its panel, catalog entry and zero-init.

    CROSS-TASK OBLIGATIONS
    (a) no new config key; (b) retirements carry their catalog/panel/alert/runbook references; (c) series hygiene as above; (d) test-gates MH section implemented here, paired with test; (e) R4 label hygiene (no id labels; transitions-only logs). Tier: full.
  slug: 2026-11-10-mh-heartbeat-verdicts
  tag: story-2026-10-31-survive-handler-loss-task-MH-2
- id: 16
  status: completed
  specialist: meeting-controller
  deps:
  - 3
  - 5
  - 12
  prompt: |-
    MC-D: MC-minted media_dial credential, delivery and refresh; ref-keyed client status and FAILED re-push

    Specialist: meeting-controller. Pair with security (crypto, credential custody) and test.
    Depends on: SEC-A (security policy content for MC-minted dial credentials), P1 (RegisterMeetingRequest.media_dial_public_key with its key-material guard allow-list entry), MC-A (versioned ref-addressed set, Registering/Active, register-before-token).
    Must land BEFORE C1 (SDK switch) and MH-1 (MH media_dial-only admission), and before any mid-meeting handler add (MC-C).
    Governing design: ADR-0039 §5 (key, token, delivery, refresh, MH verification contract, left participant, KEK guard), §4 (version semantics).
    Requirements satisfied: R-5, R-13 (MC side), R-14, R-15 (MC side), R-17 (refs in client reports).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (its section for this task is reproduced below). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    MC signs a short-lived per-handler dial credential locally (no AC call) and keeps every participant supplied with a valid one for every handler in its set, so a client can dial a handler at any point in a meeting of any length.

    CONCRETE CHANGES
    1. Key (§5, L4-1): one Ed25519 keypair per meeting, from ring SystemRandom, generated when the meeting actor is created, held only in that actor's memory beside and separate from the KEK (media_admission/kek.rs), never derived from it. Name the private half so the credential vocabulary recognises it (e.g. `dial_signing_key`, see crates/dt-guard/src/common/pii_vocabulary.rs CREDENTIAL_TOKENS) and wrap it in a redacting type with no Debug output. Never persisted (not Redis), logged, traced or serialized; dies with the meeting; no rotation. Never in any JWKS.
    2. Send only the 32-byte public half in RegisterMeetingRequest.media_dial_public_key on every RegisterMeeting for the meeting (crates/mc-service/src/grpc/mh_client.rs).
    3. Token (§5, security ruling): compact JWS, header exactly {"alg":"EdDSA","typ":"JWT"} (NO kid), claims {token_type="media_dial", sub (from MC's authenticated session for that participant, never from a request), meeting_id, aud = the handler's handler_id, iat, exp = iat + TTL}.
    4. Delivery (§5, L4-3): every MediaHandlerSet carries a media_dial_token for EVERY member, minted per participant (sets become per-participant messages). Register-before-token holds from MC-A: only Active handlers are listed, so no token is ever issued for an unregistered handler.
    5. Refresh: re-push each participant's set every TTL/3, jittered per participant (seedable RNG), on a meeting-actor sleep_until arm, through the existing per-turn coalesced dirty flush (actors/meeting_media.rs SLOT_VIEW_FLUSH_BATCH discipline), plus on every membership change and on (re)join. A token-only refresh keeps handler_set_version and refs unchanged. No per-MC limiter (user ruling).
    6. Config: MC_MEDIA_DIAL_TOKEN_TTL_SECONDS, default 300, hard-fail at startup outside [60, 900]. Required configMapKeyRef.
    7. Client status by ref (§4): MhConnectionStatus arrives with handler_ref + handler_set_version (P2 fields). Route it from webtransport/connection.rs (MediaConnectionUpdate handling, ~line 931) to the meeting actor, resolve handler_ref exactly against the participant's CURRENT set, and keep the latest status per (participant, ref) received while the ref was in the set; version is correlation-only, never a discard criterion. Statuses for refs outside the set are ignored. This state is consumed by MC-E's metric; it never feeds routing. The legacy url-keyed ParticipantActor::mh_statuses map stays non-routing.
    8. FAILED fast re-push (§5): a FAILED status for an in-set ref triggers an immediate re-push of that participant's set only, rate-limited per session through the existing ClientWorkLimiter (crates/mc-service/src/client_work_limiter.rs). Credential re-push to the sender only, never routing input; add that sentence to the MhConnectionStatus contract comment if P2 did not.
    9. KEK guard by construction (§5): MC's join path verifies only AC-signed tokens (crates/mc-service/src/auth/mod.rs, common::jwt). Re-point the rejection tests at an MC-minted media_dial token with three arms: kid-less media_dial → MissingKid (crates/common/src/jwt.rs extract_kid); media_dial forged WITH a kid present in the fixture's served JWKS but signed by a different key → signature failure; AC meeting token → accepted.
    10. Extend scripts/guards/semantic/checks.md credential-leak items 11-13 (MC key custody) to the dial signing key and tokens: never in logs, metrics, traces, errors or Debug.

    TESTS (test-gates.md §MC media_dial items; paused time; seeded RNG)
    - Per-meeting keypair; RegisterMeetingRequest.media_dial_public_key is byte-equal to the meeting keypair's verifying-key bytes, and (distinguishing inputs) not equal to the meeting KEK nor to the signing seed/PKCS#8; every minted token verifies against exactly the bytes on the wire; the key is absent from logs, Debug output and AC's JWKS.
    - Every set entry carries a token: verify with the public key; sub = the session's own (distinguishing input: another roster member's sub must not appear), aud = that handler_id, exp = iat + TTL.
    - Paused time across several TTLs: an in-set client never holds only an expired token.
    - Token-only refresh keeps handler_set_version and refs; membership change advances it.
    - FAILED report → immediate re-push to that sender only; other participants receive nothing; over-rate reports are limited; out-of-set ref ignored.
    - Latest status per ref: a CONNECTED after a DISCONNECTED supersedes it; a report after the ref left the set is ignored.
    - Join rejects media_dial (three arms above). Config: 59 and 901 rejected, 60/900 accepted.
    - Env-test (pair with test): a real join's set carries a token per member that verifies against the meeting's key. MH acceptance of the token is proven in MH-1, which also owns the left-participant leg.

    TELEMETRY (telemetry-contract.md §MC)
    - mc_media_handler_set_pushes_total{reason=join|change|token_refresh|failed_report}
    - config gauges mc_media_dial_token_ttl_seconds and mc_media_dial_token_refresh_interval_seconds
    - drops reuse mc_participant_outbound_messages_dropped_total with a new payload_kind for the handler set.

    SAME-COMMIT OBLIGATIONS: (a) MC_MEDIA_DIAL_TOKEN_TTL_SECONDS row (default, range, TTL/3 refresh) in docs/runbooks/mc-deployment.md §Configuration Reference; (b) n/a retirements; (c) each series with panel + catalog + zero-init + ADR-0032 test; (d) test-gates items; (e) no id labels, tokens and keys never logged.

    Tier: full (crypto, credentials, wire).
  slug: 2026-11-10-mc-media-dial-credential
  tag: story-2026-10-31-survive-handler-loss-task-MC-D
- id: 17
  status: completed
  specialist: client
  deps:
  - 6
  - 12
  - 16
  - 14
  prompt: |-
    C1: SDK reconcile-to-set transport, dial credentials held per handler, MH close codes and per-transition loss reports.

    Specialist: client. Pair with test (unit gates, e2e migration) and security (credential retention, the scoped R-23 amendment). Protocol reviews the generated-code consumption. Tier: full (wire contracts, credentials, a transport seam).

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md, sections 4 (Migration = the join path), 5 (Dial credential, especially "Client retention"), 7 (Close codes). Supporting files: docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md (section "Client").

    Requirements satisfied: R-5 (client half), R-14 (client half), R-15 (client half), R-17 (client half: refs only), R-18, R-19 (client half). R-42 is C2. It contributes to R-36: the SDK stops reading the superseded fields. Deleting them is P3, not this task.

    Dependencies, all landed first:
    - P2: the additive signaling contract. MediaHandlerSet {handler_set_version, handlers[{media_handler_url, handler_ref, media_dial_token}]} on JoinResponse and as a ServerMessage variant; handler_ref on SendTarget and StreamAssignment; MhConnectionStatus.handler_ref + handler_set_version; the MhCloseCode enum; MhConnectRequest.media_dial_token. Regenerated TS.
    - MC-A: MC sends MediaHandlerSet (on join and on every change) and refs on directives and assignments, alongside the legacy fields.
    - MC-D: MC puts a media_dial token on every member of every set, refreshes at TTL/3, records MhConnectionStatus by (participant, ref), and re-pushes a participant's set at once on a FAILED report for an in-set ref.
    - MH-CLOSE (media-handler): MH sends a numeric MhCloseCode on every intentional close.
    Depended on by: MH-1, P3, MC-C, MC-E and C2.

    WHAT CHANGES (packages/sdk-core unless noted)

    1. Reconcile to the set (ADR section 4).
    - The single-use MediaTransport.connectAll (media/MediaTransport.ts) becomes a reconcile-to-MediaHandlerSet driven by every set message, the JoinResponse one included. Join and migration share one path, with no join special case.
    - Diff on handler_ref ONLY. A set whose refs are unchanged replaces tokens and causes NO dial and NO close: that is a token-only refresh, and handler_set_version is unchanged in it.
    - A new ref at a previously used URL is a real close plus a fresh WebTransport session (allowPooling stays false), never deduplicated.
    - Per-ref lifecycle: connecting, connected, lost, parked, removed.
    - Dial every member not connected. Never dial anything outside the latest set. Selectors over connected transports stay selectors and never dialers (the openUniStream / getDatagramChannel / incomingUniStreamsFor rule).
    - Redial a lost in-set member with full-jitter exponential backoff: each delay uniform in [0, min(cap, base * 2^n)], with base and cap as new clientConfig values (config/clientConfig.ts, logged with the config log line). The first retry is no earlier than one MH keepalive interval, because a redial before MH idles out the old session meets BINDING_CONFLICT. Stop the moment the ref leaves the set.

    2. Empty set (ADR section 4, L4-2).
    - A join-time set may be empty. Treat it as "media connecting": join completes, no dial, no fault, no loss report.
    - The first non-empty set dials through the normal reconcile.
    - If no set with a member arrives within a new clientConfig bound, emit exactly one non-fatal session event and keep the session. Rendering is C2.

    3. Removal cleanup (ADR section 4). ONE function, unit-tested, for a ref that left the set:
    - close its transport as a directed removal (no fault, no loss report), including when the server left it open (the data-plane fence);
    - abort an in-flight dial to it and close it when it resolves;
    - cancel its redial timer and park state;
    - drop its token;
    - remove its egress lane (media/pipeline/egress.ts senderFor targets; media/pipeline/videoEgress.ts setTargets lanes are retained today, so prune them);
    - end its read loops quietly, with no "ended" or "failed" transportFault (media/lifecycle/receiveTransports.ts PerHandlerReadLoops gains a quiet per-handler stop);
    - clear its per-transport hop-sequence monitor;
    - drop its recovery-request queue (media/lifecycle/recoveryRequestQueue.ts).
    Slot overlay clearing is C2.

    4. Refs on the send and receive paths (ADR section 4).
    - SendTarget and StreamAssignment are resolved by handler_ref to the connected transport for that ref. Update every URL-keyed site: session/mediaWiring.ts (sendDirectiveListener); signaling/SignalingClient.ts decode (around lines 675, 731, 745, 943); media/lifecycle/AudioPipeline.ts (around 793-803); media/lifecycle/VideoReceiver.ts (around 322); egress and videoEgress; receiveTransports; recoveryRequestQueue.
    - A ref that is not a member of the current set is ignored and surfaced once per session as a bounded contract fault (the existing event-once `[dt-signaling-contract]` WARN pattern), never through the not_connected drop. The counted series for it is C2.
    - The SDK must no longer READ JoinResponse.media_servers, SendTarget.media_handler_url, StreamAssignment.media_handler_url or MhConnectionStatus.mh_url. Assert that in unit tests. Do NOT delete the proto fields: P3 deletes them.

    5. Credentials (ADR section 5, "Client retention"; the scoped R-23 amendment).
    - Hold the latest media_dial token per in-set ref ONLY in MediaTransport's current-set state, as a private field.
    - Replace it wholesale per set. Clear it when its ref leaves the set and at session end.
    - Never persist, log, stringify, put in an error or cause chain, metric, span or the DOM.
    - The token is opaque: never parse the JWT.
    - Every dial (join, added, redial, retry) populates MhConnectRequest.media_dial_token from the held token ALONGSIDE join_token, which stays populated exactly as today. MH ignores media_dial_token until MH-1. P3 deletes join_token and the SDK line populating it.
    - A token is reusable across dials within its lifetime (MH keeps no jti cache).
    - DECLINED for a ref: park the ref, send MediaConnectionUpdate FAILED for it (this triggers MC's re-push), make no dial until a set with a newer token for that ref arrives, then exactly one dial. A refresh arriving while parked releases the park.
    - Keep the existing semantic-guard "client credential lifetime" posture: document the scoped retention at the field.

    6. Close codes (ADR section 7). Read MhCloseCode from WebTransportCloseInfo.closeCode on transport closed.
    - WRONG_HANDLER, BINDING_CONFLICT, MC_UNAVAILABLE: counted, no loss report, backoff redial while the ref is in the set. WRONG_HANDLER and BINDING_CONFLICT reuse the held token.
    - HANDLER_RELEASED: expected. If the ref has already left the set, no event. If it arrives while the ref is still in the set (drain shed, owner silence), wait a short config grace for a set update; if none comes, report DISCONNECTED and redial.
    - DECLINED: as in item 5.
    - No code (crash, idle timeout): the loss path, item 7.
    - Zero or unknown: a counted contract fault. Fail loudly, never fatal.

    7. Loss reports (ADR section 4).
    - MediaConnectionUpdate is sent on every per-ref transition, not once at join (today: session/MeetingSession.ts around 487-492, the R-21 once-only send). Each status carries handler_ref and handler_set_version, and never mh_url.
    - No report for directed removals or for a server close following one.
    - Report bounds keep the existing caps (validation/limits.ts).
    - The message stays diagnostic-only.

    8. Keys (ADR section 4 Keys). A set change never rebuilds SessionMedia or TransmitKeyManager (session/mediaStart.ts). A new egress target starts awaiting_group and keys a group forward through the one owner (media/pipeline/videoEgress.ts). The continuity tests are C2; do not break the invariant here.

    9. Existing connection series stay as they are. Do NOT add or reshape any R-42 series here; C2 does that. dt_client_mh_connection_total{status, mh_index_bucket, join label set} and dt_client_time_to_first_mh_connected_ms must keep compiling, emitting and passing their existing tests unchanged. With connectAll gone, emit the connection counter at each dial outcome exactly as today (status success|failure; mh_index_bucket = the handler's position in the set that triggered the dial), and the first-connected histogram once per session. The MediaTransport.ts spread comment and its test pins stay as they are; C2 flips them.

    10. e2e and the bus (packages/web-app).
    - lib/e2eBus.ts gains handlerSets: an ordered history of {version, handlers:[{ref, url}]}, never tokens. mediaConnected and its consumers move to refs.
    - Migrate every spec and fixture reading joined.mediaServers to bus handlerSets: e2e/fixtures.ts (around 373, 486, 509-520), e2e/cohortContexts.ts (around 167-170, waitForAllMediaConnected), e2e/videoScenario.ts (around 127), e2e/partial-connectivity.spec.ts (around 15, 100, 117, 221). blockHandlers takes URLs from handlerSets.
    - Update lib/e2eAnalysis.ts and __tests__/e2eBus.test.ts accordingly.
    - Existing specs must pass unchanged in behaviour.

    TESTS (test-gates.md section "Client"; MockWebTransport from packages/test-utils, injected clock, no sleeps; every negative has its positive arm):
    - An add dials through the same code as join. A removal closes a transport the server left OPEN, removes the egress lane, ends read loops without a transportFault and sends no loss report. A server close right after a set that removed that handler causes no loss report.
    - Remove ref 1 and add ref 2 at an IDENTICAL URL: a real close plus open.
    - A token-only refresh (same refs and URLs, new tokens) causes no close or open. The latest token is reused for redials.
    - DECLINED: no dial until a set with a newer token arrives, then exactly one dial. FAILED is reported. A refresh while parked releases it.
    - The close-code table as in item 6, including the zero/unknown contract fault.
    - The loss report echoes handler_ref and handler_set_version, and is sent per transition.
    - An empty join set followed by a set: normal media. Past the bound: exactly one notice, session kept.
    - No read of the superseded fields.
    - The existing dt_client_mh_connection_total and time-to-first tests still pass unchanged.
    - Existing env-tests (Layer 7 browser specs) stay green. This task adds no new cluster behaviour; C3 adds the resilience spec.

    SAME COMMIT: the SDK, web-app and e2e changes, and the clientConfig keys with their config log line and documentation.

    Cross-task obligations: (a) each new config key (the redial base and cap, the empty-set notice bound, the HANDLER_RELEASED grace) is documented with default and range in the client config docs and the web-app config reference; (b) no retirements here (P3 deletes the superseded fields); (c) no new or reshaped series here (all of that is C2); (d) implement test-gates.md section "Client", paired with test; (e) R4 label hygiene, with no meeting, participant, handler, ref, URL or slot labels.
  slug: 2026-11-10-sdk-reconcile-handler-set
  tag: story-2026-10-31-survive-handler-loss-task-C1
- id: 18
  status: completed
  specialist: test
  deps:
  - 8
  - 9
  - 12
  prompt: |-
    T-1: MH pool fixture, process injector, resilience hygiene, browser mhPool.ts

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md section 7 (MH addressing and provisioning; mh.sh "Callable by automated tests") and section 9 (Verification). Machine contract: docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt (sections 1-8; section 8 is this fixture). Test gates: docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md, "Env-tests", "Round 3" and "Round 4" sections.

    Requirements satisfied: R-39 (resilience-leg hygiene and the injector the legs use), R-28 (address-invariant helper, hoisted from I-2's inline check). Enables the resilience legs that live in other tasks: MC-B2 (zombie and crash-to-survivor), MC-C (crash with replacement in a two-handler meeting, stranded pair), MC-F (planned transfer with and without a target), MH-1 (left participant), C3 (browser specs).

    GOAL
    One home, in Rust and one in the browser harness, for driving the Kind MH pool and for killing or freezing an MH process, so every resilience leg is deterministic, cleans up after itself and fails loudly instead of flaking. This task ships infrastructure and its own self-tests; it adds no resilience leg itself.

    INPUTS (from I-5, mh.sh itself, and I-2, the slot pool that puts it in use; both infrastructure)
    - `DT_MH_SH` (I-2): absolute path to infra/kind/scripts/mh.sh (I-5), exported by scripts/layer7.sh. Unset is a loud PRECONDITION failure; never resolve a fallback path.
    - `mh.sh list --json` (schema_version, additive-only; every slot including free ones; slot, host_port, address, pod_ip, state, deployment, pod, pod_uid, revision, stale (consume `stale`; there is no `current`), owner, restart_count, terminating_past_grace; the key set is pinned by mh.test.sh), `up [--slot k] --owner ID --json` returning only at Available (MH /ready), `drain <k> --owner ID [--no-wait]`, `drain --owner ID --all`, one stderr line `MH_RESULT STATUS=OK|FAIL REASON=<token>`, grouped exit codes (0 OK, 1 FAIL, 2 PRECONDITION, 3 CAPACITY, 4 REFUSED, 5 BUSY).
    - The Kind overlay sets `shareProcessNamespace: true` on the MH template mh.sh instantiates (absent from the base and every non-Kind render; security ruling, ADR-0039 section 9).
    - I-2's inline address-invariant env-test (every live MH advertises a unique address equal to its slot ConfigMap value).
    From MC-A: MediaHandlerSet with handler refs and URLs on JoinResponse and as a ServerMessage.

    WHAT TO BUILD
    1. crates/env-tests/src/fixtures/mh_pool.rs (mh-sh-contract section 8):
       - `MhPool::snapshot()` parses `list --json`; unknown schema_version or a missing contract key fails loudly.
       - `MhPool::up()` returns an `MhSlot {slot, deployment, pod, pod_uid, address}`; owner = Layer 7's per-run id (the run token layer7.sh already generates for per-run org provisioning, `__generate_org_subdomain`; export it if not already exported, one home).
       - `MhSlot::drain()` and `drain_no_wait()`: explicit, asserted.
       - BUSY (exit 5) is retried within a bounded budget; every other failure surfaces the REASON token verbatim; an unknown token is a failure.
       - Teardown: explicit `drain --owner <run> --all`, asserted (it also reclaims a pod left by not-available-timeout). Drop never panics; it prints that exact command (shape: KIND_SIZING_RESTORE_CMD in crates/env-tests/src/fixtures/deployment_config.rs:509).
       - `assert_baseline()`: before AND after every destructive leg, no MH owner outside {baseline, this run} and the baseline-owned serving count equals B, else fail with REASON foreign-mh-activity. B comes from `list --json` "baseline", never a constant. The baseline check is an eventual poll (crates/env-tests/src/eventual.rs) with a ceiling derived from GC's configured staleness (GC registry lags; a single read flakes).
       - After `up()`, wait for Prometheus `up{job="mh-service",instance="<pod_ip>:8083"}==1` before any MH metric baseline (observability). Pin by pod IP via crates/env-tests/src/fixtures/metrics.rs:instance_for_pod_ip, with pod_ip from the list/up JSON: the mh-service scrape job sets no `pod` target label, so a `pod=` matcher matches nothing. Drain outcomes are read from MC series, never from the exiting MH's last scrape.
       - `assert_addresses_unique()`: hoisted from I-2's inline check; I-2's test then calls this helper (same commit edits that test).
    2. Process injector (same module or `fixtures/mh_inject.rs`):
       - Preflight: read the target pod spec; if shareProcessNamespace is absent, fail with "pod <name> lacks shareProcessNamespace: mh-template is stale or not Kind-rendered; re-run deploy (infra/kind/scripts/deploy.sh)".
       - `kill(pod)`: `kubectl debug <pod> --target=mh-service` running as uid 65532 (the pod enforces runAsNonRoot), pinned image, `kill -KILL <mh pid>` (mh-service is not PID 1 under shareProcessNamespace; never signal PID 1). Then assert the container restarted in place (restart count +1, same pod_uid). NEVER `kubectl delete pod --force --grace-period=0` (forbidden; ADR-0039 section 9, R-28 guard).
       - `freeze(pod)` / `resume(pod)`: SIGSTOP / SIGCONT the same way. The freeze budget is DERIVED from the live pod's liveness probe (periodSeconds x failureThreshold, read from the pod spec; today infra/services/mh-service/mh-0-deployment.yaml:429-438 gives about 30 s). `freeze` returns a guard; exceeding the budget fails loudly with a probe-budget message; `resume` asserts restart count unchanged so a no-op or a kubelet restart can never pass as a zombie.
    3. Participant fixture (crates/env-tests/src/fixtures/participant.rs, mc_session.rs): expose the current MediaHandlerSet (version, entries with ref, URL and per-handler media_dial_token) and its history from MC's ServerMessages, and move `offered()` / `two_offered_handlers()` onto the set's URLs so P3 can delete JoinResponse.media_servers without touching env-tests. The token accessor returns the latest token per in-set ref. The field exists from P2 and is empty until MC-D populates it, so an empty token is surfaced as absent, not as an error. MH-1 then switches the env-test MH connect to that token in its own commit. Tokens are never logged or printed in assertion messages (credential-leak treatment, ADR-0039 §5). Map a set URL to a slot via `mh.sh list --json` address.
    4. packages/web-app/e2e/mhPool.ts: thin wrapper over the same mh.sh JSON contract for Playwright (agreed with client): execFile, no shell; fails loudly when DT_MH_SH is unset; up, drain, list, kill, freeze, resume with the same preflight and restart-count assertions; owner = the same run id. One Node-tier test in packages/web-app/tests/ per the existing one-test-per-helper rule. Precedent for node-side exec: e2e/capacity.evidence.ts:36.

    TESTS
    - Hermetic Rust unit tests: list JSON parse (golden, unknown version, missing key), REASON-token surfacing (known, unknown), BUSY retry bound, baseline predicate (foreign owner, short pool), liveness-budget derivation, Drop prints the command and does not panic.
    - One smoke env-test proving the fixture end to end on a live cluster: up, Prometheus discovery, drain, baseline restored, nothing left owned by the run. Plus an injector smoke: freeze then resume on a fixture-owned MH with the freeze-took-effect and restart-count assertions, and kill with in-place restart. These use only fixture-owned MHs, are #[serial], and restore the baseline.
    - Isolation convention (story 4 has no per-test isolation; it relies on serialization): every destructive or pool-mutating env-test lives in crates/env-tests/tests/40_resilience.rs, and EVERY test in that binary carries #[serial] — no unmarked test may exist there, because serial_test only serializes marked tests. `cargo test` runs test binaries one at a time and Playwright runs with workers=1, so within one Layer-7 run a killed, frozen or drained MH affects only the test that did it. State this convention in the file's module doc and add a unit-level check (or a dt-guard / grep in this task's tests) that fails if any #[test]/#[tokio::test] in 40_resilience.rs lacks #[serial]. Concurrent runs sharing one cluster are NOT isolated: they fail loudly with foreign-mh-activity, never pass falsely.
    - Node-tier test for mhPool.ts against a stub mh.sh.

    SAME COMMIT
    Fixture, injector, participant-set exposure, the hoisted address helper (with I-2's test switched to it), mhPool.ts and their tests. Layer 1 to 7 green.

    CROSS-TASK OBLIGATIONS
    (a) No new service config key; any new env var Layer 7 exports is documented in crates/env-tests/README.md. (b) No retirement. (c) No new series. (d) This is test's infrastructure section of test-gates.md. (e) No labels.

    PAIRING AND DEPENDENCIES
    Depends on I-5 (mh.sh and its contract), I-2 (slots, Kind overlay, DT_MH_SH, inline address check) and MC-A (MediaHandlerSet). MC-B2, MC-C, MC-F and C3 depend on this task; MH-1 (left-participant leg) and P3 (deletes JoinResponse.media_servers, which this task moves env-tests off) consume its participant-set exposure. Pair with infrastructure (mh.sh contract from I-5, overlay from I-2) and client (mhPool.ts). Security reviews the injector (debug container, uid, no PID-1 signalling). Tier: full (cluster mutation, shared env-test seam, privileged-ish injector).
  slug: 2026-11-10-mh-pool-fixture
  tag: story-2026-10-31-survive-handler-loss-task-T-1
- id: 19
  status: completed
  specialist: media-handler
  deps:
  - 3
  - 14
  - 16
  - 17
  - 18
  prompt: |-
    Task MH-1: MH media_dial-only admission and pending-path deletion.

    GOAL
    MH admits a client only on a current MC-minted media_dial credential for that meeting and that handler. It refuses everything else at once without holding it. Admission refusals carry MH-CLOSE's close codes. The pending-before-registration path is deleted. Story: docs/user-stories/2026-10-31-survive-handler-loss.md.

    GOVERNING DESIGN
    docs/decisions/adr-0039-mh-replacement-and-migration.md §3 (register before token; pending path deleted), §5 (Dial credential: key, token, MH verification order, reuse, pending cap, a participant who has left), §7 (close codes: the WRONG_HANDLER and DECLINED admission arms), §8 (credential-rejection telemetry); docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md (section "MH", token and pending bullets) and telemetry-contract.md (MH rows). Carry these; invent nothing.

    DEPENDENCIES (must have landed)
    - SEC-A: security policy content for MC-minted dial credentials.
    - MC-D: MC mints media_dial tokens, puts one per member in every MediaHandlerSet, and sends the meeting's public key in RegisterMeetingRequest.media_dial_public_key. MC-A (via MC-D) guarantees register-before-token, first join included.
    - C1: the SDK already populates MhConnectRequest.media_dial_token alongside join_token on every dial.
    - P1: the media_dial_public_key field.
    - MH-CLOSE: MhCloseCode emission on every intentional close (this task adds only the admission arms).
    - T-1: exposes each env-test participant's MediaHandlerSet (handler refs, addresses and per-handler media_dial tokens) to the Rust fixtures.
    P3 deletes MhConnectRequest.join_token from the proto and SDK AFTER this task. This task does NOT delete the field; MH simply stops reading it. MH NEVER accepts both credential types: the switch is atomic in this commit.

    PAIRING
    media-handler alone; client reviews (close-code semantics, dial behaviour); security reviews (verification order). Paired with protocol for the internal.proto comment edits. Requirements: R-12 (MH half), R-13, R-19 (admission arms), R-36 (MH pending path; meeting JWT on MH connect), R-34 (MH rows), R-40.

    CHANGES (crates/mh-service unless noted)
    1. Key storage. RegisterMeetingRequest.media_dial_public_key must be exactly 32 bytes, else the registration is refused INVALID_ARGUMENT. It is stored on the meeting's registration record (session/mod.rs MeetingRegistration), installed by the registration UPSERT and not by the policy apply, so a refused apply never blocks or rolls it back. Never logged or Debug-printed. Note at the field, with protocol, that the last validation-passing registration's key wins: the existing "MH does not verify meeting ownership" residual.
    2. Verification at accept, BEFORE anything is held, in exactly this order (replaces MhJwtValidator's AC/JWKS client path, crates/mh-service/src/auth/mod.rs validate_meeting_token, and its call in webtransport/connection.rs; the AC JWKS client stays ONLY for MhAuthLayer service tokens):
     (a) size cap;
     (b) JWS header alg == EdDSA only (reject none, HS*, everything else). kid / jwk / jku / x5u / x5c are IGNORED (not a rejection reason). MH NEVER fetches a key;
     (c) read the meeting_id claim unverified, only to look up that meeting's registered key. Meeting not registered: DECLINED immediately, nothing parked;
     (d) signature with that key;
     (e) meeting_id claim equals the looked-up meeting; token_type == "media_dial"; exp and iat within MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS; sub non-empty;
     (f) only then, aud == this MH's per-incarnation handler_id, else WRONG_HANDLER.
     Every failure in (a)–(e) is the one generic DECLINED. An AC meeting token is DECLINED. No jti cache: a token is valid for any number of dials within its TTL.
    3. Clock skew becomes config. MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS: default 30, startup-validated to ≤ 60 (hard fail). It replaces the hardcoded 300 in crates/mh-service/src/main.rs (MhJwtValidator::new(..., 300)).
    4. Pending cap. Connections awaiting MC's bind answer (NotifyParticipantConnected in flight) are capped per (meeting, sub) at MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT (small; default 2). The excess is closed DECLINED and counted as a new decline outcome on mh_media_session_starts_total. A participant who has left but still holds a valid token: MC answers sender_id 0, MH closes DECLINED, and no frame reaches that connection while it awaits the bind.
    5. Delete the pending-before-registration path: PendingConnection, meeting_notifiers, AddPendingConnection/RemovePendingConnection, await_meeting_registration (webtransport/connection.rs), RegistrationOutcome, the register-meeting timeout config MH_REGISTER_MEETING_TIMEOUT_SECONDS (config.rs register_meeting_timeout_seconds, config.env, the manifests, RETIRED_KEYS), and the metric mh_register_meeting_timeouts_total with its catalog entry, mh-overview panel(s), alert and runbook references (obligation (b)). handle_register_meeting no longer drains or promotes pending connections. handle_end_meeting's pending-related branches go.
    6. Close codes: use MH-CLOSE's MhCloseCode mapping (it has landed). This task adds the two admission arms: DECLINED for every (a)–(e) failure and the pending-cap excess, and WRONG_HANDLER for step (f) only. Codes concern only the caller's own token-validated meeting; no reason strings.
    7. Proto comment edits with protocol (no field deletion): reword NotifyParticipantConnectedRequest.participant_id's precondition to "the sub of a media_dial token MH has signature-verified against the meeting's registered key", and reword EndMeetingRequest's pending-connections sentence to match the deleted path.

    TELEMETRY (obligation (c): panel + catalog + zero-init + ADR-0032 test)
    - mh_jwt_validations_total on the client path: token_type="media_dial" (replacing "meeting"); failure_reason splits into expired | signature_invalid | aud_mismatch | wrong_type | no_meeting_key | malformed, replacing the single validation_failed in webtransport/connection.rs. These reasons are metric-only; clients see only DECLINED or WRONG_HANDLER.
    - mh_media_session_starts_total gains the pending-cap decline outcome.
    - Deleted: mh_register_meeting_timeouts_total.
    No id labels. Tokens and keys never appear in logs, metrics, traces or errors.

    CONFIG (obligation (a))
    MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS and MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT become required configMapKeyRefs in infra/services/mh-service/config.env and whichever MH manifest exists (I-2's template if landed), with rows in docs/runbooks/mh-deployment.md's configuration reference. MH_REGISTER_MEETING_TIMEOUT_SECONDS is removed everywhere and added to RETIRED_KEYS.

    TESTS (obligation (d): test-gates.md "MH" token and pending bullets; Tier 1b rigs; every negative test has its positive arm)
    - A token signed with the meeting's registered key is accepted, including repeated dials within the TTL.
    - Rejected with DECLINED:
      - another meeting's key (cross-meeting);
      - expired beyond the configured skew (and accepted within it);
      - a token_type other than media_dial;
      - an AC-signed meeting token;
      - malformed or oversized;
      - a correctly signed token for a meeting MH does not hold (never registered, or already released), with nothing parked.
    - aud ≠ own handler_id: WRONG_HANDLER. Distinguishing input: a bad-signature token with a wrong aud gets DECLINED, not WRONG_HANDLER.
    - kid/jwk headers present: ignored, and the outcome is unchanged.
    - Pending cap: the cap-th connection is admitted and the cap+1-th DECLINED.
    - Left participant: sender 0 → DECLINED, and no frame is delivered while pending.
    - Admission closes carry DECLINED or WRONG_HANDLER exactly as specified (the full code mapping is MH-CLOSE's test).
    - Key validation: 31 and 33 bytes are refused, 32 accepted.
    - The skew range validation.

    ENV-TESTS (Covers: env-tests; paired with test)
    FIXTURE SWITCH, SAME COMMIT, or every MH-connecting env-test breaks: crates/env-tests/src/fixtures/participant.rs reach() and the mc_session MH connect must send MhConnectRequest.media_dial_token taken from the participant's MediaHandlerSet entry for that handler (exposed by T-1), instead of the AC join token.
    In crates/env-tests, from a Rust WebTransport client against a live meeting:
    - a valid MC-issued token connects;
    - wrong aud closes with WRONG_HANDLER;
    - an AC meeting token closes with DECLINED;
    - a token for a meeting this MH does not hold closes with DECLINED;
    - a dial after the join JWT's lifetime would have expired still succeeds;
    - the left-participant leg: a participant leaves, redials with its still-valid token, gets DECLINED and receives no media.
    - crates/env-tests/tests/01_mh_deployment_config.rs covers the new keys and the absence of the retired one.

    SAME COMMIT
    The env-test fixture switch to media_dial_token (participant.rs reach() and mc_session).
    The admission switch together with deleting the pending path, and the code, config and metric retirement together with every reference to them. MH must never accept both credential types in any commit.

    CROSS-TASK OBLIGATIONS
    (a)–(e) as listed in the story plan. Tier: full (auth/crypto, client-facing wire, GSA).
  slug: 2026-11-10-mh-media-dial-admission
  tag: story-2026-10-31-survive-handler-loss-task-MH-1
- id: 20
  status: completed
  specialist: media-handler
  deps:
  - 15
  prompt: |-
    Task MH-3: MH planned drain on SIGTERM, owner-silence release, load-report hygiene, QUIC idle timeout as config.

    GOAL
    When told to shut down, MH stops being selectable, tells every MC that holds a meeting on it that it is draining and how long it has left, and keeps forwarding while MC moves each meeting away. It exits as soon as it holds nothing, or at the drain deadline. That is the MH half of planned transfer: one mechanism for mh.sh drain, upgrade, scale-down and node maintenance. MH also releases a meeting whose owning MC has been silent for a long configured period (owner-silence release), becomes GC-selectable without relying on a timer quirk, and a silently dead MH is noticed by clients within a configured idle timeout. Story: docs/user-stories/2026-10-31-survive-handler-loss.md.

    GOVERNING DESIGN
    docs/decisions/adr-0039-mh-replacement-and-migration.md §7 (Drain on SIGTERM, which reverses ADR-0036 §11 "MH sheds media sessions on restart"; QUIC max_idle_timeout as config; close codes for a drain-deadline shed), §8 (drain series; interpretability pairing of the idle timeout with N), §10; §6 (Owner-silence release, user ruling 2026-10-31). Also docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md (section "MH", the load-report and drain bullets) and telemetry-contract.md (MH rows). Carry these; invent nothing.

    DEPENDENCIES
    MH-2: the per-meeting heartbeat carries ServingState and drain_remaining_ms, which this task drives, and its verdicts (OWNED/RELEASED from the owning MC) are what reset the owner-silence timer. MH-2 itself depends on MC-B1, so verdicts flow before owner-silence can ever fire. MC-F (planned transfer) and OPS depend on this task.

    PAIRING
    Paired with operations (shutdown semantics, runbook rows) and test (paused-time gates). Requirements: R-3 (MH half), R-11 (owner-silence release), R-21, R-22, R-34 (MH rows), R-40.

    CHANGES (crates/mh-service unless noted)
    1. Drain on SIGTERM. Restructure main.rs's shutdown sequence (today: set_not_ready, cancel everything, sleep the derived window, exit). New order:
     - signal → set_not_ready (readiness no longer gates client dials under hostPort slots, but it tells mh.sh and the Deployment the pod is going away);
     - → enter a process-wide draining state (e.g. a watch channel read by the GC task and the session actor);
     - → the GC task sends an IMMEDIATE out-of-cycle SendLoadReport with health HEALTH_STATUS_DRAINING (internal.proto HealthStatus = 4; GC already parses it and excludes it from selection) and keeps sending DRAINING on every later report until exit. GC overwrites health_status per report, so draining is process state, not a one-shot. Today the GC task stops at cancel and MH always reports health=1;
     - → the session actor resets every held meeting's heartbeat deadline to now, so each MC gets an immediate beat with ServingState DRAINING and drain_remaining_ms. That value is relative, computed from MH's own deadline, so MH's grace stays the only copy. Every later beat carries both;
     - → servers and media keep running;
     - → exit when MH holds no registered meetings, or at the deadline MH_TERMINATION_GRACE_SECONDS − SHUTDOWN_MARGIN_SECONDS (both existing; the grace is kustomize-substituted from the pod's own terminationGracePeriodSeconds);
     - → only then cancel the shutdown token.
     Sessions still held at the deadline are closed with MhCloseCode HANDLER_RELEASED (MH-CLOSE's mapping), and MH writes the number of meetings shed in its final log line (exit-time counters may never be scraped).
    2. Remove the 2 s SHUTDOWN_SETTLE_TARGET_SECONDS cap and the DrainWindowSource arm it produces (crates/mh-service/src/config.rs, around lines 1153-1170 and 2450). The drain is now the full grace − margin. Update the doc comment that obliges amending ADR-0036 §11; ADR-0039 §7 is that amendment. Keep the existing strict parse and saturating_sub refusal rules for the grace value.
    3. Load-report hygiene (ADR §7, §10):
     - after EVERY successful RegisterMH (initial and re-registration after NotRegistered), send one load report IMMEDIATELY, then enter the ticker. This replaces reliance on tokio::time::interval's first tick (main.rs run_gc_task), so a restarted MH flips GC from pending to healthy within one RPC;
     - a rejected registration (including GC's FAILED_PRECONDITION "address_conflict") keeps being retried while MH stays not-ready (already true in gc_client.rs register and main.rs run_gc_task; pin it with a test);
     - MH uses the load-report interval GC pushes in RegisterMHResponse (already stored; keep it).
    4. QUIC idle timeout as config. MH_MAX_IDLE_TIMEOUT_MS replaces the constant MAX_IDLE_TIMEOUT_SECONDS = 30 (config.rs, used by webtransport/server.rs build_transport_config and main.rs's startup log). It stays validated by the existing MIN_KEEPALIVE_TO_IDLE_RATIO relationship with MH_KEEPALIVE_INTERVAL_MS. The deployed value is sized near MC's heartbeat timeout N (ADR-0039 §7, user-accepted: idle about 3-6 s with keepalive about 1-2 s), so a silent MH death surfaces at the browser as WebTransport.closed with no app-level keepalive. Update the comments that cite MAX_IDLE_TIMEOUT_SECONDS as a wall for the drain derivation and the receive window.
    5. Graceful-shutdown boundary: draining never self-fences on MC silence. The owner-silence release below stays the only silence rule.

    6. Owner-silence release (user ruling 2026-10-31; ADR §6). This moved here from MH-2, which provides the heartbeat verdicts it depends on:
     - A per-meeting MONOTONIC timer.
     - It is reset ONLY by a validation-passing RegisterMeeting or EndMeeting carrying the registered mc_id, or by an OWNED or RELEASED verdict from the registered endpoint.
     - A validation-passing RegisterMeeting from a different mc_id is the existing takeover. It makes that MC the owner and resets the timer.
     - Transport errors, timeouts, FAILED_PRECONDITION, UNSPECIFIED verdicts and invalid registrations NEVER reset it.
     - On expiry of MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS (default 900; startup-validated to [300, 3600], hard fail outside), release exactly like RELEASED: HANDLER_RELEASED, no Disconnected.
     - It is counted as mh_media_handler_releases_total{trigger="owner_silence", clients=present|none} (below), with one INFO log line carrying meeting_id, the owner mc_id and the silence duration (ids are allowed in logs).
     - There is NO per-registration interval check (lead ruling). Instead the config reference states the relationship: MC's N ≤ 30 s, so its interval is ≤ 10 s, and silence ≥ 300 s is ≥ 30 intervals.
     - Not an MC-failover mechanism. Say so in the code comment.

    TELEMETRY (obligation (c): panel + catalog + zero-init + ADR-0032 test)
    - mh_drain_duration_seconds (histogram)
    - mh_drain_meetings_remaining (gauge)
    - mh_drain_meetings_shed_total (counter; best-effort, because MC's mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"} is the alert source)
    - mh_media_handler_releases_total gains the MH-local trigger value owner_silence, documented as MH-local (outside the ReleaseCause projection) in the code enum and the catalog. With clients="present" it is a WARNING (OBS-ALERTS); with clients="none" it is dashboard-only routine cleanup after an MC rollout
    - mh_media_meeting_owner_silence_timeout_seconds (config gauge)
    - the config gauge for the QUIC idle timeout (the value R-33 publishes beside MC's N; coordinate the series name with observability's contract).
    No id labels. Logs on transitions.

    CONFIG (obligation (a))
    MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS (default 900, range [300, 3600], with the stated relationship to MC's N) and MH_MAX_IDLE_TIMEOUT_MS become required configMapKeyRefs in infra/services/mh-service/config.env and whichever MH manifest exists (I-2's template if landed), plus rows in docs/runbooks/mh-deployment.md's configuration reference, the idle row stating its relationship to the keepalive and to MC's N. Retire MAX_IDLE_TIMEOUT_SECONDS. The drain-window row's text changes (no 2 s cap).

    TESTS (obligation (d): test-gates.md "MH" load-report and drain bullets; paused time, no sleeps; positive and negative arms)
    - A structural load report goes out at t=0 after every successful register and re-register.
    - On SIGTERM: a DRAINING load report is sent with no tick, and later reports stay DRAINING; every held meeting gets an immediate DRAINING heartbeat with a decreasing drain_remaining_ms.
    - No exit while holding a meeting before the deadline. Exit at the deadline with the shed count logged and sessions closed HANDLER_RELEASED. Early exit when the last meeting is released.
    - The deadline is derived from MH_TERMINATION_GRACE_SECONDS − margin.
    - A rejected registration is retried while not-ready.
    - Owner silence: it resets on each owner message type (validation-passing owner RegisterMeeting/EndMeeting, OWNED, RELEASED); it is NOT reset by transport errors, timeouts, FAILED_PRECONDITION, UNSPECIFIED verdicts or invalid registrations; it fires exactly at the timeout, closing sessions HANDLER_RELEASED with no Disconnected and counting trigger owner_silence; a takeover registration resets it; startup refuses 299 and 3601 and accepts 300 and 3600.
    - The idle timeout config is honoured by build_transport_config, and the ratio validation rejects a too-small timeout.

    ENV-TESTS (Covers: env-tests; paired with test)
    crates/env-tests/tests/01_mh_deployment_config.rs covers MH_MAX_IDLE_TIMEOUT_MS, MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS and the retired constant. Owner-silence firing is covered deterministically only (paused time); a live leg would need a 300 s+ wait. The live planned-drain legs (mh.sh up + drain, with and without a target) belong to MC-F only; this task's owner reviews them.

    SAME COMMIT
    Removing the settle cap together with the new drain loop. Each config key together with its manifest wiring, runbook row and RETIRED_KEYS entry if one applies. Every new series together with its panel and catalog entry.

    CROSS-TASK OBLIGATIONS
    (a)–(e) as listed in the story plan. Tier: full (ADR-decided shutdown semantics, the GC seam).
  slug: 2026-11-11-mh-drain-owner-silence
  tag: story-2026-10-31-survive-handler-loss-task-MH-3
- id: 21
  status: completed
  specialist: meeting-controller
  deps:
  - 13
  - 15
  - 18
  prompt: |-
    MC-B2: Detection (assess), Dead/Impaired declaration, removal; zombie env-test leg

    Specialist: meeting-controller. Pair with test (zombie leg, test gates); media-handler reviews the zombie leg.
    Depends on: MC-B1 (heartbeat intake, per-pair liveness state, verdicts, binding gate), MH-2 (MH per-meeting heartbeat sender and RELEASED handling — detection must not land before MHs heartbeat, or every handler reads Dead at N), T-1 (MH pool fixture and freeze/resume injector).
    Governing design: ADR-0039 §2 (assess, states, ownership), §1 (apply_refusal mapping), §6 (removal, cleanup invariant, EndMeeting cause, binding release order).
    Requirements satisfied: R-1 (survivor arm), R-4, R-10, R-39 (zombie leg).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (its section for this task is reproduced below). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    The MC meeting actor decides, per (meeting, handler), whether a handler is healthy, lagging, impaired, dead or draining, and on a Dead/Impaired transition removes the handler from the meeting completely. Replacement is MC-C: here a meeting with a surviving handler continues on it; a meeting left with none waits (MC-C adds acquisition).

    CONCRETE CHANGES
    1. New crates/mc-service/src/media_routing/handler_health.rs: pure assess(now, observations, previous_state, cfg) -> HandlerHealth, no I/O, no timers. Inputs: last accepted receipt, applied-vs-sent generation and when the lag started, apply_refusal, serving_state, client loss reports (input only, weight 0 in this story; MC-E computes the metric).
       - Dead = entry age > N, the clock starting at the first confirmed RegisterMeeting for that pair (an unconfirmed handler never goes Dead before that). One-way.
       - Lagging (reversible): applied < sent. While Lagging, MC re-publishes the current snapshot to that handler's push worker (replaces the re-assert's self-correction on a live MH).
       - Impaired (one-way) when the lag persists longer than L = 2N (derived from MC_MH_HEARTBEAT_TIMEOUT_MS, no new key), or on a load-dependent apply_refusal STREAM_CEILING or EDGE_CAP.
       - INVALID / TRANSPORT_INCONSISTENT refusals are MC contract faults: loud ERROR, counted, never a replacement.
       - Draining from serving_state: recorded and exposed for MC-C's planned path; no removal here.
       - gRPC channel connectivity events are NOT an input.
    2. actors/meeting.rs: call assess on heartbeat arrival and on a tokio sleep_until arm for the earliest deadline (same pattern as settle_wake); act only on transitions.
    3. Removal on Dead/Impaired (§6 "MC: forget it"), in this order:
       - drop the handler from the set and write it to the removed record (one-way; MC-A structure), bump handler_set_version;
       - purge its keys from every participant's connectivity and recompute through the normal render, so edges re-place on surviving shared handlers with no special case;
       - push the set without it to every client (MC-D tokens for remaining members);
       - only then bulk-release its HandedOutBindings (media_admission/epoch.rs);
       - move its push worker out and drain it (media_routing/teardown.rs quiesce, restricted to one handler), then send EndMeeting(old, cause) best-effort with the ReleaseCause (HEARTBEAT_TIMEOUT or APPLY_LAG) in EndMeetingRequest.cause; a failed EndMeeting never blocks removal and is never retried (the next heartbeat's RELEASED is the retry);
       - evict policy_generations for the pair.
       From then on MC-B1's verdict branch answers RELEASED(trigger) and the binding gate refuses the handler.
    4. Meeting teardown (actors/meeting.rs hand_off_teardown) releases only the CURRENT set; removed handlers were already ended. Fill EndMeetingRequest.cause = MEETING_ENDED there.

    TESTS (test-gates.md §MC; Tier 1a pure + Tier 1b paused time, fake MHs)
    - assess table: Healthy⇄Lagging reversible; Lagging held > L → Impaired; age > N → Dead; age clock from first confirmed RegisterMeeting; stale lower generation not liveness; client reports alone never reach Dead; DRAINING → planned (no removal); STREAM_CEILING/EDGE_CAP → Impaired; INVALID/TRANSPORT_INCONSISTENT → no replacement, loud.
    - Apply-failure gate carried over from ADR-0036 §10: inject an MH apply failure; applied generation does not advance; Lagging → Impaired; a catch-up before L stays Healthy.
    - Removal ordering: set-without-X sent before bulk binding release; (X, conn) keys survive Lagging/Impaired/Draining; EndMeeting carries the cause; failed EndMeeting → removal completes, no retry loop, next heartbeat RELEASED.
    - Two-handler meeting, H1 stops beating: edges re-place onto H2, nobody reported unreachable who shares H2.
    - Zombie Disconnecteds after removal → handler_not_in_set, the survivor's connectivity untouched.
    - Env-test in crates/env-tests/tests/40_resilience.rs (feature resilience; every test in this binary is #[serial] per T-1's isolation convention; pair with test; uses T-1's fixture, restores baseline, waits derived from mc_media_handler_heartbeat_deadline_seconds; failure-only ceilings; process kill from a debug container, never force-delete):
      - crash survivor leg: two-handler meeting, kill one MH process → declarations_total{trigger="heartbeat_timeout"} rises and media continues on the survivor; mc_media_handler_heartbeats_total{outcome="owned"} rose before the kill. GC assertions (GC asked): after the killed MH restarts in place, gc_mh_registrations_total{outcome="superseded",match="both"} rises by 1 and gc_mh_load_reports_total{outcome="unknown_handler"} is unchanged.
      - zombie leg: freeze an MH (SIGSTOP via the injector, which proves the freeze took effect or fails loudly), wait for MC's declaration, resume → its next heartbeat gets RELEASED(heartbeat_timeout), MH ends the meeting closing sessions with HANDLER_RELEASED, no Disconnected sent; deterministic-only fallback if the injector reports unavailable.

    TELEMETRY (telemetry-contract.md §MC)
    - mc_media_handler_declarations_total{trigger}
    - mc_media_handler_suspicions_total{outcome=declared|cleared}
    - mc_media_handler_detection_seconds{trigger}
    - mc_media_meetings_without_handlers (recompute-and-set gauge: meetings with zero live handlers, "awaiting handler"; refreshed on the controller check_meeting_health walk like MutedSourceCensus). A removal can leave a meeting with none already in this task; MC-C adds recovery.
    - Declarations are per (meeting, handler); the handler-level census is MC-E.
    - Log one INFO per health transition: meeting_id, handler_id, trigger, signals (ids in logs only).

    SAME-COMMIT OBLIGATIONS: (a) no new config key (L derived); document L = 2N beside N in docs/runbooks/mc-deployment.md; (b) n/a; (c) each series with panel + catalog + zero-init + ADR-0032 test; (d) test-gates items; (e) no id labels; transitions logged once with meeting_id, handler id and cause in the log line only.

    Tier: full (architectural seam, cross-service behaviour).
  slug: 2026-11-12-mc-handler-detection-removal
  tag: story-2026-10-31-survive-handler-loss-task-MC-B2
- id: 22
  status: completed
  specialist: protocol
  deps:
  - 12
  - 17
  - 19
  - 18
  prompt: |-
    Task P3: Retire superseded signaling/connect surfaces

    Specialist: protocol. Tier: full (wire deletions across MC, MH-adjacent SDK code and generated TS).
    Pairing: pair with client, who reviews the SDK and test edits. Meeting-controller reviews the MC population-line removals. proto/ is a GSA owned by protocol.
    Depends on (every consumer move must have landed first):
    - MC-A: MC emits MediaHandlerSet and handler_ref on SendTarget/StreamAssignment;
    - C1: the SDK reads only refs and the set, and populates MhConnectRequest.media_dial_token;
    - MH-1: MH accepts media_dial ONLY, and the Rust env-test MH connect (participant.rs reach(), mc_session) already uses media_dial_token;
    - T-1: env-test reads of JoinResponse.media_servers have moved onto MediaHandlerSet.
    MC-D is transitively in place through C1: tokens are populated, and MC consumes MhConnectionStatus by handler_ref.
    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md (Accepted), sections 4 and 5.
    Requirements satisfied: R-36 (these surfaces), R-17 (handlers named only by reference; addresses only in the set).

    GOAL
    This is the ONE task that owns the invariant "these superseded surfaces are gone". P2, MC-A, MC-D, C1 and MH-1 added the replacements alongside them and moved every consumer. This task deletes every remaining instance, from wire, generated code, production code, tests, env-test fixtures and docs, in one commit. No dual path survives it, and nothing is renamed to "legacy".

    DELETE (signaling.proto), adding `reserved` for each vacated tag AND name, per docs/protocol/CONVENTIONS.md Rule 5 (required by buf breaking WIRE_JSON; this is gate compliance, not compatibility):
    1. JoinResponse.media_servers (tag 4), superseded by JoinResponse.media_handler_set. The NON-AUTHORITATIVE text already lives on MediaHandlerSet (P2), so remove the pointer.
    2. SendTarget.media_handler_url, superseded by SendTarget.handler_ref.
    3. StreamAssignment.media_handler_url, superseded by StreamAssignment.handler_ref. That message's comment documents a deliberate name-reservation exception for this name (the second of two in the file). Now that the name is vacated, reserve the name too and remove the exception text, plus the cross-reference in ServerMessage's comment that counts the exceptions.
    4. MhConnectionStatus.mh_url, superseded by MhConnectionStatus.handler_ref. Remove its 256-byte truncation text, and the MediaConnectionUpdate / limits text that referred to it.
    5. MhConnectRequest.join_token, superseded by MhConnectRequest.media_dial_token. Remove the "same semantics as JoinRequest.join_token" text.
    CAUTION: JoinRequest.join_token (the client->MC join credential) is NOT retired. Only MhConnectRequest.join_token goes. Update crates/proto-gen/build.rs's MhConnectRequest skip_debug note to name media_dial_token.
    Sweep every comment in both protos (and MediaServerInfo's "Same spelling as SendTarget.media_handler_url and StreamAssignment.media_handler_url" note) so no text names a deleted field except the reserved lines.

    REMOVE THE NOW-DEAD CODE IN THE SAME COMMIT (mechanical: every consumer has already moved, so nothing here changes behaviour)
    - MC population of the deleted fields:
      - crates/mc-service/src/webtransport/connection.rs (JoinResponse media_servers);
      - media_signaling/directive.rs (SendTarget url);
      - media_signaling/assignments.rs (StreamAssignment url);
      - any MhConnectionStatus.mh_url read left in connection.rs (handle MediaConnectionUpdate);
      - related helpers in actors/participant.rs, actors/messages.rs, actors/meeting_media.rs, media_routing/placement.rs, observability/metrics.rs that exist only to build or validate those urls.
      If anything there still carries behaviour, STOP and report it: that would mean a consumer move is incomplete.
    - SDK: the MhConnectRequest.join_token population line (packages/sdk-core/src/media/MediaTransport.ts), and any remaining reference to the deleted fields in packages/sdk-core/src (e.g. validation/limits.ts mh_url cap, errors/MediaConnectionError.ts) and packages/web-app/src. Test helpers and fixtures follow (signaling/__tests__, media/__tests__, session/__tests__, web-app __tests__).
    - Env-test fixtures: T-1 and MH-1 have already moved the reads. Delete any remaining reference to the deleted fields (crates/env-tests/src/fixtures/mc_session.rs, participant.rs, collector.rs), including the join_token population on the MH connect. gc_client.rs uses JoinRequest.join_token, which STAYS.
    - Regenerate TS (packages/proto-gen/scripts/codegen.sh) into packages/sdk-core/src/proto.
    - If grep finds a reference outside these paths, it belongs to this task too (cut by invariant: no "everywhere except X").

    TESTS
    - packages/proto-gen/scripts/verify-codegen.sh ABSENCE checks, using its existing helpers (assert_message_field_absent / assert_not_generated): JoinResponse.media_servers, SendTarget.media_handler_url, StreamAssignment.media_handler_url, MhConnectionStatus.mh_url, MhConnectRequest.join_token. Plus a PRESENCE check that JoinRequest.join_token still exists, which guards the caution above.
    - crates/proto-gen/tests/signaling_roundtrip.rs: drop the roundtrips of the deleted fields, and assert the reserved tags decode as unknown (no field).
    - The process_start_epoch_ms and connection_id legacy-empty absence items in test-gates.md's Protocol section belong to the tasks that delete those surfaces, not here.
    - Layer 1–7 green, including Layer 7 env-tests (the fixtures above) and the browser specs that read the debug panel or e2e bus (R-20 shows refs and addresses from the set).

    CROSS-TASK OBLIGATIONS
    (a) No config keys.
    (b) Remove every catalog, panel, alert and runbook reference to the deleted fields in the same commit: grep docs/ (docs/API_CONTRACTS.md, docs/WEBTRANSPORT_FLOW.md, docs/observability/, docs/runbooks/) and infra/ for media_servers, media_handler_url, mh_url and MhConnectRequest join_token.
    (c) No series added. If an MC or SDK metric existed only for a deleted field (e.g. an mh_url truncation counter), remove its emission, catalog row, panel and zero-init together.
    (d) Test-gates Protocol absence checks, as above, paired with test.
    (e) No ids become labels.
  slug: 2026-11-12-retire-superseded-signaling-surfaces
  tag: story-2026-10-31-survive-handler-loss-task-P3
- id: 23
  status: completed
  specialist: meeting-controller
  deps:
  - 11
  - 16
  - 17
  - 19
  - 18
  - 21
  prompt: |-
    MC-C: Crash replacement via GC

    Specialist: meeting-controller. Pair with test (env-tests) and global-controller (consumer review of AcquireMediaHandler).
    Depends on: GC-3 (AcquireMediaHandler rpc + messages, RegisterMCResponse.max_exclude_handler_ids populated), MC-D (tokens), C1 (SDK follows sets by ref — no mid-meeting add before clients follow), MH-1 (media_dial-only admission), T-1 (MH pool fixture, mh.sh from env-tests), MC-B2 (detection and removal).
    Governing design: ADR-0039 §3 (removal first, AcquireMediaHandler, statuses, bounds, awaiting handler, register-before-token), §4 (crash = break-before-make, recovery is the join path). Planned transfer is MC-F, which builds on this task.
    Requirements satisfied: R-1, R-2, R-6, R-23 (MC side).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (its section for this task is reproduced below). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    After a removal (MC-B2), MC obtains a replacement handler from GC and moves the meeting onto it through the normal join path. Removal never waits on GC. This task covers the heartbeat_timeout and apply_lag triggers; DRAINING/planned transfer is MC-F. Build the acquisition and Registering→Active path so MC-F reuses it unchanged.

    CONCRETE CHANGES
    1. crates/mc-service/src/grpc/gc_client.rs:
       - read RegisterMCResponse.max_exclude_handler_ids; 0 or absent fails MC registration (same treatment as the heartbeat intervals at ~line 375).
       - AcquireMediaHandler {meeting_id, controller_id, exclude_handler_ids, trigger, replaced_handler_id} → MhAssignment. exclude = current ∪ removed ∪ refused-at-admission (handlers whose RegisterMeeting after a grant failed), by id. replaced_handler_id is label context only.
    2. Meeting actor migration state machine (actors/meeting.rs; never blocks a turn):
       - on a removal that leaves the meeting below its handler count, spawn the Acquire call as a task tagged with the current handler_set_version; the result returns as a mailbox message; a grant for a superseded version is discarded (never registered, nothing leaks).
       - a grant enters the set as Registering (heartbeats OWNED, binding gate admits), gets a push worker and RegisterMeeting (with heartbeat_interval_ms and media_dial_public_key), becomes Active on ack, then appears in clients' MediaHandlerSet with tokens (MC-A/MC-D machinery). A RegisterMeeting that fails after retries → the handler is removed, added to refused-at-admission, and acquisition continues.
       - Statuses: OK → migrate. RESOURCE_EXHAUSTED or UNAVAILABLE/DEADLINE_EXCEEDED → stay on survivors (or awaiting handler if none), retry with bounded jittered backoff (config-derived, no new literal). FAILED_PRECONDITION and INVALID_ARGUMENT → no retry, loud, counted.
       - Bounds (§3, user ruling: NO per-MC limiter): per-meeting minimum interval between replacements, and a per-meeting attempt bound = max_exclude_handler_ids − current set size (default cap 16 ≈ 14 lifetime replacements). A meeting at its bound stops acquiring, counted rate_limited, staying on survivors or awaiting handler.
       - Awaiting handler: a meeting with zero live handlers (counted on MC-B2's mc_media_meetings_without_handlers gauge) keeps acquiring with backoff and recovers by itself on a later grant.
    3. Crash path = break-before-make (MC-B2 already sent the set without the dead handler; the replacement appears once Active). Recovery is the join path: participants connect, MH sends Connected, settle, normal render places edges on the new handler — the stranded A/B pair hears each other with no special case.

    TESTS (test-gates.md §MC; paused time; GC mock; fake MHs)
    - Stranded A/B: set {H1(,H2)}, A and B only on H1; H1 stops beating; GC mock grants H3; H3's RegisterMeeting precedes the client MediaHandlerSet naming H3; after A and B send Connected on H3, H3's snapshot carries the A↔B edges and neither is reported unreachable to the other. Assert on snapshots.
    - Two-handler variant with GC UNAVAILABLE: survivors re-place without GC.
    - Per GC outcome: OK migrates; RESOURCE_EXHAUSTED/UNAVAILABLE with survivors → stay, back off, counted; with none → awaiting handler counted, later grant completes recovery; FAILED_PRECONDITION/INVALID_ARGUMENT → no retry, loud; bounded attempts when the replacement is also bad; late grant for a superseded version discarded.
    - cap 0/absent → MC registration fails; positive arm with cap 16.
    - Env-tests in crates/env-tests/tests/40_resilience.rs (feature resilience; every test in this binary is #[serial] per T-1's isolation convention; T-1 fixture; restore baseline, no leaked owned MH, fail loudly on foreign pool activity; Prometheus scrape wait before MH baselines; waits from mc_media_handler_heartbeat_deadline_seconds; failure-only ceilings):
      - crash with replacement: GC's initial count is 2, so kill one MH process in a two-handler meeting (a survivor remains); MC requests a replacement after any removal → each participant's final set holds a handler_ref not present before, media resumes, mc_media_handler_migrations_total{outcome="completed"} rises, and gc_mh_replacement_requests_total{outcome="granted"} rises (GC asked).
      - stranded pair (Rust).
      - assert the address invariant after each leg (fixture helper).

    TELEMETRY (telemetry-contract.md §MC)
    - mc_media_handler_migrations_total{trigger, outcome=completed|no_capacity|gc_unavailable|admission_refused|rate_limited|timed_out|abandoned}
    - mc_media_handler_migration_duration_seconds{trigger, phase=replacement|registration|reconnect}
    - Catalog and zero-initialise the full trigger vocabulary (heartbeat_timeout, apply_lag, planned_transfer) and every outcome now, so MC-F only starts emitting planned_transfer values without a catalog change.
    - Publish the replacement retry budget as a config gauge (the awaiting-handler page bound in OBS-ALERTS must not use a literal).
    - Log the migration start and its terminal outcome: meeting_id, old and new handler_id (logs only).

    SAME-COMMIT OBLIGATIONS: (a) any backoff/interval config key gets its Configuration Reference row; (b) n/a; (c) series with panel + catalog + zero-init + ADR-0032 test; (d) test-gates items; (e) no id labels.

    Tier: full (>1 service interplay, architectural seam).
  slug: 2026-11-12-mc-crash-replacement-via-gc
  tag: story-2026-10-31-survive-handler-loss-task-MC-C
- id: 24
  status: completed
  specialist: meeting-controller
  deps:
  - 23
  - 20
  prompt: |-
    MC-F: Planned transfer (make-before-break on a draining handler)

    Specialist: meeting-controller. Pair with test (env-tests); media-handler reviews (drain interplay).
    Depends on: MC-C (acquisition via GC, Registering→Active for a grant, migration series), MH-3 (MH reports serving_state DRAINING with drain_remaining_ms on SIGTERM, keeps forwarding until it holds no meetings or its deadline).
    Governing design: ADR-0039 §4 (planned transfer ordering, survivor target, timed_out, shed), §7 (drain on SIGTERM), §3 (removal and register-before-token reused from MC-C).
    Requirements satisfied: R-3 (MC side).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (MC-C's migration series, now emitted for trigger=planned_transfer). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    When a handler announces it is draining (graceful shutdown, mh.sh drain, upgrade, scale-down), MC moves each of its meetings off it make-before-break: the set adds the new handler, then drops the old one, and clients are told to drop it before it ends the meeting. A small blip only: no reconnecting state, no transport fault, no session shed, no observation on the required loss metric.

    CONCRETE CHANGES
    1. Trigger: MC-B2's assess already yields Draining from the heartbeat's serving_state; on that transition the meeting actor starts a planned transfer with trigger PLANNED_TRANSFER. The old handler stays in the set and keeps serving meanwhile; it is NOT removed on the transition.
    2. Target: acquire through MC-C's path (AcquireMediaHandler, trigger PLANNED_TRANSFER, replaced_handler_id = the draining handler, the same per-meeting interval and attempt bound). The grant enters as Registering, gets RegisterMeeting, becomes Active, and is added to clients' MediaHandlerSet with tokens (set {old} → {old, new}).
    3. Wait until every routable participant is MH-reported Connected to the new handler, OR until the cutover deadline = drain_remaining_ms (from the draining heartbeat) − MC_PLANNED_TRANSFER_MARGIN_MS.
    4. Remove the old handler, in this order (§4): set without it to clients FIRST (so clients close it as a directed removal, not a loss); then drain its push worker; then EndMeeting(old, PLANNED_TRANSFER) best-effort; bulk binding release only after the set was sent. Edges move at this removal (ADR-0036 §9 edge stability keeps them on the old handler while both parties are on it), so the blip is one push, directive change and keyframe.
    5. No grant (RESOURCE_EXHAUSTED or GC unavailable): the surviving handlers are the target (clients already dial the whole set); remove the old handler once every routable participant is on a survivor, or at the cutover deadline.
    6. Stragglers not on the target at the cutover deadline are cut over anyway, outcome timed_out. Sessions are shed only when there is neither a survivor nor a grant.
    7. A planned transfer produces no observation on the required loss metric (MC-E asserts this unobserved).
    8. Config: MC_PLANNED_TRANSFER_MARGIN_MS, default 5000, startup-validated to [1000, 30000], published as a config gauge (lead ruling; ADR-0039 names the margin without a value). Required configMapKeyRef; Configuration Reference row in docs/runbooks/mc-deployment.md with default, range and its relation to MH's drain deadline (grace − margin).

    TESTS (test-gates.md §MC "Planned ordering"; paused time; GC mock; fake MHs; positive arm for every negative)
    - Ordering: add Y → clients connected to Y → set without X sent → pusher drain → EndMeeting(X, PLANNED_TRANSFER); assert each step's order on observed messages.
    - Cutover at drain_remaining − margin with a straggler: removal happens, outcome timed_out.
    - No grant: survivor target; old handler removed once all routable participants are on the survivor; shed only with neither a survivor nor a grant.
    - Config: 999 and 30001 rejected; 1000/30000 accepted; zero/absent fails startup.
    - No loss-metric observation for a planned transfer.
    - Env-tests in crates/env-tests/tests/40_resilience.rs (feature resilience; every test in this binary is #[serial] per T-1's isolation convention; pair with test; T-1 fixture; restore baseline, no leaked owned MH, fail loudly on foreign pool activity; failure-only ceilings derived from deployed config; address invariant asserted after each leg):
      - planned transfer with a target (`mh up` then `mh drain` on a handler in the meeting): set {old} → {old,new} → {new}, no session shed, mc_media_handler_migrations_total{trigger="planned_transfer",outcome="completed"} rises.
      - planned transfer without a target (no free capacity for a grant): participants move to the survivor, outcome completed, no shed.

    TELEMETRY (telemetry-contract.md §MC)
    - Emit MC-C's mc_media_handler_migrations_total and mc_media_handler_migration_duration_seconds for trigger="planned_transfer" (already catalogued and zero-initialised by MC-C); outcome timed_out is the drain-shed signal OBS-ALERTS reads.
    - mc_media_handler_declarations_total{trigger="planned_transfer"} at removal (MC-B2's series).
    - Config gauge for MC_PLANNED_TRANSFER_MARGIN_MS.
    - Log the transfer start and terminal outcome: meeting_id, old and new handler_id (logs only).

    SAME-COMMIT OBLIGATIONS: (a) MC_PLANNED_TRANSFER_MARGIN_MS Configuration Reference row; (b) n/a; (c) the config gauge with panel + catalog + zero-init + ADR-0032 test; (d) test-gates items above; (e) no id labels.

    Tier: full (cross-service behaviour with MH drain, architectural seam).
  slug: 2026-11-12-mc-planned-transfer
  tag: story-2026-10-31-survive-handler-loss-task-MC-F
- id: 25
  status: completed
  specialist: meeting-controller
  deps:
  - 17
  - 21
  prompt: |-
    MC-E: Required loss metric, client-loss disagreement, HandlerCensus

    Specialist: meeting-controller. Pair with observability (series shape) and test.
    Depends on: C1 (SDK sends MhConnectionStatus with handler_ref + handler_set_version on every transition), MC-B2 (Dead/Impaired declarations). MC-D already records the latest status per (participant, ref).
    Governing design: ADR-0039 §8 and docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md §MC; brief.md "Required metric".
    Requirements satisfied: R-33, R-34 (MC own-path and loss series).
    Field deletions are NOT in this task (P3 owns them).

    Telemetry obligations: the story file (docs/user-stories/2026-10-31-survive-handler-loss.md) §Observability "Per-task telemetry obligations" (its section for this task is reproduced below). Every series carries key_custody="operator" and no meeting, participant, connection, handler_id, handler_ref, slot or revision label; env-test 32 stays green.

    GOAL
    At every dead or impaired declaration an operator can read how many of the clients MC believed connected to that handler had already reported losing it — the evidence for promoting client connectivity to a trigger later — and MC exposes a handler-level own-path signal.

    CONCRETE CHANGES
    1. At each declaration with trigger heartbeat_timeout or apply_lag (never planned_transfer), from ONE snapshot taken at the transition in the meeting actor:
       - denominator: clients MC believed connected to that handler (MH-reported connectivity, not client reports);
       - numerator: of those, participants whose LATEST status for that handler_ref is a loss received while the ref was in the set (MC-D's state; a later CONNECTED after redial supersedes; a report after removal is ignored; version is correlation-only);
       - counted at MC receipt time, never client time.
    2. Client-loss disagreement: when at least a configured fraction AND a configured minimum count of a handler's believed-connected clients report loss while its heartbeats stay healthy for at least N, count it once per episode. This is assess's client second-opinion output (crates/mc-service/src/media_routing/handler_health.rs from MC-B2); it never triggers removal in this story. The fraction and minimum count are config, published as gauges: `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` default 0.5, range (0, 1]; `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` default 3, ≥ 1; both hard-fail startup validation (lead ruling; the contract gives no values).
    3. HandlerCensus (MutedSourceCensus pattern, actors/meeting_media.rs): an MC-wide std-Mutex structure, recompute-and-set, never deltas:
       - each meeting actor writes its absolute set of handler ids on every membership change;
       - the controller's check_meeting_health walk prunes dead meetings and re-sets mc_media_handlers_in_use (distinct handlers across this MC's meetings);
       - a bounded recently-declared set, touched only at declaration and pruned when a handler leaves every meeting's set, increments mc_media_handler_first_declarations_total{trigger} once per handler per MC however many meetings declare it.
       Handler ids never become labels. No await; touched only at membership change and declaration.

    TESTS (test-gates.md §MC loss metric; ADR-0032 MetricAssertion, unobserved semantics for "never")
    - k of n injected loss reports → count and ratio delta; only the latest status per ref counts; a report after removal is ignored; positive arm: an unsuperseded DISCONNECTED is counted.
    - Two set members sharing a URL are attributed by handler_ref.
    - A PLANNED_TRANSFER declaration produces no observation (unobserved).
    - Disagreement: fraction+min reached with healthy heartbeats → counted once; below either threshold → not.
    - Census: one handler declared by three meetings → first_declarations +1; handlers_in_use recomputed after a meeting ends.
    - Env-test (pair with test): extend MC-B2's crash leg in crates/env-tests/tests/40_resilience.rs to assert the loss-metric observation count rises (rises only, never absolute values).

    TELEMETRY (telemetry-contract.md §MC)
    - mc_media_handler_loss_clients_total{reported=before_declaration|not_reported}
    - mc_media_handler_loss_prior_report_ratio (histogram, buckets 0, .1, .25, .5, .75, .9, 1)
    - mc_media_handler_loss_client_lead_seconds (observed only when ≥1 client reported)
    - mc_media_handler_client_loss_disagreements_total
    - mc_media_handler_first_declarations_total{trigger}
    - mc_media_handlers_in_use (gauge)
    - config gauges for the disagreement fraction and minimum count.

    SAME-COMMIT OBLIGATIONS: (a) the disagreement fraction and minimum-count keys get Configuration Reference rows in docs/runbooks/mc-deployment.md (defaults as stated above); (b) n/a; (c) each series with panel (infra/grafana/dashboards/mc-media.json) + catalog (docs/observability/metrics/mc-service.md) + zero-init + ADR-0032 test; (d) test-gates items; (e) no meeting/participant/handler labels.

    Tier: light — MC-internal instrumentation over state MC-D and MC-B2 already maintain; no wire, crypto or cross-service change.
  slug: 2026-11-13-mc-handler-loss-metric
  tag: story-2026-10-31-survive-handler-loss-task-MC-E
  tier: light
  tier_reason: 'MC-only telemetry over existing inputs: no wire or proto change (P3 owns deletions), no auth/crypto, no GSA, one service'
- id: 26
  status: pending
  specialist: client
  deps:
  - 17
  prompt: |-
    C2: Migration UX, receive-side continuity, and all client telemetry for MH replacement (R-42 series plus the per-slot media-dark timer).

    Specialist: client. Pair with security (the key-continuity invariant) and test (component and unit gates). Tier: full (it pins a crypto nonce-reuse invariant).

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md section 4 ("Slot states", "Keys", the empty-set rule) and section 8. Supporting files: docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md section "Client" (frozen) and test-gates.md sections "Client" and "Manual plan".

    Requirements satisfied: R-16, R-20, R-18 (the UI half of the empty-set handling), R-42 (all of it).

    Dependency: C1, landed first. It provides the reconcile-to-set transport with per-ref lifecycle events (lost, removed, reconnected), the empty-set session event and notice, and the bus handlerSets history. Depended on by: C3 and OBS-ALERTS.

    WHAT CHANGES

    1. Client-local "reconnecting" slot state (ADR section 4, "Slot states"). There is NO new wire SlotState.
    - A slot whose assigned handler ref this client has OBSERVED lost (transport closed with no code, or HANDLER_RELEASED while still in the set), or that MC removed from the set, renders "reconnecting". That is a client-observed fact, never inferred from silence (ADR-0036 section 6).
    - The slot keeps its previous occupant as a reconnecting avatar. It is never a frozen live video frame and never a vanished participant.
    - It clears when a StreamAssignments gives the slot a source on a live handler, or when a new clientConfig cap expires. On expiry it shows what the latest assignment says.
    - A planned transfer (the set adds the new handler, edges move, then the old handler is removed after the client is connected to the new one) must NOT show reconnecting. The removal of a ref whose slots have already moved to another connected handler sets no overlay.
    - SDK: emit the per-slot reconnecting state through the session event surface (session/events.ts) and the media store. sdk-svelte: stores/MediaStore.svelte.ts and stores/mediaCells.svelte.ts per-slot cells. web-app: one new row in the exhaustive table lib/slotState.ts, rendered in views/VideoSlotTile.svelte and the audio rows in views/InMeeting.svelte.
    - C1's removal-cleanup function gains "clear the reconnecting state on its slots".

    2. Empty set UI (ADR section 4, L4-2). Render "connecting media" while the set is empty, and C1's single notice as a non-fatal notice (views/InMeeting.svelte, alongside views/ResumeNotice.svelte's pattern). Copy goes in lib/errorText.ts or lib/slotState.ts as appropriate.

    3. Debug panel (R-20). views/DebugPanel.svelte with lib/debugStats.ts shows the current handler_set_version and each handler's ref and URL, never tokens. It stays local-only (tests/debugNeverExported.test.ts keeps passing).

    4. e2e bus (test-gates section "Browser"): lib/e2eBus.ts records reconnecting overlay set and clear per slot id, plus transportFault events. C3 uses them for the planned leg's negative assertion and the crash leg's positive arm. Update __tests__/e2eBus.test.ts.

    5. dt_client_media_resume_ms{media_kind, content_kind, outcome=resumed|reassigned_away|abandoned} (telemetry-contract section "Client").
    - Measured per slot, only for slots Active (with a source) when the episode starts.
    - Episode start = this client observing loss of the transport carrying the slot, or a set change removing that handler.
    - End = the first decoded video frame or played audio frame for the slot via any handler other than the lost or removed one.
    - outcome reassigned_away: the next assignment gives the slot no source or a different sender. outcome abandoned: the session ended, or the published clientConfig abandonment cap was reached.
    - Solo or receive-only clients with no Active slot start no episode.
    - Labels through media/setup/mediaMetrics.ts.
    - Lands with the catalog entry (docs/observability/metrics/client.md), collector allowlist (infra/services/otel-collector/collector.yaml), GC filter (crates/gc-service/src/services/telemetry_filter.rs), export-guard fixture (crates/dt-guard), panel (infra/grafana/dashboards/client-media.json), zero-init and ADR-0032 test.

    6. Replay cross-handler reason token (telemetry-contract section "Client"). The ingress replay drop (media/pipeline/ingress.ts and media/pipeline/videoIngress.ts, with receivePolicy and frameAccounting) gains one bounded reason token for a duplicate whose first copy arrived on a DIFFERENT transport: the make-before-break overlap. The replay window records the arriving transport per entry. Extend the branch table __tests__/receivePolicy.branchTable.test.ts. Any future replay alert must exclude this token; note that in the catalog.

    7. R-42 connection and disruption series (telemetry-contract.md section "Client", frozen; carry the exact names). Moved here from C1 by the 2026-11-01 resize. They are driven by C1's per-ref lifecycle (dial start and outcome, close code, directed removal, loss, redial).
    - dt_client_mh_connection_total{phase=join|added|redial, outcome=connected|wrong_handler|binding_conflict|mc_unavailable|handler_released|declined|transport_failure|timeout|aborted_by_removal|contract_fault}. One per dial, every dial. Outcomes projected from MhCloseCode by a total match.
    - dt_client_time_to_first_mh_connected_ms (join to first MH connected).
    - dt_client_mh_dial_duration_ms{phase, outcome=connected|failed}.
    - dt_client_mh_disconnects_total{cause=transport_loss|directed_removal|handler_released|wrong_handler|binding_conflict|mc_unavailable|declined|contract_fault}. One per end of an ESTABLISHED session. The lost-handler count is cause in {transport_loss, handler_released}.
    - dt_client_mh_reconnect_duration_ms{outcome=reconnected|removed|abandoned}, from loss of an in-set handler; the abandonment cap is a published clientConfig value.
    - Both existing series leave the ADR-0036 section 11 grandfathered join-flow roster: drop meeting_id_hash, status and mh_index_bucket, build labels through media/setup/mediaMetrics.ts (mediaMetricLabels: client_version, org_id, key_custody plus the listed labels), and EXPORT them.
    - Same commit:
      - flip the MediaTransport.ts spread comment (around 618-653) and its pins (media/__tests__/media-transport.test.ts, session/__tests__/meeting-session.test.ts, __tests__/joinLabelSpread.test.ts, telemetry/__tests__/nameGuard.test.ts, telemetry/__tests__/sinks.test.ts);
      - update docs/observability/metrics/client.md (entries and the frozen roster);
      - add dt_client_mh_ to the media-path-prefixes block in docs/observability/label-taxonomy.md (around 665) and the hygiene kernel const copy;
      - add the collector allowlist entries (infra/services/otel-collector/collector.yaml, around 55-75, including its "not a prefix rule" comment);
      - add GC's filter (crates/gc-service/src/services/telemetry_filter.rs plus tests/telemetry_proxy_tests.rs);
      - add export-guard fixtures (crates/dt-guard/src/client_metrics_export.rs, crates/dt-guard/tests/client_metrics_export_fixtures.rs);
      - add panels on the client dashboard (infra/grafana/dashboards/client-media.json);
      - add zero-init and ADR-0032 tests (dt-guard application-metrics checks 3-6).
    - The telemetry-contract.md section "Client" already holds these names. Observability owns it; change it only with observability's agreement.
    - Non-member handler ref (ADR section 4: non-member counted and ignored; C1 ignores it and WARNs once): add one bounded count for it, with its name and label agreed with observability and added to telemetry-contract section "Client" in this commit, using the same export path.

    8. Continuity tests (ADR section 4 Keys, R-16; security pairs):
    - A handler set change, both an add and a remove, never rebuilds SessionMedia or TransmitKeyManager (session/mediaStart.ts).
    - The key id strictly increases across a swap after generation 0, so no (sender, stream, generation) is reused under one KEK, because a rebuild would reuse wrap nonces.
    - A newly named egress target starts awaiting_group and the keyframe policy keys a group on every target (media/pipeline/videoEgress.ts, media/lifecycle/keyframePolicy.ts), so video on the new handler resumes from a keyframe.
    - A frame arriving via two handlers during an overlap is decoded and played once.
    - A slot's decoder, jitter buffer and replay window survive the swap (media/lifecycle/VideoReceiver.ts slot pipelines are keyed by slot id).

    TESTS: the unit and component tests above, every R-42 series with its labels, zero-init and ADR-0032 semantics (the dial outcome projection is total over MhCloseCode, including zero or unknown as contract_fault), on MockWebTransport with an injected clock and no sleeps; sdk-svelte store granularity (__tests__/MediaStore.test.ts); web-app component tests for the reconnecting and connecting-media states. Every negative has its positive arm. This task adds no Layer-7 spec; C3 exercises it live.

    SAME COMMIT: SDK events, stores, web-app rendering and copy, the bus events, every R-42 series and resume_ms with the full export path (the media-path prefix block plus the hygiene-kernel const, collector allowlist, GC filter, export guard and fixtures, catalog and roster change, spread-comment and test-pin flip, panels), the replay token, the non-member count, and the new clientConfig keys (the reconnecting cap, the resume abandonment cap, the reconnect abandonment cap) with their config log line and documentation.

    Cross-task obligations: (a) the new config keys are documented with default and range; (b) the two existing series' old label sets (meeting_id_hash, status, mh_index_bucket) and their non-exported catalog entries are removed in this commit; (c) every series lands with panel, catalog, zero-init and an ADR-0032 test; (d) implement the matching test-gates.md "Client" items, paired with test; (e) R4 label hygiene, with no meeting, participant, handler, ref, URL or slot labels.
  tag: story-2026-10-31-survive-handler-loss-task-C2
- id: 27
  status: pending
  specialist: infrastructure
  deps:
  - 9
  - 10
  prompt: |-
    Story 4 task I-3 (infrastructure): deploy guards for the MH slot model.

    GOAL
    ADR-0039 §7 "Template invariants" and the slot-table drift guard, plus the §9 force-delete ban (docs/decisions/adr-0039-mh-replacement-and-migration.md). Validation (Layer 3 / dt-guard) fails loudly when the MH slot model's structural guarantees regress. I-2 built the slot pool, the single template and mh.sh. GC-2 added GC's MH endpoint allowlists rendered from I-2's slot table. This task adds the guards that keep them true.

    Requirements satisfied: R-31 (template, Kind-only instrumentation and drift guards) and R-28 (the force-delete ban). docs/user-stories/2026-10-31-survive-handler-loss.md.

    DEPENDENCIES
    I-2 (slot pool, single template, mh-template, slot-table source) and GC-2 (GC_MH_WT_ALLOWED_HOSTS, GC_MH_WT_ALLOWED_PORTS, GC_MH_GRPC_ALLOWED_SCHEMES, GC_MH_GRPC_ALLOWED_CIDRS, GC_MH_GRPC_ALLOWED_PORTS and their Kind render).

    GUARDS (machinery in crates/dt-guard, under ADR-0034 conventions: subcommand-per-policy, a STATUS line, wrappers in scripts/guards/simple/)
    1. MH template invariants. Extend the kustomize policy (crates/dt-guard/src/kustomize.rs, kustomize_tools.rs; pod-template walkers in common/pod_spec.rs) over the rendered MH template, in the base render AND the Kind root render. It requires:
       - labels app=mh-service and component=media (Prometheus pod-SD keeps on these plus port 8083: infra/kubernetes/observability/prometheus.yml; the MC and MH NetworkPolicies select on them);
       - containerPorts 8083/TCP, 50053/TCP, 4434/UDP, with 4434 declaring a hostPort;
       - the mh-service-tls Secret volume mounted;
       - the MH NetworkPolicy still covering the template's labels with MC 50052 egress (infra/services/mh-service/network-policy.yaml).
       A missing item fails the guard and names the property.
    2. Kind-only test instrumentation. shareProcessNamespace: true must be PRESENT on the MH template in the Kind environment-root render and ABSENT from the base render and from every non-Kind render. Fail loudly in either direction. This is security policy: the property is test instrumentation registered in the ADR-0038 parity allowlist.
    3. No MH workload in the root apply set. The environment-root apply set contains no MH Deployment; the template reaches the cluster only as the mh-template ConfigMap that mh.sh instantiates. Reuse deploy.sh's derivation of the root apply set where the guard can see it; otherwise check the render and the exclusion rule together.
    4. Slot-table drift. The six-slot table in crates/devloop-helper/src/ports.rs (the ONE home of K) must agree with:
       - the MH UDP extraPortMappings in infra/kind/kind-config.yaml.tmpl and the static infra/kind/kind-config.yaml;
       - the per-slot advertise-address ConfigMap sources;
       - GC's GC_MH_WT_ALLOWED_PORTS (the slot host-port set);
       - GC_MH_GRPC_ALLOWED_CIDRS (the kind-config networking.podSubnet).
       Compare derivations; never retype a value in the guard. Any disagreement fails and names both sides.
    5. Force-delete ban (§9, runbook "force-delete ban"). A simple guard over infra/, scripts/, crates/env-tests/ and packages/*/e2e fails on any command that combines pod deletion with --force and --grace-period=0, whatever the argument order. It is the one way to defeat the hostPort slot lock. There is no allowlist and no suppression; docs/ is out of scope (runbooks state the ban in prose).

    TESTS
    Every rule gets a fixture-proven negative and its positive arm in the dt-guard test suite:
    - a template missing each required property;
    - shareProcessNamespace in the base, and missing from Kind;
    - an MH Deployment in the root set;
    - each drift pair;
    - each force-delete argument ordering.
    The shipped tree passes. Wire the wrappers into the guard pipeline (scripts/guards/simple/) so Layer 3 runs them; follow the existing wrapper prelude.

    OWNERSHIP NOTE (CLAUDE.md guard-crate ownership)
    This task owns the construct shapes and matchers (machinery). The membership and meaning of rule 2 is security policy content; security reviews it.

    PAIRING: security reviews rules 2 and 5; global-controller reviews the allowlist half of rule 4.

    TIER: light. Guard machinery only, with no runtime behaviour change; security reviews the policy content.

    CROSS-TASK OBLIGATIONS
    (a) No config keys. (b) No retirements. (c) No metric series. (d) Implements test-gates.md "dt-guard: the MH template declares hostPort, and no MH workload is in the root apply set (fixture-proven negative)". (e) N/A.

    INDEX: add pointers for the new guard modules in docs/specialist-knowledge/infrastructure/INDEX.md (pointers only; run scripts/guards/simple/validate-knowledge-index.sh).

    COVERS: code (guards), unit-tests.
  tag: story-2026-10-31-survive-handler-loss-task-I-3
  tier: light
  tier_reason: 'Guard machinery only: dt-guard rules and fixtures with no runtime behaviour, no wire contract, no secrets'
- id: 28
  status: pending
  specialist: operations
  deps:
  - 9
  - 19
  - 15
  - 20
  - 10
  - 11
  - 23
  - 24
  - 25
  prompt: |-
    OPS: MH replacement runbooks: deployment procedures, incident scenarios, TODO dispositions

    GOAL
    Make MH replacement and migration (ADR-0039) operable at 3am, in one documentation commit:
    - Part A: rewrite the MH deployment procedures around the per-incarnation slot pool, `infra/kind/scripts/mh.sh` and SIGTERM drain (upgrade, rollback, scale, node maintenance, 24-hour watch, configuration relationships).
    - Part B: add an incident scenario for every failure and expected-but-surprising behaviour ADR-0039 introduces, under the exact anchors that OBS-ALERTS will point its rules at.
    - Part C: record ADR-0039's closed, moot and new entries in docs/TODO.md.
    Documentation only: no code, config, alert-rule or wire changes.

    GOVERNING DESIGN (read first)
    - docs/decisions/adr-0039-mh-replacement-and-migration.md: §7 (MH addressing and provisioning, mh.sh, drain on SIGTERM), §10 (Operations), §6 (MC restart and owner-silence release), §8 (alerting posture).
    - docs/debates/2026-10-28-mh-replacement-and-migration/runbook-alert-changes.md: sections B, F, G, H, I (Part A) and C, D, E, I, J (Part B). Items marked "Superseded" are NOT to be implemented. Section I wins over older sections.
    - docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt: commands, JSON fields, exit codes and REASON tokens. Cite them exactly; do not paraphrase token names.
    - docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md: metric names for every checklist line.
    - docs/decisions/adr-0039-mh-replacement-and-migration.md §1–§5, §3 gray-failure known gap, Consequences and TODO dispositions (Part B and C).
    Items marked "Superseded" or "DROPPED" in runbook-alert-changes.md are NOT implemented; section I wins over older sections. Carry the ADR detail; do not invent procedures, metrics or config keys these documents do not name. If something is missing, stop and raise it.

    DEPENDENCIES (all must have landed: Part A describes shipped behaviour, and Part B's metric and log names must exist)
    - I-2: slot pool, single MH template, deploy.sh calling `mh.sh upgrade`. I-2 already fixed the runbook COMMANDS that broke when mh-0/mh-1 went away, and added K and B to mh-deployment.md's config reference. Build on those lines; do not redo them.
    - MH-1: media_dial admission and close codes, `mh_jwt_validations_total{token_type="media_dial", failure_reason}`. It also retires MH Scenario 13 (pending-path deletion).
    - MH-2: per-meeting heartbeat and verdicts, `mh_media_handler_releases_total`.
    - MH-3: drain on SIGTERM, load-report hygiene, QUIC idle timeout config, and the owner-silence release (MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS [300, 3600], default 900; trigger="owner_silence"; `mh_media_meetings_without_clients`).
    - GC-2: registration rule and `gc_mh_registrations_total`.
    - GC-3: AcquireMediaHandler, `gc_mh_replacement_requests_total` and the `gc.mh.replacement_report` INFO event.
    - MC-C: crash replacement via GC, awaiting-handler state, migrations for heartbeat_timeout|apply_lag.
    - MC-F: planned transfer (DRAINING → make-before-break, survivor target, timed_out), planned_transfer migration series.
    - MC-E: loss metric, disagreements, HandlerCensus (`mc_media_handler_first_declarations_total`, `mc_media_handlers_in_use`).
    Read the landed code and config. Where shipped behaviour differs from this prompt, the code and ADR win; note the difference in your output. If a name here did not land, stop and report it.
    OBS-ALERTS depends on this task: Part B's anchors must exist before its rules land.

    PART A: DEPLOYMENT PROCEDURES

    1. docs/runbooks/mh-deployment.md, §Rollout With Media Flowing: REWRITE.
    - Replace "MH sheds media sessions on restart. That is the decision." with the drain model (ADR-0039 §7 reverses ADR-0036 §11). Sequence:
      1. `mh.sh up` returns only when the new MH is Available, which means GC-selectable.
      2. Drain the old MH. On SIGTERM it reports HEALTH_STATUS_DRAINING to GC and flags DRAINING in each meeting's heartbeat, with remaining drain time.
      3. MC moves each meeting make-before-break, or onto surviving handlers when there is no grant.
      4. The old MH keeps forwarding and stays reachable on its own hostPort address until it exits.
      5. It exits at zero meetings, or at grace minus margin.
      6. The slot frees only when the pod is gone.
    - Client-visible consequence: a small blip per meeting, no "reconnecting", no rejoin. Delete "In this build they do not get it back on their own", "Recovery is unchanged: rejoin" and "Do not open an incident for this".
    - The drain deadline is derived from terminationGracePeriodSeconds, which is the ONE value to change (no second drain key).
    - Shed signal: MC's `mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}`, never MH's exit-time counter (the pod is gone within one scrape). The evidence is MH's final exit log line with the shed count. Causes: drain deadline too short for the meeting count, or GC unavailable with no survivors. There is no per-MC limiter (user ruling 2026-10-31), so do not name one as a cause.
    - Rolling every MH at once may move some meetings twice. That is expected, and it shows as planned_transfer migrations.
    - "Upgrade stuck, new MH never Available = check GC". MH is Ready only once GC has accepted its first load report. The old MH keeps serving, which is the safe direction. mh.sh exits with REASON=not-available-timeout and leaves the pod in place for diagnosis.

    2. Rollback (mh-deployment.md §Rollback and the Story 3 cutover rollback layers).
    - MH rollback = run the upgrade (deploy.sh converge / `mh.sh upgrade`) from the previous checkout. Meetings migrate as in a normal upgrade.
    - Remove every remaining `kubectl rollout undo` instruction for MH, and the content-addressed ConfigMap undo-safety rationale (ADR-0039 §7).
    - Images of every live MH pod, plus the previous revision, survive deploy-time pruning. ConfigMap generations referenced by a live MH pod are never pruned (ADR §10), so a mid-upgrade old-revision MH can still crash-restart in place.
    - State plainly: the first deploy of this story changes Kind's host port mappings to hostPort slots, so that deploy, and any rollback across it, requires a cluster recreate (R-30). There is no database schema change.

    3. New subsections in mh-deployment.md:
    - Node maintenance: `mh.sh up` (capacity elsewhere) → drain the MHs on the node → drain the node. Plain `kubectl drain` is safe because MHs are Deployment-managed and the replacement pod is held Pending by the hostPort until the old pod exits. The PDB stays and gates evictions.
    - Address pool exhausted: `mh.sh up`/`upgrade` exits 3 with REASON=mh-no-free-slot BEFORE touching any running MH. K = 6 slots, fixed at cluster creation (helper port table), so changing K means a cluster recreate. Diagnose with `mh.sh list --json` (state, owner, stale, terminating_past_grace).
    - mh.sh refusals operators will meet, each with its exit code and token: last-serving-mh, stuck-terminating and foreign-owner (exit 4), mh-pool-busy (exit 5), drain-timeout (exit 1). For each, say what to check and what to do.
    - Force-delete ban: never `kubectl delete pod --force --grace-period=0` on an MH pod. It removes the API object while the process may still run, letting a second MH take the same slot, which is the one way around the one-process-per-address invariant (ADR §7, R-28). A pod stuck terminating is investigated, not forced; mh.sh drain refuses and mh.sh list flags it. The crash drill is a process kill (in-place container restart: same slot, same pod IP, new handler_id; GC supersedes the dead row).
    - Scale-to-zero section: keep the conclusion. Draining ONE MH now moves its meetings onto others, so it is not a kill switch. Scaling ALL to zero still blocks new meetings and darkens existing ones, now visible as `mc_media_meetings_without_handlers` > 0 (paged by MCMediaMeetingsAwaitingHandler).
    - Replacement attempt budget: each meeting's replacement attempts are bounded by GC's exclusion cap (GC_MH_MAX_EXCLUDE_HANDLER_IDS, default 16). Its exclude list is current ∪ removed ∪ refused handlers, so a meeting can survive about 14 handler replacements in its lifetime. After that its migrations report outcome="rate_limited" and it stops acquiring (loud, counted). Say when an operator would see this: a very long meeting during repeated upgrades or a flapping detector. Say what it means: the meeting keeps its survivors, and rejoining resets it.

    4. Post-deploy checklist (mh-deployment.md §Post-Deploy Monitoring Checklist and §Post-rollout verification, media path):
    - Remove the re-assert caveats and the `mc_mh_notifications_without_connection_id_total` item (legacy mode is retired).
    - Add the 24-hour watch:
      - `mc_media_handler_migrations_total` with outcome timed_out and no_capacity unobserved;
      - planned_transfer completed ≈ meetings on the rolled MHs;
      - `mc_media_meetings_without_handlers` == 0;
      - `gc_mh_registrations_total` address_conflict flat, and superseded only at crash restarts;
      - client `dt_client_mh_connection_total` declined/wrong_handler near zero;
      - `mh_media_handler_releases_total{trigger="owner_silence"}` appearing only about 15 min after an MC rollout;
      - `mh.sh list --json` showing ≥ B serving and ≥ 1 free slot.

    5. docs/runbooks/mc-deployment.md, rollout section: add "MC rollout and MH meetings".
    - MC ids are per process (MC_ID unset), so a restarted MC answers its predecessor's meetings' heartbeats with FAILED_PRECONDITION (owner mismatch).
    - MHs keep those meetings until the owner-silence release (MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS, default 900) ends them with sessions closed HANDLER_RELEASED.
    - Expected after every MC rollout: MH owner-mismatch counts rise; about 15 min later `mh_media_handler_releases_total{trigger="owner_silence"}` rises, and MH meeting counts and `mh_media_meetings_without_clients` drop.
    - Use the ADR §6 wording: incidental, not an MC-failover mechanism. Those meetings are already lost when MC restarts (MC failure is out of scope), and the release reclaims MH capacity.

    6. Configuration reference relationships (mh-deployment.md and mc-deployment.md §Configuration Reference). Rows for new keys land with their own feature tasks (cross-task obligation a). Verify each row exists and is correct, and add the relationship notes:
    - MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS: range [300, 3600], default 900. MC's heartbeat timeout N (MC_MH_HEARTBEAT_TIMEOUT_MS): range (0, 30000] ms, default 6000. The owner-silence floor (300 s) is at least 10× the largest N, so a live MC always declares a silent MH dead and moves off long before MH releases on owner silence. Never shorten owner silence toward N.
    - GC_MH_MAX_EXCLUDE_HANDLER_IDS: default 16 → about 14 lifetime replacements per meeting (item 3).
    - GC_MH_LOAD_REPORT_INTERVAL_MS: the MH staleness bound is derived from it (k × interval). Note the GCMhRegistrationPingPong derivation that cites it.
    - MH QUIC idle timeout ≈ N, published beside N, for the R-33 interpretability condition.
    If a row is missing, report which task should have added it. Do not silently add rows for another task's key unless the key has landed and only the doc row is absent; in that case add it and say so.

    PART B: INCIDENT SCENARIOS
    Anchor contract (agreed with observability):
    Part B lands BEFORE OBS-ALERTS. OBS-ALERTS adds every new or changed rule with runbook_url set to the anchors below, so they must exist first and must match EXACTLY. Anchors are GitHub-style slugs of the headings. The list is also recorded in section J of docs/debates/2026-10-28-mh-replacement-and-migration/runbook-alert-changes.md. Retired MH Scenario 13 is NOT reused (MH-1 retired it).
    - docs/runbooks/mc-incident-response.md:
      - heading "Scenario 24: Media Handler Declared Dead or Impaired" → #scenario-24-media-handler-declared-dead-or-impaired
      - "Scenario 25: Meetings Awaiting a Media Handler" → #scenario-25-meetings-awaiting-a-media-handler
      - "Scenario 26: Media Handler Migration Timed Out" → #scenario-26-media-handler-migration-timed-out
      - "Scenario 27: Several Media Handlers Declared Dead by One MC" → #scenario-27-several-media-handlers-declared-dead-by-one-mc
      - "Scenario 28: Clients Report Handler Loss While Heartbeats Look Healthy" → #scenario-28-clients-report-handler-loss-while-heartbeats-look-healthy
      - "Scenario 29: Heartbeat Misroute or Owner Mismatch" → #scenario-29-heartbeat-misroute-or-owner-mismatch
    - docs/runbooks/mh-incident-response.md:
      - "Scenario 24: Media Dial Credential Rejections" → #scenario-24-media-dial-credential-rejections
      - "Scenario 25: Meetings Held Without Clients" → #scenario-25-meetings-held-without-clients
      - "Scenario 26: Owner-Silence Release With Clients Present" → #scenario-26-owner-silence-release-with-clients-present
      - Scenario 1 "Complete Service Outage": keep the heading text (anchor stable) and re-scope the body to the fleet/region-level MHDown (`gc_mh_available{state="healthy"} == 0`, no replacement capacity at all). A single MH death is routine and no longer this scenario.
    - docs/runbooks/gc-incident-response.md:
      - "Scenario 14: Two Live Media Handlers on One Address" → #scenario-14-two-live-media-handlers-on-one-address
      - "Scenario 15: GC Keeps Handing Out a Handler MCs Keep Replacing" → #scenario-15-gc-keeps-handing-out-a-handler-mcs-keep-replacing
    - Existing, kept: mc-incident-response #scenario-12-registermeeting-coordination-failures (MCRegisterMeetingFailureRate).
    Add each new scenario to its file's Table of Contents. Each scenario follows the file's existing shape: Alert, Severity, Runbook Section, Symptoms, Diagnosis (concrete PromQL and log filters), Resolution, Escalation. For scenarios whose alert lands in OBS-ALERTS, write the Alert line with the planned rule name and "(lands with OBS-ALERTS)".

    MC:
    - 24 Declared dead/impaired: triggers heartbeat_timeout | apply_lag (`mc_media_handler_declarations_total`, detection_seconds, migration_duration_seconds). One MH death produces one declaration PER MEETING within about N, all migrating at once (no per-MC limiter). That is expected; never act on raw volume. Use the required metric (`mc_media_handler_loss_prior_report_ratio`, loss_clients_total, client_lead_seconds) and say it is interpretable only when MH's QUIC idle timeout ≈ N. A false declaration is a safe outcome (one blip); count it via MH `mh_media_handler_releases_total{trigger="heartbeat_timeout",clients="present"}`. A contract-fault apply refusal (invalid, transport_inconsistent) is loud and NOT replaced. The pod-restart fence is safe. Restore fleet count with `mh.sh up`.
    - 25 Awaiting handler (PAGE): `mc_media_meetings_without_handlers` > 0 past the bound. Causes: GC unavailable, GC no_capacity (`gc_mh_replacement_requests_total{outcome="no_capacity"}`), MH admission refusals. Self-recovers when a grant arrives. Remedy: `mh.sh up` (check free slots), fix GC. FAILED_PRECONDITION means the caller is not the meeting's MC: never retried, escalate. Attempt budget: GC_MH_MAX_EXCLUDE_HANDLER_IDS default 16 ≈ 14 lifetime replacements per meeting, after which outcome="rate_limited". Explain both the sustained rate_limited warning (flap or forged loop) and the per-meeting minimum interval.
    - 26 Migration timed out (also the drain-shed reading `mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}`): the make phase did not complete by the drain deadline. Check:
      - client `dt_client_mh_connection_total` by outcome (declined, wrong_handler, timeout);
      - `mh_jwt_validations_total{token_type="media_dial"}` by failure_reason;
      - MH exp skew config;
      - MC_MEDIA_DIAL_TOKEN_TTL_SECONDS (default 300, [60, 900]) and the client FAILED-report re-push;
      - GC reachability;
      - drain deadline (grace minus margin).
      AC is NOT on this path (MC signs media_dial).
    - 27 Several handlers declared dead by one MC: by instance, first_declarations{trigger="heartbeat_timeout"} ≥ 2 over handlers_in_use above a fraction. The same signature on SEVERAL MCs = real multi-MH loss (node/AZ): check `mh.sh list --json` and node state. Only THIS MC = its own network path: leave the handlers alone. gc_view at request time is NOT the discriminator (GC lags by design). Also the MH crash-rate warning on first_declarations (per handler).
    - 28 Client loss disagreement: `mc_media_handler_client_loss_disagreements_total` > 0, i.e. most clients report loss while heartbeats look healthy. This is either a forged heartbeat (residual risk S1b: another MH pod forging for a dead one) or a client-side network path problem. Diagnosis and escalation to security for the forged case. Zero disagreements is NOT evidence of no forgery: a forged heartbeat with an empty connection_ids list retires the believed-connected set through the reconcile and empties the counter's denominator (story 4 MC-E finding).
    - 29 Misroute / owner mismatch: `mc_media_handler_heartbeats_total{outcome=~"released_unassigned|owner_mismatch"}` > 0, a contract or routing defect. Explain that an owner mismatch after an MC rollout is EXPECTED from the MH side (link to mh-incident-response 25 and the mc-deployment.md MC-rollout note written in Part A item 5).
    - Update existing scenarios:
      - 12: replace the "No alert today" TODO with MCRegisterMeetingFailureRate; a RegisterMeeting failure against a fresh grant adds that handler to the meeting's exclude list and re-acquires.
      - 13 (Unexpected MH Notifications): Connected from an out-of-set or removed handler is expected after a declaration and gets sender_id 0 with no binding recorded; delete the `mc_mh_notifications_without_connection_id_total` bullet.
      - 15 (Generation Divergence): re-assert wording out; generation lag is now the APPLY_LAG input → scenario 24. Keep the anchor; OBS-ALERTS rewrites the rule text.
      - 20 (Teardown / budget ratchet): a lost EndMeeting is retried by the RELEASED verdict on the next heartbeat.
      - Remove any remaining Scenario 13 cross-reference into MH if MH-1 left one.
    MH:
    - 24 Credential rejections: failure_reason expired sustained = MC's TTL/3 refresh push broken (the only signal for it); no_meeting_key sustained = register-before-token ordering bug; wrong_type > 0 = token-type confusion. WRONG_HANDLER is sent only after full validation; every other failure is DECLINED.
    - 25 Meetings held without clients (`mh_media_meetings_without_clients`, dashboard): normally an MC restart. A restarted MC has a new per-process id and answers FAILED_PRECONDITION; MH takes no action and keeps the meetings until the owner-silence release (MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS, default 900), so the gauge self-clears within 15 min. Act only if it persists past that bound (owner-silence broken → escalate to media-handler; `mh.sh drain` as relief). A pod restart is a safe manual fence.
    - 26 Owner-silence release with clients present: `mh_media_handler_releases_total{trigger="owner_silence", clients="present"}` > 0. People stayed attached for the whole silence window to a meeting no MC owned. clients="none" after an MC rollout is routine.
    - Update existing scenarios:
      - 10: lost Connected/Disconnected self-corrects within about two heartbeat intervals (remove-only reconcile, `mc_media_handler_connected_set_corrections_total`); a sustained correction rate is the warning.
      - 15/17/18: delete re-assert and legacy-connection_id text; a quiescent diverged meeting is detected via APPLY_LAG.
      - Recovery Procedures: planned drain = `mh.sh drain <slot>`; crash drill = process kill; never force-delete with zero grace.
      - Severity table: drop references to retired metrics if MH-1 left any.
    GC:
    - 14 Two live MHs on one address: signals are GCMhRegistrationPingPong (superseded ≥ 20 AND unknown_handler ≥ 3 in 10m), address_conflict persisting past the staleness bound plus one retry, and unknown_handler > 0. A superseded registration alone is routine: every in-place crash restart supersedes (match=both). Cause is almost always the hostPort lock defeated by a zero-grace force delete. Remedy: find the duplicate MH pod on that slot (`mh.sh list --json`, pod_uid) and retire it. Never touch GC rows by hand.
    - 15 Gray failure (known gap, ADR §3): GC keeps handing out a handler MCs keep replacing. Signals: `gc_mh_replacement_requests_total{trigger,outcome,gc_view}` (dashboard, no page) and the `gc.mh.replacement_report` INFO event (replaced handler_id, webtransport_endpoint, controller_id, trigger, gc_view, outcome). Procedure: group the events by replaced handler_id over the window. If ≥ 2 distinct controller_ids reported it AND it is still load-reporting healthy past the staleness bound (`mh.sh list --json` state=serving, not stale), it is a likely gray failure: `mh.sh drain <slot>` (slot from the endpoint). If only one controller_id reported it, suspect that MC's path and leave the handler. Caveats: controller_id is self-asserted (fine for a human decision, not yet for automation); a dead handler goes stale by itself and needs no drain.

    PART C: docs/TODO.md DISPOSITIONS (R-38; ADR-0039 "TODO dispositions" + telemetry-contract "TODO.md dispositions")
    - Close: "MC cannot see a lost NotifyParticipantDisconnected on a LIVE MH" (around :1470; remove-only reconcile); the MH half of "Connectivity repair after an MH or MC restart" (around :1467 item 2).
    - Do NOT touch "Missing alert: MC RegisterMeeting Coordination Failures" (around :1011): OBS-ALERTS closes it in the commit where the rule lands.
    - Moot: "MC's policy_generation resets to 1 …" (around :1539) and "EndMeeting has no fence against a late RegisterMeeting" (around :1681); no timer re-push, and RELEASED verdicts fence late registrations.
    - "A meeting whose MC never sends EndMeeting is never reclaimed" (around :1683): closed by the owner-silence release (R-11), with the residual stated: capacity is held for up to the silence timeout.
    - Update with security's wording (ask security in review): the S6 entry (around :1476: MH's bearer now crosses the plaintext MH→MC channel on every heartbeat) and the MH-ownership residual.
    - Add: gray-failure automatic quarantine (M ≥ 2 independent MC reports within W; preconditions: state shared across GC replicas and authenticated MC identity), owner global-controller + security; mc-0/mc-1 RollingUpdate surge putting two live MCs behind one NodePort during an MC rollout (task-sized, MC out of scope), owner infrastructure + meeting-controller; a reference that CI runs no Kind or Layer 7 (existing gap; point to the existing entry if one exists rather than duplicating).
    Line numbers are approximate: find entries by title. Follow the file's entry format (owner, provenance, fix shape). The SLO register line (docs/observability/slos.md) is OBS-ALERTS's, not this task's.

    MUST LAND TOGETHER
    Parts A, B and C in one commit. Do not edit alert rule files (OBS-ALERTS owns the rules). Keep every existing anchor that rules or docs link to: check with scripts/guards/simple/validate-alert-rules.sh and grep for inbound links before touching any heading.

    VALIDATION
    Layer 1–7 green (docs only, so mainly Layer 3: alert runbook refs resolve, links, knowledge index). Verify:
    - every metric name against the landed catalogs in docs/observability/metrics/;
    - every mh.sh command, exit code and REASON token against infra/kind/scripts/mh.sh;
    - each Part B anchor slug, by rendering the heading the way the alert validator resolves anchors.

    PAIRING / REVIEW
    Implemented by operations. Reviewers:
    - infrastructure: the mh.sh procedures;
    - media-handler: the drain description and MH 24–26;
    - meeting-controller: the MC-rollout note and MC 24–29;
    - global-controller: GC 14–15;
    - observability: anchors and PromQL;
    - security: scenario MC-28 and the TODO residual wording.

    REQUIREMENTS
    R-37 (runbooks and configuration reference), R-38 (TODO part; the SLO register line is OBS-ALERTS'), R-30 (rollback documented, first-deploy recreate), R-35 (anchors exist for every added or changed alert, before OBS-ALERTS), R-3 / R-10 / R-11 / R-33 (operator view of planned transfer, fence, owner-silence and the required metric).

    CROSS-TASK OBLIGATIONS
    (a) No new keys; verify the Configuration Reference rows per Part A item 6 (rows land with their feature tasks).
    (b) Remove remaining runbook references to retired surfaces: re-assert, legacy connection_id, and the pending-path timeout if MH-1 left any.
    (c)–(e) Not applicable.
    Tier: light. Documentation only: no proto, no wire, no secrets, no code.
  tag: story-2026-10-31-survive-handler-loss-task-OPS
  tier: light
  tier_reason: 'Documentation only: runbooks and docs/TODO.md entries, no code, wire, auth or GSA'
- id: 29
  status: pending
  specialist: observability
  deps:
  - 7
  - 10
  - 11
  - 16
  - 19
  - 14
  - 20
  - 23
  - 24
  - 25
  - 26
  - 28
  prompt: |-
    OBS-ALERTS: story-4 alert rules, alert inventory, SLO line and cross-service panels

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md (story 4, "Survive a handler loss").

    Governing design:
    - ADR-0039 (docs/decisions/adr-0039-mh-replacement-and-migration.md) §8 Telemetry and §10 Operations.
    - The telemetry contract, docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md, sections "Alerts", "SLO" and "R4-5 evidence". It is the source of truth for every expression and threshold below.
    - Runbook anchors: runbook-alert-changes.md section J (same directory).
    - Where runbook-alert-changes.md section E disagrees with the contract, the contract wins. Section E predates rounds 3 and 4: the timed_out page, the address-uniqueness counter and the narrowing of MCMediaGenerationDivergence were all superseded.
    Carry detail from these documents; invent nothing.

    Specialist: observability.
    Required reviewers: the owners of each rule file under ADR-0031 (meeting-controller for mc-alerts.yaml, media-handler for mh-alerts.yaml, global-controller for gc-alerts.yaml), and operations for runbook linkage and severity.
    Pair with test for the env-test changes.

    Dependencies (all must have landed):
    - the series these rules read: GC-1, GC-2, GC-3, MC-D, MH-1, MH-CLOSE (the session-close counter), MH-3 (owner-silence release, drain, QUIC idle gauge), MC-C (crash migrations, awaiting-handler bound), MC-F (planned_transfer migrations, i.e. drain shed), MC-E, C2 (R-42 client series and resume_ms). MC-B1, MH-2, MC-B2 and C1 come in transitively;
    - OPS, which creates every runbook anchor cited below.
    Before writing a rule, verify that each series and anchor exists in the tree. If one is missing, stop and report it. Do not stub it.

    Goal (R-35): a single media-handler death never pages by itself. Real user impact pages, and the rest of the mechanism warns. Every rule's runbook_url resolves and its inventory entry in docs/observability/alerts.md is byte-identical to the rule's expr (dt-guard alert-rules, inventory_expr_drift). Thresholds the contract derives from config use the published config gauges, never a copied number. Where a literal is unavoidable (a for: duration) it carries an ANCHOR (DRY) comment naming the gauge, and an env-test compares it against the live gauge.

    infra/docker/prometheus/rules/mc-alerts.yaml (meeting-controller reviews):
    - PAGE MCMediaMeetingsAwaitingHandler. mc_media_meetings_without_handlers > 0, sustained past the published awaiting-handler bound gauge from MC-C. Runbook: mc Scenario 25.
    - KEEP MCMediaGenerationDivergence. Remove the comment and description text premised on the periodic re-assert ("no periodic re-assert (story 4)"). MC-B1 already removed handler_id_mismatch from its expr; verify that. Its existing Scenario 15 anchor stays.
    - WARNING, migration failures while a handler survives. mc_media_handler_migrations_total with outcome no_capacity, gc_unavailable, admission_refused or timed_out. Scenario 24, or 26 for timed_out.
    - WARNING, drain shed. mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"} increase > 0. Scenario 26. Never key this on MH's exit-time counters.
    - WARNING, MC own-path signature, per instance. increase(mc_media_handler_first_declarations_total{trigger="heartbeat_timeout"}[w]) >= 2 AND that increase over mc_media_handlers_in_use above a fraction. w and the fraction are rule-owned, with the derivation in the rule's ADR-0031 block. Scenario 27.
    - WARNING, crash rate. rate(mc_media_handler_first_declarations_total{trigger="heartbeat_timeout"}[1h]) above a rule-owned threshold. This counts per handler, never per meeting. Scenario 24.
    - WARNING. mc_media_handler_client_loss_disagreements_total increase > 0. Scenario 28.
    - WARNING, sustained. mc_media_handler_migrations_total{outcome="rate_limited"}: the per-meeting interval or attempt bound only. With the default exclude cap of 16 (GC_MH_MAX_EXCLUDE_HANDLER_IDS), about 14 lifetime replacements per meeting are allowed before this. Scenario 24.
    - WARNING, sustained. mc_media_handler_connected_set_corrections_total. Scenario 24.
    - WARNING. mc_media_handler_heartbeats_total{outcome=~"released_unassigned|owner_mismatch"} increase > 0. Scenario 29.
    - WARNING, removal-release contract defect (added at MC-B2 review, observability + media-handler). mc_media_handler_removal_releases_total{outcome=~"rejected_ownership|invalid_argument"} increase > 0. MH refused the removal's EndMeeting owner or cause: an MC defect. The removal path is deliberately off mc_media_end_meeting_total, so MCEndMeetingOwnershipRejected does not see it. Never select unavailable_exhausted, error or unimplemented, which are expected after a crash. Scenario 29.
    - WARNING, heartbeat contract faults (MC-B1 named this a candidate, and MC-B2 added the apply_refused_* reasons). mc_media_handler_heartbeat_anomalies_total increase > 0. Every reason is expected zero in production. In the dev cluster, though, the env-test policy `Injection` fixture produces applied_ahead_of_sent by design (MC-B2, Layer 7), so the rule's description must name that out-of-band writer as a known benign cause. Scenario 29. Describe apply_refused_invalid and apply_refused_transport_inconsistent as MC contract faults, never a replacement.
    - WARNING MCRegisterMeetingFailureRate. The mc_register_meeting_total{status="error"} ratio > 0.10 for 5m, shaped like MCHighJoinFailureRate. Existing Scenario 12. This closes the docs/TODO.md entry "Missing alert: MC RegisterMeeting Coordination Failures"; mark it closed in this commit.

    infra/docker/prometheus/rules/mh-alerts.yaml (media-handler reviews):
    - PAGE MHDown, re-scoped. Replace the per-pod up{job="mh-service"} == 0 expression with a fleet/region-level one: gc_mh_available{state="healthy"} == 0 per region. Keep the alert name and the Scenario 1 anchor; the scenario's title is kept by ops. Make the description true ("no healthy non-draining MH").
    - KEEP MHPodRestartingFrequently. MH restarts in place.
    - WARNING, false declarations. mh_media_handler_releases_total{trigger="heartbeat_timeout", clients="present"} sustained. mc Scenario 24.
    - WARNING, owner-silence release with clients present. mh_media_handler_releases_total{trigger="owner_silence", clients="present"} increase > 0. Scenario 26. clients="none" is routine after an MC rollout and is not alerted.
    - WARNING, credential rejections. mh_jwt_validations_total{token_type="media_dial"}: failure_reason="expired" sustained (refresh push broken), failure_reason="no_meeting_key" sustained (register-before-token bug), failure_reason="wrong_type" increase > 0, failure_reason="aud_mismatch" sustained (WRONG_HANDLER rate, from runbook-alert-changes.md section E). Scenario 24.
    - WARNING, meetings held without clients. mh_media_meetings_without_clients > 0 sustained past the owner-silence timeout plus margin: owner-silence release should have cleared them. The for: literal carries an ANCHOR to mh_media_meeting_owner_silence_timeout_seconds. Scenario 25.
    - REWORD MHMCNotificationFailures' impact. Lost notifications now self-correct within N through the connected-set reconcile.

    infra/docker/prometheus/rules/gc-alerts.yaml (global-controller reviews):
    - PAGE GCMhRegistrationPingPong. sum(increase(gc_mh_registrations_total{outcome="superseded"}[10m])) >= 20 and sum(increase(gc_mh_load_reports_total{outcome="unknown_handler"}[10m])) >= 3, for 10m. The derivation block:
      - Ping-pong evicts at the load-report cadence (cite gc_mh_load_report_interval_seconds), which is about 120 supersedes per 10m.
      - A dead process never sends load reports, so crash-restart churn never moves unknown_handler, at any fleet size.
      - The floor of 20 keeps a one-off duplicate on the warning.
      - Re-derive if the interval changes.
      Scenario 14.
    - WARNING. gc_mh_load_reports_total{outcome="unknown_handler"} increase > 0. Scenario 14.
    - WARNING. gc_mh_registrations_total{outcome="address_conflict"} still increasing, with for: = the staleness bound plus one MH registration retry. ANCHOR (DRY) to gc_mh_staleness_threshold_seconds. Scenario 14.
    - WARNING. gc_mh_registrations_total{outcome=~"endpoint_changed|invalid_endpoint"} increase > 0. Scenario 14.
    - WARNING. gc_mh_replacement_requests_total{outcome="no_capacity"} sustained (capacity headroom gone). Scenario 25.
    - No alert on gc_view or on the gc.mh.replacement_report evidence: it is dashboard and log only (Scenario 15, gray failure).

    Docs:
    - docs/observability/alerts.md: add an inventory entry for every new or changed rule, with Condition, Impact, Runbook and Threshold Rationale. Update the convenience list of MC page alerts. Remove the re-assert premises from the MCMediaGenerationDivergence entry.
    - docs/observability/slos.md, Open items:
      - Replace "No SLO for the MC→MH control plane's applied-generation echo" with "MH replacement: migration completes within deadline; media-dark window (MC detection + migration as the server bound, dt_client_media_resume_ms as the truth)". Target OPEN: observed, not gated (ADR-0036 §10).
      - Delete the "MCRegisterMeetingFailureRate re-assert-failure alert" row.

    Cross-service panels, infra/grafana/dashboards/mc-media.json (meeting-controller reviews):
    - N (mc_media_handler_heartbeat_deadline_seconds) beside MH's QUIC idle-timeout gauge.
    - The R-33 interpretability panel: the loss-share histogram and count, shown next to those two gauges. Its description states that the metric is meaningful only when client detection time is about N.

    Env-tests (pair with test):
    - crates/env-tests/tests/33_alert_rules_loaded.rs covers every new rule.
    - Add a check that the address_conflict and meetings-without-clients for: values match their live config gauges (gauge_by_live_pod).
    - No env-test may require an alert to fire on a single MH death.

    Same commit: each rule, its inventory entry and the cited anchors (which already exist from OPS). Layers 1–7 must be green.

    Cross-task obligations:
    - (a) No new config key here.
    - (b) No retirement here: the gauge and outcome retirements are MC-B1 and MH-1.
    - (c) The panels read only existing series.
    - (d) Implement test-gates.md items touching alerts.
    - (e) R4 label hygiene: no identity labels in any expr grouping.

    Requirements: R-35 (all), R-33 (presentation), R-34 (cross-service panels), R-38 (SLO line and the TODO closure above).
    Tier: full (three services' rule files and the paging posture).
  tag: story-2026-10-31-survive-handler-loss-task-OBS-ALERTS
- id: 30
  status: pending
  specialist: client
  deps:
  - 18
  - 23
  - 24
  - 26
  prompt: |-
    C3: Browser resilience spec, packages/web-app/e2e/mh-replacement.spec.ts.

    Specialist: client. Pair with test (fixture discipline, pool hygiene, ADR-0032 metric semantics). Tier: light (e2e only, no production code).

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md sections 4 and 9. Supporting files: docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md (sections "Browser", "Kind verification gap", the stranded leg note, and the crash assertions) and mh-sh-contract.txt.

    Requirements satisfied: R-1, R-2, R-3 (the browser legs), R-20 (browser assertions), R-39 (the browser legs).

    Dependencies, all landed first:
    - T-1: the MH pool fixture, the process-kill / freeze injector, resilience hygiene, and e2e/mhPool.ts wrapping mh.sh's JSON contract (one home; execFile, no shell; fails loudly when the mh.sh path is unset).
    - MC-C: crash replacement via GC (the crash and stranded legs).
    - MC-F: planned transfer (DRAINING, make-before-break, survivor target; the planned leg).
    - C2: the reconnecting overlay and transportFault bus events.
    These transitively need C1, MH-1, MH-3, MC-B2, I-2 and I-5.

    THE SPEC (Playwright, retries 0, cohort fixtures in e2e/cohortContexts.ts and e2e/cohort.ts):

    1. Crash leg.
    - A cohort joins with tone publishing. Take the pre-kill bus handlerSets.
    - Kill the MH process carrying the meeting through the T-1 injector (kill -KILL from a debug container). NEVER kubectl delete pod --force --grace-period=0: it defeats the slot lock and is guarded against (R-28).
    - Assert:
      - pairwise tone resumption within a config-derived, failure-only ceiling (from N and the MH QUIC idle timeout in deployed config, read via e2e/configEnv.ts, never a literal);
      - every participant's final bus handlerSets holds a handler_ref absent from the pre-kill set;
      - the reconnecting overlay was observed (the positive arm of the planned leg's negative).
    - Do NOT assume the replacement URL differs from the dead one: an in-place restart reuses the slot. Record the media-dark window (dt_client_media_resume_ms or the bus timings), never gate on it (ADR-0036 section 10).

    2. Planned transfer leg, with a target.
    - mhPool up for a target, then mhPool drain of the carrying MH.
    - Assert:
      - the bus saw the set sequence add-then-remove in order (the new ref added, then the old ref removed);
      - no reconnecting overlay and no transportFault for the whole leg (a deterministic negative);
      - tone continues within the failure-only ceiling.

    3. Stranded A/B leg.
    - Use the existing blockHandlers option (e2e/cohortContexts.ts around 55; pattern in e2e/partial-connectivity.spec.ts around 105-112), with URLs read from bus handlerSets.
    - Block the SURVIVOR's URL for A and B; C stays unblocked.
    - Kill the carrying MH.
    - Assert A's and B's final sets hold a handler_ref not in the pre-kill set, and that A and B hear each other within the ceiling. This is the same as a fresh join, with no special case (ADR section 4).

    Hygiene, every leg:
    - Restore the baseline pool and leak no owned MH (T-1 fixture). Fail loudly on foreign pool activity.
    - Take metric baselines only after Prometheus scrapes a new MH.
    - Use explicit teardown assertions, not Drop-style backstops.
    - Read bounds from deployed config.
    - The test bus never carries tokens.

    SAME COMMIT: the spec, any harness helper it needs in e2e/ (reusing receiveEvidence.ts, toneDetector.ts, windowSampling.ts and failureDiagnostics.ts), and its entry in packages/web-app/e2e/README.md (the env and catalog).

    Cross-task obligations: (a) no new config keys; (b) no retirements; (c) no new series, and any metric read uses ADR-0032 semantics; (d) this implements test-gates.md "Browser", paired with test; (e) no identity in anything exported.
  tag: story-2026-10-31-survive-handler-loss-task-C3
  tier: light
  tier_reason: 'Browser e2e spec only: no production code, wire contract, auth or GSA change'
- id: 31
  status: pending
  specialist: test
  deps:
  - 22
  - 27
  - 28
  - 29
  - 30
  prompt: |-
    T-2: Survive-handler-loss manual test plan, written and run

    Story: docs/user-stories/2026-10-31-survive-handler-loss.md. Governing design: docs/decisions/adr-0039-mh-replacement-and-migration.md sections 4 (migration), 5 (dial credential), 6 (fencing), 7 (addressing, mh.sh), 9 (verification: "The manual browser plan includes a planned transfer in a meeting older than max(join JWT TTL, media_dial TTL)"). Test gates: docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md "Manual plan". Precedent for shape and tone: docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md and docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md.

    Requirements satisfied: R-41 (manual test plan written and run before /close-story). It also gives a human eye to R-1, R-2, R-3, R-5, R-10, R-11, R-18, R-20, R-29 and R-33/R-42 readings, which automation reads only through wire tokens, metrics and the dev-only e2e bus.

    GOAL
    Write docs/user-stories/2026-10-31-survive-handler-loss-manual-test-plan.md and run it on a fresh Kind cluster with the whole story landed, recording results and evidence in the same file. Observed numbers (media-dark windows, detection times, R-33 share, R-42 dial/disruption timers) are recorded as evidence, never as pass/fail thresholds (ADR-0036 section 10: latency is observed, never gated).

    SETUP THE PLAN STATES
    - Fresh cluster (the first deploy of the slot pool requires a recreate; ADR-0039 section 7), baseline B serving MHs, `infra/kind/scripts/mh.sh list` shown at the start.
    - Three browser tabs in one meeting (pnpm dev or the dev-web launcher, scripts/dev-web.sh), debug panel open in each: handler-set version, each handler's ref and address, never credentials (R-20).
    - Crash and freeze use the injector commands documented by T-1 and the runbooks from OPS: `kubectl debug <pod> --target=mh-service` as uid 65532, `kill -KILL` / `kill -STOP` / `kill -CONT` of the mh-service process. Never `kubectl delete pod --force --grace-period=0` (forbidden; ADR-0039 section 9).

    STEPS (each with expected observation and a result line)
    1. Crash by process kill of the MH carrying the call: tiles show "reconnecting" (never a frozen live frame, never a vanished participant), the set version bumps, media resumes on a surviving or replacement handler; final set shows a ref not present before; the pod restarted in place (same slot, new incarnation, new ref). Record dark window and MC detection time (mc_media_handler_detection_seconds).
    2. Planned transfer with a target: `mh.sh up`, then `mh.sh drain <k>` of the carrying MH: the set adds the new handler then drops the old; no "reconnecting", no transport fault; mh_drain_meetings_shed_total unchanged; no observation on the R-33 metric.
    3. Planned drain without a target (no free capacity granted): participants move to the surviving handler; no shed.
    4. `mh.sh upgrade` with a live meeting: start-new-then-drain-old, one at a time; the meeting survives every step; never below baseline.
    5. Zombie: freeze the carrying MH until MC declares it dead and clients have moved, then resume within the liveness budget: no double audio/video, the old handler leaves every panel, MH logs/counts the release (mh_media_handler_releases_total).
    6. Stranded pair: with blockHandlers (packages/web-app/e2e harness lever, or the documented manual equivalent) so two tabs reach only the carrying MH; kill it; the pair hear and see each other again on the replacement.
    7. Long call: keep a call open longer than max(join JWT TTL 900 s, MC_MEDIA_DIAL_TOKEN_TTL_SECONDS default 300 s), then perform step 2; it succeeds with no user action (R-5).
    8. Pool exhaustion: fill every slot with `mh.sh up` until it refuses with its capacity reason; `mh.sh upgrade` refuses before touching any old MH; the old handlers keep serving.
    9. MC rollout releasing its meetings: restart an MC holding a meeting; MH ends that meeting after MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS (state the value used; a reduced in-range value [300, 3600] is acceptable for the run and must be noted), closing sessions with HANDLER_RELEASED and counting the owner-silence cause.
    10. Readings: after steps 1 and 5, read the R-33 metric (count, 0-1 share, lead time) and the R-42 client series (dial outcomes and time to connect, disconnects by cause, media-dark window, lost-handler count) from Prometheus/Grafana; record them.

    EVIDENCE
    A "Run record" section: date, commit, host description, cluster recreate noted, per-step result (pass/fail with observation), recorded numbers, and any defect found filed or fixed per the working conventions (fix trivial in-tree issues now; failures are loud, never waived).

    CROSS-TASK OBLIGATIONS
    (a) No config keys. (b) No retirements. (c) No series. (d) Test's manual-plan section of test-gates.md. (e) n/a.

    PAIRING AND DEPENDENCIES
    Depends on P3, I-3, OPS, OBS-ALERTS, C3, and transitively on every feature task. Pair with operations (steps 4, 8, 9 follow their runbooks; a runbook step that does not work is a defect) and client (panel and tile states). Tier: light (documentation plus a human run; no code).
  tag: story-2026-10-31-survive-handler-loss-task-T-2
  tier: light
  tier_reason: 'Documentation and a human-run manual test plan: no code, wire, auth or GSA change'
```

---

## Revisions
