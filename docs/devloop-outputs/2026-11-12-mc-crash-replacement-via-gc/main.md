# Devloop Output: MC-C crash replacement via GC

**Date**: 2026-11-12
**Task**: Story 4 task MC-C (run-story #23): after MC-B2 removal, acquire a replacement handler from GC (AcquireMediaHandler) and move the meeting onto it via the join path; bounds, backoff, awaiting-handler, migration telemetry, env-tests in 40_resilience.rs.
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired: global-controller; test on standing panel)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h45m (planning 4 rounds, 1 review→impl round, 2 Gate-2 runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e08bee53456a7b208c6b005f849c1473b0046340` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired Global Controller | `paired-global-controller` |

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
After an MC-B2 removal (heartbeat_timeout / apply_lag), MC obtains a replacement media handler from GC via AcquireMediaHandler and moves the meeting onto it through the normal join path (break-before-make), with per-meeting bounds, jittered backoff, awaiting-handler recovery, and migration telemetry. The acquisition and Registering→Active path is built so MC-F (planned transfer) can reuse it unchanged.

### Scope
- **Service(s)**: mc-service (primary); gc-service (one lockstep mirror of the assignment-intake predicate in `mc_assignment.rs`); env-tests; infra config/dashboards; runbook + observability docs
- **Schema**: No
- **Cross-cutting**: Yes (MC↔GC AcquireMediaHandler consumer; paired with global-controller)

### Debate Decision
NOT NEEDED - governed by ADR-0039 §3/§4; one contract clarification (migrations_total counted per acquisition attempt) ruled by observability (contract owner) at Gate 1 and recorded in telemetry-contract.md + catalog.

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
| `crates/mc-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mc-service/src/grpc/mc_service.rs` | Mine | — |
| `crates/mc-service/src/media_routing/pusher.rs` | Mine | — |
| `crates/mc-service/src/main.rs` | Mine | — |
| `crates/mc-service/src/config.rs` | Mine | — |
| `crates/mc-service/src/media_routing/replacement.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/media_routing/placement.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/health.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/replacement.rs` (new) | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | Mine | — |
| `crates/mc-service/tests/handler_replacement_integration.rs` (new) | Mine | — |
| `crates/mc-service/tests/handler_replacement_log_reasons.rs` (new; own binary: TRACE capture) | Mine | — |
| `crates/env-tests/src/fixtures/metrics.rs` (`mc_config_gauge_max` + `mc_replacement_retry_budget`; test review F7) | Not mine, Minor-judgment | test |
| `crates/mc-service/tests/*.rs` (fixture wiring: deps gain the acquirer; gc_integration cap arms) | Mine | — |
| `crates/mc-test-utils/src/mock_gc.rs` | Mine | — |
| `crates/gc-service/src/services/mc_assignment.rs` (`selection_is_malformed` lockstep mirror of the S2 intake predicate + test) | Not mine, Minor-judgment | global-controller |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md` (migrations_total row: per-attempt unit, rate_limited = bound refusal; SSoT sync per observability round-4 ruling) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment | observability |
| `infra/services/mc-service/config.env` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Mechanical | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Mechanical | infrastructure |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `crates/env-tests/tests/40_resilience.rs` | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Not mine, Minor-judgment | test |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-11-12-mc-crash-replacement-via-gc/main.md` | Mine | — |

---

## Planning

**Mechanism restatement.** "Recover after removal" restated as a mechanism: *keep a meeting's handler set at its hand-off size by acquiring members whenever it is short, where every member, whether from the hand-off or from a grant, goes through ONE Registering→Active path, and a member whose first RegisterMeeting never acks is removed and replaced.* This covers more than the task names. ADR-0039 §3 says "if the initial handler never acks … the unacked handler takes the normal replacement path", and today an initial handler whose `InitialRegistrationFailed` arrives stays Registering forever. The plan completes this invariant for hand-off handlers as well as grants (one rule, no "everywhere except the first join"). Existing T3 (`handler_set_integration.rs`) still holds: the join succeeds with an empty set and nothing ever names the handler.

**1. GC client (`grpc/gc_client.rs`)**
- `max_exclude_handler_ids: AtomicU32`. `register()` and `attempt_reregistration()` both go through one `accept_registration(&resp)`. If the value is 0 or absent, that is a NON-retried `McError::Config("… max_exclude_handler_ids …")` (same arm as a GC rejection, so `register` returns `Err` and `run_gc_task` keeps retrying loudly). Otherwise it is stored, so re-registration refreshes it. No GC range is copied into MC.
- Trait `HandlerAcquirer: Send + Sync` (mirrors `MhRegistrationClient`): `acquire(&AcquireCall) -> Pin<Box<Future<Output = AcquireResult>>>` and `max_exclude_handler_ids() -> Option<NonZeroU32>`. `impl HandlerAcquirer for GcClient`. `AcquireCall {meeting_id, exclude: BTreeSet<HandlerId> (deduped by construction), trigger: RemovalTrigger, replaced: Option<HandlerId>}`. The request is built with `trigger.release_cause()`, the ONE projection (`ReleaseCauseExt`). `replaced_handler_id` is omitted when there is none.
- Classification `classify_acquire(Result<Response, Status>) -> AcquireResult`, an explicit arm per code. Error values keep `code` + `message` (not only logs):
  - OK with an assignment, a non-empty `mh_id` and both endpoints → `Granted(HandlerEndpoint)`.
  - OK that is missing any of those → `Rejected{Malformed}`: fail closed, not retried, loud.
  - RESOURCE_EXHAUSTED → `NoCapacity`.
  - UNAVAILABLE, DEADLINE_EXCEEDED, INTERNAL, UNIMPLEMENTED (rollout), UNAUTHENTICATED/PERMISSION_DENIED (token expiry; TokenManager refreshes) and UNKNOWN/ABORTED/CANCELLED/other transport → `Unavailable{code,message}` (retried).
  - FAILED_PRECONDITION → `Rejected{NotOwner}`; INVALID_ARGUMENT → `Rejected{InvalidRequest}`. Neither is retried; both log ERROR.
- Plumbed: `main.rs` passes `Arc<GcClient> as Arc<dyn HandlerAcquirer>` into `WebTransportServer::new` → `MediaRoutingDeps.acquirer`. Deps are installed at first join; removal requires a confirmed handler, which requires deps, so deps are always present when acquisition can start.

**2. Pure policy (`media_routing/replacement.rs`, new; no I/O, paused-time testable)**
- `ReplacementTiming { min_interval, backoff_max }` from config. Backoff base = min interval. Attempt k waits `d = min(min_interval·2^k, backoff_max)`, jittered earlier-only into `[d/2, d]` but never below `min_interval`. A seedable `StdRng` (refresh.rs pattern) with a `MeetingSeams.replacement_seed` test seam.
- `retry_budget()` = the deterministic sum of the un-jittered ladder up to and including the first capped step, plus one `backoff_max`: by then a meeting awaiting a handler has had every attempt the ladder allows. Published as config gauge `mc_media_handler_replacement_retry_budget_seconds` (the awaiting-handler page bound; OBS-ALERTS reads it). The min interval and backoff max are also published as `mc_media_handler_replacement_min_interval_seconds` and `mc_media_handler_replacement_backoff_max_seconds`, so env-tests derive ceilings from live values.
- `MigrationOutcome` (completed|no_capacity|gc_unavailable|admission_refused|rate_limited|timed_out|abandoned) and `MigrationPhase` (replacement|registration|reconnect). Both `ALL` arrays drive zero-init, crossed with `RemovalTrigger::ALL` (planned_transfer included now).
- `ReplacementState`, the per-meeting state machine:
  - `target` = the hand-off count.
  - A FIFO of owed replacements `{trigger, replaced: Option<HandlerId>, declared_at}`.
  - `in_flight: Option<{version, owed, sent_at}>`. One Acquire per meeting at a time.
  - `next_attempt_at`, `last_attempt_at`, `failures`.
  - `registering: BTreeMap<HandlerId, {trigger, declared_at, granted_at, replaced}>`.
  - `reconnecting: Option<{handler, trigger, active_at}>`.
  - Decisions are pure functions returning actions: `on_removal`, `due(now, set) -> Attempt | RateLimited | Idle`, `on_result(version, result, now)`, `on_registered`, `on_registration_failed`.
- **Bound** (MC-side, so an over-cap list is never sent): `exclude = members ∪ removed`, deduped. Refused-at-admission ⊆ removed, because a refused grant is removed (one source, no second set). An attempt is allowed iff `|exclude| < cap`. Cap 16 with 2 initial handlers gives 14 grants. At the bound: record `rate_limited` once and set `stopped=true` permanently (exclude only grows). The meeting stays on survivors or in awaiting-handler.
- **Min interval**: an attempt due earlier than `last_attempt_at + min_interval` is deferred to that instant and counts `rate_limited` (contract: "deferred by the per-meeting minimum interval").

**3. Actor driver (`actors/meeting/replacement.rs`, new; `meeting.rs` select arms)**
- After `remove_handler_completely` (and on a refused-at-admission removal), call `replacement.on_removal(trigger, Some(id))`. If the set is below `target` and nothing is in flight, the attempt is due now (or at the min interval).
- A timer arm `next_attempt_at` → `start_acquire()`. It snapshots `handler_set_version` and the exclude list, then `tokio::spawn`s the `acquirer.acquire(call)` task. The result comes back on a dedicated `mpsc::UnboundedSender<AcquireReport{version, result}>` drained by its own `select!` arm (the registrations pattern). The actor never awaits GC or a JoinHandle. The task's `AbortHandle` is aborted on actor exit; GC persists nothing, so an abandoned call leaks nothing.
- Result handling:
  - **Version ≠ current** (superseded) → `abandoned`. The grant is discarded, never added, no worker. A fresh attempt is scheduled if the set is still short.
  - **Granted** → if `exclude` contains the id (a GC contract violation) → `abandoned`, loud. Otherwise `media.add_granted(endpoint)` → `MeetingHandlers::add` (Registering: heartbeats OWNED, binding gate admits). Then `reconcile_media(Affected::All)`, which spawns its push worker with `heartbeat_interval_ms` + the 32-byte `media_dial_public_key` already on `MeetingMedia` (the existing `PushTarget` builder; no second request builder). Observe phase `replacement` (declaration→grant). Census refreshed. `add` refusal (DuplicateUrl etc.) → `abandoned`, ERROR, retry scheduled.
  - **NoCapacity / Unavailable** → `no_capacity` / `gc_unavailable`, backoff, retry. Applies with or without survivors; with none the meeting is in awaiting-handler on the existing gauge.
  - **Rejected** → `abandoned` + ERROR (code, message), no retry for this owed entry. A later removal can start a new one.
- `handle_registration`:
  - `Registered` for a granted handler → `completed`. Observe phase `registration` (grant→ack). The existing `on_registration` → `broadcast_handler_set` (with tokens) → reroute runs unchanged, so RegisterMeeting(H3) precedes any set naming H3. Then arm `reconnecting`.
  - `InitialRegistrationFailed` for a grant OR a hand-off handler still Registering → `remove_handler_completely(id, ApplyLag)` (ApplyLag = "a load-dependent ApplyRefusal"; the RELEASED cause and EndMeeting cause both come from the one removal path). For a grant, record `admission_refused` with the migration's trigger. Acquisition continues (the id is now in removed → excluded). A hand-off failure owes a replacement with trigger `apply_lag`. No declaration counter: this is not a health declaration.
- Phase `reconnect`: observed once, when every admitted participant holds a live connectivity key on the new handler after it became Active. It is dropped if membership changes first; it is never a terminal outcome.
- `timed_out` is catalogued and zero-initialised but never emitted by the crash path; it is MC-F's (drain margin).
- **Exactly one terminal outcome per attempt**: completed, no_capacity, gc_unavailable, admission_refused, rate_limited (bound stop or min-interval deferral), abandoned (superseded grant, Rejected, malformed grant, add refusal, meeting ended with an attempt in flight). On meeting end, in-flight work and grants still Registering record `abandoned` from the exit path.
- Logs (INFO): migration start `{meeting_id, old handler_id, trigger}` and terminal `{meeting_id, old handler_id, new handler_id (if any), trigger, outcome}`. Ids are in logs only. No Debug of any struct holding tokens. New types hold ids, endpoints and instants only, never key material, so derived Debug is safe.

**4. Config (`config.rs` + ConfigMap + deployments + Configuration Reference rows)**
- `MC_MH_REPLACEMENT_MIN_INTERVAL_MS`, REQUIRED, default 1000, range [100, 60000]. Per-meeting minimum interval between Acquire attempts and backoff base.
- `MC_MH_REPLACEMENT_BACKOFF_MAX_MS`, REQUIRED, default 30000, range [min_interval, 300000]. Startup rejects max < min.
- Both are fail-loud on missing or invalid, like `MC_MH_HEARTBEAT_TIMEOUT_MS`. Rows go in `docs/runbooks/mc-deployment.md` Configuration Reference; coverage goes in env-test 02.

**5. Telemetry** (metrics.rs + catalog + mc-media.json panels + zero-init + ADR-0032 tests)
- `mc_media_handler_migrations_total{trigger, outcome}` and `mc_media_handler_migration_duration_seconds{trigger, phase}`, plus the three config gauges above.
- Every series carries `key_custody="operator"`. No id labels. env-test 32 stays green.
- Zero-init covers trigger × outcome for all 3 triggers (planned_transfer included) and the histogram per trigger × phase.

**6. Tests**
- Tier 1a: unit tests in `media_routing/replacement.rs`:
  - the ladder and its bounds;
  - budget arithmetic tied to the gauge;
  - the bound at cap − set;
  - deferral by the min interval;
  - the outcome per result arm;
  - `classify_acquire` per code, every arm.
- Tier 1b: `tests/handler_replacement_integration.rs` (paused time, fake MHs via `MockMhRegistrationClient`, a GC mock in `mc-test-utils/src/mock_gc.rs` implementing `HandlerAcquirer`: scripted results, a call log, a held reply on a oneshot):
  - Stranded A/B (assert on ONE shared sequenced event log that RegisterMeeting(H3) precedes the set naming H3; H3 snapshot A↔B edges; `unreachable_sender_ids` empty for both).
  - Two-handler with UNAVAILABLE (survivors re-place, no GC).
  - Each GC outcome arm.
  - Awaiting-handler then a later grant (the gauge goes up then down).
  - FAILED_PRECONDITION/INVALID_ARGUMENT: the call count stays at 1, paired with a RESOURCE_EXHAUSTED arm showing > 1.
  - Every grant refused at admission: exactly cap − set attempts, then `rate_limited`, then no further calls.
  - A superseded late grant via a held reply (positive arm: an unsuperseded grant registers).
  - The initial-handler registration failure → replacement.
  - Metrics through `MetricAssertion`, with unobserved semantics for "not counted"; a zero-init test for the full catalog.
- `gc_integration.rs`: one test with cap 0 → fail, absent → fail, 16 → registered.
- Env-tests (40_resilience.rs, paired with test; their proposal adopted):
  - EXTEND the MC-B2 crash leg into "crash with replacement". Per participant, a new ref with len == pre-kill len. Break-before-make order from history. The token is present and `reach` succeeds on the new entry. `migrations_total{trigger="heartbeat_timeout",outcome="completed"}` rose. `sum(gc_mh_replacement_requests_total{outcome="granted"})` rose over the serving GC pods, with gc_view not pinned.
  - A NEW stranded-pair leg as test specified (the unreachable signal is `StreamAssignments.unreachable_sender_ids`).
  - Ceilings derive from the N gauge + settle window + `mc_media_handler_replacement_retry_budget_seconds`.
  - `assert_addresses_unique()` after each leg. Participants leave inside the leg before teardown.
  - The zombie leg is rechecked so it does not pin the set size.

**Semantic-guard items 1–6:** confirmed as above. The public key only; the existing PushTarget/`register_meeting_request` builder; the typed `push_handler_set` path; no Debug on token-bearing types; Acquire and backoff in a spawned task or timer; Status code and message kept in `AcquireResult`; the outcome per exit path enumerated in §3.

**Gate-1 resolutions (round 1):**
- (c) DECIDED: a min-interval deferral is NOT a terminal outcome. The unit is one owed replacement, and each records exactly one terminal outcome. `rate_limited` is recorded only when the attempt bound stops the meeting. Rationale: the contract row says "terminal per meeting migration", and counting deferrals would give one migration two outcomes. A flap still shows: on declarations, and on the bound stop once its grants run the bound out. Deferrals are visible in the start/terminal log timing. Observability is asked to confirm. Tier 1b asserts exact per-migration deltas (deferral arm: completed +1 and rate_limited unobserved), plus a property check: the sum of terminal deltas equals migrations started.
- Registration term of the env-test ceiling: no literal. `pusher::initial_registration_bound()` = `MAX_REGISTER_ATTEMPTS × (MH_RPC_TIMEOUT + MH_CONNECT_TIMEOUT) + Σ REGISTER_BACKOFF_DELAYS` becomes the ONE derivation (unit-tested against the constants). It is folded INTO `mc_media_handler_replacement_retry_budget_seconds` = acquisition ladder (to the first capped step) + one backoff_max + the registration bound. So the awaiting-handler page bound and the env-test ceiling read one gauge.
- GC: the `|exclude| < cap` check carries a comment that it is deliberately one stricter than GC's `len > cap` rejection (it is the task's cap − set bound). A grant whose id is already in exclude is discarded, `abandoned` + ERROR, and never reaches `add`. A malformed OK is `abandoned`, no retry, with an ERROR naming the code and the missing field. Tier 1a has one arm per missing field (no assignment, empty mh_id, empty webtransport, empty grpc).
- (a) A dedicated Tier 1b arm covers a hand-off handler that fails registration: it is removed, a replacement is granted, and the set names only the replacement. T3 is unchanged. Any existing test that assumed "stays Registering forever" is reported here, never silently rewritten.

**Gate-1 resolutions (round 2):**
- Security S1: the UNAUTHENTICATED and PERMISSION_DENIED arms log WARN on every attempt with the bounded `code`. The request, the headers and `Request<T>` are never logged or Debug-printed. GC's `Status.message` goes to logs only and never reaches a client path.
- Security S2: ONE intake predicate `handler_endpoint_from_assignment(&MhAssignment) -> Result<HandlerEndpoint, AssignmentRejected>` rejects an empty mh_id, an empty webtransport endpoint or an empty grpc endpoint. It is used by BOTH the hand-off (`grpc/mc_service.rs` `handler_set_from_handoff` / store path) and the grant. This tightens the hand-off by also rejecting an empty mh_id/url there. Added row: `crates/mc-service/src/grpc/mc_service.rs` (Mine).
- Security S3: the bound is compared in `usize` (`exclude.len() < cap.get() as usize` via `usize::try_from`, with failure treated as at-bound). There are unit tests at cap−1 (allowed) and cap (stopped). A comment names GC's `validate_acquire` `len > cap` as the outer wall and says MC is deliberately stricter by one.
- Security S4: the superseded-grant test asserts on the shared event log that there was no RegisterMeeting to that id, no set named it, and no `media_dial` token was minted for it (aud never equals it). The positive arm is the control.
- Code-quality 1: the counting unit is documented once, on `MigrationOutcome`: one terminal outcome per owed replacement. A deferral is not an outcome; `rate_limited` is the bound stop only. This is pinned by the property test.
- Code-quality 2: `RemovalTrigger` for a registration failure lives in one named fn, `replacement::registration_failure_trigger()`, which cites ADR-0039 §3.
- Code-quality 3: `reconnecting` is a `BTreeMap<HandlerId, …>`, keyed like `registering`.
- Code-quality 4: a bounded structural channel replaces the unbounded one: `Option<oneshot::Receiver<AcquireReport>>` held by the replacement state, with a select arm armed only while `in_flight`. At most one report can exist by construction.
- `meeting.rs` grows only by the select arms and delegating calls. The logic lives in `actors/meeting/replacement.rs` and `media_routing/replacement.rs`.
- DRY: the per-attempt delay and `retry_budget()` both come from one ladder fn. Registration `BACKOFF_*` in gc_client stays separate.

**Gate-1 resolutions (round 3, operations):**
- Ops 1: `GcClient::acquire` uses the same `self.channel` (with `.timeout(GC_RPC_TIMEOUT)` and `connect_timeout`) as register, so a hung GC resolves as DEADLINE_EXCEEDED → `gc_unavailable` within the RPC timeout and `in_flight` cannot wedge. `HandlerAcquirer` sits above the channel, so a Tier-1b mock cannot exercise the tonic timeout. Coverage is instead a gc_client test against a lazily-connected channel to a GC that accepts but never answers (paused time where possible; otherwise a sub-second-timeout channel via `with_channel`), asserting `Unavailable{DeadlineExceeded|Unavailable}`.
- Ops 2: `mc-deployment.md` Pre-Deployment Coordination gets a bullet: forward GC ≥ GC-3 before MC; rollback MC before GC goes below GC-3; the symptom. The `McError::Config` text and the ERROR log name the field and the cause ("GC sent max_exclude_handler_ids=0 (absent): GC predates story 4 GC-3 — roll GC forward or MC back") and point to the GC runbook anchor.
- Ops 3: the Configuration Reference rows cover required, default, range, the max < min rejection, the under-load trade-off, and the retry budget formula with its computed default value and gauge name (the awaiting-handler page derives from it). Also: image-only rollback is safe (an old image ignores the keys); roll-forward needs the keys in the same apply. Env-test 02 covers both keys on mc-0 AND mc-1.
- Ops 4a: the initial-registration-failure path emits the INFO migration-start log with `reason="initial_registration_failed"`.
- Ops 4c: the rate_limited semantics stay as decided in round 1 (the bound stop only; observability owns the final call). Every rate decision is logged with a reason: deferral INFO `reason="min_interval"` (non-terminal), terminal `reason="attempt_bound"`.

**Gate-1 resolutions (round 4, observability ruling on the counting unit; SUPERSEDES the round-1/round-2 "per owed replacement" unit):**
- The unit of `mc_media_handler_migrations_total` is ONE ACQUISITION ATTEMPT: one Acquire call, or one owed replacement refused at the attempt bound. Each attempt records exactly one outcome. completed / no_capacity / gc_unavailable / admission_refused / abandoned are per attempt, so failures during an ongoing outage are emitted as they happen (needed for the OBS-ALERTS "migration failures while a survivor exists" warning).
- A min-interval deferral is NOT an attempt and is never counted. It is logged INFO with `reason="min_interval"`.
- `rate_limited` = one owed replacement dropped at the attempt bound: once per owed replacement, including a later loss on an already-stopped meeting. The terminal log carries `reason="attempt_bound"`.
- Per-migration completion = completed ÷ declarations_total{trigger!="planned_transfer"}, never ÷ sum(migrations_total). The catalog entry says so in words: "per acquisition attempt; a migration that retries contributes one failure outcome per failed attempt and at most one completed; not a migration count". It also records this clarification of the contract row's "terminal per meeting migration".
- Tier 1b: exact per-arm deltas; a property check that the sum of outcome deltas == Acquire calls made + bound refusals; a deferral arm (no outcome for the deferral, then the later attempt's own outcome).
- Reconnect phase: snapshot the participants admitted at Active; ignore joiners; remove leavers from the waiting set (no discard); clear a pending reconnect on the next migration for that meeting and on meeting end. Observed when the waiting set empties.
- Histogram buckets: explicit, via `set_buckets_for_metric`, derived like `detection_seconds` from config (N, settle window, retry budget), so the replacement phase's top lands in a real bucket and not +Inf.
- Initial-registration-failure removal (apply_lag) touches NO declaration-side series (declarations, first_declarations, suspicions, detection, loss_*). Removal-release counters may move. The catalog notes for both series say apply_lag includes initial-registration failure.
- The retry budget gauge is computed by the same ladder fn + `initial_registration_bound()` that the scheduler/pusher use, and is unit-tested. The catalog row lists its three terms. All five series carry `key_custody="operator"`.

**Open questions put to reviewers (all answered):**
- (a) Initial handler on `InitialRegistrationFailed` → removed with apply_lag: ACCEPTED (security, ops, test, code-reviewer, observability). See round 2 (`registration_failure_trigger()`), round 3 (start-log reason), round 4 (no declaration series).
- (b) Malformed OK → `abandoned`, no retry, ERROR: ACCEPTED (GC, security, test, code-reviewer, ops). See round 1.
- (c) rate_limited semantics: DECIDED by observability in round 4 (per-attempt unit; deferral not counted; rate_limited per owed replacement dropped at the bound). Supersedes rounds 1 and 2.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Global Controller | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

### GC client (`grpc/gc_client.rs`, `grpc/mc_service.rs`)
| Item | Before | After |
|------|--------|-------|
| `max_exclude_handler_ids` | ignored | `accept_registration` (shared by register + re-register): 0/absent → non-retried `McError::Config` naming the field and cause; stored in an atomic, refreshed on re-registration |
| AcquireMediaHandler | — | `HandlerAcquirer` trait + `impl for GcClient` (same timeout-bounded channel); `classify_acquire` one arm per code; code + message kept |
| Intake predicate | hand-off rejected only empty grpc | ONE `handler_endpoint_from_assignment` (empty id / url / grpc) for hand-off AND grant; GC `selection_is_malformed` mirrored in lockstep (ANCHOR) |

### Replacement (`media_routing/replacement.rs` pure, `actors/meeting/replacement.rs` driver)
- THE ladder (`ladder_step`) feeds both `retry_delay` (earlier-only jitter, seeded via `MeetingSeams.replacement_seed`) and `retry_budget` (ladder to cap + one cap + `pusher::initial_registration_bound()`; 139 s at defaults).
- `ReplacementState`: target = hand-off size, owed FIFO, one in flight (version-tagged), registering grants, reconnect waits; bound `|exclude| < cap` (one stricter than GC).
- Driver: Acquire in a spawned task → oneshot select arm; superseded/excluded/unaddable grants `abandoned`; grant → `add_granted` (Registering) → existing push path; ack → `completed`; first-RegisterMeeting failure (grant OR hand-off) → removal with `registration_failure_trigger()` (apply_lag) → re-owed; meeting end → `abandoned` for every open attempt.
- Removal transaction step 6 feeds `after_handler_removed` (removal never waits on GC).

### Config / telemetry / docs
- `MC_MH_REPLACEMENT_MIN_INTERVAL_MS` [100, 60000] (1000), `MC_MH_REPLACEMENT_BACKOFF_MAX_MS` [min, 300000] (30000): config.rs, config.env, mc-0/mc-1 deployments, Configuration Reference rows, Pre-Deployment Coordination (GC ≥ GC-3) bullet, env-test 02.
- `mc_media_handler_migrations_total{trigger,outcome}` (21 cells zero-init), `mc_media_handler_migration_duration_seconds{trigger,phase}` (config-derived buckets), three config gauges; catalog section; mc-media "Handler Replacement" row (3 panels); telemetry-contract row synced to the per-attempt unit.

### Existing-test assumption changed (reported per test reviewer)
- `tests/heartbeat_intake_integration.rs::verdicts_all_arms_in_one_test` used `fail_handler(mh-b)` as "stays Registering forever". A failed first RegisterMeeting now removes and replaces the handler (ADR-0039 §3), so the arm uses `hold_handler` (push in flight, still Registering) and does not await the parked first join (awaiting it would run paused time past N). The arm's intent (Registering → OWNED) is unchanged.

### Tests
- Tier 1a: `media_routing/replacement.rs` (ladder, jitter bounds, budget, buckets, bound at cap−1/cap, state machine arms), config bounds, metric cell isolation + config gauges + zero-init render.
- Tier 1b: `tests/handler_replacement_integration.rs` (9: stranded pair with held RegisterMeeting proving register-before-token, survivors without GC, awaiting handler then grant, refusals not retried vs no_capacity retried, bound incl. second loss +2, superseded grant with no register/set/token, min-interval deferral uncounted, hand-off registration failure replaced, boot gauges); property "Σ outcomes == calls + bound refusals" in every arm.
- `tests/gc_integration.rs`: cap 0(=absent)/16/re-register 7; request shape + every status class + malformed-per-field; hung GC → retryable within the RPC timeout.
- Env-tests (`40_resilience.rs`): MC-B2 crash leg extended to "crash with replacement" (new ref at pre-kill size, break-before-make from history, token admits a dial, completed rose, GC granted rose summed over replicas); new stranded-pair leg; meetings drained inside each handler-loss leg; address invariant after each.

---

## Files Modified

```
 crates/env-tests/src/fixtures/metrics.rs           |  51 +-
 crates/env-tests/tests/02_mc_deployment_config.rs  |  17 +-
 crates/env-tests/tests/40_resilience.rs            | 467 ++++++++++-
 crates/gc-service/src/services/mc_assignment.rs    |  32 +-
 crates/mc-service/src/actors/meeting.rs            |  44 +
 crates/mc-service/src/actors/meeting/health.rs     |  29 +-
 .../mc-service/src/actors/meeting/replacement.rs   | 505 ++++++++++++
 crates/mc-service/src/actors/meeting_media.rs      |  66 ++
 crates/mc-service/src/config.rs                    | 130 +++
 crates/mc-service/src/grpc/gc_client.rs            | 339 +++++++-
 crates/mc-service/src/grpc/mc_service.rs           |  43 +-
 crates/mc-service/src/main.rs                      |  11 +-
 crates/mc-service/src/media_routing/mod.rs         |   7 +-
 crates/mc-service/src/media_routing/placement.rs   | 130 +++
 crates/mc-service/src/media_routing/pusher.rs      |  14 +
 crates/mc-service/src/media_routing/replacement.rs | 885 +++++++++++++++++++++
 crates/mc-service/src/observability/metrics.rs     | 247 +++++-
 crates/mc-service/src/webtransport/server.rs       |   8 +
 crates/mc-service/tests/common/accept_loop_rig.rs  |   2 +
 crates/mc-service/tests/common/mod.rs              |  52 ++
 crates/mc-service/tests/gc_integration.rs          | 293 ++++++-
 .../tests/handler_replacement_integration.rs       | 815 +++++++++++++++++++
 .../tests/handler_replacement_log_reasons.rs       | 125 +++
 .../tests/heartbeat_intake_integration.rs          |  12 +-
 .../tests/otel_grpc_outbound_integration.rs        |   2 +
 crates/mc-test-utils/src/mock_gc.rs                | 127 +++
 .../telemetry-contract.md                          |   2 +-
 docs/observability/metrics/mc-service.md           |  69 +-
 docs/runbooks/mc-deployment.md                     |  50 +-
 .../meeting-controller/INDEX.md                    |   8 +-
 infra/grafana/dashboards/mc-media.json             | 298 ++++++-
 infra/services/mc-service/config.env               |  28 +
 infra/services/mc-service/mc-0-deployment.yaml     |  14 +
 infra/services/mc-service/mc-1-deployment.yaml     |  14 +
 34 files changed, 4786 insertions(+), 150 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/mc-service/src/grpc/gc_client.rs` | `accept_registration` (cap 0/absent fails both register paths); `HandlerAcquirer` impl; `classify_acquire` per-code arms |
| `crates/mc-service/src/media_routing/replacement.rs` (new) | Pure policy: backoff ladder, retry budget, attempt bound, outcome/phase enums, `ReplacementState` |
| `crates/mc-service/src/actors/meeting/replacement.rs` (new) | Actor driver: spawned Acquire + oneshot result arm, version-superseded discard, `end_attempt` single outcome site |
| `crates/mc-service/src/media_routing/placement.rs` | `TryFrom<&MhAssignment> for HandlerEndpoint` intake; `MeetingHandlers::is_registering` |
| `crates/mc-service/src/config.rs`, `infra/services/mc-service/*` | `MC_MH_REPLACEMENT_MIN_INTERVAL_MS`, `MC_MH_REPLACEMENT_BACKOFF_MAX_MS` (required, range-validated) |
| `crates/mc-service/src/observability/metrics.rs` | migrations_total, migration_duration_seconds, 3 config gauges; zero-init |
| `crates/gc-service/src/services/mc_assignment.rs` | `selection_is_malformed` lockstep mirror (Minor-judgment, global-controller confirmed) |
| `crates/env-tests/tests/40_resilience.rs` | Crash-with-replacement leg; stranded-pair leg; hygiene drain wait |
| `crates/mc-service/tests/handler_replacement_integration.rs` (new) | Tier 1b paused-time suite (stranded pair, per GC outcome, bounds, superseded grant) |
| `docs/runbooks/mc-deployment.md` | Config Reference rows, GC-3 skew, MC-C rollback |

---

## Devloop Verification Steps

Gate 2 run 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, log `/tmp/all2.log`): `TOTAL_RESULT=OK`, 2187s.

### Layer 1: Compile
**Status**: PASS (6s)

### Layer 2: Format
**Status**: PASS (7s)

### Layer 3: Guards
**Status**: PASS (120s)

### Layer 4: Tests
**Status**: PASS (135s) — cargo-nextest, doctests, nx test

### Layer 5: Lint
**Status**: PASS (1s) — clippy, buf lint, nx lint

### Layer 6: Audit
**Status**: PASS (6s) — no dep changes; buf breaking passed

### Layer 7: Env-tests
**Status**: PASS (1912s) on attempt 2.
Attempt 1 (`/tmp/all1.log`) FAILED: 3 legs of 40_resilience panicked at a baseline read because the round-1 hygiene drain helper fed bare `sum(mc_meetings_active)` into `instance_counter_map` (requires `instance`). Fixed to `sum by (instance)` + caller-side sum; implementer verified 40_resilience 8/8 directly, then the full Layer 7 passed.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | S1–S4 plan asks verified in code |
| Test | RESOLVED-FIXED | 7 | 7 | 0 | break-before-make anchor, hygiene drain wait, re-placement proof, call-time deferral proof, per-phase histogram, dead const, shared gauge helper |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | `by (instance)` on 5 config panels; admission_refused wording widened + reason log |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 | end_attempt single site; TryFrom<&MhAssignment>; split retry bool; loud invariant returns; is_registering |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | hand-rolled counter poll → `rose()` |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | runbook skew symptom, stale checklist, handler_replaced text, MC-C rollback entry |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | re-checked after round-1 fixes |
| Paired Global Controller | CLEAR | 0 | 0 | 0 | confirms Minor-judgment edit to gc-service mc_assignment.rs |


### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No findings. Gate-1 asks S1 (auth WARN, bearer never logged), S2 (one intake predicate for hand-off + grants), S3 (no truncating cast on cap; cap−1/cap tests), S4 (superseded grant leaves no trace) verified.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 7 found, 7 fixed, 0 deferred

Break-before-make anchor, hygiene drain wait, re-placement proof, call-time deferral proof, per-phase histogram, dead const, shared config-gauge helper.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

Config panels `by (pod)` → `by (instance)` (incl. 2 pre-existing MC-D panels); admission_refused wording widened + terminal log reason.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 5 fixed, 0 deferred

`end_attempt` single outcome site; intake predicate → `TryFrom<&MhAssignment>`; boolean `retry` split; loud invariant returns; `MeetingHandlers::is_registering`.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- D1: hand-rolled counter-poll in `gc_granted_rose` → in-file `rose()` (fixed)

**Extraction opportunities** (appended to `docs/TODO.md`):
None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 4 found, 4 fixed, 0 deferred

Runbook: GC-3 skew symptom for running pods, stale awaiting-handler checklist, stale handler_replaced text, MC-C rollback entry.

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found, 0 fixed, 0 deferred

No findings (Credential Leak, Client Credential Lifetime, Actor Blocking, Error Context Preservation, Metrics Path Completeness).

### Paired Global Controller
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

AcquireMediaHandler consumer matches GC contract; confirms Minor-judgment edit to `crates/gc-service/src/services/mc_assignment.rs`.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `e08bee53456a7b208c6b005f849c1473b0046340`
2. Review all changes: `git diff e08bee53..HEAD`
3. Soft reset (preserves changes): `git reset --soft e08bee53`
4. Hard reset (clean revert): `git reset --hard e08bee53`
5. No schema changes.
6. Infra: mc-0/mc-1 deployments and config.env gain two required keys; reverting the image without reverting manifests is harmless (unknown env ignored); reverting manifests without the image fails MC startup.
7. **Safe-revert unit**: **Answer**: `the whole commit` — MC image + its config.env/deployment keys + dashboard/catalog move together; the gc-service `selection_is_malformed` mirror is lockstep with MC's intake predicate (ANCHOR) and must not be reverted alone. Deployment order: GC must be at GC-3 before this MC rolls out; on rollback MC goes back before GC drops below GC-3 (runbook §Rollback).

---

## Issues Encountered & Resolutions

### Issue 1: Layer 7 attempt 1 — hygiene helper query shape
**Problem**: `sum(mc_meetings_active)` passed to `instance_counter_map`, which requires an `instance` label; 3 resilience legs panicked before any assertion.
**Resolution**: `sum by (instance)(...)` + caller-side sum; Layer 7 green on attempt 2.

### Issue 2: migrations_total counting unit
**Problem**: Contract said "terminal per meeting migration"; per-migration counting would hide no_capacity/gc_unavailable during an outage.
**Resolution**: Observability ruled per-acquisition-attempt counting; telemetry-contract.md + catalog updated in the same commit.

---

## Lessons Learned

1. Env-test hygiene helpers added during review need the same live-cluster run as evidence code — the review-round change was the only Layer-7 failure.
2. Completing an ADR invariant (initial hand-off handler that never acks also takes the replacement path) was cheap when done alongside the new path.

---

## Appendix: Verification Commands

```bash
DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh
DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh
./scripts/guards/simple/validate-cross-boundary-classification.sh docs/devloop-outputs/2026-11-12-mc-crash-replacement-via-gc/main.md
```
