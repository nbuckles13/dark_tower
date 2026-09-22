# Devloop Output: Story 2 protocol contract (hear each other)

**Date**: 2026-09-22
**Task**: Story 2 protocol contract — internal.proto (server_muted_sources, EndMeeting RPC, loopback comment rewrite), signaling.proto (slot-state docs, kek_rotation_debounce_seconds, unreachable_sender_ids), ADR-0036 §4/§8 edits, no-bytes internal guard, codegen
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `32c01b278c03e72ec05d7db5a9962db1aae7d7c8` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `protocol` |
| Tier | `full` |
| Iteration | `1 (1 review→impl fix round; Gate 2 attempts: 3)` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Paired meeting-controller | `seated; Gate-1 confirmed` |
| Paired media-handler | `seated; Gate-1 confirmed` |
| Paired infrastructure | `seated; Gate-1 confirmed` |
| Paired auth-controller | `seated (GSA intersection, internal.proto)` |

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
Story 2 protocol contract (ADR-0036 story "hear each other", docs/user-stories/2026-09-21-hear-each-other.md, requirements R-9, R-11, R-13, R-14, R-20, R-3). Every `proto/**` edit here is a Guarded Shared Area edit. Pair with security as reviewer of the ADR edits and with meeting-controller and media-handler as consumers.

In `proto/dark_tower/internal/v1/internal.proto`: (1) R-9 — add a meeting-level `repeated MutedSource server_muted_sources = 7;` to `RegisterMeetingRequest` with `message MutedSource { uint32 sender_id = 1; }`. Sender-only, deliberately: MH is keyless and type-blind and cannot read a datagram's publisher stream number (it lives inside the opaque SFrame key id, verified at `crates/media-protocol/src/frame.rs:106-126`; the relay region carries only the subscriber stream id and hop sequence), so a stream field would be enforced nowhere. Document in the comment that per-kind mute, when video lands in story 3, will key on transport (datagram vs uni-stream), which MH can observe. The set rides the full snapshot so it survives re-assert and MH restart, participates in `policy_generation`, and `sender_id` inherits the meeting-scoped malformed-reject / not-yet-connected-hold validation already documented on `SubscriberSlot` and `CandidateSource`; mirror that SSoT discipline (reference the sender_id rules, do not restate them). Add a MUST: MH rejects a registration whose `server_muted_sources` count exceeds the configured per-meeting bound (`MH_MAX_MUTED_SOURCES_PER_MEETING`) BEFORE building any routing table, the same posture as `egress_streams` and `candidate_sources` — an unbounded caller-controlled repeated field is attacker-sized work before the policy check; enforcement owner is the MH server-mute task. (2) R-20 — add `rpc EndMeeting(EndMeetingRequest) returns (EndMeetingResponse);` to `MediaHandlerService`, with `EndMeetingRequest { string meeting_id = 1; string mc_id = 2; }` and `EndMeetingResponse { bool acknowledged = 1; }`. No generation, no reason field. Semantics in the comment: unknown or already-released meeting is an idempotent acknowledged no-op; `mc_id` equal to the registering MC releases the meeting's routing state and edge budget and acknowledges; `mc_id` MISMATCH is REJECTED with a gRPC error status and counted by MH under its own reason — never idempotent-accept — because a different MC must never tear down another MC's meeting, and legitimate failover re-registers first (overwriting mc_id) before it can release. Distinguish it in the comment from the MC-to-GC `NotifyMeetingEnded`. Narrow the existing "ONE RPC BY DESIGN" comment on `MediaHandlerService` to: forwarding policy gains fields, not sibling RPCs; meeting lifecycle teardown is a distinct concern with its own RPC. Note the `method` metric label now takes `register_meeting` and `end_meeting`. (3) R-3 — loopback is removed from the product; rewrite the five comments that describe loopback as the normal case (around lines 89, 125, 227-228 "the subscriber's own audio", 257 "the loopback audio egress stream", 459 "passes every loopback test") to the static-fill multi-party normal case, keeping the underlying reasoning (pending-promotion hold, meeting-scoped index, single-candidate-this-story, all-frames-forward) intact.

In `proto/dark_tower/signaling/v1/signaling.proto`: (4) R-11 — broaden `SLOT_STATE_SOURCE_MUTED`'s comment to cover BOTH client mute (§5) and server mute (§7); the cause and who-muted-whom are carried on `ParticipantMuteUpdate`, never distinguished by this state; no new enum value. (5) R-14 — add `uint32 kek_rotation_debounce_seconds` (W, the meeting's KEK rotation debounce window in seconds, the same value MC reads from `MC_KEK_ROTATION_DEBOUNCE_SECONDS`) to `JoinResponse` (tag 11; the reconnect response is JoinResponse-shaped, and a reconnecting participant may hold an older generation) and to `MeetingKekUpdate` (tag 3). The client derives its previous-generation retention window as W/2 at the consumer, so the retention-shorter-than-W relationship is structural (state that in the field doc as the guarantee's home); the client validates the derived retention exceeds its own transmit-key re-wrap latency; on zero-or-absent (a non-optional scalar, so both are one observable) the client substitutes a documented client-side floor constant, counts it and logs loudly, and never hard-fails, because an older MC that does not send the field is exactly the supported one-version rollback (R-24). Not key material. A first joiner ignores it. (6) R-33 — add `repeated uint32 unreachable_sender_ids = 2;` to `StreamAssignments` (the per-subscriber container; tag 1 is `assignments`): the roster participants THIS subscriber cannot reach because they are on a different handler and there is no cross-handler forwarding (§9 visibility rule; per-subscriber because reachability is relative to the subscriber's own handler; MC computes the visibility graph and stays authoritative — do not put each participant's handler on the roster and let clients compute it, which exposes topology and contradicts §7/§9). The client marks those roster entries distinctly (never a spinner, never an empty slot; §6 "absence of frames is not a signal", §9 "needs distinct treatment"). `SLOT_STATE_SOURCE_UNREACHABLE` stays in the vocabulary for the pinned-unreachable-source case and is retained-but-unemitted this story (no client pins under static fill), like ZERO_REQUESTED; say so in its comment. Per-participant handler URLs for attach already exist (`StreamAssignment.media_handler_url`, `JoinResponse.media_servers`); no change. (7) Document the `SLOT_STATE_ZERO_REQUESTED` contract: MC never fabricates a `slot_id` to carry it and never emits it in story 2 (a declared-but-unfilled slot is `FEWER_SOURCES_THAN_SLOTS`); keep the value in the vocabulary rather than reserving it, because per-kind slots in story 3 make it naturally emittable (docs/TODO.md line ~1228 records the open question — update that entry to point at this comment).

ADR bundle, one edit to `docs/decisions/adr-0036-media-flow.md`: (a) R-20 — narrow the one-RPC invariant to forwarding policy in §8, the Appendix "Internal contract", and the service block; update `docs/specialist-knowledge/protocol/INDEX.md` line 22 ("RegisterMeeting only — one RPC by design"). (b) R-13 — apply security's three-site §4 correction VERBATIM. Site 1, new row in the Rotation table immediately after "Transmit-key cadence": "Meeting KEK rotated | Every sender rotates its transmit keys on receipt of a new KEK. Rotation is O(1), involves nobody else and needs no signalling, so this costs nothing. Without it the leave bound above is false: a departed member keeps decrypting from the unwrapped transmit keys it cached before the rotation, and re-wrapping under the new KEK cannot revoke a key the leaver already holds in plaintext. This row is what makes the leave row's W bound true." Site 2, in the leave row replace the sentence beginning "A leaver is bounded by W and by nothing else" with: "A leaver is bounded by W, and only because senders rotate transmit keys when the KEK rotates (below). Holding the old KEK, it unwraps every new transmit key carried before the rotation lands, so the ordinary transmit-key cadence does not shorten the window; and because receivers cache the unwrapped key, re-wrapping alone would not end the leaver's access either — the sender has to move to a key the leaver never held." Site 3, in the "Costs accepted" bullet beginning "Both membership bounds are KEK bounds" replace its final sentence "Transmit-key rotation does not tighten either" with: "The ordinary transmit-key cadence tightens neither; the KEK-triggered rotation above is what closes the leaver's forward window, and nothing closes the joiner's backward one short of rotating on every join, which this design exists to avoid." Add beside the new row a dated correction note in the repository's convention: "Correction (2026-09-22, story 2 planning). The leave row previously read 'bounded by W and by nothing else'. That reasoning tracked only the wrapped key and missed the cached plaintext: receivers cache the unwrapped transmit key, so a leaver continued to decrypt after the rotation until each sender's next generation bump — a bound of W plus one transmit-key interval, with that interval unconfigured anywhere in the tree. A reader who trusted the original would have stated an exposure window shorter than the one the system delivered. The fix is the new trigger row rather than a weaker claim, because sender rotation is free and restores the bound as written." The §4 Consequences/Negative bullet about a departed participant retaining keys stays as written; §11 and the Appendix are untouched. (c) Repoint story 1's stale "attestation → story 2" references in `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (lines ~34, 83, 175, 271) to "the attestation story (ADR-0036 addendum)"; also add a one-line superseded note to that file's R-1 saying loopback was removed by story 2 (R-3).

Security verified that `proto/dark_tower/internal/v1/internal.proto` contains zero `bytes` fields (the client-facing contract has four, exactly where key material legitimately lives), which turns "no key material crosses MC→MH" from a naming-based review rule into a mechanical one: no `bytes` field may ever appear on the internal contract. Record that rule in the internal proto's header comment and on the filed `.proto`-scanner entry in `docs/TODO.md`, and arm it as a one-line simple guard (a grep for `^\s*(repeated\s+)?bytes\b` over that file that must match nothing) under `scripts/guards/simple/` — pair with infrastructure for the guard machinery and security for the rule; it holds today and after this story's two additions, so it arms without reddening. Regenerate Rust (`crates/proto-gen`) and TypeScript (`packages/proto-gen`) bindings; extend `crates/proto-gen/tests/internal_roundtrip.rs` for the new field and RPC messages; update the codegen oracle in `packages/proto-gen/scripts/verify-codegen.sh`. No `skip_debug` or redaction changes — the new field and messages carry sender ids, meeting ids and a duration only, no key material. `buf breaking` must be clean (all additive) and `buf STANDARD` satisfied (distinct request/response types per `docs/protocol/CONVENTIONS.md`). No env-tests in this task: the behaviours are cluster-observable in the MC, MH and client consumer tasks.

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
| `proto/dark_tower/internal/v1/internal.proto` | Mine (GSA; manifest key `[protocol, auth-controller, security]` — security paired; auth-controller stake is only that EndMeeting rides the unchanged `service.write.mh` scope, co-sign need is a lead call; consumers meeting-controller, media-handler) | protocol |
| `proto/dark_tower/signaling/v1/signaling.proto` | Mine (GSA; security reviews; consumers meeting-controller, client) | protocol |
| `crates/proto-gen/src/lib.rs` | Mine (GSA `proto-gen/**`; add `kek_rotation_debounce_seconds` to two hand-written Debug impls — not a redaction change) | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` | Mine (GSA `proto-gen/**`) | protocol |
| `crates/proto-gen/tests/signaling_roundtrip.rs` | Mine (GSA `proto-gen/**`; new-field roundtrips + struct-literal compile fixes) | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh` | Mine (GSA `proto-gen/**`; oracle presence asserts) | protocol |
| `docs/decisions/adr-0036-media-flow.md` | Not mine, Domain-judgment (§4 R-13 text is security's VERBATIM; §8/Appendix one-RPC narrowing is mine) | security |
| `docs/protocol/CONVENTIONS.md` | Mine (dated correction: `breaking.ignore` covers internal.proto too since 2026-09-01; green buf breaking is evidence for neither file) | — |
| `docs/runbooks/mh-incident-response.md` | Not mine, Minor-judgment (Scenario 13 root-cause clause only; triage ladder/discriminators unchanged; owner pre-confirmed at Gate 1) | operations |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment (§6.3 row + §8 catalogue rows for the new guard's tokens) | operations |
| `docs/API_CONTRACTS.md` | Mine (§4.1 one-RPC narrowing + EndMeeting; attestation→story-2 repoint) | — |
| `docs/WEBTRANSPORT_FLOW.md` | Mine (one loopback-as-normal comment) | — |
| `docs/specialist-knowledge/protocol/INDEX.md` | Mine | — |
| `docs/TODO.md` | Mine (ZERO_REQUESTED entry repoint; `.proto`-scanner entry records no-bytes rule + transitive caveat) | — |
| `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` | Not mine, Minor-judgment (attestation repoints ×4, R-1 superseded note) | meeting-controller |
| `docs/user-stories/2026-09-21-hear-each-other.md` | Not mine, Minor-judgment — **APPROVED by lead at Gate 1** (items (i)-(iv): :89/:98 stale wire names + add `MH_MAX_MUTED_SOURCES_PER_MEETING`, :122, :383, :453 `PermissionDenied`→`FAILED_PRECONDITION`, :165 manifest off-by-one, :141 premise list) | meeting-controller |
| `scripts/guards/simple/validate-internal-proto-no-key-material.sh` | Not mine, Domain-judgment (new guard: machinery infrastructure, rule content security) | infrastructure |
| `scripts/guards/validate-internal-proto-no-key-material.test.sh` | Not mine, Domain-judgment (guard self-test) | infrastructure |
| `scripts/layer3.sh` | Not mine, Minor-judgment (wire the self-test via `run_and_emit`) | infrastructure |
| `crates/mh-service/src/grpc/mh_service.rs` | Not mine, Minor-judgment (compile-forced `end_meeting` → `Status::unimplemented` stub; module doc one-RPC narrowing) | media-handler |
| `crates/mh-service/src/observability/metrics.rs` | Not mine, Minor-judgment (comment-only: one-RPC claims narrowed; code untouched, task 11 owns) | media-handler |
| `crates/mh-service/src/config.rs` | Not mine, Minor-judgment (comment-only: "has one RPC, so no meeting-ended signal") | media-handler |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment (`method` label single-valued claim → two-member set, `end_meeting` not yet emitted) | observability |
| `crates/mc-test-utils/src/mock_mh.rs` | Not mine, Minor-judgment (compile-forced `end_meeting` on stub, recording calls) | meeting-controller |
| `crates/mc-service/src/grpc/mh_client.rs` | Not mine, Minor-judgment (compile-forced explicit `server_muted_sources: Vec::new()` ×2) | meeting-controller |
| `crates/mc-service/src/webtransport/connection.rs` | Not mine, Minor-judgment (compile-forced explicit `kek_rotation_debounce_seconds: 0`) | meeting-controller |
| `crates/mc-service/src/media_signaling/assignments.rs` | Not mine, Minor-judgment (compile-forced explicit `unreachable_sender_ids: Vec::new()`) | meeting-controller |
| `crates/mh-test-utils/src/media_policy.rs` | Not mine, Minor-judgment (compile-forced field, only if the literal is exhaustive) | media-handler |
| `crates/mh-service/tests/*.rs` | Not mine, Minor-judgment (compile-forced field in exhaustive `RegisterMeetingRequest` literals, only where needed) | media-handler |
| `crates/mc-service/src/media_admission/mod.rs` | Not mine, Mechanical (doc comment: "attestation … is story 2" → the attestation story) | meeting-controller |
| `crates/mc-service/src/media_admission/identity_key.rs` | Not mine, Mechanical (doc comment: same attestation repoint) | meeting-controller |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Not mine, Mechanical (comment: "attestation … is story 2" → attestation story) | client |
| `infra/grafana/dashboards/mh-overview.json` | Not mine, Minor-judgment (panel 28 description only: one-RPC claim narrowed) | observability |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Not mine, Minor-judgment (header comment only: "single RPC" claim narrowed; no rule change) | operations |
| `crates/mh-service/tests/errors_grpc_metrics_integration.rs` | Not mine, Minor-judgment (module doc only: single-valued claim narrowed; assertions untouched) | media-handler |

---

## Planning

### Mechanism restatement

1. **The MC→MH contract gains a meeting-level input set (mute) and a lifecycle RPC (teardown).** The
   standing invariant "MediaHandlerService has exactly one RPC" is *narrowed*, not dropped: forwarding
   policy gains fields (so mute is a field on the snapshot), meeting lifecycle teardown is a distinct
   concern with its own RPC. **Wider class surfaced:** every artefact asserting "one RPC" — enumerated
   by grep: internal.proto service block; INDEX.md:22 and :30; API_CONTRACTS.md §4.1; ADR-0036 §8 +
   Appendix "Internal contract"; `mh_service.rs` module doc; `metrics.rs` docstring (:164-173) and test
   comment (:1516); `config.rs:105`; `mh-service.md:223`, a REWRITE of :228 (its single-value defence evaporates), and the SEPARATE
   `**Cardinality**` bullet — a distinct line from the value-set claim, so re-tightening ":223 + the
   paragraph" leaves it stale; it keeps its named factors rather than becoming a bare integer:
   "2 today = 1 emitting method x 2 statuses; 4 once `end_meeting` emits"; `mh-overview.json` panel 28 description;
   `mh-alerts.yaml:25` header comment (in-file convention: clause corrected + dated `CORRECTED
   2026-09-22:` line; plus a new named deliberate omission — teardown-never-arrives / absent-series,
   owner story task 18); `errors_grpc_metrics_integration.rs:17` module doc; TODO.md:347
   entry; **TODO.md:1314** (the never-reclaimed-routing-entry entry — I am a listed owner; its stated
   blocker "MediaHandlerService has exactly one RPC, so MH is given no meeting-ended signal it could act
   on" is what this diff removes. Entry STAYS OPEN — the code still does not reclaim — but its diagnosis
   is rewritten to: the signal now exists on the contract, and the outstanding work is MH-side
   reclamation in task 11. Highest-value of the seven: the others read as a stale design claim, this one
   hands a reader a false root cause for an open availability defect).
   **Consequence-phrasing sweep (operations, OPS-10).** My first pass grepped the invariant's WORDS, which
   finds sites stating it as a FACT and misses sites stating its CONSEQUENCE — and a stale fact reads as
   stale while a stale consequence reads as a root cause. Three more, all corrected two-part (cause clause
   only; symptom and triage stay valid because MH genuinely does not reclaim yet):
   `docs/runbooks/mh-incident-response.md:780` (Scenario 13's operator-facing root cause, read mid-incident
   — the worst of the set); `docs/TODO.md:600` (the alert-design entry, whose purpose is telling a future
   author how to design it); `docs/TODO.md:1320` (records reclamation shapes REJECTED because "there is no
   teardown signal to hang it on … really 'add meeting teardown to MediaHandlerService', a GSA contract
   change" — this diff IS that change, so shape (i) is now available and the reasoning is INVERTED, not
   merely stale). None marked resolved; MH-side reclamation is task 11's.
   I ran the same consequence sweep for the loopback invariant (`own audio`, `subscribes_to`,
   `one-participant meeting`, sender==receiver phrasings). Result: every hit outside internal.proto:228 is
   MC/client/test/runbook BEHAVIOUR owned by the R-3 tasks (6, 15, 17 — e.g. `mh-incident-response.md:1000`
   and `:1471` are task 17's "MH 17 loopback subsection revised"), not orphaned prose. No action here. All narrowed here to "label values derived from `MediaHandlerService`'s method names (the proto
   service block names the method set; the recorder vocabulary is `metrics.rs`'s `GRPC_METHOD_*` consts —
   member list written once, derived sites cite it, no restated counts); `end_meeting` not emitted
   until the MH EndMeeting task (recorder parameter, zero-init, integration assertion, dashboard query)"
   — comment/description text only; no recorder, zero-init, rule or query change. ADR-0003:146 is a *dated* amendment note ("after the reshape", 2026-09-01) — true as history,
   left as is.
2. **Loopback removed (R-3):** five named internal.proto comments rewritten to the static-fill
   multi-party normal case. **Wider class:** internal.proto:625 "true for this story (one audio egress
   stream, datagram)" (stated homogeneity now true because audio-only, not because one stream);
   WEBTRANSPORT_FLOW.md:552 "loopback carries none". Network-sense "loopback" (infra/kind, dev-web) is a
   different concept — untouched.
3. **Attestation moved out of story 2:** story-1 file ×4 per task, plus the same stale statement in
   signaling.proto :200, :300, :770, API_CONTRACTS.md:256 and SignalingClient.ts:413 — all repointed to
   "the attestation story (ADR-0036 addendum)". signaling.proto:866 (`MeetingKekUpdate` "UNUSED THIS
   STORY, so that story 2's rotation…") rewritten to present tense.
4. **"No key material crosses MC→MH" becomes mechanical** for the direct case (no `bytes` token in
   internal.proto) AND the transitive case (no signaling *message* embedded) — see guard design below.

### internal.proto

- `message MutedSource { uint32 sender_id = 1; }`; `repeated MutedSource server_muted_sources = 7;` on
  `RegisterMeetingRequest`. Comment: sender-only because MH **does not parse the payload and, being
  keyless, could not authenticate the stream component** of the key id (frame.rs key-id layout ANCHOR),
  so a stream field would be enforced nowhere; story-3 per-kind mute keys on transport (datagram vs
  uni-stream), which MH observes. Rides the full snapshot (survives re-assert/MH restart), swaps
  atomically with it (all-or-none; a stale/gen-0/failed apply leaves the prior set live); a set change
  IS an assignment-output change so MC MUST advance `policy_generation`. `sender_id` → reference to
  `SubscriberSlot.sender_id` rules (malformed rejects, not-yet-connected holds), not restated.
  Duplicate `sender_id` in the set → REJECT the registration (same posture as duplicate
  `egress_stream_id`). BOUND MUST: reject when count > `MH_MAX_MUTED_SOURCES_PER_MEETING` BEFORE any
  routing table build; enforcement owner: MH server-mute task. Added to the SubscriberSlot block's
  "IMMEDIATE HARD REJECTS" list (duplicate-muted-source and muted-count bound).
- `rpc EndMeeting(EndMeetingRequest) returns (EndMeetingResponse);` with
  `EndMeetingRequest { string meeting_id = 1; string mc_id = 2; }`,
  `EndMeetingResponse { bool acknowledged = 1; }` — distinct types (buf STANDARD). Comment pins:
  unknown/already-released → idempotent `acknowledged: true` (incl. a handler in the assigned set that
  never received a registration — MC calls every handler in its set); `mc_id` equal → release routing
  state + edge budget and forget applied generation (a later RegisterMeeting starts fresh, 1 is not
  stale), ack; `mc_id` MISMATCH → gRPC `FAILED_PRECONDITION` (security + operations + auth-controller, independently: NOT `PERMISSION_DENIED`, which would encode an authz claim the system does not make; `FAILED_PRECONDITION`'s no-retry-until-state-fixed semantics match. **Status-code facts, corrected by auth-controller against `auth_interceptor.rs` — do NOT write "a scope failure surfaces as PERMISSION_DENIED":** a missing/invalid token OR a missing `service.write.mh` scope returns **`UNAUTHENTICATED`** (":201", ":215" — Layer 1); `PERMISSION_DENIED` is Layer 2 only (caller `service_type` mismatch or unknown path, ":232"). The argument is unchanged and in fact cleaner — `PERMISSION_DENIED` stays reserved for a real authz boundary), counted by MH under a bounded
  `outcome` value — never idempotent-accept, never `acknowledged:false`. Compared against the `mc_id`
  of the most recent **validation-passing** `RegisterMeeting` (reference `RegisterMeetingRequest.mc_id`),
  NOT gated on `policy_generation` outcome — otherwise a failover MC's re-register at ≤ generation
  strands the meeting. `mc_id` is self-asserted: **NOT an authorization control** (misroute/stale-MC
  guard only; `MhAuthLayer` binds no caller to `mc_id` — cite the greppable title fragment "MH does not
  verify meeting ownership" (`docs/TODO.md:1322`), NOT an invented entry name). Channel authenticity is
  written as `MhAuthLayer`'s **two-layer** gate, not one (A1): scope `service.write.mh`
  (`auth_interceptor.rs:40`, path-INDEPENDENT, every method) AND caller `service_type` =
  `meeting-controller` (the `MediaHandlerService/` path prefix, `:223`, unknown paths fail closed).
  EndMeeting inherits both with no auth change — a reader who later hardens one layer must know both
  exist. The
  ack-vs-error asymmetry is a meeting-existence oracle to `service.write.mh` holders — ACCEPTED and named,
  and must not be 'fixed' by making mismatch idempotent; channel authenticity is the existing `service.write.mh` gate, which already covers the
  new method by path prefix. Connections: promoted connections of the released meeting forward nothing
  and are closed by MH; pending (provisional) ones are left to the registration timeout (a re-join
  racing the teardown is rescued by the next registration). MH's close is NOT the client's
  meeting-ended signal (MC signalling is); a client reconnecting after the close is held pending,
  bounded by the registration timeout. Ordering: story 2 relies on MC stopping
  pushes before calling; a RegisterMeeting arriving after EndMeeting re-creates the meeting — accepted
  in story 2 because no cadence re-assert exists; story 4's periodic re-assert needs a fence — recorded
  as a `docs/TODO.md` §Media Path Obligations entry with trigger = story 4 periodic re-assert. Operator
  symptom named: an erroneous/misrouted release drops every participant of exactly one meeting at once,
  handler-side, with no client-visible cause. `MH_MAX_MUTED_SOURCES_PER_MEETING` worded as landed by the
  MH configuration task (not claimed to exist today).
  Distinguished from MC→GC `NotifyMeetingEnded` (fleet bookkeeping vs handler resource release).
  Rollout (R-24): MH gains the RPC before any MC calls it; rollback reverses. `UNIMPLEMENTED` from a
  one-version-older MH is expected and non-fatal — MC counts it, does not retry, and teardown at MC/GC
  proceeds (old MH keeps the state until restart = today's behaviour). "Forget applied generation" is
  tied to MC dropping its per-(meeting, handler) generations at teardown: a same-id re-creation pushes
  from 1 and must be a fresh install; where EndMeeting was UNIMPLEMENTED/never landed, that re-register
  reads stale on the old MH until restart (today's behaviour; surfaces as MC's applied/sent mismatch). A `mc_id`-mismatch rejection
  MUST NOT be retried; transport failures retried bounded with backoff; MC teardown never blocks on
  the ack (`acknowledged` is best-effort, not a liveness gate).
  Label: `meeting_id`/`mc_id` never labels. Service block: this block names the METHOD SET; the MH gRPC
  `method` label values are DERIVED from those method names, and the label vocabulary's own home is
  `metrics.rs`'s `GRPC_METHOD_*` consts (per §1 — no competing SSoT claim here). Recorder, zero-init,
  test and dashboard changes are the MH EndMeeting task's.
- Header comment: NO `bytes` FIELD, EVER, on this contract + the transitive caveat (only signaling
  ENUMS may be referenced; embedding a signaling message is the other way key material could arrive and
  is also mechanically blocked).
- R-3 rewrites at :89, :125-133 (keep "single-meeting tests can't distinguish indexes"; point at the
  existing MH test `routing::…::sender_id_valid_in_one_meeting_does_not_resolve_in_another` rather than
  "no gate exists"), :227-230 (one pinned candidate per egress under static fill; still repeated for
  story 5), :257, :459 ("passes every single-meeting test"), plus :625.

### signaling.proto

- `SLOT_STATE_SOURCE_MUTED`: client mute (§5) OR server mute (§7); cause/who carried on
  `ParticipantMuteUpdate`; no new value.
- `SLOT_STATE_ZERO_REQUESTED`: MC never fabricates a `slot_id`, never emits in story 2
  (declared-but-unfilled = `FEWER_SOURCES_THAN_SLOTS`); kept not reserved because per-kind slots
  (story 3) make it emittable. `SLOT_STATE_SOURCE_UNREACHABLE`: pinned-unreachable-source case only,
  retained-but-unemitted in story 2 (no pins under static fill); cross-handler peers go to
  `unreachable_sender_ids`.
- `uint32 kek_rotation_debounce_seconds` — `JoinResponse = 11`, `MeetingKekUpdate = 3`. **Full rule
  lives once at `JoinResponse`**; `MeetingKekUpdate` references it. Rule: W in seconds, same value MC is
  configured with via `MC_KEK_ROTATION_DEBOUNCE_SECONDS` (worded so it does not claim MC reads it
  today); client derives previous-generation retention = W/2 at the consumer — this field doc is the
  home of the retention<W guarantee; client validates derived retention > its own transmit-key re-wrap
  latency; zero-or-absent (one observable) → client substitutes its documented floor constant (lives
  at the client's config point, no number in the proto), counts it (unlabelled counter or existing
  allowlisted `dt.event`; any new attribute key needs GC's telemetry allowlist), logs at WARN with the
  received value and substituted floor (never the KEK/generation) — never hard-fails (R-24 rollback);
  that counter being non-zero is the only signal a fleet is on the floor. **Retention is
  `min(W/2, ceiling)`** (ops + security ruling): retention serves frames already in flight at a
  rotation — a latency-scale need — and must not scale with policy-scale W without bound
  (key-minimisation against client memory compromise). Ceiling value is the client task's, derived from
  a stated frames-in-flight assumption, no number in the proto; clamped the same way (count, WARN,
  never hard-fail). Clamping downward keeps retention < W at both bounds. The count MUST distinguish floor-substituted (older MC; expected during rollback) from ceiling-clamped (MC misconfig; unexpected) — two counters or one bounded dimension on an allowlisted key, values homed in the client's vocabulary, not minted in the proto. Not key material; safe to
  log; first joiner ignores it. Reconnect is JoinResponse-shaped, hence both carriers.
- `repeated uint32 unreachable_sender_ids = 2;` on `StreamAssignments`: per-subscriber, MC-computed
  (§9); full snapshot replaced on every `StreamAssignments` (state replace semantics for both fields);
  excludes the subscriber itself; excludes same-handler senders unassigned only because of the slot cap;
  lists only roster participants with a `Participant.sender_id`; the message MAY carry zero
  `assignments` with a non-empty set. Client renders distinctly (never spinner/empty slot). Negative
  space (security): no handler identity/URL, no handler count, no other subscriber's partition, and no
  co-location inference *among* the unreachable set.
- Attestation repoints (:200, :300, :770); `MeetingKekUpdate` header present-tense.
- **Additive-only evidence does NOT come from Layer-6 `buf breaking`**: `proto/buf.yaml`
  `breaking.ignore` lists BOTH signaling.proto and internal.proto (2026-09-01 scope extension). Evidence
  instead: (a) `buf breaking` from `proto/` with an inline `--config` identical minus `ignore`, against
  `.git#ref=32c01b278c03e72ec05d7db5a9962db1aae7d7c8,subdir=proto` (pinned start commit; post-reshape, so
  an additive diff must yield zero findings for both files) — command + output pasted here; (b) tag audit: internal tag 7 + new messages; signaling 11/3/2, all
  free and un-reserved; no existing field, enum value or reservation touched.

### Rust / TS consumers (compile-forced, no behaviour change)

tonic without `generate_default_stubs` (kept off — workspace-wide Unimplemented defaults would mask):
- `mh_service.rs`: explicit `end_meeting` → `Status::unimplemented("EndMeeting: MH task 11")`, does NOT
  call `record_grpc_request` (would mislabel as `register_meeting`). The stub's comment states WHY no metric is recorded (the recorder hard-codes the method label, so calling it would forge R-26's RegisterMeeting receipt signal) and that the MH EndMeeting task gives the recorder its parameter.
- `mock_mh.rs`: `end_meeting` records call count + last request and acks (MC task 12's seam).
- MC literals: explicit `server_muted_sources: Vec::new()`, `unreachable_sender_ids: Vec::new()`,
  `kek_rotation_debounce_seconds: 0` each with a one-line comment naming the populating task; all are
  the truthful current value (0 is the contract's rollback observable). Test literals likewise.
- `proto-gen/src/lib.rs`: add `kek_rotation_debounce_seconds` (in the clear — a duration) to the
  JoinResponse and MeetingKekUpdate `Debug` impls as a single added `.field(...)` line each. The impls
  stay hand-written (NOT converted to a derived `Debug`, which would defeat `build.rs`'s `skip_debug`),
  and the `RedactedLen` calls at `:110`, `:139`, `:149`, `:183` are untouched.

### Tests / oracle

- `internal_roundtrip.rs`: `server_muted_sources` with >1 entry roundtrips values and order; empty set is checked structurally (encoded bytes carry no field-7 key; derived, no golden),
  meeting-level (not per-egress); EndMeeting request/response roundtrip; default `EndMeetingResponse`
  decodes `acknowledged=false` (why the mismatch path uses a status, not false); header "does NOT
  assert" gains the runtime MUSTs (mc_id comparison, count bound).
- `signaling_roundtrip.rs`: `kek_rotation_debounce_seconds` value on both messages; absent decodes 0; `unreachable_sender_ids` with >1 value (value + order);
  `unreachable_sender_ids` alongside empty `assignments`.
- `verify-codegen.sh`: presence for `MutedSource`, `EndMeetingRequest`, `EndMeetingResponse`, and
  generated field names (`serverMutedSources`, `kekRotationDebounceSeconds`, `unreachableSenderIds`,
  `endMeeting`) — casing confirmed from regenerated output.

### No-key-material guard (machinery infrastructure, rule security)

`scripts/guards/simple/validate-internal-proto-no-key-material.sh` (mode 100755). Pure bash+grep: the
same grep-shaped anchor-guard class as validate-ts-fmt-proto-excluded and validate-frame-vectors. It needs
no parser and no cargo, so it stays hermetic in Layer 3, and there is no reason to make it a dt-guard
subcommand. The repo root comes from BASH_SOURCE, run-guards' `$1` is ignored, and there is one
`DEVLOOP_TEST=1`-gated `INTERNAL_PROTO_GUARD_ROOT` seam that derives both proto paths. It does not source
common.sh because it uses none of it (validate-frame-vectors.sh:44 precedent).
- Vacuity (`internal-proto-scan-vacuous`): both files must exist and be non-empty. internal.proto must carry
  the `package dark_tower.internal.v1;` and `service MediaHandlerService` markers. A `/*` block comment is
  rejected. Any grep rc outside {0,1} is vacuous.
- Strip `//` comments (`sed 's://.*::'`). Check 1 (`internal-proto-bytes-field`) uses security's
  pattern verbatim: `\bbytes[[:space:]>]|BytesValue`.
- Check 2 (`internal-proto-cross-package-type`) is G2, spelling-independent. Every qualified type
  reference matching `\.?\b([A-Za-z_][A-Za-z0-9_]*\.)+[A-Z][A-Za-z0-9_]*` must EXACTLY equal an entry in a
  bash-array allowlist of canonical FQNs, `{dark_tower.signaling.v1.TransportMode}`. That catches
  relative (`signaling.v1.X`), leading-dot and `google.protobuf.*` spellings, and a non-canonical
  spelling of an allowlisted enum also reds. Membership is security-owned content and the construct
  is machinery. For each entry, `enum <last-segment> {` must be declared in the stripped
  signaling.proto: declared as a message → `internal-proto-allowlist-not-enum`; absent → vacuous. This
  makes the enums-only boundary structural.
- Check 3 (`internal-proto-import-not-allowed`, G3): every `import` line must be in the allowlist
  `{"dark_tower/signaling/v1/signaling.proto"}`. This stops a same-package import from bringing in
  unqualified message types.
- Output is `VIOLATION <token>` lines with line numbers on stderr, exit 0/1, and no STATUS line. There is
  **no suppression path**. The MESSAGE (not only the header — the header protects the reader who opens
  the file, the message the one who does not) says the guard is a mechanical proxy for ADR-0036 §4 "No
  key material crosses the MC→MH contract", names which check fired, and routes a legitimate need to a
  rule re-derivation with @security rather than reading as "delete the field". The HEADER states two
  bounds honestly: fully-qualified spelling is REQUIRED (a red on
  `signaling.v1.TransportMode` is intended — fix the spelling, never loosen the comparison), and check 3
  covers internal.proto's DIRECT imports only (transitive visibility via an `import public` inside an
  allowlisted file is out of scope).
- Self-test `scripts/guards/validate-internal-proto-no-key-material.test.sh` covers infra's N7 case list:
  the real tree; a synthetic baseline; check-1 positives (`optional bytes`, `bytes`, `repeated bytes`,
  `map<string, bytes>`, `BytesValue`); check-1 negatives (`//` prose, `total_bytes`, `bytes_sent_label`);
  check-2 positives (FQN, relative and leading-dot `MeetingKekUpdate`, `google.protobuf.Timestamp`,
  `google.protobuf.Any`, relative-spelled `TransportMode`); check-2 negatives (canonical TransportMode,
  prose MeetingKekUpdate); allowlist-not-enum; a disallowed import, with the real import staying green;
  and the five vacuity cases. Each case asserts both rc and token. It is wired in
  `scripts/layer3.sh` via `run_and_emit`.
- Guard header carries `Runbook: docs/runbooks/devloop-validation.md §6.3`; §6.3 row + §8 rows split
  content-lane tokens (`-bytes-field`, `-cross-package-type`, `-import-not-allowed`: contract violation,
  re-derive with @security, no suppression) from could-not-evaluate tokens (`-scan-vacuous`,
  `-allowlist-not-enum`).
- Header comment and TODO.md `.proto`-scanner entry: the THREE-PART rule (no bytes field; no cross-package type except allowlisted enums by canonical FQN; no import outside the allowlist), no residual.
- MH-task obligation (observability) recorded in `docs/TODO.md` §Media Path Obligations. `end_meeting`
  is added to `zero_initialize_counters()` in the same commit as the recorder path, both statuses.
  The derived-copy sites are re-tightened from "not emitted yet" to the real set, and, as its OWN
  item (not folded into the site list), the `**Cardinality**` bullet re-tightened to the real product.

### ADR-0036 + docs

- §4: security's three sites VERBATIM (row after "Transmit-key cadence"; Site 2 replaces the bolded
  sentence through "…shorten this window."; Site 3 across the line wrap), correction as a
  `> **Correction (2026-09-22, story 2 planning).**` blockquote after the table. Consequences/Negative,
  §11 untouched.
- §8: one paragraph — forwarding policy gains fields, not RPCs; teardown is `EndMeeting`. Appendix
  "Internal contract": one sentence to the same effect + server mute is sender-scoped. **Deliberate
  one-line extension beyond task text (security ruling):** append one sentence (reword nothing) to the
  Appendix 2026-09-01 correction note — "Partly superseded 2026-09-22 (story 2): the direct case is now
  structural for `internal.proto` via the no-key-material guard; the general `.proto` scanner is still
  absent and the TODO entry stays open" — because two of its sentences ("no scanner", "reviewer-enforced,
  not structural") go stale when the guard lands. §4 R-13 Appendix-untouched still holds for R-13 text.
- INDEX.md :22/:30; API_CONTRACTS §4.1 (+ §4.2 End Meeting); TODO.md ZERO_REQUESTED entry → points at
  the proto comment and is marked resolved-as-documented; `.proto`-scanner entry gains the no-bytes
  rule + transitive caveat as a partial mechanisation (entry stays open for a real scanner).
- `docs/TODO.md:1322` ("MH does not verify meeting ownership…") gains a DATED AMENDMENT (A3,
  auth-controller; security co-signs the content). Entry stays OPEN. EndMeeting is a SECOND
  ownership-sensitive operation not bound to the caller: `mc_id` is a pod identity, not a secret, so any
  `service.write.mh` holder can either send a matching self-asserted `mc_id` or re-register and then
  end — releasing a victim meeting outright, a sharper effect than the generation wedge. Whatever
  eventually binds caller identity (token `sub`/`client_id`) to `mc_id` MUST cover **both**
  RegisterMeeting and EndMeeting; without this line a future fix closing only RegisterMeeting reads as
  complete. **Security co-signed, with three additions to the same dated amendment:**
  (a) state plainly it is a STEP UP from the accepted existence oracle (availability impact, not
  read-only probing) — accepted because the capability already exists in a strictly worse form (the
  entry's `policy_generation: u64::MAX` permanent blackhole with every signal green); EndMeeting is a
  cleaner, more legible, LESS persistent primitive. Not "no new exposure".
  (b) VISIBLY CORRECT the entry's load-bearing claim that generation monotonicity has "no recovery path
  within a process" / "nothing short of a pod restart lets the owning MC re-take the meeting" — EndMeeting
  forgets the applied generation, which is the first in-process recovery path. EndMeeting is
  simultaneously a new attack primitive and a mitigation of the entry's headline exposure.
  (c) WRITE DOWN the recovery ordering, because the mismatch check is not availability-neutral: in the
  wedge case the attacker's RegisterMeeting overwrote `mc_id`, so the victim MC's direct EndMeeting is
  rejected `FAILED_PRECONDITION`. Recovery is **re-register (reclaim `mc_id`) → EndMeeting (clear the
  generation) → re-register (fresh policy applies)**. Without it an operator tries EndMeeting first, gets
  FAILED_PRECONDITION, and wrongly concludes the meeting is unrecoverable. The proto comment stays short
  and points at the entry.
- **Citation form: by TITLE FRAGMENT everywhere, never `docs/TODO.md:<line>`** — "MH does not verify
  meeting ownership" in the proto comment, the TODO amendment, the ADR, and the EndMeeting
  not-an-authz-control sentence. `cite_extract.rs` scans `.md` files only, so a line-number citation in
  TODO.md or the ADR trips `validate-doc-citations-no-line-numbers`; in the `.proto` it would not trip,
  but it is just as fragile and fails silently. **The line separating the two cases** (security, on the
  record so it is not re-litigated): the rule binds DURABLE ARTEFACTS — the proto comment, the TODO
  amendment, the ADR — where a drifted line number sends a reader to the wrong paragraph and quietly
  breaks the link between a contract and its rationale. Line numbers in THIS devloop output are reviewer
  navigation in a working document nobody will resolve a citation from in six months, and they stay.
- Story-1 doc: 4 repoints + R-1 superseded note.
- Story-2 doc — **four independent prose-vs-reality drifts**, all one-liners in a file already open:
  (i) :89/:98/:122/:383 stale wire names + the superseded MC-derives-retention design; (ii) :453
  `PermissionDenied` → `FAILED_PRECONDITION`, so the task prompt matches the contract; (iii) **:165's
  off-by-one against the manifest (OPS-11, verified against rows :231-233)** — it reads "(task 16
  corrections early; task 17 new scenarios)", but task 16 is observability and owns no runbook text,
  task 17 is the corrections (:552), task 18 is the new scenarios (:568); both labels point one task too
  low, so routing runbook work from that line — which is what the line is for — lands on the wrong task.
  (iv) **:141's premise list is incomplete in observability's domain** (offered by @observability, lead
  call — I do NOT touch dashboards or catalogs for loopback prose, which story :535 + task 16 own, and
  rewriting operator-facing panel text before MH ships static fill would be the same error as pre-adding
  the `end_meeting` zero-init). The list names "the mh-media egress-drop panel copy" — the DERIVED copy —
  but not `docs/observability/metrics/mh-service.md:629`, the panel's own CITED SOURCE
  (`mh-media.json:203` says in terms "NOT restated here - see … §Media Forward Path"); fixing the panel
  and leaving its source is backwards, and :141's own framing ("nothing mechanical catches prose") is why
  a missing entry never gets caught. Also unnamed: `docs/observability/label-taxonomy.md:471`, where a
  surviving rule stands on two grounds and loopback was one — one clause to cut, not a rewrite; the
  subtle case, neither a stale fact nor a false cause but a rule left on one leg.
  (v) **task 17's scope enumeration has two orphans** (offered by @operations, lead call; verified):
  `client-dev-label`-adjacent `docs/runbooks/client-dev-local.md:359` promises "can still hear the
  loopback tone" — a retired observable, and in the bad direction, since an operator with no microphone
  follows it, hears silence, and blames the fake-device flag that worked (repoint to `__DT_TEST_TONE__`,
  not a deletion); and `:726` opens §4.5's triage ladder with "This is the section the loopback story
  exists for" — the one-leg case again, where the section becomes MORE load-bearing with N senders while
  its stated reason cites a retired story. Task 17 owns `client-dev-local` F13/F14 (:552), but neither of
  these is an F-entry, so both are currently owned by nobody. **Neither belongs in THIS diff** — they
  describe client/dev-loop behaviour that changes when R-3 and R-25 land, so editing them now would put
  the runbook ahead of the deployment (same error as pre-adding the `end_meeting` zero-init).
  **LEAD RULING (Gate 1): (i)-(iv) APPROVED and land in this loop** — :89/:98 (the :98 names-SSoT also
  GAINS `MH_MAX_MUTED_SOURCES_PER_MEETING`, which the R-9 MUST references but that line does not yet
  list), :122, :383, :453, :165, :141. The ADR Appendix one-sentence extension is approved too.
  **(v) was NOT in the lead's enumeration, so it does not land here** — @operations carries the two
  `client-dev-local.md` sites into task 17, which is the disposition they offered and the correct one
  (both describe behaviour that changes when R-3 and R-25 land).

  **Methodological note for the lead, worth more than any single site.** Three reviewers each found
  unenumerated sites by a DIFFERENT search axis: the invariant's WORDS (my first pass), its CONSEQUENCE
  stated without its words (@operations, OPS-10 — where a stale fact reads as stale but a stale
  consequence reads as a root cause), and the GROUNDS OF A SURVIVING RULE (@observability — where the
  rule lives, one of its two justifications dies, and a later reader concludes the rule died with it).
  That is not three oversights; it is one enumeration method that does not cover the space. Story :141
  already carries an ADR-0031 obligation to grep runbooks and dashboards when a requirement retires a
  behaviour — the obligation exists, but it names one axis. Naming all three wherever that obligation
  is recorded is the durable fix. **That text is NOT authored here** — story :539 schedules the ADR-0031
  obligation in task 16, so it does not exist in the tree yet; writing it in this diff would create it in
  one place while task 16 creates it in another (@observability, who owns it and will fold the three-axis
  wording in from the start). This paragraph is an observation for the lead, not text to land. The
  separate, still-open question is (iv) above — story :141's premise list, which is THIS story's list
  feeding task 16, not the durable obligation. The two are easy to conflate; only :141 is a question for
  me and the lead.

  Axis 1 leaves text that reads as stale; axis 2 leaves a false root cause; axis 3 is the quiet one and
  fails OPPOSITE to the other two — nothing reads as wrong, but a reader who notices the dead ground may
  conclude the RULE is vestigial. At `label-taxonomy.md:471` that rule is a privacy constraint, so the
  failure mode is relaxing it on the strength of a retired deployment shape. A one-axis instruction reads
  as discharged once the first grep comes back clean, which is exactly what happened: a careful
  enumeration still missed seven sites across three specialists.

### Validation

`./scripts/layer-fast.sh`; `cargo test -p proto-gen`; `verify-codegen.sh`; the new guard's self-test;
and a check that the guard reds on a scratch copy.

**`buf breaking` is SUPPRESSED for BOTH files this diff edits** (`proto/buf.yaml` `breaking.ignore`
lists signaling.proto and internal.proto), so a green Layer-6 asserts nothing about additivity here —
vacuity mechanism 1, an assertion over an empty set. The ignore list is NOT narrowed (buf.yaml forbids
it during the story; precision returns when the key is deleted post-merge). Evidence that IS real:
(a) PRIMARY — the pinned-ref ignore-free `buf breaking` run, with a positive control (below); (b) a hand read of the
proto diff — every change an added field or new message, no tag renumbered, nothing deleted, no type
changed — which the carve-out comment requires for a suppressed file; (c) `internal_roundtrip.rs` /
`signaling_roundtrip.rs` and `verify-codegen.sh`; (d) `buf lint` (STANDARD), unaffected and real.
`scripts/lang/proto/breaking.sh`'s `SUPPRESSED=<paths>` line is an EXPECTED Layer-6 line, captured as
known rather than triaged as a surprise.

**Ignore-free run — exact invocation (E1, verified; the two obvious forms FAIL).** `--against` resolves
the git path relative to the input dir, so `.git#…` from `proto/` errors ("does not appear to be a git
repository"); and input `proto` from the repo root errors ("contained by module at path \".\""). The
working form, and note buf exits **100** (not 1) when it reports findings:

The config is **DERIVED from `proto/buf.yaml`, not hand-mirrored** (security; CLAUDE.md single-source-of-
truth — and this command gets re-run after merge to confirm the ignore key's removal, so a transcribed
copy is stale by construction). No `yq` needed, and the skip predicate below does not depend on `ignore:` being the last key:

```bash
# derive: delete the `ignore:` key and its `    - ` entries ONLY, then resume printing
awk '/^  ignore:$/{skip=1; next} skip && (/^    / || /^[[:space:]]*$/){next} {skip=0; print}' \
  proto/buf.yaml > /tmp/buf-noignore.yaml
# TWO post-condition asserts on the OUTPUT, before it is passed to buf — they pin the
# derived config in both directions: enforcement present, suppression absent.
grep -qE '^    - FILE$' /tmp/buf-noignore.yaml \
  || { echo "derivation ate the FILE rule" >&2; exit 1; }
sed 's:#.*::' /tmp/buf-noignore.yaml \
  | grep -qE '^[[:space:]]*-[[:space:]].*\.proto[[:space:]]*$' \
  && { echo "derivation left a suppression entry" >&2; exit 1; }

cd proto && pnpm exec buf breaking . \
  --against "../.git#ref=32c01b278c03e72ec05d7db5a9962db1aae7d7c8,subdir=proto" \
  --config /tmp/buf-noignore.yaml
```

**Stop tuning the extractor; assert on its OUTPUT.** Three inputs have now defeated three patterns, and
every leak was fail-OPEN with the `- FILE` assert still passing (enforcement genuinely is still
declared) — only the positive control caught them, two steps downstream, where it triages as "the tool
is broken". The second assert above checks the output for **what must not survive**, so it closes every
interleave variant — security's comment-before-first-entry, the blank-line one, @paired-infrastructure's
column-zero-comment-between-entries (which I reproduced: security's pattern leaks 1 entry there, and the
assert CATCHES it), and the next one nobody has thought of — independently of how the text was extracted
and independently of awk dialect. The extractor's job is to be right on the file as it stands; the
assert's job is to make being wrong loud, at the step that went wrong, with a message naming it.

It keys on **"any surviving list entry that is a `.proto` path"**, NOT on the `dark_tower/` prefix that
happens to be there today — in a buf config the only legitimate lists are `modules.path`, `lint.use` and
`breaking.use`, so a `.proto` list entry can only be an ignore entry. Measured on four inputs: real
derived config → clean (its only entries are `- path: .`, `- STANDARD`, `- FILE`); column-zero-interleave
leak → fires; **a leaked entry NOT under `dark_tower/` → the prefix form MISSES and this one fires**
(that is the hole); a commented-out example entry → does not fire, which is what the `#`-strip buys.
And do not let a later reader "simplify" this to "assert no `ignore:` key survives": that MISSES the
column-zero case, where the key is stripped correctly and only an orphan entry leaks. Key-absence and
entry-absence are complementary, and entry-absence is the one that matters.

**The skip predicate is "more indented than the key", NOT "is a list entry"** (security, verified by me).
The earlier `skip && /^    - /` form resumes printing at the first line that is not a list entry, so a
**comment or blank line between `ignore:` and its first entry** resets the skip and BOTH entries survive
into the supposedly ignore-free config. I built that input and measured it: old form keeps 2 entries, new
form keeps 0. It fails in the dangerous direction — the derived config silently retains the suppression,
the run reports zero findings for the wrong reason, and the `- FILE` assert still passes because
enforcement is still declared. Only the positive control catches it. Not contrived, either: buf.yaml has
a 40-line comment block above `ignore:`, so the next person to edit that key demonstrably writes
comments.

**No interval quantifiers — awk here is `mawk 1.3.4` (`/usr/bin/awk` → `/usr/bin/mawk`), which does not
support them.** I confirmed `[[:space:]]{4,}` silently fails to match, which produced a derived config retaining both ignore entries while
looking correct. Same fail-open shape, arriving from the tool rather than the logic. Any re-derivation of this
line MUST be tested against a fixture that MUST produce entries, never eyeballed.

**`awk`, not `sed '/^  ignore:$/,$d'`** (machinery ruling): the `sed` form deletes to EOF, so it is
correct only while `ignore:` is the file's last key — and the remedy for that is a detector, where the
`awk` form removes the fragility outright. I tested both against a buf.yaml with a key appended below
`ignore:`: **sed silently ate the appended key; awk preserved it.** Prefer the construct that cannot go
wrong over a detector for it going wrong. The `- FILE` assert stays anyway — one line, and with `awk` it
should never fire, which is the point.

Verified on three inputs: real buf.yaml (entries gone, `breaking.use: [FILE]` intact),
comment-between-key-and-entries (entries gone), key-appended-below (`newkey:` preserved).
Verified by me end-to-end with the awk-derived config: output is exactly
`version/modules/lint(STANDARD)/breaking(FILE)` with the ignore key and both entries gone; vs pre-reshape
→ rc=100 / 11 findings; vs start commit → rc=0. **Use FULL shas** — the short `29d609bd` does not resolve
inside buf's clone and fails with a confusing `pathspec … did not match` on a correct-looking ref.

**Why there are THREE runs, stated so a later reader does not prune two as redundant:** the start-commit
run is the actual RESULT (must be zero findings); the pre-reshape pair is the POSITIVE CONTROL proving
the run can produce findings at all. Without the pair, a zero is indistinguishable from a mistyped ref
or an ignored `--config` — the same vacuity removed from the gate, relocated into its replacement.

**Positive control (E2) — run and recorded, because zero findings is indistinguishable from the command
silently doing nothing** (typo'd ref, ignored `--config`, empty comparison — vacuity mechanism 5, and it
would pass in my hands every time). Against the pre-reshape parent
`29d609bd1a639c51b7cab32efd1362d1189b9961` (measured independently by @paired-infrastructure,
@security and me — three runs, identical):

| Run | Tree state measured | rc | Findings |
|---|---|---|---|
| ignore-free inline config | pre-reshape ref `29d609bd`; **tree state irrelevant — this row measures the TOOL, not the diff, and is NOT re-run** | 100 | **11, all in internal.proto** — 8 × "Previously present message … was deleted" (`CascadeDestination`, `RegisterRequest`, `RegisterResponse`, `RouteMediaRequest`, `RouteMediaResponse`, `RoutingOptions`, `StreamTelemetryRequest`, `StreamTelemetryResponse`) + 3 × "Previously present RPC … was deleted" (`Register`, `RouteMedia`, `StreamTelemetry`) |
| the REAL repo config, same ref | same; also not re-run | 0 | zero |

That pair proves three things at once: the `--config` strip does real work, FILE enforcement genuinely
reaches internal.proto, and the suppression is what silences it. The expectation is **in-repo-derived,
not written from memory** — `proto/buf.yaml`'s own comment records the 2026-09-01 scope extension as
"11 findings (8 MESSAGE_NO_DELETE + 3 RPC_NO_DELETE)", and the measured breakdown matches exactly.
**The baseline row MUST be measured post-edit, and is labelled with its tree state.** Measured at
planning it is clean only because nothing is edited yet — a number produced where it could observe
nothing, sitting in a table that reads like validation of the diff, which nobody re-runs because it is
already green. That is the same shape as the gate this section removes. So: re-run the start-commit
ignore-free invocation AFTER THE LAST proto edit lands (not at planning, not mid-way), overwrite the
row, and label it `start-commit ref; tree = post-edit (all R-9/R-20/R-3 + signaling edits applied)`.
Expected rc=0 / zero findings, since every change is additive. **A non-zero result is a finding about
the diff, not a tooling problem** — check the new tags (`server_muted_sources = 7`,
`kek_rotation_debounce_seconds` 11 and 3, `unreachable_sender_ids = 2`) against prior use of those
numbers and escalate to @paired-infrastructure and @security rather than adjusting the invocation.

| Baseline run | Tree state measured | rc | Findings |
|---|---|---|---|
| ignore-free (awk-derived config, both post-condition asserts passed), `--against` start commit `32c01b278c03e72ec05d7db5a9962db1aae7d7c8` | **start-commit ref; tree = post-Gate-3 edits (all R-9/R-20/R-3 + signaling edits, plus the Gate-3 comment fixes: per-handler mute scope, quiesce MUST, re-emit obligation, MutedSource note, rewrap). Re-measured after the LAST proto edit, 2026-09-22** | 0 | **zero** |
| positive control re-confirmed with the SAME derived config in the same session | pre-reshape ref `29d609bd1a639c51b7cab32efd1362d1189b9961`; measures the tool, not the diff | 100 | 11 (all internal.proto, the recorded 8 + 3) |
| real repo config, pre-reshape ref | same; shows the suppression is what silences it | 0 | zero |

**Hand read of the proto diff (corroborating leg, as the carve-out comment requires for a suppressed
file).** `git diff -U0 -- proto/` filtered to non-comment, non-blank lines has **zero removed lines**.
Every added line is one of: `message MutedSource { uint32 sender_id = 1; }`,
`repeated MutedSource server_muted_sources = 7;` (RegisterMeetingRequest), `rpc EndMeeting(...)`,
`message EndMeetingRequest { string meeting_id = 1; string mc_id = 2; }`,
`message EndMeetingResponse { bool acknowledged = 1; }`,
`uint32 kek_rotation_debounce_seconds = 11;` (JoinResponse), `= 3` (MeetingKekUpdate),
`repeated uint32 unreachable_sender_ids = 2;` (StreamAssignments). Tags 7/11/3/2 were verified free and
un-reserved. No tag renumbered, no field or RPC deleted, no type changed, no enum value added.
`buf lint` (STANDARD) is clean.

**E3 is closed, not merely stated.** The hand-mirrored config is gone: the config is DERIVED from
`proto/buf.yaml` by the `awk` one-liner above, so it cannot drift (CLAUDE.md single-source-of-truth —
derive one from the other rather than maintaining two encodings). The positive control stays but its job
has changed: it now defends the TOOL and the REF, not the mirror — a zero would otherwise be
indistinguishable from a mistyped ref or a `--config` buf ignored. Also adding a line to `docs/TODO.md`'s "Restore buf breaking
enforcement after ADR-0036 story 1" entry: `ci-client.yml` hand-rolls its own `buf breaking` and so
carries the carve-out WITHOUT the `SUPPRESSED=` loudness line (silent green in CI), closed by deleting
the key.

### Gate 1 — Plan Confirmations (Lead)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired meeting-controller | confirmed |
| Paired media-handler | confirmed |
| Paired infrastructure | confirmed |
| Paired auth-controller | confirmed (GSA intersection: internal.proto keyed [protocol, auth-controller, security]) |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Plan approved 2026-09-22.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Wire contract (all additive; tags audited free)
| Item | Before | After |
|------|--------|-------|
| `RegisterMeetingRequest` | tags 1-6 | + `repeated MutedSource server_muted_sources = 7` (meeting-level, sender-only, dup-reject, `MH_MAX_MUTED_SOURCES_PER_MEETING` bound, bumps `policy_generation`) |
| `MutedSource` | — | new `{ uint32 sender_id = 1 }` |
| `MediaHandlerService` | `RegisterMeeting` only, "ONE RPC BY DESIGN" | + `EndMeeting(EndMeetingRequest{meeting_id, mc_id}) → EndMeetingResponse{acknowledged}`; invariant narrowed to forwarding policy |
| `JoinResponse` / `MeetingKekUpdate` | — | + `uint32 kek_rotation_debounce_seconds` = 11 / = 3 (W; rule's single home at JoinResponse: retention `min(W/2, ceiling)` at the client) |
| `StreamAssignments` | `assignments = 1` | + `repeated uint32 unreachable_sender_ids = 2` (per-subscriber, replace semantics, negative-space clause) |
| `SlotState` comments | client mute only; ZERO_REQUESTED/SOURCE_UNREACHABLE unspecified | SOURCE_MUTED covers client + server mute; ZERO_REQUESTED and SOURCE_UNREACHABLE retained-but-unemitted with reasons |
| internal.proto header | — | THREE-PART no-key-material rule, mechanically enforced |

### Comment corrections (complete-the-invariant)
R-3 loopback-as-normal: internal.proto (the five named sites + the "one audio egress stream" scope-limit line) and WEBTRANSPORT_FLOW.md. One-RPC claims, stated either as a fact or as a consequence, were narrowed across 13 sites: proto, INDEX, API_CONTRACTS, ADR-0036 §8 and Appendix, mh_service.rs, metrics.rs ×2, config.rs, errors_grpc_metrics_integration.rs, mh-service.md, mh-overview.json, mh-alerts.yaml (with a dated `CORRECTED` line and a new named teardown omission), mh-incident-response.md Scenario 13, and TODO.md entries (panel-28, alert design, never-reclaimed, shapes-considered). The stale "attestation → story 2" reference was repointed in signaling.proto ×3, API_CONTRACTS, SignalingClient.ts, mc-service media_admission ×2, and the story-1 doc ×4 (plus an R-1 superseded note). Historical task prompts in the story-1 manifest were left as the record of what was asked.

### ADR / docs
ADR-0036 §4: security's three-site correction applied verbatim, with the dated Correction blockquote. §8: the forwarding-policy vs teardown paragraph. Appendix: one sentence, and a one-sentence "Partly superseded" addendum to the 2026-09-01 correction note (lead-approved). CONVENTIONS.md: a dated correction that `breaking.ignore` covers internal.proto too, and the rule-3 example repointed to `EndMeeting`. TODO.md: ZERO_REQUESTED answered-in-contract; the `.proto`-scanner entry partly mechanised; the ownership entry amended per A3 (step-up, "no recovery path" corrected, three-step recovery order); the buf restore entry amended; two new §Media Path Obligations entries (story-4 fence, `end_meeting` zero-init timing). Story-2 doc fixes (i)-(iv) applied per the lead.

### Guard
`scripts/guards/simple/validate-internal-proto-no-key-material.sh` (100755). It runs three checks plus vacuity checks, has no suppression path, and adds a runbook §6.3 row and §8 rows. Self-test `scripts/guards/validate-internal-proto-no-key-material.test.sh` (64 assertions, rc and token on each), wired in `scripts/layer3.sh`. Demonstrated red on a scratch copy with planted `optional bytes` and `signaling.v1.MeetingKekUpdate` fields.

### Additional Changes (compile-forced, no behaviour change)
tonic trait: `end_meeting` on MH returns `Status::unimplemented` and deliberately records no metric. The MC mock records requests and acks. There are explicit empty/zero literals at the MC production sites, each naming its populating task. `proto-gen/src/lib.rs` shows W in both hand-written `Debug` impls; the `RedactedLen` calls are untouched. Rust bindings regenerate via build.rs, and TS bindings regenerate via `buf generate` into the gitignored `packages/sdk-core/src/proto/` (not committed; verify-codegen green).

---

## Files Modified

```
 crates/mc-service/src/grpc/mh_client.rs            |   7 +
 .../mc-service/src/media_admission/identity_key.rs |   2 +-
 crates/mc-service/src/media_admission/mod.rs       |   3 +-
 .../mc-service/src/media_signaling/assignments.rs  |   9 +-
 crates/mc-service/src/webtransport/connection.rs   |   6 +
 crates/mc-test-utils/src/mock_mh.rs                |  39 ++-
 crates/mh-service/src/config.rs                    |   6 +-
 crates/mh-service/src/grpc/mh_service.rs           |  37 ++-
 crates/mh-service/src/observability/metrics.rs     |  52 ++--
 crates/mh-service/tests/auth_layer_integration.rs  |   1 +
 .../tests/errors_grpc_metrics_integration.rs       |  10 +-
 crates/mh-service/tests/otel_grpc_integration.rs   |   2 +
 .../tests/register_meeting_integration.rs          |   1 +
 crates/mh-test-utils/src/media_policy.rs           |   3 +
 crates/proto-gen/src/lib.rs                        |  12 +
 crates/proto-gen/tests/internal_roundtrip.rs       | 164 ++++++++++++-
 crates/proto-gen/tests/signaling_roundtrip.rs      | 114 +++++++++
 docs/API_CONTRACTS.md                              |  55 ++++-
 docs/TODO.md                                       |  21 +-
 docs/WEBTRANSPORT_FLOW.md                          |   2 +-
 docs/decisions/adr-0036-media-flow.md              |  26 +-
 docs/observability/metrics/mh-service.md           |   6 +-
 docs/protocol/CONVENTIONS.md                       |  31 ++-
 docs/runbooks/devloop-validation.md                |   5 +-
 docs/runbooks/mh-incident-response.md              |   2 +-
 docs/specialist-knowledge/protocol/INDEX.md        |   5 +-
 .../2026-08-27-hear-yourself-through-handler.md    |  10 +-
 docs/user-stories/2026-09-21-hear-each-other.md    |  14 +-
 infra/docker/prometheus/rules/mh-alerts.yaml       |  22 +-
 infra/grafana/dashboards/mh-overview.json          |   2 +-
 packages/proto-gen/scripts/verify-codegen.sh       |  21 ++
 packages/sdk-core/src/signaling/SignalingClient.ts |   2 +-
 proto/dark_tower/internal/v1/internal.proto        | 270 ++++++++++++++++++---
 proto/dark_tower/signaling/v1/signaling.proto      | 173 +++++++++++--
 .../validate-internal-proto-no-key-material.sh     | 197 +++++++++++++++
 ...validate-internal-proto-no-key-material.test.sh | 160 ++++++++++++
 scripts/layer3.sh                                  |  11 +
 37 files changed, 1378 insertions(+), 125 deletions(-)
```

Self-check: `./scripts/layer-fast.sh` EXIT=0 (layers 1-6; guard PASSED, self-test 64/0). Also run by hand: `cargo test -p proto-gen` (new tests included), `cargo test -p mh-service -p mh-test-utils -p mc-test-utils` (0 failures), `cargo test -p mc-service --lib` (414 passed), `verify-codegen.sh` (all passed), `buf lint` clean, the post-edit ignore-free `buf breaking` (§Validation).

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

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

1. **Every form of the buf-breaking evidence step was fail-open, and each broke only when someone ran it.**
   There were five artefacts in sequence: the hand read, the uncontrolled re-run, the hand-mirrored
   config, the first awk extractor, and the first output assert. The first output assert hardcoded the
   `- dark_tower/` prefix, and a leaked entry outside that prefix defeated it. Each one was reviewed by at
   least two people and survived review. None survived execution against an input that had to produce
   output. This is the failure shape the no-key-material guard exists to catch, showing up in the
   verification of that guard.
2. **Moving the check from the extractor to its output was necessary but not sufficient.** A
   post-condition can be scoped as badly as an extractor. The final form holds because it is derived from
   a property of the artefact, not from the strings the file happens to contain today. In a buf config
   the only legitimate list entries are `modules.path`, `lint.use` and `breaking.use`, so any `.proto` list
   entry can only be a suppression. This is the same distinction as deriving an expectation from the
   artefact instead of writing it from memory.
3. **Enumerating the "one RPC" invariant by its words missed seven sites.** Three reviewers found them by
   three different search axes:
   - the invariant's words;
   - its consequence stated without those words;
   - the grounds under a rule that survives.
   The misses were not carelessness: a one-axis search counts as done as soon as the first grep comes
   back clean. The durable fix is task 16's ADR-0031 obligation text, which is not authored here.

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

---

## Gate 2 — Lead Validation (attempt 1)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=N/A`, EXIT=0 (log `/tmp/gate2-run1.log`).

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 9s |
| 2 Format | OK (no FMT_APPLIED) | 3s |
| 3 Guards | OK — incl. `validate-internal-proto-no-key-material` PASSED, self-test 64/0 | 91s |
| 4 Test | N/A (aggregate: rust+ts OK; proto `not-applicable-to-this-lang`) | 232s |
| 5 Lint | OK | 2s |
| 6 Audit | N/A (aggregate: cargo/pnpm audit OK, `buf-breaking-passed` — vacuous per §Validation, ignore-free run is the evidence) | 3s |
| 7 Env-tests | OK — `env-tests-passed`, `browser-e2e-passed` | 1094s |

Every N/A is a wrapper self-justified `REASON=` (ADR-0033 §6). No FAIL / PRECONDITION_FAILURE / NOT-RUN.

## Gate 2 — Lead Validation (attempts 2 and 3, after the Gate-3 fix round)

- **Attempt 2** (`/tmp/gate2-run2.log`): `EXIT=2`, Layer 4 `cargo-test-failed`. 19 gc-service DB-backed integration tests failed (health_checker, mh_health_checker, assignment_cleanup, gc `mh_service`). None of them touch this diff. Cause: the implementer's `layer-fast.sh` Layer-4 run overlapped this run on the shared test database. The implementer reproduced the same 19-test cluster under overlap, then got a clean serialized run (`gc-service --lib` 371/371). The attempt was counted, not waived.
- **Attempt 3** (`/tmp/gate2-run3.log`), run on the final tree with the implementer idle: `TOTAL_RESULT=N/A`, `EXIT=0`. L1 OK, L2 OK, L3 OK (guard PASSED, self-test 72/0), L4 N/A (aggregate; rust+ts OK), L5 OK, L6 N/A (aggregate), L7 OK (env-tests + browser E2E), 1189s. Every N/A comes with the wrapper's own `REASON=`.

## Gate 3 — Final Verdicts (Lead)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 13 | 13 | 0 | §4 text checked verbatim by script; guard hit with 18 adversarial inputs plus a root-seam probe; tag audit over full history |
| Test | CLEAR | 0 | 0 | 0 | Tag audit taken as the primary evidence that the change is additive |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | MutedSource OBSERVABILITY note premise |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | Stale guard comment after normalise() |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | Bare cardinality integer; ANCHOR used as a citation |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | Two line rewraps (mh-alerts.yaml, signaling.proto) |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | Credential-leak items 11-13; comment-vs-code drift |
| Paired meeting-controller | RESOLVED-FIXED | 5 | 5 | 0 | Per-handler mute scope; StreamAssignments re-emit; EndMeeting quiesce MUST |
| Paired media-handler | RESOLVED-FIXED | 2 | 2 | 0 | Teardown race; test for the end_meeting stub |
| Paired infrastructure | RESOLVED-FIXED | 3 | 3 | 0 | Guard whitespace bypass; self-test env leakage |
| Paired auth-controller | CLEAR | 0 | 0 | 0 | GSA intersection co-sign for internal.proto |

The Lead approved these scope extensions: story-2 doc lines :89/:98/:122/:383/:453/:165/:141 and :178/:472; one sentence in the ADR-0036 Appendix (security ruling); auth-controller was seated rather than waived.

## Accepted Deferrals (Lead)

- (none surfaced in this devloop)
