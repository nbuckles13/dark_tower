# Devloop Output: C1 — SDK reconcile-to-set transport, per-handler dial credentials, MH close codes, per-transition loss reports

**Date**: 2026-11-10
**Task**: Story 2026-10-31-survive-handler-loss task #17 (C1) — see task prompt `task-17.prompt` (ADR-0039 §4/§5/§7)
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired: test, security; conditional: protocol; semantic-guard spawned — credential-lifetime surface)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ddd9cfd59a4c19bd2db7ce24cda6139175a3c141` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Infrastructure (conditional, guard hunk) | `spawned` |
| Protocol | `spawned` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |

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
C1 of story `docs/user-stories/2026-10-31-survive-handler-loss.md`: SDK reconcile-to-`MediaHandlerSet` transport (join and migration share one path), per-ref media_dial credentials held only in MediaTransport private set state, MhCloseCode handling, per-ref-transition loss reports, refs on send/receive paths, e2e bus `handlerSets` migration (ADR-0039 §4/§5/§7).

### Scope
- **Service(s)**: client (sdk-core, sdk-svelte, web-app, test-utils); guard script (infra-owned hunk); dt-guard fixtures (security-owned content); metrics catalog sentences (observability).
- **Schema**: No
- **Cross-cutting**: Yes (client-only behaviour; no proto/server code changes)

### Debate Decision
NOT NEEDED - governed by ADR-0039 and debate `docs/debates/2026-10-28-mh-replacement-and-migration/`.

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
| `packages/sdk-core/src/**` (MediaTransport reconcile, new `media/handlerSet.ts`, `media/redialBackoff.ts`, `media/closeCode.ts`, `signaling/handlerSetIntake.ts`, ref-keyed pipelines, session wiring, clientConfig, limits, and every unit test touched) | Mine | — |
| `packages/test-utils/src/**` (new `FakeScheduler.ts`, barrel export) | Mine | — |
| `packages/sdk-svelte/src/**` (ref-keyed `MeetingStore`, `mediaCells.sameAssignment`, `bindMeetingSession`, tests) | Mine | — |
| `packages/web-app/src/**` (`lib/e2eBus.ts` handlerSets, `lib/testLevers.ts`, `views/MeetingRoom.svelte`, `globals.d.ts`, tests) | Mine | — |
| `packages/web-app/tests/handlerSetConfigDocs.test.ts` (new) | Mine | — |
| `packages/web-app/README.md` | Mine | — |
| `packages/web-app/e2e/fixtures.ts` | Mine | — |
| `packages/web-app/e2e/cohortContexts.ts` | Mine | — |
| `packages/web-app/e2e/videoScenario.ts` | Mine | — |
| `packages/web-app/e2e/env.ts` (comment) | Mine | — |
| `packages/web-app/e2e/README.md` | Mine | — |
| `packages/web-app/e2e/partial-connectivity.spec.ts` | Not mine, Minor-judgment (spec migration off `joined.mediaServers`, behaviour unchanged) | test |
| `packages/web-app/e2e/solo-participant.spec.ts` | Not mine, Minor-judgment (same migration) | test |
| `packages/web-app/e2e/join-happy-path.spec.ts` | Not mine, Minor-judgment (same migration) | test |
| `docs/runbooks/client-dev-local.md` | Mine | — |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |
| `docs/observability/metrics/client.md` (one-sentence note on `dt_client_mh_connection_total`; shape unchanged) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` (one sentence at `mc_participant_mh_status_total`: per-ref transitions since C1) | Not mine, Minor-judgment | observability |
| `scripts/guards/simple/validate-keyframe-floor-sync.sh` (generalised to a row-driven pair table: keyframe pairs + mh-keepalive pair, one `extract()` kernel, overlay fail-closed check) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/validate-keyframe-floor-sync.test.sh` (keepalive + token-bound arms, overlay rc/unreadable/missing arms) | Not mine, Minor-judgment | infrastructure |
| `scripts/layer3.sh` (comment above the floor-sync self-test registration only) | Not mine, Mechanical | infrastructure |
| `infra/services/mh-service/config.env` (one-line pointer comment at `MH_KEEPALIVE_INTERVAL_MS`) | Not mine, Minor-judgment (approved by infra) | infrastructure |
| `crates/dt-guard/tests/fixtures/ts_retained_credentials/lens/**` (5 new dial-token lens poles) | Not mine, Minor-judgment (policy content) | security |
| `crates/dt-guard/tests/ts_retained_credentials_fixtures.rs` (lens-count bump 3 -> 8) | Not mine, Mechanical | security |
| `scripts/guards/semantic/checks.md` (Known-blind-spots bullet: poles now planted, named) | Not mine, Minor-judgment (policy record) | security |

Note: the floor-sync guard now also READS `crates/common/src/jwt.rs` (`MAX_JWT_SIZE_BYTES`, a GSA auth/crypto path) to pin the SDK token-length bound; that file is NOT edited.

---

## Planning

### Mechanism restatement
Instance: "replace connectAll with reconcile". Mechanism: **the handler identity on the client becomes `handler_ref`, everywhere** — the transport, the selectors, every downstream per-handler map (egress lanes, read loops, hop monitors, recovery queues), the public event payloads, the svelte stores and the e2e bus. The task names sites in sdk-core and four e2e files; the same rule ("nothing keys on / reads the handler URL except the set") also reaches `solo-participant.spec.ts`, `join-happy-path.spec.ts`, `MeetingRoom.svelte`+`testLevers.ts` (lever check), `sdk-svelte` `mediaCells.sameAssignment` + `MeetingStore.applyMediaConnected`, and `MediaConnectionError` docs (dry-reviewer list). All are in scope here (complete the invariant). No `proto/**` / `proto-gen/**` edits: P2's generated TS is consumed as-is (no regen).

### Design

**D1. One identity type.** `media/handlerSet.ts` exports a branded `HandlerRef` (`number & {__brand}`), the public token-free `HandlerSetView {version: bigint, handlers: readonly {ref, url}[]}`, the internal `HandlerSetIntake` (view + per-ref token, never exported from `index.ts`), and the pure `diffHandlerSet(prevRefs, next) -> {added, removed, retained}` keyed on ref ONLY. Every selector and downstream map switches its key type to `HandlerRef`, so a leftover URL key fails tsc. URL-keyed sites converted: `MediaTransport` (getDatagramChannel / openUniStream / incomingUniStreamsFor / sendRecoveryRequest), `egress.ts senderFor`, `videoEgress.ts setTargets`/`GroupStreamOpener`, `receiveTransports.ts` (PerHandlerReadLoops + ReceiveTransports + hop monitors), `recoveryRequestQueue.ts`, `AudioPipeline.ts` (~793-803), `VideoReceiver.ts` (~322 + options), `VideoPublisher.ts openStream`, `mediaStart.ts` wiring, `mediaWiring.ts` (sendDirectiveListener, assignments), `SignalingClient.ts` (675/731/745/943), `signaling/events.ts` (`SendStreamDirective.targets: HandlerRef[]`, `StreamAssignmentEvent.handlerRef: HandlerRef | undefined` replacing `mediaHandlerUrl`, `JoinedEvent.mediaServers` removed → `handlerSet: HandlerSetView`).

**D2. Intake (token hygiene, protocol #6 / security S3).** `SignalingClient` decodes `JoinResponse.media_handler_set` and `ServerMessage.media_handler_set` at ONE function `takeHandlerSet()` (new, beside `takeMeetingKek`, `signaling/handlerSetIntake.ts`): validates, projects to `HandlerSetIntake`, hands it to a private `handlerSetSink` option (same seam shape as `kekSink`), then **blanks every `mediaDialToken` on the decoded message**. Public surface gets only `HandlerSetView` (`JoinedEvent.handlerSet`, new signaling/session event `mediaHandlerSet`). Validation (untrusted input, protocol #1 / security S7), each a non-fatal per-entry contract fault through ONE event-once `[dt-signaling-contract]` WARN (`FaultOnce`, fixed messages, no ref/url/token in text): ref 0; duplicate ref; duplicate URL; known ref whose URL changed; empty or oversized token (new `MAX_MEDIA_DIAL_TOKEN_LENGTH` in `validation/limits.ts`); set larger than new `MAX_HANDLER_SET_SIZE` (entries past the cap dropped). A faulted entry is dropped from the applied set (so it is a non-member — never dialed, never silently redirected). For a duplicate ref or duplicate URL, EVERY entry involved is dropped (no wire-order dependence; security); a connected ref so dropped goes through `#removeRef`. `SignalingClient` keeps only the current member-ref set (no tokens) for membership checks.

**D3. Refs on directives/assignments (protocol #2).** Membership checked at decode in SignalingClient against the latest set: `SendTarget.handler_ref` non-member (incl. 0) → dropped from `targets` + event-once WARN. `StreamAssignment.handler_ref` 0 on UNAVAILABLE / FEWER_SOURCES_THAN_SLOTS / AVATAR_NO_VIDEO → `handlerRef: undefined`, silent; 0 or non-member on any other state → `handlerRef: undefined` + event-once WARN. Assignments are re-checked on emit (stored assignments re-emitted after a set change). Never through the `not_connected` drop. Superseded fields (`media_servers`, both `media_handler_url`s, `mh_url`) are never read; `mh_url` is left empty on send.

**D4. MediaTransport = reconcile-to-set.** `connectAll` and `getStatusReports` are removed. ONE record per ref: `{ref, url, index, state: 'connecting'|'connected'|'lost'|'parked'|'removed', transport?, generation, attempt, redialTimer?, graceTimer?, control?, datagram?}` (no parallel maps). Tokens live in ONE separate private field (D6). API:
- `applyHandlerSet(intake)` (the sink): replace tokens wholesale; diff; removed → `#removeRef`; then, if armed, `#reconcile(joinToken = '')` dials every in-set ref in no live state (`connecting`/`connected`/`parked`/redial-pending are skipped). Token-only refresh ⇒ no dial, no close (diff empty). A new ref at a used URL is an add + the old ref's removal ⇒ real close + fresh `connectFn` (no URL dedupe; `allowPooling` false unchanged).
- `arm(joinToken): Promise<void>` — called ONCE by `MeetingSession` after the signaling join resolves, with the call-scoped JWT; runs the same `#reconcile(joinToken)` over the latest set and resolves when those dials settle. Sets arriving before `arm` are stored, not dialed (closes the "set in the same read chunk as the JoinResponse" race). Empty set at arm ⇒ resolves at once, starts the empty-set notice timer.
- `#dial(ref, joinToken)` — THE one dial path (join, add, redial, post-park): builds `MhConnectRequest{joinToken, mediaDialToken: #dialTokens.get(ref)}` at write time; same deadline/datagram-queue/control-channel code as today. A dial whose ref left the set (generation mismatch) is closed when it resolves and emits nothing (no metric, no report, no event).
- `#removeRef(ref)` — THE one removal-cleanup function: mark removed; cancel redial/grace timers and park; drop token; emit `handlerRemoved(ref)` SYNCHRONOUSLY so the session's one forgetter (D5) stops read loops quietly and prunes lanes BEFORE the close; then close the transport (a server-left-OPEN transport included) / abort+close an in-flight dial. No fault, no report. Its later `closed` settlement is ignored (state removed). `disconnect()` runs `#removeRef` over every ref, then clears the empty-set timer and `#dialTokens`.
- On `closed` of a live in-set ref: classify with `closeCode.ts` (D7); report/redial/park per table.

**D5. Downstream per-ref forget (dry #5).** One `forgetHandler(ref)` wired in `mediaStart.ts` to `transport.on('handlerRemoved')`, calling per-owner stops: `Egress.forget(ref)` (drop sender/lane), `VideoEgress.forget(ref)` (prune lane from `setTargets` retention), `PerHandlerReadLoops.stop(ref)` (quiet: sets a per-ref stopped flag checked before every fault, cancels reader; deletes hop monitor), `RecoveryRequestQueue.drop(ref)`. Slot overlay clearing is C2. No SessionMedia / TransmitKeyManager rebuild on any set change (item 8): new egress targets start `awaiting_group` via the existing videoEgress owner.

**D6. Credentials (security S1-S6, semantic-guard 1-7).** `#dialTokens: Map<HandlerRef, string>` — one private `#` field on MediaTransport, scoped-retention comment at the field citing ADR-0039 §5 "Client retention"; rebuilt as a NEW Map per set (never merged, never gated on version), deleted in `#removeRef`, cleared in `disconnect()` (reached on every MeetingSession end path incl. join failure). No getter, never on a returned/emitted object. Timers and dial closures capture the REF and read the map at fire time; absent ⇒ no dial. Opaque: no split/atob/decode; DECLINED park release = string inequality at replace time (a per-ref `parked` flag cleared when the incoming token differs; no copy of the declined token).
**The join JWT is NOT retained** (security S1 / checks.md scoped exception). It is a parameter of `arm(joinToken)` only, used by the dials `arm` triggers. Every later dial (add, redial, retry, post-park, first non-empty set after an empty join set) sends `join_token = ""` + `media_dial_token`. Interim consequence until MH-1: such dials are DECLINED by MH → park → FAILED → MC re-push → exactly one dial per newer token, bounded by MC's FAILED re-push rate limit, never a client tight loop. Written at the field and here so nobody "fixes" it by retaining the JWT.

**D7. Close codes (`media/closeCode.ts`).** `classifyClose(info | 'loss') -> 'retry' | 'released' | 'declined' | 'loss' | 'contract_fault'`, exhaustive `switch` over generated `MhCloseCode` with a `never` check (no numeric literals). Only `closeInfo.closeCode` is read; `reason` never read or logged. `closed` rejects ⇒ loss.
| Outcome | Report | Next |
|---|---|---|
| WRONG_HANDLER / BINDING_CONFLICT / MC_UNAVAILABLE | none | counted; backoff redial while in set (token reused at fire time) |
| HANDLER_RELEASED, ref already removed | none | none |
| HANDLER_RELEASED, ref in set | DISCONNECTED after grace if no set update arrives | redial (a set update during grace that removes it ⇒ removal; that keeps it ⇒ redial at once, no report) |
| DECLINED | FAILED | park; one dial on a set with a different token |
| no code (closed rejects) | DISCONNECTED | backoff redial |
| 0 / unknown | DISCONNECTED (as loss) | counted + event-once `[dt-signaling-contract]` WARN; backoff redial; never fatal |
| dial failure (timeout/transport) | FAILED | backoff redial |
| dial success | CONNECTED | attempt counter reset |
"Counted" = in-memory per-outcome counters on MediaTransport exposed by a numbers-only `diagnostics()` snapshot (allowlisted fields) for tests; NO new metric / label / status value (C2 owns R-42; observability #2).

**D8. Backoff (`media/redialBackoff.ts`, pure, injected `random`).** `delay(n) = (n == 0 ? MH_KEEPALIVE_FLOOR_MS : 0) + U[0, min(cap, base·2^n)]` — the floor is ADDED on the first retry so random()=0 still honours "no earlier than one keepalive" and the first retry keeps its jitter (a `max(floor, …)` would collapse every crash survivor's first redial onto the same instant). `MH_KEEPALIVE_FLOOR_MS = 10_000` is a compiled ANCHOR (DRY) constant in `clientConfig.ts` naming `MH_KEEPALIVE_INTERVAL_MS`, pinned by the generalised `scripts/guards/simple/validate-keyframe-floor-sync.sh` pair table against `infra/services/mh-service/config.env`. ADR-0039 §4 names the keepalive interval explicitly, so that is what is anchored (ops #2 noted; MH's idle timeout ≈ N is MH-3's knob).
Thundering herd on ADD (ops #3): ADR §4 says the client "dials members it is not connected to"; first dials of added members are immediate (recovery latency is the story's objective). Bounded server-side by MH's per-(meeting, sub) pending cap (§5). Recorded, not changed.

**D9. Config (4 keys, `MediaConfig.handlers`).** `redialBaseMs` default 1_000 range [100, 60_000]; `redialCapMs` default 30_000 range [1_000, 300_000], validated base ≤ cap; `emptySetNoticeMs` default 15_000 range [1_000, 300_000]; `releasedGraceMs` default 2_000 range [0, 30_000]. Validated in `validateMediaConfig` with `ClientConfigError` naming the key (no clamping). New `handlerSetConfigLogLine(media, source)` → one `[dt-media-handlers-config]` line logged once per session at `MeetingSession` arm (numbers + configured|default + the compiled floor), unit-tested. Injected into MediaTransport via `MediaTransportOptions` (no hardcoded defaults in MediaTransport). Docs: defaults' SSoT is `clientConfig.ts`; README §Configuration and `client-dev-local.md` restate default+range, and a node-tier test in `packages/web-app/tests/` asserts the README table matches the SDK defaults (drift caught).

**D10. Seams.** `MediaTransportOptions` gains `scheduler {setTimeout, clearTimeout}` (shape of `MeetingKekHolderOptions`) and `random`. `test-utils` gains a hand-driven `FakeScheduler` (list pending, read delay, fire one) and a per-call connect factory returning a NEW `MockWebTransport` per call (existing `makeConnect` dedupes per URL and would hide G3). Connect deadline uses the scheduler too.

**D11. Loss reports (item 7).** MediaTransport emits `statusChanged {handlerRef, handlerSetVersion, state: connected|failed|disconnected, failureCode?, failureReason?, observedAtMs}` on each per-ref transition in D7's table; `MeetingSession` forwards each as one `MediaConnectionUpdate` (best-effort, catch → bounded warn; caps from `limits.ts` unchanged). `MhConnectionStatusReport` reshaped to ref + version (bigint echoed, no Number() round-trip); `mh_url` left empty. No report for removals or closes after a removal.

**D12. Join contract.** `join()` awaits `media.arm(jwt)`. **Proposal (Lead to adjudicate):** keep today's join gate — a NON-EMPTY join set whose first dials ALL fail rejects `ALL_FAILED` (`failure_stage=mh_connect`), exactly as today; an empty join set completes ("media connecting"). Rationale: keeps `dt_client_join_attempts_total` semantics and existing tests unchanged; under D6 a retry of a failed join-time dial would carry `join_token=""` and be DECLINED pre-MH-1 anyway, so tolerating all-fail at join buys nothing until MH-1. The dial path itself is shared (no join special case in dialing). Alternative: never fail join on MH outcomes (retire the `mh_connect` stage) — deferred to P3/MH-1 if wanted.

**D13. Empty set.** At `arm` with an empty set, a scheduler timer of `emptySetNoticeMs`; the first set with ≥1 valid member cancels it; on expiry emit exactly one session event `mediaHandlersOverdue` (no payload beyond nothing; non-fatal, session kept). Rendering is C2.

**D14. Metrics (item 9, observability #1/#3).** `#emitMetric` body, comment block and the spread untouched; called once per dial outcome (join, add, redial, post-park) with `mh_index_bucket` = the ref's index in the set that triggered that dial; aborted-by-removal dials emit nothing. `dt_client_time_to_first_mh_connected_ms` latch stays session-scoped in MeetingSession (never reset; not fired by token refresh). Spread comment + its pins unchanged (C2 flips them). Doc note sentence in `client.md`.

**D15. Svelte / web-app / e2e.** `mediaConnected` payload → `MediaHandlerView {ref, url}`; `MeetingStore` keys connections by ref; `mediaCells.sameAssignment` compares `handlerRef`. `e2eBus`: `handlerSets` ordered history of allowlisted `{version: string, handlers:[{ref, url}]}` (field-by-field, never spread; version stringified for JSON), fed by `joined.handlerSet` + `mediaHandlerSet`; `joined` bus entry drops `mediaServers`; `mediaConnected {handlerRef, url}`. `MeetingRoom` lever check uses `joined.handlerSet` URLs. e2e: `waitForAllMediaConnected(page)` reads refs from the latest bus handlerSet (keeps "must not be empty"); `blockHandlers` takes URLs from handlerSets; migrate fixtures.ts, cohortContexts.ts, videoScenario.ts, partial-connectivity / solo-participant / join-happy-path specs; `e2eAnalysis.ts`, `e2eBus.test.ts`. Goal: zero `mediaServers` under `packages/`.

**D16. Ops F-entries** in `client-dev-local.md`: non-member ref / unknown close code WARN, empty-set notice, parked-DECLINED (where to look: the WARN tag, the `mediaHandlersOverdue` event, MC's `mc_media_handler_set_pushes_total{reason=failed_report}`). Rollback: revert the whole commit, no server action (MC/MH are additive and still populate legacy fields).

### Tests (paired-test G1-G12, security S10, observability #3/#4, ops #5)
New: `media/__tests__/handlerSet.test.ts` (diff), `redialBackoff.test.ts` (random 0/0.5/0.999…, cap saturation, floor on n=0), `closeCode.test.ts` (table derived from the generated enum, asserts coverage), `media/__tests__/reconcile.test.ts` (G1-G9, G12; removal-during-dial; stale timer after removal; no pending timers after disconnect; join_token on armed dials only, `""` later; token-only refresh then redial carries the new token, opaque non-JWT tokens), `signaling/__tests__/handlerSetIntake.test.ts` (validation faults, token blanking, G10 getter-poison non-read with positive controls, G11 once-only WARN, both no-edge arms), `lifecycle` tests for quiet `stop(ref)`, lane pruning, queue drop. Updated: `media-transport.test.ts` (connectAll → arm; existing metric tests kept with same assertions), `meeting-session.test.ts` (R-21 once-only → per-transition, updated not deleted; time-to-first exactly once across lose→redial and migration; config line), `serverMessageSinkScan.test.ts` (dial-token residency subjects), svelte + web-app tests, `e2eBus.test.ts` (sentinel token absent + positive control). Guard poles: `lens/` pos merge-keeps-dropped-ref / neg wholesale replace; pos timer-captures-token / neg captures-ref; pos token-on-public-event; count bumped.

### Lead rulings / revisions
- D6 join_token: ADOPTED by Lead — `arm(joinToken)` only; later dials `join_token=""` + `media_dial_token`; JWT never retained. Interim (until MH-1): later dials are DECLINED by MH → park → FAILED → MC re-push → one dial; documented at the `#dialTokens` field and here.
- Interim until MH-1 / P3 (ops): expect MH closing non-join dials DECLINED and `mc_media_handler_set_pushes_total{reason="failed_report"}` rising (bounded by MC's FAILED re-push limiter: burst 4, then 1 per 2 s per session); one dial per newer token. MC's legacy URL-keyed status map collapses onto key "" (mh_url empty) until P3 — no operational effect. Neither is a bug in this window.
- Layer 7: no existing browser spec depends on a post-join dial succeeding (today there are no redials). `partial-connectivity` blocks handlers via the `blockHandlers` connect lever: those join-time dials fail as today (FAILED reported), then back off and redial against the same lever and keep failing (one FAILED report per attempt, delays ≥ floor, capped at `redialCapMs`), which the spec does not observe; its assertions (mediaConnected for the unblocked handler, offered set, media) are unchanged.
- Security tightening: duplicate ref / duplicate URL drops every entry involved; test arms for both. Test: join fails after a set was taken but before `arm` → no tokens held.
- D12 join gate: Lead ruled PRESERVE ALL_FAILED for a non-empty join set whose first dials all fail. Existing join tests unchanged; add the empty-set arm (join resolves, no failure_stage).
- Code-review Q1: `#removeRef` DELETES the record from `#refs`; in-flight dials/timers detect staleness by identity (`#refs.get(ref) !== record`). 'removed' is never stored; the map is bounded by the current set.
- Q2: membership SSoT — `takeHandlerSet()` validates once; SignalingClient updates its member-ref copy from that SAME intake BEFORE invoking the sink; test pins "a faulted entry is a non-member in both".
- Q3: `diagnostics()` readonly numbers-only, NOT exported from `index.ts`, documented as the C2 hook.
- Q4: `classifyClose(closeCode: number | undefined)`; undefined = no code (caller maps a rejected `closed` to undefined).
- Observability amendment (D7/D11, protects paging `MCMediaConnectionAllFailed`): a `MediaConnectionUpdate` is sent only on a real per-ref state CHANGE — a repeated failure while the ref is already failed/disconnected sends nothing; DECLINED is the exception (FAILED every time: it is MC's re-push trigger, bounded by MC's limiter). Test: N failed redials of one lost ref → exactly one loss report, then CONNECTED on recovery; DECLINED twice → two FAILED.
- Floor guard (Lead ruling): NO new script. `validate-keyframe-floor-sync.sh` is generalised into a pair table over (file, declaration-shape) sources sharing its ONE `extract()` kernel; it gains the pair `MH_KEEPALIVE_INTERVAL_MS=<n>` (`infra/services/mh-service/config.env`, path env-overridable) ↔ `export const MH_KEEPALIVE_FLOOR_MS = <n>;` (`clientConfig.ts`). Name and layer3 registration unchanged; its existing self-test gains keepalive arms on copies (drift fails, match passes, declaration-moved fails). `ANCHOR (DRY):` spelling. README drift test imports SDK defaults and asserts a row parsed. Reviewer: infrastructure (machinery) + dry-reviewer. Infra points adopted: rows carry (A file, A shape, B file, B shape, meaning, failure text) — no inherited keyframe text; env shape anchored on the whole line `^MH_KEEPALIVE_INTERVAL_MS=\([0-9]*\)$`, override `KEYFRAME_FLOOR_SYNC_MH_ENV`, missing-file check; fail-closed check that `infra/kubernetes/overlays` (override `KEYFRAME_FLOOR_SYNC_OVERLAYS`) never mentions the key; self-test arms via `fixture()` on copies (positive control from real files, env drift, SDK drift, both moved passes, key commented/renamed, duplicate key, missing env file, overlay mentions key fails, empty overlay passes; no literal 10000); header + `coverage-exempt` + layer3 comment updated; pointer comment in `config.env` and at the clientConfig declaration.
- Token blanking happens inside `takeHandlerSet()` immediately after projection, commented.
- Paired-test T1-T7 adopted: JWT sentinel only in armed-dial envelopes (positive control); interim chain triggered by an ADD; arm race (set before arm → 0 dials; set during arm's dials reconciled once, no double dial); floor on every first retry for every trigger incl. BINDING_CONFLICT, attempt reset after connect; hermetic self-test for the new floor guard (drift fails, match passes, missing anchor fails) wired like the keyframe one; config min/max/±1 + base>cap / base==cap; README drift test with mutated-fixture positive control; disconnect leaves 0 pending timers and a post-disconnect set/timer dials nothing.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed |
| Test (paired) | confirmed |
| Protocol (conditional) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Infrastructure (conditional, guard hunk) | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead rulings: join_token not retained (D6); D12 keep ALL_FAILED; floor guard generalised (no new script), infrastructure added. Teammates were given their INDEX.md / review-protocol paths to read rather than inlined contents.

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 2 | 2 | 0 | F-SEC-1 vacuous test titles; F-SEC-2 dial-token cap = MAX_USER_TOKEN_LENGTH pinned to jwt.rs |
| Test (paired) | RESOLVED-FIXED | 7 | 7 | 0 | fan-out cleanup, refresh-while-lost, poison invalid refs, counter positives, T3, FakeScheduler tests, guard arms |
| Protocol (conditional) | RESOLVED-FIXED | 2 | 2 | 0 | absent set = empty (no fault); silent re-check after removal |
| Observability | CLEAR | 0 | 0 | 0 | |
| Code Quality | RESOLVED-FIXED | 11 | 11 | 0 | |
| DRY | RESOLVED-FIXED | 4(+1 joint) | 5 | 0 | 2 extraction opportunities → docs/TODO.md |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | backoff exponent preserved across retry-class closes |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | park-release liveness note → Lead-routed fix (generation counter) |
| Infrastructure (conditional, guard hunk) | RESOLVED-FIXED | 2 | 2 | 0 | overlay grep rc fail-open; missing-root arm |

---

## Pre-Work

None.

---

## Implementation Summary

### Transport: reconcile to the set (ADR-0039 §4)
| Item | Before | After |
|------|--------|-------|
| `MediaTransport` dialing | single-use `connectAll(urls, jwt)` over `JoinResponse.media_servers` | `applyHandlerSet(intake)` for EVERY set + `arm(joinToken)` once after join; ONE `#dial`; diff on `handler_ref` only (`media/handlerSet.ts:diffHandlerSet`) |
| Per-handler state | parallel URL-keyed maps | ONE `RefRecord` per in-set ref (`connecting`/`connected`/`lost`/`parked`); removed = record deleted (identity check for stale work) |
| Removal | n/a | ONE `#removeRef`: token, record, timers, park, in-flight dial abort, `handlerRemoved` (sync, before close) → `mediaStart.ts:forgetHandler` fan-out (egress lane, video lane, quiet read-loop stop + hop monitor, recovery queue), control loop, datagram writer, transport close; `disconnect()` runs it over every ref |
| Redial | none | full jitter `redialBackoff.ts` (`floor + U[0,min(cap,base·2ⁿ)]` on n=0, floor = `MH_KEEPALIVE_FLOOR_MS`), injected scheduler + random; stops when the ref leaves |
| Close codes | not read | `closeCode.ts:classifyClose(closeCode)` exhaustive over generated `MhCloseCode`; retry / released(grace) / declined(park) / loss / contract_fault (event-once WARN, handled as loss) |
| Loss reports | one `MediaConnectionUpdate` at join (R-21) | `status` event per real transition (DECLINED always), ref + bigint version, `mh_url` empty; session forwards each, flushes before a join rejection |
| Empty join set | n/a | join completes; `mediaHandlersOverdue` once after `emptySetNoticeMs` |
| Join gate (Lead D12) | ALL_FAILED when every MH fails | unchanged for a non-empty join set |

### Credentials (ADR-0039 §5, scoped R-23)
- `#dialTokens` (one private Map, rebuilt per set, deleted on removal, cleared at disconnect), read only at `MhConnectRequest` build; timers/closures capture the record/ref. Park release by token string inequality.
- Meeting JWT: parameter of `arm()` only (Lead ruling) — later dials send `join_token=""`. Interim DECLINED cycle documented at `arm`, here, and runbook F30.
- Intake: `signaling/handlerSetIntake.ts:takeHandlerSet` validates (ref 0, url, token size, set size, duplicate ref/url → drop ALL involved, known ref at new url → drop), projects to token-free `HandlerSetView` + private token map, BLANKS every decoded token; `SignalingClient.handlerSetSink` (private seam like `kekSink`).

### Refs on send/receive
- `SendStreamDirective.targets: HandlerRef[]`, `StreamAssignmentEvent.handlerRef` (no-edge 0 silent on UNAVAILABLE/FEWER_SOURCES/AVATAR_NO_VIDEO; non-member → undefined + event-once `[dt-signaling-contract]`), membership re-checked on set change; `JoinedEvent.mediaServers` → `handlerSet`. Every selector and downstream map keyed by the branded `HandlerRef`.

### Config, docs, guard
- `MediaConfig.handlers` {redialBaseMs 1000 [100,60000], redialCapMs 30000 [1000,300000] ≥ base, emptySetNoticeMs 15000 [1000,300000], releasedGraceMs 2000 [0,30000]}; `HANDLER_SET_CONFIG_RANGES`; `[dt-media-handlers-config]` line; README table pinned by node test.
- `validate-keyframe-floor-sync.sh` row-driven, + MH keepalive pair + overlay fail-closed; self-test arms.

### Additional Changes
- sdk-svelte `MeetingStore.mediaConnections` keyed by ref; web-app bus `handlerSets` history + `mediaConnected{handlerRef,url}`; e2e fixtures/specs read `handlerSets`; `PerHandlerReadLoops` skips not-yet-connected refs silently and reopens on `connected` (a ref's next transport after a redial).
- No proto / generated-code edits. No new or reshaped metric series.

---

## Files Modified

See `git diff --stat` at commit time (~95 files: sdk-core ~60, sdk-svelte 7, web-app 20, test-utils 2, docs 6, scripts 4, dt-guard 6, infra 1).

### Key Changes by File
| File | Changes |
|------|---------|
| `packages/sdk-core/src/media/MediaTransport.ts` | reconcile-to-set, per-ref lifecycle, `#dialTokens`, close codes, redial, reports |
| `packages/sdk-core/src/signaling/handlerSetIntake.ts` | new: the one set decode/validate/blank |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | set intake + membership, refs on directives/assignments, ref-keyed status send |
| `packages/sdk-core/src/session/MeetingSession.ts` | transport before join, `arm`, per-transition reports, events, config line |
| `packages/sdk-core/src/media/__tests__/reconcile.test.ts` | new: test-gates §Client G1-G12, T1-T7 |
| `packages/sdk-core/src/signaling/__tests__/handlerSetIntake.test.ts` | new: validation, blanking, membership, superseded-field poison |
| `scripts/guards/simple/validate-keyframe-floor-sync.sh` | pair table + keepalive pair |

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → **TOTAL_RESULT=OK** (TOTAL_DURATION=1825s). An earlier attempt was killed by the Lead harness's 10-min background limit mid-Layer-7 (tool limit, not a pipeline result; not counted) and re-run with a longer limit.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 10s |
| 2 Format | OK | 6s |
| 3 Guards | OK | 130s |
| 4 Test | OK | 146s |
| 5 Lint | OK | 2s |
| 6 Audit | OK (dep audit SKIPPED-NO-DIFF no-dep-changes; proto test/audit N/A placeholders) | 5s |
| 7 Env-tests | OK — `env-tests-passed`, `browser-e2e-passed` | 1526s |

Fast check (Step 6) before review: layers 1-6 OK.

---

## Code Review Results

Per-reviewer detail is in §Gate 3 — Verdicts above.

### Security Specialist (paired)
**Verdict**: RESOLVED-FIXED — 2 found, 2 fixed, 0 deferred (F-SEC-1 vacuous test titles; F-SEC-2 dial-token length cap raised to MAX_USER_TOKEN_LENGTH, pinned to `common::jwt::MAX_JWT_SIZE_BYTES` by a sync-guard row requiring equality).

### Test Specialist (paired)
**Verdict**: RESOLVED-FIXED — 7 found, 7 fixed, 0 deferred.

### Protocol (conditional)
**Verdict**: RESOLVED-FIXED — 2 found, 2 fixed (absent join set = empty, no fault; silent membership re-check after removal).

### Observability Specialist
**Verdict**: CLEAR — no findings.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED — 11 found, 11 fixed, 0 deferred.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings**: F1 unpinned runbook copy of defaults; F2 BusHandlerSet declared 3x; F3 private manualTime copy of FakeScheduler; F4 wrong TransportScheduler doc; joint F-SEC-2 derivation — all fixed.

**Extraction opportunities** (appended to `docs/TODO.md`):
- §Cross-Service Duplication — "From DRY Reviewer — story 4 C1 (2026-11-10)": kekSource.test.ts private fakeScheduler → FakeScheduler
- §Cross-Service Duplication — "From DRY Reviewer — story 4 C1 (2026-11-10)": five SDK timer seams share one named type
- §Cross-Service Duplication — addendum to "Four KEY=VALUE env-file parsers": guard bash `env_key()` fifth instance

### Operations Reviewer
**Verdict**: RESOLVED-FIXED — 1 found, 1 fixed (redial exponent was reset by retry-class closes; now preserved, grows to cap).

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0. Non-blocking liveness note (park release compared against previous set's token) was Lead-routed as a fix: per-ref token-generation counter; a DECLINED dial whose ref already has a newer token dials once immediately.

### Infrastructure (conditional, guard hunk)
**Verdict**: RESOLVED-FIXED — 2 found, 2 fixed (overlay grep rc fail-open; missing overlays-root arm).

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `ddd9cfd59a4c19bd2db7ce24cda6139175a3c141`
2. Review all changes: `git diff ddd9cfd59a4c19bd2db7ce24cda6139175a3c141..HEAD`
3. Soft reset (preserves changes): `git reset --soft ddd9cfd59a4c19bd2db7ce24cda6139175a3c141`
4. Hard reset (clean revert): `git reset --hard ddd9cfd59a4c19bd2db7ce24cda6139175a3c141`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit` — client/e2e/bus/guard changes move together; MC/MH are additive and still populate legacy fields, so reverting needs no server action. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Interim state until MH-1: non-join dials are DECLINED (expected, not a defect)
**Problem**: Per the Lead ruling on D6 the join JWT is not retained, so every non-join dial (add, redial, migration, post-park) sends `join_token=""` + `media_dial_token`. MH reads only `join_token` until MH-1 and closes with DECLINED → client parks → FAILED → MC re-push → one dial per newer token.
**Resolution**: Bounded by MC's FAILED re-push limiter (`crates/mc-service/src/media_dial/status.rs`: FAILED_REPUSH_BURST=4, refill 2000 ms) and one dial per newer token: worst case ~1 DECLINED dial / 2 s per participant per lost handler. Join-time media unaffected; no regression (no redials existed before C1). **Expected signature during handler-loss testing before MH-1**: `mc_media_handler_set_pushes_total{reason="failed_report"}` rises together with MH DECLINED closes. Ends when MH-1 lands.

### Interim state until P3: MC legacy url-keyed status map collapses onto `""`
**Problem**: C1 sends `MhConnectionStatus` with `mh_url` empty (task item 7), so MC's legacy url-keyed map (`crates/mc-service/src/webtransport/connection.rs` ~L1003, `participant.rs` `handle_record_mh_statuses`) files every status under key `""`.
**Resolution**: No operational effect: counters still count each status, the ref-keyed path (MC-D) is authoritative, P3 deletes the legacy map.

---

## Lessons Learned

1. A task-text phrase ("join_token populated exactly as today") can conflict with a credential-lifetime rule; resolving it at Gate 1 (join-set dials only) avoided retaining the JWT.
2. Per-transition reporting changes the input to MC's paging ratio alert; report-on-change (DECLINED excepted) keeps the alert semantics.
3. A "connected on request write" success point resets backoff unless retry-class closes restore the attempt counter.

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
