# Devloop Output: signaling.proto media-contract reshape

**Date**: 2026-08-31
**Task**: Reshape `proto/dark_tower/signaling/v1/signaling.proto` for the ADR-0036 media path (Appendix "Signalling contract")
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, `--paired-with=meeting-controller`
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: 2026-08-31 ~20:50 → 2026-09-01 05:0x UTC, across three sessions (implementation+review; a Gate-2 escalation; and this post-decision resumption), interrupted once by an `auth-expired` runner incident

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3a7104f341c4b1130048d4419d65788149c43dd9` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Story | `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` task 3 |
| Headless | yes (`DEVLOOP_HEADLESS=1`) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (escalated at Gate 2 2026-09-01 03:19; human accepted the wire break and directed the `buf.yaml` carve-out; resumed and re-gated — see §Human Decision and Resumption) |
| Implementer | `complete` (all Gate-2 + Gate-3 round 1/2/3 findings applied) |
| Implementing Specialist | `protocol` |
| Iteration | `3` (Gate-3 round 2) |
| Security | `RESOLVED-DEFERRED` (Gate 3: all 8 findings closed; media-protocol anchors → task 8. **Delta review 2026-09-01**: S1 CONVENTIONS.md false "no suppression mechanism" claim — FIXED by @team-lead; S2 carve-out scope disclosure — closed by the narrowing; S3 no dated hard-fail expiry — accepted deferral, owner infrastructure+security. Confirmed carve-out does not widen into lint or the wire-format guards and that no other suppression was added. **Disclosed a process incident** — see §Process incident) |
| Test | `RESOLVED-FIXED` (TEST-1..4 verified; B22 added) |
| Observability | `RESOLVED-DEFERRED` (final; ESCALATION cleared. 9 findings, 8 fixed. F5 closed at the type — reviewer enumerated both `Participant` carriers and confirmed no third. F8 dt-guard label-category change is the one accepted deferral) |
| Code Quality | `RESOLVED-FIXED` (F1/F2/F3 verified) |
| DRY | `RESOLVED-DEFERRED` (D1–D4 verified; remaining bullets are TODO entries) |
| Operations | `RESOLVED-FIXED` (Gate 3: OPS-1/2/3/4 all fixed. Re-measured Layer 6 independently: 54/54 set-identical. **Delta review 2026-09-01**: OPS-D1 carve-out narrowed to one file with measurement; OPS-D2 `SUPPRESSED=` loudness added to `breaking.sh`; OPS-D3 runbook corrected; OPS-D4 ci-client `SUPPRESSED=` gap deferred to infrastructure; OPS-D5 record fixes incl. the Layer-3 `owner_not_in_manifest` red) |
| Semantic Guard | `CLEAR` (zero findings; B16 checks.md vocab tracked in docs/TODO.md, re-confirmed) |
| Paired: Meeting Controller | `RESOLVED-FIXED` (trailer corrected + B19 row confirmed, both verbatim) |
| Conditional: Client | `RESOLVED-FIXED` (B14 SignalingCodec export made under Lead fix-now ruling) |
| Conditional: Auth Controller | `RESOLVED` (both length-check sites now name task 10, verified) |

---

## Task Overview

### Objective
Land the ADR-0036 signalling contract in `proto/dark_tower/signaling/v1/signaling.proto`: delete the
layout-subscription messages, the duplicate media-kind enums and the single-key `EncryptionKeys`;
add receive-capability, slot state, send directive, stream assignment, roster identity key, join-time
KEK + generation + `sender_id`, and an additively-defined KEK-push message; rename `HostMuteRequest`
to server-mute terminology. Regenerate Rust (`proto-gen`) and TypeScript (`sdk-core`) bindings.

### Scope
- **Service(s)**: proto contract + generated bindings; consumers MC, SDK (client), AC (roster identity key)
- **Schema**: No database schema changes
- **Cross-cutting**: Yes — wire format is a Guarded Shared Area (ADR-0024 §6.4)

### Debate Decision
NOT NEEDED — ADR-0036 is the governing decision; this task implements its Appendix.

---

## Cross-Boundary Classification

**Commit trailer** (supplied by @paired-meeting-controller, covering the **six** Minor-judgment MC
rows — connection.rs, handler.rs, participant.rs, messages.rs, meeting.rs, and docs/TODO.md, the last
four upgraded to Minor-judgment at Gate 1; the Mechanical rows are review-only and need no trailer.
Count and enumeration corrected at Gate 3 on the owner's finding: participant.rs shipped
server-mute renames but was missing from the rename enumeration, and the count still read "four"):

```
Approved-Cross-Boundary: meeting-controller — ADR-0036 §5 server-mute terminology across MC's actor contract (actors/meeting.rs, actors/messages.rs, actors/participant.rs: MeetingMessage::HostMute, host_mute(), handle_host_mute(), ParticipantInfo + ParticipantStateUpdate::MuteChanged fields, meeting-actor participant record, test names) and ADR-0036 §4 join/roster field plumbing (webtransport/connection.rs::build_join_response, webtransport/handler.rs roster construction) — compiler-checked rename plus struct-literal field additions, no behaviour change; sender_id allocation and KEK provisioning are story task 10. docs/TODO.md: MC-owned JoinResponse.sender_id follow-up (task 10) supersedes the user_id==0 entry.
```

`proto/**`, `proto-gen/**` and `build.rs` are Guarded Shared Areas (ADR-0024 §6.4) — `Mechanical` is
disallowed there; no row below is classified `Mechanical` inside one.

**Co-review notes for the three post-escalation rows (recorded here, not in the Owner column, for
the same reason as the §6.4 note below — the guard parses that cell as a bare specialist name).**
`proto/buf.yaml` is a pipeline surface as well as a GSA: owner protocol, co-reviewed by operations.
`scripts/lang/proto/breaking.sh` is shared gate tooling: the change is operations' (a `SUPPRESSED=`
warning, no gate-logic change), and infrastructure is the natural co-reviewer since the companion
`.github/workflows/ci-client.yml` follow-up is theirs. The first version of the `proto/buf.yaml` row
put "protocol (pipeline surface: operations co-review)" in the Owner cell and tripped
`validate-cross-boundary-classification` with `owner_not_in_manifest` — the exact failure the §6.4
note below warns about.

**§6.4 intersection rule (recorded here, not in the Owner column — the guard parses that cell as a
bare specialist name against the ownership manifest).** The identity-key and KEK hunks in
`signaling.proto` span wire-format *and* auth-routing-policy, so they require protocol +
**auth-controller** + **security** confirmation at **both** Gate 1 and Gate 3. All three are on this
team; @auth-controller and @security have confirmed at Gate 1 and will re-confirm the literal field
text at Gate 3.

**`crates/media-protocol/**` is deliberately not touched by this devloop.** @operations' requested
reciprocal `ANCHOR (DRY)` line on `frame.rs::PROTOCOL_VERSION`, and @security's requested mirror of
the compile-time-constant-lengths invariant into `frame.rs:384`'s `Debug` impl, are both GSA edits
requiring **media-handler** co-sign at Gate 1 *and* Gate 3 — a full review cycle from a specialist
with no other stake in this diff, bought for two comment lines. Lead's decision: do not pull MH in;
drop both hunks here and record them. The load-bearing half lands regardless — the cross-language
anchor to `proto/test-vectors/frame-v2.vectors.json` is the control that prevents drift and it is on
the protocol side of the boundary; the `frame.rs` lines are reciprocal back-pointers. Recorded in
`docs/TODO.md` (see P5) naming `frame.rs::PROTOCOL_VERSION`, `frame.rs:384`, media-handler as the
required co-owner, and the plain statement that this devloop declined to edit another owner's
Guarded Shared Area for a comment.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/signaling/v1/signaling.proto` | Mine (GSA) | protocol |
| `proto/buf.yaml` | Mine (GSA — `proto/**`), **post-escalation add, human-directed** | protocol |
| `scripts/lang/proto/breaking.sh` | **post-escalation add (@operations delta review)** — `SUPPRESSED=` loudness for the carve-out; comment + one emission block, gate logic and `--against` invocation unchanged | operations |
| `docs/runbooks/devloop-validation.md` | **post-escalation add (@operations delta review)** — Layer-6 symptom rows for the new suppression channel | operations |
| `crates/proto-gen/build.rs` | Mine (GSA) | — |
| `crates/proto-gen/src/lib.rs` | Mine (GSA) | — |
| `crates/proto-gen/tests/signaling_roundtrip.rs` (new) | Mine (GSA) | — (co-review: @test) |
| `packages/proto-gen/scripts/verify-codegen.sh` | Mine (GSA — `proto-gen/**`) | — (co-review: @test) |
| `docs/protocol/CONVENTIONS.md` | Mine | — |
| `docs/API_CONTRACTS.md` | Mine | — |
| `docs/WEBTRANSPORT_FLOW.md` | Mine | — |
| `docs/specialist-knowledge/protocol/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-08-31-signaling-media-contract-reshape/main.md` | Mine | — |
| `docs/TODO.md` | Not mine, Minor-judgment | meeting-controller |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` (ANCHOR-location line) | Not mine, Minor-judgment | dry-reviewer (@dry-reviewer) |
| `crates/mc-service/src/webtransport/connection.rs` | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/webtransport/handler.rs` | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/actors/participant.rs` | Not mine, **Minor-judgment** (upgraded at Gate 1) | meeting-controller |
| `crates/mc-service/src/actors/messages.rs` | Not mine, **Minor-judgment** (upgraded by owner at Gate 1) | meeting-controller |
| `crates/mc-service/src/actors/meeting.rs` | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/tests/join_tests.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/otel_webtransport_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/webtransport_accept_loop_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/media_connection_update_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/env-tests/tests/24_join_flow.rs` | Not mine, Mechanical | meeting-controller |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/lib.rs` (stale "layout subscription" doc line) | Not mine, Mechanical | meeting-controller |
| `packages/sdk-core/src/signaling/events.ts` | Not mine, Minor-judgment | client (@client) |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Not mine, **Minor-judgment** (public-surface `Codec` pass-through) | client |
| `packages/sdk-core/src/signaling/errorCodeMap.ts` | Not mine, Mechanical | client |
| `packages/sdk-core/src/signaling/__tests__/maps.test.ts` | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/helpers.ts` | Not mine, Mechanical | client |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` | Not mine, Minor-judgment | client (bigint assertions become number) |
| `packages/sdk-core/src/signaling/codecMap.ts` (new) | Not mine, Minor-judgment | client |
| `packages/sdk-svelte/src/__tests__/MeetingStore.test.ts` | Not mine, Mechanical | client |
| `packages/sdk-svelte/src/__tests__/subscribeSession.test.ts` | Not mine, Mechanical | client |
| `packages/web-app/src/lib/e2eBus.ts` | Not mine, Mechanical | client |
| `packages/web-app/src/__tests__/e2eBus.test.ts` | Not mine, Mechanical | client |
| `packages/web-app/src/__tests__/helpers/MockMeetingSession.ts` | Not mine, Mechanical | client |
| `packages/web-app/e2e/join-happy-path.spec.ts` (comment only) | Not mine, Mechanical | client |
| `packages/sdk-core/src/index.ts` | Not mine, **Minor-judgment** (Gate-3 add: root-export `SignalingCodec`) | client (@client) |
| `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` | Not mine, **Minor-judgment** (Gate-3 add: task-10 prompt fix under @team-lead authority; **further edited post-escalation by the human at 04:21 — a substantive design propagation, not prose**: task 10's prompt now states `user_id` is deleted with tag 2 reserved and NOT repurposed, and `sender_id` is a fresh tag bounded by `NonZeroU16`. That matches what task 3 actually implemented — verified against `JoinResponse` `reserved 2, 5;` + `sender_id = 8` — so the manifest and the contract agree) | meeting-controller (MC owns task 10) |
| `docs/runbooks/mc-deployment.md` | Not mine, **Minor-judgment** (Gate-3 add: MC+SDK wire-lockstep coordination line) | operations |

Any file discovered during implementation that is not listed here gets a row added **before**
Gate 2, so the scope-drift guard compares against a complete list.

---

## Lead Ruling — Layer-6 `buf breaking` (Gate 1, 2026-08-31)

Raised by @operations (OPS-1) and @paired-meeting-controller before Gate 1, and verified
independently by the Lead against source.

**The situation.** `proto/buf.yaml` runs `breaking.use: [FILE]` (not `[WIRE_JSON]` — changed in
`2e4120a`; `docs/protocol/CONVENTIONS.md` still says WIRE_JSON and is fixed in this commit). FILE
fires on symbol deletion, field deletion, field rename and field type change — every category this
reshape uses. `scripts/lang/proto/breaking.sh` has no suppression path by design.
`scripts/lang/_common.sh:137` `run_and_emit` normalizes buf's exit 100 to `STATUS=FAIL` + rc 1, and
`scripts/workflow/run-story.sh:1589` routes `gate_rc -eq 1` to `escalate pipeline-red-layer6` — the
**implementer** lane. The comment block above that line states outright that the runner cannot route
what a model asserts in prose, so a Lead's Gate-2 acceptance does not change `gate_rc`.

The project's override for an intentional wire-break (`docs/TODO.md`) is recorded as an explicitly
mechanism-less convention: the human names the expected Layer-6 fire in the `/devloop` prompt and the
Lead accepts at Gate 2. `task-3.prompt` carries no such declaration. ADR-0033 §13 defers the override
mechanism to Wave 3, pending ≥2 real wire-breaking PRs as case studies; case study #1 (R-61 task #31)
never actually fired, because the file moved and buf saw new-file additions.

**Ruling.**

| # | Decision |
|---|----------|
| L6-1 | **No masking.** No `buf.yaml` carve-out, no `# buf:breaking:ignore`, no `--exclude-path`, no env bypass, no ruleset relaxation. CLAUDE.md "fail loudly; never mask". |
| L6-2 | **The ADR-0033 §13 Wave-3 override mechanism is NOT pulled forward.** The ADR defers that design decision and its own ≥2-case-study trigger is not met. Building it here would be a Lead self-authorizing an ADR-deferred decision on a pipeline surface owned by operations/infrastructure. Recorded as a live option for the human, not dismissed. |
| L6-3 | The implementer's in-tree `## Expected Layer-State` declaration (main.md + commit message) **is** ADR-0033 §13's "acknowledged explicitly in-tree", and lands. It must be precise enough that the real Layer-6 log is diffable against the prediction. |
| L6-4 | **Implementation proceeds in full.** The reshape is identical work under either resolution; only the record-keeping around the red differs. Escalating pre-implementation would cost the whole task and hand the human a hypothetical instead of a reviewed diff and a measured log. |
| L6-5 | **The Lead escalates at Gate 2 rather than accepting a red Layer 6.** A Lead accepting a red gate in a headless run is the self-approved risk acceptance devloop Headless Mode forbids, and the acceptance could not change `gate_rc` in any case. |

**Discriminator for Gate 2**: the actual Layer-6 finding set must match the declared prediction. An
*unpredicted* finding is a bad diff, not a deliberate break, and belongs in the implementer lane on
its merits. @operations owns that comparison.

**Scope note**: task 4 (`internal.proto` reshape) hits the identical wall, so the human's decision
should resolve the story, not this task alone.

---

## Planning

### Governing sources

ADR-0036 §2 (frame format / header version 2 / key id), §3 (sender authentication), §4 (KEK model,
roster identity key, "no key material on the roster"), §5 (send directives, transport mode, server
mute), §6 (receive capability, seven slot states), §7 (priority group is an MC→MH egress property,
NOT on the send directive) and the Appendix "Signalling contract". Conventions:
`docs/protocol/CONVENTIONS.md` (buf STANDARD, no `lint.ignore`, no `// buf:lint:ignore`, `vN`
package, bare RPC request/response names, distinct response type per RPC).

### P0 — BLOCKER surfaced before implementation: `buf breaking` will fire, by design

`proto/buf.yaml` pins `breaking.use: [FILE]`. `scripts/lang/proto/breaking.sh` is an always-run
Layer-5/6 gate with **no** `--exclude-path`, no `--against` override, no env bypass (the in-tree
annotated allowlist is deferred to ADR-0033 §10 Wave 3 and does not exist). Every mandated item —
the five layout deletions, `EncryptionKeys`, the `StreamType`/`MediaType` collapse, the
`HostMuteRequest` rename, the freed oneof tags, and the `JoinResponse` tag-2 change — trips
`MESSAGE_NO_DELETE` / `ENUM_NO_DELETE` / `FIELD_NO_DELETE` / `FIELD_SAME_NAME` / `FIELD_SAME_TYPE`.
The sibling-`v2`-package escape does not avoid it either (deleting `v1` fires `FILE_NO_DELETE`, and
`internal.proto`'s cross-package `MediaStream` reference changes type).

**Chosen route: accept the fire, loudly and on the record.** This is the documented in-tree
convention for an intentional wire break (`docs/TODO.md:820`): the expected Layer-6 fire is declared
up front and the Lead accepts it at Gate 2. Basis: `docs/protocol/CONVENTIONS.md` §"Why STANDARD"
records that there are no on-the-wire clients outside this codebase; the in-file `ClientMessage`
tag-11 reuse is the same precedent; ADR-0036's Appendix is the governing decision. See
`## Expected Layer-State` below.

### P1 — The reshaped contract

*(This heading, its intro sentence and the table header below were reconstructed at Gate 2. A bad
edit during drafting had spliced the whole `## Expected Layer-State` body over them, leaving P0
ending mid-sentence and this table headerless — see §Issues Encountered & Resolutions #5. The table
rows themselves are the original text.)*

New messages, per ADR-0036's Appendix "Signalling contract":

| Message | Shape | ADR-0036 § |
|---------|-------|------------|
| `ReceiveSlot` | `slot_id` (uint32, 16-bit semantics), `media_kind`, `optional uint32 pinned_sender_id` | §6 |
| `ReceiveCapability` | `repeated ReceiveSlot slots` | §6 |
| `EncodingParameters` | `codec`, `max_bitrate_bps`, `width`, `height`, `frame_rate` | §5 |
| `SendTarget` | `media_handler_url`, `TransportMode transport_mode` — **no priority group** | §5/§7 |
| `SendStream` | `stream_number` (uint32, **8-bit** semantics per the key id), `media_kind`, `EncodingParameters encoding`, `repeated SendTarget targets` (empty = send nothing) | §5 |
| `SendDirective` | `repeated SendStream streams`, `uint32 header_version` (meeting-wide, MC-assigned) | §5/§2 |
| `StreamAssignment` | `slot_id` (uint32, 16-bit), **`optional uint32 sender_id`** (1..=65535, 0 never valid — same shape as JoinResponse and the roster), `media_kind`, `media_handler_url`, `SlotState slot_state`, `optional uint64 switch_command_id` | §2/§3/§6/§7 |
| `StreamAssignments` | `repeated StreamAssignment assignments` | §6 |
| `MeetingKekUpdate` | `bytes meeting_kek`, `uint32 kek_generation` (u16 semantics) — additive, unused this story, wired into the `ServerMessage` oneof so story 2 is behaviour-only | §4 |

`StreamAssignment` carries **exactly one** positional concept (`slot_id`) — the deleted message's
`stream_id` + `slot_index` pair is not carried forward (MC item 11). The pin is an `optional` field
**inside** `ReceiveSlot`, never a parallel list, so "seven pins into six slots" is unrepresentable
(§6); `LayoutConfig.pinned_users`/`excluded_users` do not survive in any form.

**Slot-id ownership — RESOLVED at Gate 1: client-chosen.** `slot_id` is chosen by the subscriber in
its capability declaration and echoed by MC in the assignment — same shape as the deleted
`SubscribeToLayout.stream_ids` ("subscriber-chosen IDs for each slot"), and it is the subscriber's
own rendering surface that the relay-region stream id addresses (§2). @paired-meeting-controller
reversed their MC-allocated ask and confirmed this shape. Four things the comment carries, because
tasks 4/11/14 would each otherwise invent them:
- **`slot_id` is scoped to one subscriber's connection, NOT globally unique.** MH's routing-table key
  must be `(subscriber, slot_id)`, never `slot_id` alone, or two subscribers both picking slot 0
  collide. Task 4's `EgressStream` already carries the pair; the scoping has to be *stated* or it
  gets flattened the first time someone writes the lookup.
- **Unique within one `ReceiveCapability`** — MC **rejects** duplicates, never last-write-wins
  (a duplicate is "two sources into one slot").
- **MC validates** the 16-bit range and the server-side slot-count cap, and **rejects the whole
  declaration** on violation. `slot_id` is attacker-controlled input that MC forwards into the
  relay-region address space (MC → task 4's `EgressStream` subscriber reference → MH writes it into
  the frame's relay-region `stream_id`), so the comment forecloses the two implementations that would
  read as satisfying "validates": **not a `debug_assert!`** (`[profile.release]` does not set
  `debug-assertions`, so it would be compiled out in exactly the builds facing untrusted clients —
  the same hazard story task 6 exists for), and **not clamp-or-truncate** (reject, never silently
  truncate into the u16 relay field) — the same fail-closed-never-truncate words as `sender_id`'s
  OPS-7 paragraph, so the two read as one discipline. Blast radius stated at the definition too:
  `slot_id` is per-subscriber-scoped, so a malformed one cannot reach another participant's slots or
  another subscriber's routing entries — which is *why* rejecting suffices and no cross-participant
  validation is needed, **and which depends entirely on the `(subscriber, slot_id)` keying above
  holding in MH**. The two conditions are load-bearing for each other; if task 11 ever keys on the
  bare id, this bound evaporates. Enforcement owner: story task 14, validation site
  `u16::try_from(...)`.
- `slot_id` **is the same value space** as the frame's relay-region `stream_id`
  (`ANCHOR (DRY): frame.rs::STREAM_ID_FIELD_BYTES = 2`), which is what makes `docs/TODO.md`'s
  "receiver must validate `stream_id` against its own declared slots (§6)" obligation expressible.

**Fields added / changed on existing messages.**

- `ParticipantCapabilities` gains `repeated uint32 supported_header_versions`. Closes OPS-6's gap:
  §2 says MC selects the meeting-wide version *from the participants' declared capabilities*; with
  no declaration the version is de facto compiled in on the client. **Downgrade floor (G1-1)** — this
  is a negotiation surface and header v2 is the version carrying the signature and key-bearing flag,
  so one patched client declaring only `1` could otherwise drag the *whole meeting* onto a header
  with no §3/§4 guarantees. The proto cannot enforce, so the comment states: MC selects from a
  server-side allowlist with a configured **minimum version floor**; declarations are an upper bound
  on what a client can do, never a lower bound on what the meeting accepts; a client declaring
  nothing at or above the floor is **rejected at join**, never accommodated by lowering the meeting;
  an empty list is **not** "accept anything" — fail closed; the list is unbounded on the wire and
  capped by server-side configuration. `SendDirective.header_version` carries the reciprocal: MC
  MUST NOT emit a version below the floor, even as a policy downgrade. **Enforcement owners:** story
  task 14 (MC send-directive composition — allowlist and floor), story task 10 (MC join path —
  rejection of a client below the floor).
- `JoinRequest` gains `bytes identity_public_key` — raw 32-byte Ed25519 signing public key (§4
  step 2).
- `JoinResponse`: `reserved 2, 5;` **and** `reserved "user_id", "encryption_keys";` (`// was:`
  comments on top of the statements, not instead of them); gains `sender_id`
  (**`optional uint32`**, valid 1..=65535, 0 never valid), `meeting_kek` (bytes, 32 = AES-256), `kek_generation` (uint32, u16
  semantics).
- `Participant` (roster) gains **both** `sender_id` and `identity_public_key` — task 14's receiver
  maps arriving frame → `sender_id` → roster identity key → Ed25519 verify, which is unexpressible
  with the key alone. **No other key material on the roster** (§4).
- `MediaStream` / `PublishStream`: `stream_type` → `media_kind` and `metadata` → `encoding` on fresh
  tags, with `reserved 2, 3;` **and** `reserved "stream_type", "metadata";` on each, per Rule R —
  the enum's zero value changes meaning and the metadata type is deleted.
- `HostMuteRequest` → `ServerMuteRequest` (same message, same tag 8, no behaviour change), oneof
  field `host_mute_request` → `server_mute_request`. **Sibling rename, same owner / same mechanism**:
  `ParticipantMuteUpdate.audio_host_muted`/`video_host_muted`/`host_muted_by` and the "host-muted"
  prose on `MuteRequest`/`UnmuteRequest`/`UnmuteResponse` rename to server-mute terminology too —
  §5 avoids *host mute* precisely because it presumes an undefined role model, and renaming one of
  four occurrences leaves the term alive on live surface MC consumes.

**Deleted.** `SubscribeToLayout`, `UpdateLayout`, `UnsubscribeLayout`, `LayoutConfig`, `LayoutType`,
the layout-form `StreamAssignment`/`StreamAssignments`, `EncryptionKeys`, `StreamType`, `MediaType`.
Freed oneof tags reserved at **message scope** (`reserved` cannot live inside a `oneof`):
`ClientMessage` — `reserved 3, 4, 5;` and
`reserved "subscribe_to_layout", "update_layout", "unsubscribe_layout";`
`ServerMessage` — `reserved 5;` **number only, deliberately no name reservation**: the field name
`stream_assignments` is re-used by the new shape on a fresh tag, and reserving a name you then use is
a protoc error.
`ParticipantCapabilities` — `reserved 1 to 4;` and
`reserved "video_codecs", "audio_codecs", "supports_simulcast", "max_video_streams";` (tag 4
`max_video_streams` folded in from the scope expansion — see the correction in
`## Expected Layer-State`).

**`StreamAssignment` — corrected at Gate 2 (finding raised by @team-lead).** The name is *reused* by
the ADR-0036 shape, so the message is not deleted; the five layout-form tags inside it are. As first
implemented the new shape re-pointed tags 1–5 in place with no `reserved` statement, which is
precisely what Rule R forbids: tag 2 `uint64 user_id` → `optional uint32 sender_id` is varint-to-varint
and tag 3 `MediaType` → `MediaKind` changes the enum's zero meaning, so an old peer decodes both
**successfully, with the wrong semantics** — byte-for-byte the hazards `JoinResponse` tag 2 and
`MediaStream`/`PublishStream` tags 2–3 already reserve against. Tags 1, 4 and 5 were
wire-incompatible repurposes that fail loud, but an inconsistently applied rule is worse than none,
so all five are reserved. Fixed shape: `reserved 1, 2, 3, 4, 5;` plus
`reserved "stream_id", "user_id", "media_type", "slot_index";`, and the six ADR-0036 fields move to
fresh tags 6–11 (`slot_id = 6`, `sender_id = 7`, `media_kind = 8`, `media_handler_url = 9`,
`slot_state = 10`, `switch_command_id = 11`) so declaration order stays ascending.
`media_handler_url` is deliberately **absent from the name reservation** — the new shape reuses that
name on tag 9, and reserving a name you then use is a protoc error. That makes **two** places the
name half of Rule R does not apply, not one: `ServerMessage`'s `stream_assignments` and this. Both
are called out in-file rather than left looking like omissions.

`StreamAssignments` (plural) takes **no** reservation: its single field keeps its name
(`assignments`), its tag (1), its cardinality and its type *name* across the reshape — only the
element message's contents changed, and that hazard is reserved inside `StreamAssignment`.
Reserving tag 1 there would vacate a field nothing repurposed. Stated in-file so the omission reads
as a decision.

**Also deleted, answering the DRY "which encoding shape survives" question:** `StreamMetadata`,
`VideoMetadata`, `SimulcastLayer`. `EncodingParameters` is the single encoding vocabulary in the
file and `MediaStream` is retyped onto it. Two encoding-parameter shapes in one file is the exact
duplication class this task exists to remove, and simulcast/scalable coding is explicitly out of
scope in ADR-0036. `internal.v1`'s `RegisterRequest.streams` keeps working (it imports `MediaStream`,
whose identity is preserved).

### P2 — Answers to specific reviewer pre-conditions

**Naming floor (hard).** The roster/join field is `identity_public_key`. Never "attested",
"verified", or "trusted" in the name or the comment — including not importing the Appendix's
"AC-attested" adjective, which describes the story-2 end state. The comment states the honest floor:
*this story performs no cnf/thumbprint check, so MC accepts the key trust-on-first-use; a valid
frame signature proves same-keyholder consistency only, never a verified identity, and for guests
never more than a pseudonym.* No `attestation`/`cnf`/`thumbprint` sibling field or reserved slot —
story 2 adds the attestation additively (@auth-controller items 1–4). Nothing here touches the
client→GC join-body edge. Comment states keys are meeting-scoped, never reused across meetings.

**Identity-key shape.** Raw 32-byte Ed25519 public key — not PEM, JWK, base64 or SPKI. `bytes` has
no length bound in protobuf, so the comment says the proto does **not** validate: MC MUST length-check
32 before publishing a client-controlled blob to the whole roster, and empty means *no key
published* — consumers MUST fail closed and MUST NOT fall back to "accept unsigned". Whether MC
rejects the join or publishes empty on a wrong-length key is MC behaviour (task 10); the contract
makes "absent" representable and documents the fail-closed requirement.

**KEK comments.** Never described as end-to-end or zero-trust; the deleted message's
`// Encryption keys for end-to-end encryption` phrasing does not survive. The accurate sentence,
from §4: *media is encrypted between clients; MH, transport and storage cannot read it; MC can*;
telemetry spelling is `key_custody=operator`. `SECURITY:` blocks matching the in-file
`MhConnectionStatus.mh_url` style at both KEK sites: live key material, never logged, never a span
attribute, never a metric label, never in an error or panic payload, and never crosses the MC→MH
contract. Empty KEK means *not yet provisioned* — a consumer MUST fail closed and MUST NOT wrap or
unwrap under a KEK of any length other than 32; an all-zero / empty KEK is never a usable key.

**KEK Debug leak — a control, not a comment (observability #1).** `crates/proto-gen/build.rs` is a
bare `tonic_build::configure()`, so every message derives `Debug` and any `{:?}` on a `ServerMessage`
prints the KEK. Fix: `.skip_debug("dark_tower.signaling.v1.JoinResponse")` and
`.skip_debug("dark_tower.signaling.v1.MeetingKekUpdate")` (verified present on tonic-build 0.12.3,
`src/prost.rs:598`), plus hand-written redacting `impl Debug` for both in
`crates/proto-gen/src/lib.rs` — the types are local to that crate so there is no orphan problem, and
`crates/common/src/jwt.rs` is the in-tree precedent. **Same-mechanism sibling (G1-3):**
`JoinRequest` gets the same treatment, because it carries `join_token` (a bearer meeting JWT) and
`binding_token` (the HMAC session-binding credential), derives `Debug` today, and rides inside a
`ClientMessage` that also derives it — same crate, same file, same machinery being stood up in this
changeset. The `JoinResponse` redacting impl redacts `binding_token` as well as
the KEK; a redacting `Debug` that still prints a session-binding credential is worse than none,
because it looks handled. **`correlation_id` stays in the clear** (@observability's push-back,
accepted by @security after re-checking): it is an identifier, not a credential — possessing it does
not authenticate a reconnect, the HMAC on `binding_token` does
(`crates/mc-service/src/actors/session.rs:117`) — and it is the ADR-0023 reconnect correlation key,
i.e. the first field an operator reaches for when triaging a failed session recovery. Redacting it
buys nothing and costs incident legibility on exactly the wrong field. (Being HMAC *preimage*
material at `session.rs:92` is not a counter-argument: a keyed MAC does not rest on preimage secrecy,
and `participant_id` and `nonce` are inputs on that same line.) **Final redaction set** —
`JoinResponse`: `meeting_kek` + `binding_token` redacted, `correlation_id` and `participant_id` in the
clear; `JoinRequest`: `join_token` + `binding_token`; `MeetingKekUpdate`: `meeting_kek`. **The redaction is tested, not intended (G1-4):** a `proto-gen` unit test
formats a `JoinResponse` with a non-empty `meeting_kek` and a `JoinRequest` with a non-empty
`join_token` and asserts the secret bytes do not appear in `{:?}` output — a `skip_debug` entry
otherwise stops working silently if the fq path drifts or the message is renamed. This keeps
`join_tests.rs:307,328,960`'s `{other:?}` compiling (which plain `skip_debug` alone would break) and
makes the guard-vocabulary gap moot (`\bkek\b` cannot match `meeting_kek`; `_` is a word character).
Extending `scripts/guards/semantic/checks.md` vocabulary is **not** taken here — it is MC-side
behaviour surface and belongs with task 10, which is where key material first becomes non-empty.
**Durably handed off (G1-5), not just noted in this main.md** (which nobody re-reads when task 10
starts): a `docs/TODO.md` entry naming both the `checks.md` vocabulary extension **and** the
TypeScript-side sink control — the Rust redacting `Debug` is Rust-only, connect-es types are plain
objects and `JSON.stringify(serverMessage)` would print the KEK — with story task 10 as owner. The
existing whitelist projections (`JoinedEvent`, `e2eBus.ts`) keep it non-live today and this plan
keeps `JoinedEvent` KEK-free.

**§11 cardinality prohibitions in comments.** `sender_id` (a per-participant identity — note the
rename *removes* the incidental `user_id` CATEGORY_B vocabulary coverage, so the comment carries
that weight now), `switch_command_id` (unbounded cardinality) and `identity_public_key` (public, but
a stable per-participant identifier) each carry: never a metric label, never a span attribute,
never a per-frame log dimension. `sender_id` additionally states it is per-meeting, MC-allocated,
**not** derived from and **not** a stable user identity (cross-meeting linkability is exactly what
§4's meeting-scoped keys exist to prevent), and records the nonce-uniqueness invariant: a key id is
never reused under one KEK; generation is monotonic per sender and never reset; `sender_id` is never
recycled while a KEK is live — which is what reduces AES-GCM nonce uniqueness to key-id uniqueness.
**`sender_id` has no zero sentinel (OPS-8).** `uint32` in proto3 has no field presence, so a
documented "0 = not yet assigned" sentinel is byte-identical to *MC forgot to set it* — and it would
disarm the very compile error this rename creates at
`crates/mc-service/src/webtransport/connection.rs:995`, letting a future implementer re-land
ADR-0036's named defect (`JoinResponse.user_id` hardcoded 0) while looking correct. Worse, a shared
sentinel is an attribution **collision** on one roster entry, not a missing identity. So both halves
of the fix are taken: `sender_id` is `optional uint32` **and** its valid range is **1..=65535 with 0
never valid** — absent means *MC has not assigned one* (the state this story ships, expressed as an
explicit `None` at the MC call site rather than a plausible-looking `0`), present means a real
allocation, and 0 stays loud everywhere downstream including the KID. Same shape on the roster
`Participant` and on `StreamAssignment`.
**The crypto reason, not just the attribution one (G1-7).** A sentinel shared by N participants is
not a *recycled* sender_id, it is N *concurrently live colliding* ones — and since the key id encodes
(sender, stream, generation), the wrap nonce is derived from the key id, and the key id is the
associated data, two senders at sentinel 0 with the same stream and generation wrap distinct
transmit keys under the same KEK at the same derived nonce. §4 names that case: for AES-GCM it is
**authentication-key recovery**, not merely confidentiality loss. It also breaks receivers a second
way — §4 step 4 has a receiver ignore a wrapped key for a key id it already holds, so the second
sender's key-bearing frames are discarded and its media is decrypted under the first sender's key.
None of it is live this story (no allocator, no media, empty KEK); the defect would be that the
*contract* legalises it, and the contract is what tasks 10 and 14 build against. Receiver-side rule
stated at the field: a consumer MUST NOT use `sender_id = 0` for key-id construction or roster
lookup, and MUST reject rather than treat it as an anonymous participant.
`ReceiveSlot.pinned_sender_id` (already `optional`) says 0 is not a legal pin target — and it is the
one place MC *receives* an attacker-supplied `sender_id`, so `Some(0)` is rejected like any other
out-of-range pin. **Absent must not decay back into the sentinel:** the definition states that absent
means *not yet assigned*, a consumer MUST NOT substitute 0 (no `unwrap_or(0)`), MUST NOT attribute
frames to an absent id, and MUST fail closed — the same shape as the `identity_public_key`
empty-means-no-key-published rule.
**`kek_generation` is deliberately NOT given a sentinel**: 0 is a plausible legal first generation,
so the not-provisioned signal is the one that already exists — `meeting_kek` not being exactly 32
bytes. The comment says so explicitly, so nobody later gates on `kek_generation != 0` and gives one
field two meanings.

**`sender_id` range bound — legible in-file, owner named (OPS-7).** The wire type permits
0..2^32-1 while the contract permits **1..=65535**, and an out-of-range value does not fail: it
silently truncates into the KID's 16-bit sender field and corrupts attribution, surfacing as an
Ed25519 verification failure against the *wrong participant's* key. The constraint is therefore
stated once, in full, at `JoinResponse.sender_id` — permitted range, MC is the allocator **and** the
enforcement point, derivation from the 16-bit sender allotment in the frame KID, and fail-closed on
violation (refuse the join / refuse the assignment; never truncate or wrap). The roster and
stream-assignment occurrences reference that one statement rather than restating it, so there is one
place to change if the KID layout moves. **Enforcement owner: story task 10 (MC `sender_id`
allocation)** — named here so it cannot fall between tasks. The enforcement type is **`NonZeroU16`**,
not a bare `u16::try_from(...)`: `u16::try_from` accepts 0, which is the value the contract reserves
as invalid, so the plain form would let an allocator hand out exactly what the comment forbids
(@paired-meeting-controller finding 3). `NonZeroU16` carries both halves of the bound in the type. The proto
cannot enforce it and says so.

**Attribution-chain comment** on `StreamAssignment`: `sender_id` → roster `identity_public_key` →
Ed25519 verify, and attribution comes from *signature verification*, never from the assignment or
roster mapping itself — a compromised MC can mis-map, and this story's chain is TOFU-rooted.

**Amplification bound.** `ReceiveCapability`'s slot list is unbounded on the wire; the comment says
the slot-count cap is server-side configuration and the proto does not enforce it (§6's
"structurally impossible" refers to unsatisfiable *constraints*, not to list length).

**String direction discipline.** Every new string field states its direction. Client→server strings
get the existing `SECURITY: client-controlled, truncate before logging (256-byte bound)` block;
server→client strings (`media_handler_url`, `codec`, the server-mute `reason`) say so explicitly.
`ServerMuteRequest.reason` and `ParticipantMuteUpdate.server_muted_by` gain the truncation/no-log
note they currently lack.

**Trace envelope.** `trace_parent = 20` / `trace_state = 21` come through byte-identical on all
three envelopes — same tags, same types, same comments. Each oneof gains a one-line comment that
variant tags stay below 20 because 20+ is envelope-level. I am **not** adding `reserved 22 to 39`:
`reserved` means "never use", so earmarking numbers we intend to use later inverts its meaning, and
protoc already rejects an actual collision. New variants: `ClientMessage` 12 (`receive_capability`);
`ServerMessage` 12 (`stream_assignments`), 13 (`send_directive`), 14 (`meeting_kek_update`).

**SSoT / DRY answers.**
1. Media kind, slot state and transport mode each exist **once**, in `signaling.v1`; `internal.v1`
   imports them in task 4. No hand-written Rust or TS mirror is added by this task.
2. Header version is a plain numeric MC-assigned field — **not** an enum with one member, not
   "always 2" — because §2 requires a mid-meeting downgrade to be MC's policy call (OPS-6).
   **Cross-language anchor (OPS-9)**: adding `supported_header_versions` makes the version a value
   the TypeScript client must also know, so a Rust-only anchor would leave TS hardcoding its own 2
   with nothing to derive from and no drift guard. Both `header_version` and
   `supported_header_versions` therefore carry `ANCHOR (DRY):` naming
   `proto/test-vectors/frame-v2.vectors.json` (story task 8) as the cross-language single source of
   truth — the same forward-reference pattern `frame.rs:185 MAX_PAYLOAD_BYTES` already uses — and
   additionally name `crates/media-protocol/src/frame.rs::PROTOCOL_VERSION` as the Rust-side
   derivation for MC task 14. **Precedence between the two is stated, not left to the reader**
   (@dry-reviewer): the vectors file pins the cross-language value and `PROTOCOL_VERSION` is the Rust
   home; on disagreement the recorded rule at `docs/TODO.md` §Media Path Obligations applies —
   vectors beat prose, **but a vector contradicting a recorded ruling is a defect to escalate rather
   than conform to**, because vectors are generated from the Rust encoder and then frozen. No new
   rule invented; the anchor cites the existing one. The **reciprocal** back-pointer on
   `frame.rs::PROTOCOL_VERSION` is **not** written here — see the GSA note under
   §Cross-Boundary Classification — and is recorded instead.
2b. **`media-protocol` obligations handed off, in one `docs/TODO.md`
   §Media Path Obligations entry with four bullets, each its own checkbox** (that section exists for exactly this: constraints the codec
   cannot enforce, recorded so they survive a prompt re-scope, and it already carries a task-8-shaped
   entry at `docs/TODO.md:539`). The entry names three things: (a) **story task 8 owes
   `frame-v2.vectors.json` a header-version key with drift-guard coverage** — without it this task's
   proto anchor points at a key that never materialises, and @operations is right that a dangling
   anchor is worse than the Rust-only anchor they objected to, because it reads as guarded and is
   not; (b) `frame.rs::PROTOCOL_VERSION` gains the reciprocal `ANCHOR (DRY)` line noting its new
   cross-language status; (c) `frame.rs:384`'s `impl Debug for WrappedTransmitKey` gains the
   compile-time-constant-lengths invariant in a doc comment — verified absent there today, so it is a
   real gap and the same reasoning at the site that established the pattern. Owner: protocol +
   media-handler; all land in task 8, a `media-protocol` task where the MH co-sign is required anyway
   and therefore costs nothing extra. Plus **(d)**, folded in from @dry-reviewer's own entry at
   @operations' argument: promote `KEY_ID_BYTES`'s prose KID layout
   (`sender_id(16)|stream(8)|generation(40)`) to named constants, with the TypeScript re-spelling
   risk and task 8's vectors as the answer, and the plain statement that **no guard covers the
   proto-side anchors today**. A cross-reference between two entries would itself be a second place
   encoding one relationship, and (d) is structurally identical to (a)–(c) — a `media-protocol` edit,
   blocked on MH co-sign, targeting task 8 — so the task-8 implementer finds one entry, not two plus
   a pointer. **Per-bullet checkboxes, not one for the entry**: four bullets make partial completion
   possible for the first time in that section, and a single checkbox invites whoever closes it after
   the vectors key lands to delete the entry and silently take the unfinished bullets with it.
   Nothing in `todo_tracking.rs` constrains entry structure inside `docs/TODO.md`, so the checkboxes
   are free. @dry-reviewer's §Cross-Service Duplication entry keeps only the disjoint MC-side
   material and does not point here. The `docs/TODO.md` edit itself needs no MH co-sign.
3. KEK generation: `uint32` with u16 semantics on **both** the join response and the KEK push
   (divergence would silently break story-2 rotation); comment anchors
   `KEK_GENERATION_FIELD_BYTES = 2`. No `MAX_KEK_GENERATION = 65535` constant is introduced — the
   range check is `u16::try_from(...)`, derived from the type.
4. `sender_id`'s 16-bit width: same treatment — expressed as `u16::try_from(...)` at the MC check
   site (task 10), comment anchoring `frame.rs::KEY_ID_BYTES`'s documented
   `sender_id(16)|stream(8)|generation(40)` layout. I am **not** promoting that prose to named
   constants in `crates/media-protocol/` this devloop: it is a Guarded Shared Area requiring
   protocol + **media-handler** co-sign (ADR-0024 §6.4) and @media-handler is not on this team;
   no Rust code in *this* task consumes such a constant. Named for the task where MH is present.
5. Recorded false-positive boundaries at the definitions, two of them. (a) `sender_id`'s 65535 and
   `kek_generation`'s 65535 are the same magnitude and different concepts — sharing one constant
   would falsely couple sender allocation to key rotation, exactly like the `AEAD_TAG_BYTES` vs AC
   storage-envelope boundary at `frame.rs:104`. (b) `sender_id`'s 65535 (an **allocation** bound:
   KID-derived, 1..=65535, never recycled while a KEK is live) and `slot_id`'s 65535 (a
   **validation** bound: relay-region-derived, per-subscriber, freely reusable across declarations)
   are likewise distinct. The tempting refactor to pre-empt is named explicitly at both sites — a
   shared `MAX_16BIT_ID` constant or a shared `validate_16bit_id()` helper — because collapsing them
   silently loses `sender_id`'s non-recycling invariant, which is the more dangerous half.
6. Handler address has **one** spelling in this file: the field name `media_handler_url`, reused by
   `SendTarget` and `StreamAssignment` (wrapping the one-field `MediaServerInfo` message would add
   indirection without removing a spelling). `internal.v1`'s `webtransport_endpoint` is task 4's to
   align — flagged, not touched.
7. No residual field means "source participant" alongside `sender_id` after the reshape.

### P3 — Call sites (fixing them is in scope; behaviour is not)

Rust (small — MC's post-join dispatch has a `Some(_)` catch-all, so oneof churn does not break it):
- `crates/mc-service/src/webtransport/connection.rs` — `build_join_response`: `user_id: 0` →
  `sender_id: None` (explicit, per OPS-8 — no zero sentinel), drop `encryption_keys: None`, add
  `meeting_kek: Vec::new()` / `kek_generation: 0`. Values stay unpopulated this story (task 10 lands allocation); the fields'
  comments carry the fail-closed rule.
- `crates/mc-service/src/webtransport/handler.rs`, `src/actors/participant.rs`,
  `src/actors/messages.rs` — `Participant` construction gains `sender_id: None` (**not** `0` — the
  roster is the worst surface for the eliminated sentinel, since it is where N unassigned
  participants would collide on one entry and where task 14's frame → `sender_id` → identity-key
  lookup resolves) / `identity_public_key: Vec::new()`, and the `*_host_muted` / `host_muted_by` rename.

TypeScript (the `uint64`→`uint32` ripple; `bigint` → `number`):
`events.ts` (`JoinedEvent.userId: bigint` → `senderId: number`, plus the stale R-17 bigint comments
and the "out of scope until layout/subscription lands" comment), `SignalingClient.ts:434`,
`__tests__/helpers.ts`, `__tests__/signaling-client.test.ts` (including `123456789012345n`, which is
above 2^32 and unrepresentable in the new field), `errorCodeMap.ts` + `__tests__/maps.test.ts`
(`UNKNOWN` → `UNSPECIFIED`), `sdk-svelte` `MeetingStore.test.ts` / `subscribeSession.test.ts`,
`web-app` `e2eBus.ts` (+ its stale bigint comment), `__tests__/e2eBus.test.ts`,
`__tests__/helpers/MockMeetingSession.ts`, `e2e/join-happy-path.spec.ts` (comment only).
No `BigInt(...)` re-wrap to preserve the old type, no `any`, no `@ts-expect-error`, no deleted or
`#[ignore]`d test — every listed test is **updated**, not removed. `JoinedEvent` does **not** gain
the KEK (it is a documented consumer object whose natural use is `console.log`), and
`SignalingClient`'s "case name only, never the payload" debug discipline is preserved. No new
consumer acquires the `meeting_id_hash` implicit label set.

### P4 — Regeneration and new test coverage

Neither binding is committed, so "byte-identical to a fresh regeneration" reduces to *regeneration
is actually re-run and the tree compiles from it*:
- Rust: `crates/proto-gen/build.rs` generates into `OUT_DIR` with `cargo:rerun-if-changed` on both
  `.proto` files — nothing in-tree, verified by `cargo check --workspace`.
- TypeScript: `packages/sdk-core/src/proto/**/*_pb.ts` is **gitignored** (`.gitignore:13`).
  Command: `pnpm exec nx run proto-gen:codegen` (`buf generate` with cwd `proto/`), and
  `pnpm exec nx run proto-gen:test` → `packages/proto-gen/scripts/verify-codegen.sh`, which
  cleans-then-generates so stale output cannot mask a broken config. No hand-edits to generated files.

**Codegen oracle hardening (@test item 6).** `packages/proto-gen/scripts/verify-codegen.sh` is
positive-presence-grep only and asserts just `JoinRequest` for signaling, so today it would stay
green through all five layout deletions, the `EncryptionKeys` removal and the `HostMuteRequest`
rename. Both halves are added: presence asserts for the new symbols (`ReceiveCapability`,
`SlotState`, `SendDirective`, `StreamAssignment`, `MeetingKekUpdate`, `ServerMuteRequest`) and a new
helper asserting `HostMuteRequest`, `EncryptionKeys`, `SubscribeToLayout`,
`UpdateLayout`, `UnsubscribeLayout`, `LayoutConfig`, `LayoutType`, `StreamType`, `MediaType`,
`StreamMetadata`, `VideoMetadata` and `SimulcastLayer` are **gone**. Precision (@test item A): each
absence match is anchored on the *export declaration* (`export type X ` / `XSchema`), never a bare
token, so a surviving doc-comment mention cannot produce a false pass; and `StreamAssignment` is
deliberately **only** in the presence set — the name is reused for the new shape, so an absence
assert on it would collide. A half-done deletion or a rename-back
must fail this script — that is the never-mask guard at the codegen layer.

Fate of the enum consumers (@test item B), all in `signaling.proto` (one file, one table row —
the classification table is per-file per ADR-0024 §6.2, not per-message): `MediaStream` and
`PublishStream` are **repointed** to `MediaKind` + `EncodingParameters` (old tags reserved per
Rule R); `StreamPublished` is unchanged (it references `MediaStream`, whose identity is preserved);
`StreamMetadata`, `VideoMetadata` and `SimulcastLayer` are **deleted** and are therefore in the
`assert_not_generated` set above.

New coverage (@test's item 2): `crates/proto-gen/tests/signaling_roundtrip.rs` — prost
encode→decode round-trip for every new/changed message (receive capability incl. pin absent vs
present; send directive with encoding params + target carrying handler address and transport mode;
stream assignment with `sender_id`/media kind/handler address/slot state; switch-pending carrying
the command identifier; `JoinRequest` identity key; `JoinResponse` KEK + generation + `sender_id`
round-tripping 65535; KEK push; server mute), plus assertions that `SlotState` has seven distinct
non-zero discriminants and that the redacting `Debug` impls do not print key bytes. TS-side shape
test in `packages/sdk-core/src/signaling/__tests__/` for the regenerated types.

### P5 — Gate 1 review resolutions (additions to the above)

**Codec vocabulary — one home (@dry-reviewer F3, option (a)).** A `Codec` enum lands in
`signaling.v1` (`CODEC_UNSPECIFIED = 0`, `CODEC_OPUS`, `CODEC_VP9`, `CODEC_AV1`, `CODEC_H264`) and is
used by `EncodingParameters.codec` and by a **single collapsed**
`ParticipantCapabilities.supported_codecs` (`repeated Codec`) that replaces the `video_codecs` /
`audio_codecs` string pair (old tags and names reserved per Rule R). Taking @dry-reviewer's option
(a): with opaque strings the audio/video split was load-bearing because nothing but the field name
said which was which, but with an enum the classification would be encoded **twice** — once by which
list a value sits in, once by the value's own identity — and `audio_codecs: [CODEC_VP9]` would be
well-typed and meaningless. Collapsing puts the classification in one home (the codec's own
identity); MC derives kind from the value when filtering for an audio directive (task 14), which is
a derivation from the single source rather than a second home. **The `Codec` retype is NOT
zero-blast-radius on the TS side** — @client found three live handwritten consumers I had missed:
the SDK's *public* `SignalingCapabilities.videoCodecs`/`audioCodecs` (`events.ts:88-89`), the
`ParticipantCapabilitiesSchema` construction that spreads them (`SignalingClient.ts:290-292`), and
`signaling-client.test.ts:114-129`. Decision — **the `errorCodeMap.ts` shape, ruled by @client as the
convention's owner after I surfaced it as a third option neither of us had costed.** The public
surface does **not** take the generated `Codec` enum: `events.ts` gains a `SignalingCodec`
const-object union mirroring `ParticipantLeaveReason`, `SignalingCapabilities.supportedCodecs?:
readonly SignalingCodec[]`, and a new `codecMap.ts` mirroring `errorCodeMap.ts` holds **one
wire-keyed oracle**, `Record<Codec, SignalingCodec | null>` (@client's own correction after
@dry-reviewer caught the inversion: a union-keyed map compiles clean when a *new wire codec* appears,
i.e. it fails **open** on the side that changes outside the SDK's control; keying on `Codec` makes
adding a proto member a compile error, which is the whole point of the pattern).
`CODEC_UNSPECIFIED → null` — the type system forces a key for it and `null` means "no public
representation, reject on receipt". The send direction (`SignalingCodec → Codec`) is **derived
programmatically** from that one table by iterating entries and skipping `null`, never hand-written
as a second `Record`, or the two can silently disagree. Task 3 only has a live consumer for the
derived direction (the capabilities send path), but the wire-keyed oracle lands now so task 19
reuses it rather than re-keying. This upholds
the rule recorded verbatim in `events.ts`'s header (generated `*_pb.ts` are gitignored and must not
reach the published `.d.ts`, or a clean checkout breaks), and matches the **two sibling enums on the
same surface** — `SignalingErrorCode` and `ParticipantLeaveReason` — where a raw generated `Codec[]`
would be the inconsistency. It does **not** reopen @dry-reviewer's F3: their objection was a *lossy*
map with two floating vocabularies, and an **exhaustive** `Record` is the compile-enforced one-oracle
pattern `errorCodeMap.ts` already embodies (a new `Codec` member will not compile until mapped).
`CODEC_UNSPECIFIED` maps to nothing on the public union — it is the drop/reject case, as
`errorCodeMap.ts` collapses out-of-range to `Unknown`. Scoped to the **send path only**: task 19
reuses the union and adds the reverse `Record<Codec, SignalingCodec>` when it consumes the send
directive's codec. No "knowing trade" writeup is needed, because this direction removes the
deviation instead of recording one.
(`test-utils`' `capabilities: ['video','audio']` is the JWT capability-grant claim — unrelated,
untouched.) `ParticipantCapabilities.supports_simulcast`
is deleted in the same pass (tag and name reserved) — simulcast is explicitly out of scope in
ADR-0036 and `SimulcastLayer` is already going, so a surviving capability bool for it would be
incoherent. Same declare-then-
select pattern as the header version, and it makes "MC directs a codec the client never declared"
structurally harder rather than merely wrong. The comment states MC MUST NOT direct a codec outside
the participant's declared set (enforcement owner: task 14) and that `CODEC_UNSPECIFIED` is never
valid in a directive. Blast radius is nil: the string lists have zero non-generated consumers
(MC's tests pass `capabilities: None`).

**`MediaServerInfo` kept, with the reason recorded in-file (@dry-reviewer F4).** It stays a wrapper
rather than collapsing `JoinResponse.media_servers` to `repeated string`, because it is the join-time
handler *descriptor* that §9 (handler unreachability) and §10 (transport seam) are expected to grow
fields on, and collapsing it would churn SDK call sites for no contract gain. `SendTarget` earns its
own wrapper (it carries transport mode). The reason is written at `MediaServerInfo` so it is not
re-raised on every read.

**Anchors completed (@dry-reviewer F2).** `ReceiveSlot.slot_id` / `StreamAssignment.slot_id` anchor
`frame.rs::STREAM_ID_FIELD_BYTES` (**not** the KID) and state they are the same value space as the
relay-region `stream_id`. `SendStream.stream_number` anchors `frame.rs::KEY_ID_BYTES`'s
`sender_id(16)|stream(8)|generation(40)` layout, with enforcement `u8::try_from(...)` and owner story
task 14 — the same treatment `sender_id` gets, so one of the pair cannot fall between tasks. Each of
the two carries a sentence saying it is **not** the other, since after the reshape the file has two
different numeric stream-ish identifiers in different value spaces.

**Sentinel rule stated once (@dry-reviewer F5).** The "0 is never a valid `sender_id`" rule is
written in full at `JoinResponse.sender_id` alongside the range, and the roster,
`StreamAssignment.sender_id` and `ReceiveSlot.pinned_sender_id` reference it rather than restating
it. Receiver-side rule kept even under `optional` (@security amendment): explicit presence says the
field *was set*, not that it was set sensibly — a peer can send `Some(0)`, and the wrap-nonce
collision follows from the value, not the presence bit.

**§11 prohibitions extended (@observability A + B).** `slot_id` and `stream_number` get the same
never-a-metric-label / never-a-span-attribute / never-a-per-frame-log-dimension line as `sender_id`
and `switch_command_id` — §11 bars per-*stream-identity* dimensions and a slot is defined as a
stream identity, and "slot 3 is stalling" is exactly the well-meant label that would land there.
`kek_generation` gets it too, **with the reason spelled out** because it looks label-safe: it is
monotonic over the meeting's life, so its label cardinality is unbounded over *time* rather than
bounded by its type, and it advances on the §4 leave debounce, which makes a per-meeting generation
series a membership-change trace — de-anonymising by inspection in the two-person case. Logging it
*inside* an already-redacted `Debug` is fine and useful; that is the distinction
`WrappedTransmitKey` already draws.

**Redacting-`Debug` shape (@observability).** Match `crates/media-protocol/src/frame.rs:384`
(`impl Debug for WrappedTransmitKey`, landed by story task 1) rather than inventing a second shape:
metadata (generation, lengths) in the clear, secret bytes as `<redacted {N} B>` via `format_args!`.
So the frame codec and the signalling layer read identically in a log line and the next author adding
a key-bearing message has one pattern to copy. Redaction set per the resolution above.

**The safety invariant goes in the doc comment, not in a review thread (@security, upgraded from
note to finding at @observability's argument).** Each redacting `impl Debug` carries a doc comment
stating *why* printing metadata in the clear is safe here: **the disclosed lengths are compile-time
constants and therefore not a function of the secret** (KEK always 32, tag always `AEAD_TAG_BYTES`).
A future field whose length varies with its content does not satisfy that and must not be printed.
Written at the site because the next author adding a field there will not have read this thread — a
safety property that lives only in reviewer messages is not a control.

**TS/E2E stringify sweep (@observability C).** During implementation, grep the test and E2E surface
for anything that stringifies or dumps a decoded `ServerMessage`/`JoinResponse` —
`packages/sdk-core/src/signaling/__tests__/helpers.ts`, `packages/test-utils/src/MockWebTransport.ts`,
`packages/web-app/e2e/join-happy-path.spec.ts` — because protobuf-es has no `skip_debug` equivalent
and Playwright trace artifacts are retained. Result reported at Gate 2 either way, so it is checked
rather than re-derived.

**MC-internal host-mute vocabulary renamed too (@paired-meeting-controller ruling as owner).**
`MeetingMessage::HostMute`, `host_mute()`, `handle_host_mute()`, the `ParticipantInfo` /
`ParticipantStateUpdate::MuteChanged` / meeting-actor participant-record fields, and the affected
test names — roughly 25 compiler-checked sites, no behaviour change. Leaving MC's domain model
saying *host* while the wire says *server* is how the term §5 rejects survives, and MC is the only
consumer of both sides. `messages.rs` and `meeting.rs` are Minor-judgment rows with an
`Approved-Cross-Boundary: meeting-controller` trailer.

**Producer-side placeholder comment (@paired-meeting-controller item 6).** `build_join_response`'s
`sender_id: None` / `meeting_kek: Vec::new()` / `kek_generation: 0` carry a one-line comment naming
story task 10, so the not-yet-provisioned state reads as a declared stage rather than a finished one.
`supported_header_versions`' comment likewise states MC performs **no** version selection this story
(task 14 sets the directive version unconditionally from `PROTOCOL_VERSION`), so the field's
existence is not read as negotiation already shipped. Both `supported_header_versions` and
`ReceiveCapability.slots` are unbounded `repeated` on a client→server path and carry the same
"proto does not enforce; server-side configuration caps it" sentence.

**Guard-precision finding — RULING: the tracking entry lands in this diff; the fix does not.**
This finding was mis-stated twice before it was measured, so only the measured version is recorded.
My first framing ("the effective constraint is one line per pointer") was wrong; @dry-reviewer's
first correction ("part of the corpus trips on wrapped bullet continuations") was also wrong and
they withdrew it after measuring — **zero** bullets in any deferrals section in the corpus wrap.
The measured root cause is a single defect at `crates/dt-guard/src/todo_tracking.rs:92-101`:
**only `-`, `*` and `+` are recognized as bullets**, so ordered lists (`1.`) and `---` thematic
breaks count as debt-body lines. One defect, both failure directions:
- **False positive, live today** — two *adjacent* numbered pointers fire the guard on a perfectly
  disciplined section. Existing files escape only because they blank-separate every item, which
  resets the run: luck of formatting, not correctness. I earlier wrote that this direction was
  "plausibly the guard doing its job" and that is **wrong** under the measured facts — firing on
  well-formed numbered pointers is markdown-incompleteness, not enforcement. The entry carries both
  directions from the one root cause.
- **False negative, sized** — **34 of 68** files carry genuine prose bodies, **59 lines**, passing
  clean today (@operations' direction: the guard is trivially satisfiable while violating its own
  stated intent, because a blank line resets the run).
Of the 61-of-68 that would trip under a naive stricter predicate, **27 are false positives and 34
are real**. Not fixed here: a predicate change plus a 34-file corpus sweep plus its own test cycle,
in shared gate tooling, inside a proto reshape — and the decisive argument is contamination, since a
guard-tooling change landing in the commit that `## Expected Layer-State` is diffed against would
make that diff unreadable. **@dry-reviewer writes the entry at review time under @operations' name**,
with all three numbers and the line reference so it is reproducible rather than taken on trust; I do
not write it, so there is no double-authorship. Owner: operations (finding), dt-guard tooling (fix).
Note for this changeset: `## Accepted Deferrals` is five single-line `-` bullets and is clean under
both the current guard and any plausible fix — it must stay `-` bullets and must not be renumbered
into `1.` form.

**`docs/specialist-knowledge/dry-reviewer/INDEX.md:70`** is updated with @dry-reviewer's supplied
wording, including the verbatim "**The proto-side anchors have no enforcing guard**" clause.
@dry-reviewer appends the `docs/TODO.md` §Cross-Service Duplication entry themselves at review time
(promoting the KID-layout prose to named constants under MH co-sign; the TS re-spelling risk with
task 8 as the answer; the absent guard) — I do not write it, to avoid two entries.


### Declared scope expansions (beyond the task prompt's literal list)

Recorded here rather than discovered at Gate 2. Each is argued in P1/P5 above:
1. **All five `// buf:lint:ignore` carve-outs drained**, not the three that die with the deletions —
   `LeaveReason` and `ErrorCode` too, with their inline `TODO(post-story)` comments removed. Posture:
   **in scope as a deliberate "take the wire breaks while they are free" cleanup**, on the same
   Clarification-Q9 / CONVENTIONS §"Why STANDARD" basis as the rest of this reshape. The two are
   treated identically — draining one and not its twin would be the inconsistency. (There is no
   `docs/TODO.md` entry tracking these; the tracking lived in the inline comments, which go.)
2. `StreamMetadata` / `VideoMetadata` / `SimulcastLayer` deleted, `MediaStream` retyped onto
   `EncodingParameters` — one encoding vocabulary in the file. Zero non-generated consumers.
3. `Codec` enum replacing the free-form codec strings on `EncodingParameters`, with
   `ParticipantCapabilities.video_codecs`/`audio_codecs` collapsed into one `supported_codecs` list.
   `supports_simulcast` **and `max_video_streams`** reserved: simulcast is out of scope in ADR-0036,
   and a scalar max-video-stream count is superseded by `ReceiveCapability`'s explicit slot list
   (@client's catch) — a scalar maximum and a per-slot list are two representations of one
   constraint, the list is strictly more expressive (media kind and optional pin per slot, and its
   length *is* the count), and keeping both lets them disagree. `ParticipantCapabilities` therefore
   reserves 1 to 4 with all four names. On the TS side the public surface takes the
   `errorCodeMap.ts` shape, not the generated enum.
4. `ParticipantCapabilities.supported_header_versions` added, so §2's "MC selects from declared
   capabilities" has something to select from.
5. The host-mute rename extended to its sibling class on the wire **and** to MC's internal
   vocabulary (owner-ruled by @paired-meeting-controller).
6. `packages/proto-gen/scripts/verify-codegen.sh` hardened with absence asserts.
7. A third `## Guard Precision —` entry in `docs/TODO.md`, recording that `todo_tracking.rs` is
   trivially satisfiable while violating its own intent (below). Genuinely unrelated to the
   signalling contract — disclosed as scope, accepted because `docs/TODO.md` is already in the diff
   for two other reasons, the section precedent exists, and the finding is perishable.

### Out of scope (scope, not deferrals)

MC `sender_id` allocation and the ≤65535 range check; MC roster publication of
`identity_public_key` and its 32-byte check; KEK generation/rotation and the KEK-source seam;
slot-state and switch-pending behaviour; the credential-leak semantic-guard vocabulary extension;
SDK audio pipeline consumption. These are story tasks 4/10/13/14/19.

---

## Expected Layer-State

**Layer 6 `buf breaking` is expected to FAIL, deliberately, and must not be suppressed.**
Declared so the actual Layer-6 log can be compared against a predicted shape — that comparison is
what distinguishes an expected fire from a regression, and it is the section's only job. Layers
1–5 and Layer 6's `cargo-audit` are expected **green**.

Mechanics: `buf` exits **100** on findings, but `scripts/lang/_common.sh:137` `run_and_emit`
normalizes any non-zero to `STATUS=FAIL` + `return 1`, so **Layer 6 exits 1**, which routes to the
implementer lane at `run-story.sh:1589`. A Lead's Gate-2 acceptance cannot change `gate_rc`; see the
Lead Ruling above.

**Predicted total: exactly 54 findings, all in `signaling.proto`, in five rule classes.**
`proto/buf.yaml` → `breaking.use: [FILE]`. Measured against
`buf breaking --against '../.git#ref=HEAD,subdir=proto'` from `proto/`.

| # | Class | Rules | Count |
|---|-------|-------|-------|
| 1 | Symbol deletion | `MESSAGE_NO_DELETE`, `ENUM_NO_DELETE` | 12 |
| 2 | Field deletion | `FIELD_NO_DELETE` | 19 |
| 3 | Field rename | `FIELD_SAME_NAME` + `FIELD_SAME_JSON_NAME` (**two per rename**) | 8 |
| 4 | Field type change | `FIELD_SAME_TYPE` | 1 |
| 5 | Enum value | `ENUM_VALUE_NO_DELETE`, `ENUM_VALUE_SAME_NAME` | 14 |
| | | **Total** | **54** |

**Class 1 — symbol deletion (12).** Enums `LayoutType`, `MediaType`, `StreamType`; messages
`EncryptionKeys`, `HostMuteRequest`, `LayoutConfig`, `SimulcastLayer`, `StreamMetadata`,
`SubscribeToLayout`, `UnsubscribeLayout`, `UpdateLayout`, `VideoMetadata`.
`StreamAssignment` and `StreamAssignments` are **not** here: both names are reused by the ADR-0036
shape, so the messages are not deleted — the layout-form *fields* inside `StreamAssignment` are, and
they land in class 2.

**Class 2 — field deletion (19).** Every one is Rule R doing its job: a repurpose that would have
been a rename or a retype in place becomes a deletion plus a fresh tag.
- `ParticipantCapabilities` tags 1 `video_codecs`, 2 `audio_codecs`, 3 `supports_simulcast`,
  **4 `max_video_streams`** — 4 findings.
- `MediaStream` tags 2 `stream_type`, 3 `metadata` — 2.
- `JoinResponse` tags 2 `user_id`, 5 `encryption_keys` — 2.
- `PublishStream` tags 2 `stream_type`, 3 `metadata` — 2.
- **`StreamAssignment` tags 1 `stream_id`, 2 `user_id`, 3 `media_type`, 4 `slot_index`,
  5 `media_handler_url` — 5.**
- `ClientMessage` oneof tags 3 `subscribe_to_layout`, 4 `update_layout`, 5 `unsubscribe_layout` — 3.
- `ServerMessage` oneof tag 5 `stream_assignments` — 1.

**Class 3 — field rename (8 = 4 renames × 2).** `ParticipantMuteUpdate` tag 4
`audio_host_muted` → `audio_server_muted`, tag 5 `video_host_muted` → `video_server_muted`, tag 6
`host_muted_by` → `server_muted_by`; `ClientMessage` tag 8 `host_mute_request` →
`server_mute_request`. Each contributes one name finding and one `json_name` finding.

**Class 4 — field type change (1).** `ClientMessage` tag 8's message type,
`HostMuteRequest` → `ServerMuteRequest`. The tag is deliberately *kept* here — the message is the
same message under a corrected name, so there is no old-peer misdecode to protect against and a
fresh tag would buy nothing.

**Class 5 — enum value (14).**
- `LeaveReason`: 4 × `ENUM_VALUE_NO_DELETE` (values 1–4 vacated by the drain, `reserved 1 to 4;`)
  plus 1 × `ENUM_VALUE_SAME_NAME` on value 0 (`VOLUNTARY` → `LEAVE_REASON_UNSPECIFIED`) = 5.
- `ErrorCode`: **9 × `ENUM_VALUE_SAME_NAME`** — value 0 `UNKNOWN` → `ERROR_CODE_UNSPECIFIED` and all
  eight other values gaining the `ERROR_CODE_` prefix. No `ENUM_VALUE_NO_DELETE`: every number is
  kept.

### Corrections (applied at Gate 2, after measurement)

Marked rather than quietly rewritten, because a prediction that gets edited to match the log without
saying so stops being evidence of anything.

1. **`ErrorCode` was predicted to contribute *no* class-5 finding. Wrong — it contributes 9.** The
   original reasoning ("`UNKNOWN = 0` → `ERROR_CODE_UNSPECIFIED = 0` is the same number with the same
   meaning, and every other value keeps its number") was sound about value *numbers* and therefore
   correct about `ENUM_VALUE_NO_DELETE` — but the class as named includes `ENUM_VALUE_SAME_NAME`, and
   draining `ErrorCode`'s `// buf:lint:ignore` carve-out renames **all nine** values. Harmless on the
   wire, since no number moved; fatal to the prediction, which has to be exact or it cannot do its
   only job. Class 5 goes from a predicted 5 to a measured 14.
2. **`StreamAssignment`'s five class-2 findings were unpredicted**, because the section declared the
   layout-form `StreamAssignment`/`StreamAssignments` *deleted* (class 1) when in fact the names are
   reused. As first implemented the message also violated Rule R — tags 1–5 re-pointed in place with
   no `reserved` statement, producing 15 rename/retype findings instead. Both are fixed: the message
   now reserves 1–5 and moves its fields to tags 6–11, and this section predicts the resulting 5
   `FIELD_NO_DELETE` findings and the *absence* of `StreamAssignment` from class 1. See the
   `StreamAssignment` paragraph under P1.
3. **`ParticipantCapabilities` tag 4 `max_video_streams` was missing from the class-2 enumeration.**
   It was declared in §"Declared scope expansions" item 3 but never folded into the prediction, so
   the log carried a fired-but-unpredicted finding. Folded in above; `ParticipantCapabilities`
   contributes 4, not 3.

The net of corrections 1–3 against the pre-fix log: class 5 was under-predicted by 9, class 2 by 6
(1 for `max_video_streams` + 5 for `StreamAssignment`), and 15 `StreamAssignment` rename/retype
findings that should never have existed are gone from classes 3 and 4.

### Rule R, and its two exceptions

**Rule R** — now written down as `docs/protocol/CONVENTIONS.md` §Rule 5 rather than living only in
this devloop output, since it is a standing protocol convention and the next devloop to break the
wire should not have to re-derive it: when a tag's meaning changes, the old tag **and** the old
field name are `reserved` and the new field takes a fresh tag. Never a repurpose in place. Applied uniformly to `JoinResponse`,
`MediaStream`, `PublishStream`, `ParticipantCapabilities`, `StreamAssignment`, `ClientMessage`,
`ServerMessage`, and — extended to enum values — `LeaveReason`. Its cost is exactly the class-2 and
class-5-deletion counts above; that is the shape of a wire break taken deliberately.

**Exception A — the name half, twice.** A name reused by the new shape on a fresh tag cannot also be
reserved (protoc rejects it). `ServerMessage` reserves number 5 without the name
`stream_assignments`; `StreamAssignment` reserves 1–5 without the name `media_handler_url`. Both are
annotated in-file so they read as decisions.

**Exception B — enum value 0.** proto3 forces value 0 to exist, so it cannot be reserved.
`LeaveReason`'s `VOLUNTARY = 0` is necessarily re-pointed to `LEAVE_REASON_UNSPECIFIED = 0`: an old
peer sending `VOLUNTARY` reads as `UNSPECIFIED`. Structurally unavoidable, and it degrades in the
**safe** direction — a real value collapsing to the fail-closed reading, never to a *different* real
value. `ErrorCode`'s `UNKNOWN = 0` → `ERROR_CODE_UNSPECIFIED = 0` is the same number with the same
meaning, so it is not even a semantic change. Recorded as the exception rather than left inside an
absolute claim a careful reader could falsify.

Everything else in this reshape is kept-identical, reserved, or fails loud on the wire.

### Basis

- `docs/protocol/CONVENTIONS.md` §"Why STANDARD" — no on-the-wire clients outside this codebase; the
  in-file `ClientMessage` tag-11 one-time reuse precedent; ADR-0036's Appendix is the governing
  decision; `docs/TODO.md:820` is the recorded override convention.
- **No suppression of any kind was added** — no `buf.yaml` carve-out, no `# buf:breaking:ignore`, no
  `--exclude-path`, no env bypass. The red is the artifact.
- The declaration is repeated in the commit message. @team-lead accepts at Gate 2; the story
  runner's mechanical `pipeline-red-layer6` escalation needs Lead absorption (see P0).

---

## Pre-Work

None.

---

## Implementation Summary

Landed ADR-0036's Appendix "Signalling contract" in
`proto/dark_tower/signaling/v1/signaling.proto`, regenerated both bindings, and fixed every call
site. Behaviour is unchanged: no `sender_id` is allocated, no KEK is provisioned, no slot is
assigned. This task ships the **contract** those story tasks (4, 10, 13, 14, 19) build against.

**1. Media vocabulary, defined once.** New `MediaKind`, `Codec`, `TransportMode` and the seven-state
`SlotState` (plus `*_UNSPECIFIED`) replace `StreamType`, `MediaType` and the free-form codec strings.
Each concept has exactly one home in `signaling.v1`; `internal.v1` imports them in task 4. Every
zero value is fail-closed by comment — `CODEC_UNSPECIFIED` is never valid in a directive,
`TRANSPORT_MODE_UNSPECIFIED` is not a convenient datagram default.

**2. Receive / send / assignment.** `ReceiveSlot` + `ReceiveCapability` (client→MC, client-chosen
`slot_id`, per-slot `optional pinned_sender_id` so "seven pins into six slots" is unrepresentable);
`EncodingParameters` + `SendTarget` + `SendStream` + `SendDirective` (MC→client, transport mode, no
§7 priority group); `StreamAssignment` + `StreamAssignments` (MC→client, one positional concept,
explicit `SlotState`, echo-only `switch_command_id`). `slot_id`'s per-subscriber scoping, its
`(subscriber, slot_id)` keying obligation on MH, and the reject-never-truncate validation rule are
stated at the definition, with story task 14 named as enforcement owner.

**3. Join, roster, keys.** `JoinRequest` gains `identity_public_key` (raw 32-byte Ed25519, TOFU this
story — never named "attested"/"verified"/"trusted"). `JoinResponse` reserves tags 2/5 and gains
`optional uint32 sender_id` (valid 1..=65535, **0 never valid**, absent means not-yet-assigned),
`meeting_kek` and `kek_generation`. `Participant` gains both `sender_id` and `identity_public_key`,
which is what makes task 14's frame → sender → key → verify chain expressible. `MeetingKekUpdate` is
additive and unused this story so story 2's rotation is behaviour-only.

**4. Terminology.** `HostMuteRequest` → `ServerMuteRequest` and the whole sibling class
(`ParticipantMuteUpdate.*_host_muted`, `host_muted_by`, the "host-muted" prose), plus MC's internal
`HostMute`/`host_mute()`/`handle_host_mute()` vocabulary. ADR-0036 §5 avoids *host mute* because it
presumes an undefined role model; renaming one of four occurrences would have left the term alive.

**5. Deletions.** `SubscribeToLayout`, `UpdateLayout`, `UnsubscribeLayout`, `LayoutConfig`,
`LayoutType`, `EncryptionKeys`, `StreamType`, `MediaType`, `StreamMetadata`, `VideoMetadata`,
`SimulcastLayer`. All five `// buf:lint:ignore` carve-outs drained (`LeaveReason` and `ErrorCode`
too, taking their wire break while it is free); the file now passes buf STANDARD with no annotation
and no config carve-out.

**6. Rule R, applied uniformly.** When a tag's meaning changes, the old tag **and** the old field
name are reserved and the new field takes a fresh tag — never a repurpose in place. Applied to
`JoinResponse`, `MediaStream`, `PublishStream`, `ParticipantCapabilities`, `ClientMessage`,
`ServerMessage`, `StreamAssignment`, and (extended to enum values) `LeaveReason`. Its two exceptions
— a name reused on a fresh tag cannot also be reserved, and proto3 forces value 0 to exist — are
annotated in-file at all three sites rather than left looking like omissions.

**7. A `Debug` leak closed as a control, not a comment.** `crates/proto-gen/build.rs` gains
`.skip_debug(...)` for `JoinRequest`, `JoinResponse`, `MeetingKekUpdate` **and `MhConnectRequest`**
(the last added at Gate 3 — B1 below), with hand-written redacting `impl Debug` in
`crates/proto-gen/src/lib.rs` routed through one `RedactedLen` newtype matching `frame.rs:384`'s
`WrappedTransmitKey` shape (metadata in the clear, secrets as `<redacted {N} B>` / `<empty>`).
Redaction set: `JoinResponse` → `meeting_kek` + `binding_token`, roster rendered as a **count** and
`participant_name` NOT applicable there; `JoinRequest` → `join_token` + `binding_token`, plus
`participant_name` → `[REDACTED]` (PII); `MeetingKekUpdate` → `meeting_kek`; `MhConnectRequest` →
`join_token`. `correlation_id` stays in the clear — it is the ADR-0023 reconnect key an operator
triages with, and the HMAC on `binding_token` is what authenticates a reconnect. The safety
invariant lives in ONE `//` block above `RedactedLen`; each impl carries a one-line `///` pointer to
it (not a triplicated copy — that would be the SSoT violation the invariant itself warns against).
The invariant is stated honestly: `meeting_kek`/`binding_token` lengths are protocol constants, but
`join_token` is a variable-length JWT whose length is disclosed and why that is acceptable, with a
stricter presence-only rule for the next author. Tested, not intended — `proto-gen` unit tests assert
the secret bytes, the PII name, and the roster keys/names do not appear in `{:?}` (including through
the enclosing `ServerMessage`/`MhClientMessage` envelopes), because a `skip_debug` entry stops
working silently when a fq path drifts.

**8. Bindings and consumers.** Rust regenerates into `OUT_DIR`; TypeScript into the gitignored
`packages/sdk-core/src/proto/**`. MC call sites updated (`sender_id: None` — explicitly `None`, not
`0`); the SDK's `uint64`→`uint32` ripple carried `JoinedEvent.userId: bigint` →
`senderId: number` through nine TS files with no `BigInt(...)` re-wrap, no `any`, no
`@ts-expect-error`, and no deleted or skipped test. The public SDK surface takes a `SignalingCodec`
const-object union with a single wire-keyed `Record<Codec, SignalingCodec | null>` oracle in the new
`codecMap.ts` (the `errorCodeMap.ts` pattern), never the generated enum — generated `*_pb.ts` are
gitignored and must not reach the published `.d.ts`. The send direction is derived from that one
table, never hand-written as a second `Record`.

**9. Codegen oracle hardened.** `packages/proto-gen/scripts/verify-codegen.sh` was
positive-presence-only and would have stayed green through all five layout deletions and the mute
rename. It now asserts the new symbols present *and* twelve deleted symbols absent, each absence
anchored on the export declaration (`export type X ` / `XSchema`) so a surviving doc-comment mention
cannot produce a false pass. `StreamAssignment` is deliberately in the presence set only — the name
is reused.

**10. Layer 6 is red on purpose.** 54 `buf breaking` findings, enumerated by rule class and count in
`## Expected Layer-State`. No suppression of any kind was added.

---

## Files Modified

**Contract (Guarded Shared Area — protocol)**

- `proto/dark_tower/signaling/v1/signaling.proto` — the reshape.
- `crates/proto-gen/build.rs` — `skip_debug` for the three credential/key-bearing messages.
- `crates/proto-gen/src/lib.rs` — redacting `impl Debug` ×3 with the constant-lengths invariant.
- `crates/proto-gen/tests/signaling_roundtrip.rs` **(new)** — 23 tests: encode/decode round-trip for
  every new and changed message, `SlotState`'s seven distinct non-zero discriminants, `sender_id`
  genuinely `uint32` on the wire, absent-decodes-as-absent-not-zero, and the redaction assertions
  including through the enclosing envelope.
- `packages/proto-gen/scripts/verify-codegen.sh` — presence + absence asserts.

**Documentation (protocol)**

- `docs/protocol/CONVENTIONS.md` — `breaking.use` corrected to `[FILE]` (it still said
  `WIRE_JSON`, changed under it in `2e4120a`); the intentional-wire-break protocol written down;
  the five drained `// buf:lint:ignore` annotations recorded; **new §Rule 5 "Reserve the vacated tag
  and name; never repurpose in place"**, promoting Rule R out of this devloop output into the
  canonical spec, with both of its exceptions enumerated.
- `docs/API_CONTRACTS.md` — signalling contract rewritten to the ADR-0036 shape.
- `docs/WEBTRANSPORT_FLOW.md` — layout-subscription flow removed, receive-capability flow added.
- `docs/specialist-knowledge/protocol/INDEX.md` — navigation for every new shape.
- `docs/devloop-outputs/2026-08-31-signaling-media-contract-reshape/main.md` — this file.

**Not mine — see the Cross-Boundary Classification table for owner and classification**

- MC (`meeting-controller`): `src/webtransport/connection.rs`, `src/webtransport/handler.rs`,
  `src/actors/participant.rs`, `src/actors/messages.rs`, `src/actors/meeting.rs`, `src/lib.rs`, and
  four `tests/` files; `crates/env-tests/tests/24_join_flow.rs`, `26_mh_quic.rs`.
- Client (`client`): `packages/sdk-core/src/signaling/{events.ts, SignalingClient.ts,
  errorCodeMap.ts, codecMap.ts (new)}` and three `__tests__/` files; `packages/sdk-svelte`
  `MeetingStore.test.ts`, `subscribeSession.test.ts`; `packages/web-app` `src/lib/e2eBus.ts`,
  `src/__tests__/e2eBus.test.ts`, `src/__tests__/helpers/MockMeetingSession.ts`,
  `e2e/join-happy-path.spec.ts` (comment only).
- `docs/TODO.md` (meeting-controller), `docs/specialist-knowledge/dry-reviewer/INDEX.md`
  (dry-reviewer).
- Gate-3 additions (classification rows added, owners confirmed): `packages/sdk-core/src/index.ts`
  (client — `SignalingCodec` root export), `docs/user-stories/2026-08-27-hear-yourself-through-handler.md`
  (meeting-controller — task-10 prompt prose fix), `docs/runbooks/mc-deployment.md` (operations —
  MC+SDK wire-lockstep line).

Generated, not committed: `packages/sdk-core/src/proto/**/*_pb.ts` (`.gitignore:13`) and the Rust
bindings in `OUT_DIR`.

---

## Devloop Verification Steps

Run from the repo root unless noted.

| Step | Command | Result |
|------|---------|--------|
| 1 | `./scripts/layer1.sh` (compile + `buf build`) | **OK** |
| 2 | `./scripts/layer2.sh` (format + `buf format`) | **OK** |
| 3 | `./scripts/layer3.sh` (guards) | **FAIL — pre-existing, not from this diff. See Issues #4.** |
| 4 | `./scripts/layer4.sh` (test) | **OK** |
| 5 | `./scripts/layer5.sh` (lint + `buf lint`) | **OK** — buf STANDARD clean with zero annotations |
| 6 | `./scripts/layer6.sh` (audit + `buf breaking`) | **FAIL, deliberately — 54 findings, exactly as predicted in `## Expected Layer-State`. `cargo-audit` OK.** |
| 7 | `./scripts/layer7.sh` (env-tests) | Host-side; needs a redeployed MC image. Not run here. |

Targeted checks:

- `cargo test -p proto-gen` — 23/23, including the redaction assertions and the four Gate-3 coverage additions (@test TEST-1/2/3): `sender_id`/`pinned_sender_id` `Some(0)` presence, `MediaStream`/`PublishStream` retype round-trips, and `receive_capability` through the ClientMessage envelope.
- `pnpm exec nx run proto-gen:codegen` then `pnpm exec nx run proto-gen:test` — the latter
  cleans-then-generates, so stale output cannot mask a broken config.
- The prediction diff itself:
  `cd proto && pnpm exec buf breaking --against '../.git#ref=HEAD,subdir=proto'` → 54 lines. This is
  the artifact @operations compares against `## Expected Layer-State` class by class at Gate 2.

Remaining host-side action: rebuild and redeploy the MC image before Layer 7, since
`build_join_response` and the roster construction changed shape.

---

## Code Review Results

Twelve reviewers across Gate-3 rounds 1 and 2. Every finding was fix-it; none was
re-litigated. Dispositions below; full per-finding detail in §Issues items 6–9.

| Reviewer | Findings | Disposition |
|----------|----------|-------------|
| test | TEST-1..4 (roundtrip Some(0), MediaStream/PublishStream retype, tag-12 envelope, `assert_not_generated` `-f` guard); B22 (TS `JoinedEvent` KEK test) | RESOLVED-FIXED |
| auth-controller | `identity_public_key` length-check un-attributed at both sites | RESOLVED (task-10 named) |
| paired-meeting-controller | cross-boundary trailer under-covered (participant.rs, count 4→6, TODO ref) | RESOLVED-FIXED (verbatim) |
| code-reviewer | F1 `MediaServerInfo` name reservation; F2 safety-invariant comment false for `join_token`; F3 redaction consolidation | RESOLVED-FIXED |
| dry-reviewer | D1 redaction one-helper; D2 slot_id↔stream_number reciprocal; D3 doc restatement→pointer; D4 CONVENTIONS tag 7→8 | RESOLVED-DEFERRED (fixes in; TODO bullets theirs) |
| operations | Layer-6 verified 54/54 set-identical; OPS-2 header-version owner misattribution; OPS-1/B19 story prompt; OPS-3/B21 deploy lockstep | RESOLVED-DEFERRED |
| security | B1 `MhConnectRequest` HIGH; B4 roster/name PII; B2 invariant; B10 owner; media-protocol anchors deferred | B1/B4 fixed; anchors RESOLVED-DEFERRED (task 8) |
| observability | F1/B1 (ESCALATED) MhConnectRequest; F2–F8 redaction/PII/prohibitions/tracking; F9/B22 TS test | fixed; re-confirm pending |
| semantic-guard | G1-5 credential-leak `checks.md` KEK vocabulary + TS sink | tracked in docs/TODO.md (task 10) |
| client | CL-1 `SignalingCodec` root export | RESOLVED-FIXED (Lead fix-now ruling B14) |

Layer 6 stays red by design at exactly 54 findings; no suppression added at any point.

---

## Accepted Deferrals

Pointer-only; debt bodies live in `docs/TODO.md`.

- `frame.rs::PROTOCOL_VERSION` reciprocal anchor (@operations) → `docs/TODO.md` §Media Path Obligations bullet (b), story task 8.
- `frame.rs:384` `impl Debug` constant-lengths invariant (@security) → same entry, bullet (c), story task 8.
- Both deferred because `crates/media-protocol/**` needs media-handler co-sign at Gate 1 *and* Gate 3 (ADR-0024 §6.4); @team-lead ruled not to pull MH in for two comment lines.
- Consequence: @security's Gate-3 verdict is RESOLVED-DEFERRED, not RESOLVED-FIXED.
- Classification guard cannot express ADR-0024 §6.4's two-owner intersection rule (@security, B17) → `docs/TODO.md` §From ADR-0024 §6 Amendment, owner dt-guard tooling + ADR-0024 owner.
- Credential-leak `checks.md` KEK vocabulary + TS `ServerMessage`-stringify sink control (@security/@semantic-guard, G1-5/B16) → `docs/TODO.md` §Credential-leak guard + TS KEK sink, owner story task 10.
- Metric-label PII guard has no coverage for the five ADR-0036 identifiers after `user_id`→`sender_id` (@observability, B18); guard change deferred, concrete list tracked → `docs/TODO.md` §Observability Debt, owner dt-guard tooling / story R-29.
- `SignalingCodec` root export (@client, CL-1/B14) → NOT deferred; fixed in-changeset under @team-lead fix-now ruling.
- `.github/workflows/ci-client.yml` hand-rolls `buf breaking`, so the new `SUPPRESSED=` warning does not reach GitHub CI (@operations, delta review) → `docs/TODO.md` §Restore buf breaking enforcement, second bullet, owner infrastructure; deferred because routing that step through the wrapper also changes push-mode base-ref semantics (`origin/main` → `HEAD~1`).
- **No dated hard-fail expiry on the `breaking.ignore` carve-out** (@security, S3; this is the deferral §Human Decision and Resumption's "Not mechanically enforced" points at). @security's argument: this repo already has the pattern in `audit-suppressions.toml`, whose entries carry an `expires` date that an always-run Layer-3 guard hard-fails past, with policy ownership assigned to security — measured against that bar, a prose TODO is not a control. **@team-lead ruling: the gap is materially but not fully closed, and the remainder is deferred with an owner.** Closed part: `scripts/lang/proto/breaking.sh` now prints `SUPPRESSED=` on *every* Layer-6 run until the key is deleted, and the line's disappearance is the completion signal — so the restore no longer depends on someone re-reading `docs/TODO.md`, which was the actual failure mode @security named. Remaining part: a warning cannot *force* the restore the way a dated hard-fail can, so an indefinitely-lived carve-out is still possible. Not built here for the reason @security itself gave — `scripts/guards/**` is dt-guard-tooling/infrastructure-owned, entries cannot reuse the `audit-suppressions.toml` manifest (its schema is `ecosystem = rust|js` and is code-generated into `.cargo/audit.toml` / `.pnpm-audit-ignore.json`, so a proto entry breaks the sync-check), and adding an always-run gate that can red the pipeline without its owner is the same unilateral-pipeline-change class L6-1 existed to prevent. → `docs/TODO.md` §Restore buf breaking enforcement, owner infrastructure + security, trigger: build it if the carve-out outlives the story PR.

## Lead Gate-2 Verdict and Escalation (2026-09-01)

**Measured by the Lead**, on the final tree, on a clean uncontended `./scripts/layer-all.sh`
(`git clean -fdX packages/sdk-core/coverage .nx/cache` first; no teammate processes running):

| Layer | Result | Note |
|-------|--------|------|
| 1 Compile | OK | |
| 2 Format | OK | |
| 3 Guards | OK | incl. `validate-subdomain-regex-sync` |
| 4 Test | N/A | rust `cargo-test-passed`, ts `nx-test-passed`; the N/A is proto's documented intentional-gap placeholder (ADR-0033 §6) |
| 5 Lint | OK | `buf lint` 0 |
| 6 Audit | **FAIL** | `cargo audit` OK; **`buf breaking` = 54 findings, exactly as declared** |
| 7 Env-tests | OK | Rust env-tests + browser E2E |

`TOTAL_RESULT=FAIL`, exit 1 — **solely** on the declared Layer-6 wire break.

**Two transient reds, both diagnosed and neither in the diff:**
1. *Layer 3, runs 1-2* — `validate-subdomain-regex-sync` counted 13 then 15 occurrences vs 10.
   All extras were gitignored build artifacts (`packages/sdk-core/coverage/**`, `.nx/cache/**`),
   copies of the one enumerated `limits.ts` site; the count *rose* between runs purely because more
   test runs generated more cached copies — the compounding signature of artifact scanning, not
   drift. Cleared with the interim workaround already recorded in `docs/TODO.md`. The Lead declined
   to fix the guard here (shared gate tooling, no row in the classification table, and a
   gate-tooling change in the commit whose purpose is a diffable `## Expected Layer-State` would
   make that diff unreadable). @operations agreed and re-confirmed the underlying defect is tracked
   with an owner and a fix direction.
2. *Layer 4, one Lead run* — E0463 "can't find crate" across `axum`/`sqlx`/`common`/`tower_http`.
   Non-reproducing: the Lead's `layer-all.sh` had collided with the implementer's concurrent `cargo`
   on the same `target/`. Re-ran green in isolation, then green again on the clean full run.
   Operator lane per `docs/runbooks/devloop-validation.md` §6.3 — did not consume an attempt.
3. *Layer 7, run 2* — `mc-token-rejection.spec.ts:50` `locator.click` timeout at 15000ms. Passed in
   run 1 (12.9s) and in all three subsequent runs (6.9s). **Confirmed flake**, recorded as such.

**Ruling: escalate, do not commit.** Lead Ruling L6-5, taken at Gate 1, binds here: a Lead accepting
a red gate in a headless run is the self-approved risk acceptance Headless Mode forbids, and the
acceptance could not change `gate_rc` in any case (`_common.sh` normalizes buf's exit 100 to
`STATUS=FAIL`/rc 1, and `run-story.sh:1589` routes rc 1 to the implementer lane mechanically —
prose cannot reroute it). The discriminator @operations owns is satisfied: **54 predicted, 54
measured, set-identical at item level — zero fired-but-unpredicted, zero predicted-but-not-fired**,
so this is a deliberate break and not a bad diff. No suppression of any kind was added (L6-1),
re-audited on the final tree. **[Superseded for the committed tree — see §Human Decision and
Resumption. This sentence is a measurement of the 03:18 tree and is left standing as such; the
tree this devloop commits DOES carry a `proto/buf.yaml` carve-out.]**

The human decides whether to accept the intentional wire break. The decision should resolve the
**story**, not this task alone — task 4 (`internal.proto` reshape) hits the identical wall.

---

## Human Decision and Resumption (2026-09-01)

**The escalation above fired as written and the runner stopped the story.** Evidence, all outside
this session's control: `task-3.escalation.json` (03:19:05, `reason: pipeline-red-layer6`),
`task-3.runner-escalation.20260901T031928Z.json` (`reason: devloop-escalated`), and
`task-3.head-before` pinned at `3a7104f` with nothing committed.

**The human then chose option (a) — accept the intentional break — and directed a `buf.yaml`
carve-out to carry it through Layer 6 and CI.** Their intervention landed in the window between the
runner's stop (03:19) and its restart (04:22:51), while no headless devloop was running:

| Time | File | Change |
|------|------|--------|
| 04:19:46 | `proto/buf.yaml` | `breaking.ignore: [dark_tower]` + a comment block naming the restore condition |
| 04:19:47 | `docs/TODO.md` | §Restore buf breaking enforcement after ADR-0036 story 1 — "added 2026-08-31 at the user's direction" |
| 04:21:10 | `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` | task-10 prompt: `user_id` deleted + tag 2 reserved (not repurposed), `NonZeroU16` bound |

The story-manifest edit is the corroborating signal that this was a human and not a stray process:
it is a story-level design decision that a stopped devloop has no path to make, and it propagates
task 3's actual implementation forward to task 10. **No other tracked file changed after 03:18:50** —
the reviewed diff is untouched; the delta is exactly these three files.

At 04:22:20 the runner recorded an `auth-expired` infra incident (`task-3.infra-incident...json`,
OAuth session expired), refreshed on the host, and relaunched task 3 as
`/devloop --continue=2026-08-31-signaling-media-contract-reshape` with the instruction *"finish
incomplete phases, then gates and commit as normal"* — the Recovery exception for headless infra
interruptions, not a fresh task.

### What this does to Lead Ruling L6-1

**L6-1 ("no masking") is overridden by the human, and that is the human's call to make.** L6-1 and
L6-5 were both reasoned from the *absence* of human authority: L6-5's whole content is that a Lead
may not self-approve a red gate in a headless run. That constraint binds the Lead; it does not bind
the person the escalation was addressed to. The escalation asked the question, the human answered
it, and the answer is recorded in-tree in the two places a reader would look.

**The record above is now partly false as written and is corrected here rather than edited in place**
(same discipline as §Expected Layer-State's Corrections): §Lead Gate-2 Verdict states "No suppression
of any kind was added (L6-1), re-audited on the final tree." That was true of the tree it measured, at
03:18. **It is not true of the tree this devloop commits** — `proto/buf.yaml` carries
`breaking.ignore: [dark_tower/signaling/v1/signaling.proto]` (narrowed from the as-directed
`dark_tower` at @operations' delta review — see the next section). Both statements stand,
timestamped.

### The carve-out's cost, stated plainly

**Corrected by @operations at the post-decision delta review (2026-09-01).** As first written this
section said `ignore: dark_tower` blankets the entire package and that "the narrower alternative
does not exist at the tool level." **That was wrong, and it was wrong in the direction that costs
enforcement.** buf's `breaking.ignore` takes **paths** (files/dirs relative to the module root), not
package names, so a per-file carve-out does exist. Measured, not reasoned:

| `breaking.ignore` | reshape findings | an injected `RegisterResponse.media_handler_url` `string`→`int32` break in `internal.proto` |
|---|---|---|
| *(none)* | 54 in `signaling.proto`, 0 elsewhere | **fires** (55 total) |
| `dark_tower/signaling/v1/signaling.proto` | 0 | **fires** — exit 100 |
| `dark_tower` (as originally written) | 0 | **silently swallowed** — exit 0 |

The blanket entry therefore disabled FILE enforcement on `dark_tower/internal/v1/internal.proto`
as well, and that was not theoretical: internal.proto is already edited in this very commit
(`REJECTION_REASON_INVALID_REQUEST`), and story tasks 4 and 8-10 reshape that MC↔MH contract while
the carve-out is in force — i.e. the blanket form would have been off for exactly the file most
likely to break next. **The carve-out is now scoped to
`dark_tower/signaling/v1/signaling.proto`**, which passes on both the host buf (1.50.0) and the
CI-pinned buf (1.72.0), and leaves internal.proto fully enforced.

What remains true: inside `signaling.proto` no break of any kind is caught while the key is present,
including unintended ones, and reviewers must read that file's diff by hand. buf has no per-finding
acceptance (ADR-0033 §13 Wave-3 mechanism unbuilt), so one file is the floor. Time is still the main
scoping control; breadth is now also controlled, one file instead of two.

### The carve-out is no longer a silent green

The original form of this change made `buf breaking` report `STATUS=OK REASON=buf-breaking-passed`
with **no** signal that enforcement was off — discoverable only by reading `proto/buf.yaml`. That is
the masked failure CLAUDE.md forbids, and it was made worse by `scripts/lang/proto/breaking.sh`'s own
header comment, which tells a 3am reader the gate cannot be silenced (true of CLI/env bypass, no
longer true of config). `docs/runbooks/devloop-validation.md` §6.6 said the same in two places ("no
override exists yet, by design").

`scripts/lang/proto/breaking.sh` now detects a `breaking.ignore` block in `proto/buf.yaml` and emits
`SUPPRESSED=<paths>` plus a one-line explanation before running the gate — the **same idiom already
used by `scripts/lang/rust/audit.sh` and `scripts/lang/ts/audit.sh`**, the other two audit-family
gates that can be silenced by a tracked in-tree list. Proto was the only one of the three without it.
Verified with three controls: fires on the current config; silent when the `ignore:` key is removed;
silent for a `lint.ignore` block (scoped to the `breaking:` block only). It is a warning, not a
failure — an intentional wire break is legitimate and this wrapper must not be the thing that blocks
one; the missing property was noticeability, not strictness.

This is also the answer to "the restore path is a prose TODO with nothing that fires." The
`SUPPRESSED=` line prints on **every** Layer-6 run until the key is deleted, and deleting the key
removes the line — the disappearance is the completion signal. The runbook's Layer-6 symptom tables
now carry a `buf-breaking-passed` + `SUPPRESSED=` row saying green is not evidence.

**Not covered: GitHub CI.** `.github/workflows/ci-client.yml` does not route through the wrapper — it
hand-rolls `pnpm exec buf breaking` with its own `git merge-base` (self-described there as an INTERIM
PATCH). The carve-out **does** apply to it (buf reads `proto/buf.yaml` regardless of caller; verified
on the CI-pinned 1.72.0, so no CI/pipeline discrepancy on the *gate result*), but the `SUPPRESSED=`
warning does not, so a GitHub PR check shows a bare green. Tracked with the recommended fix and its
one non-drop-in caveat (that workflow also runs on `push: [main, develop]`, where the hand-roll bases
on `origin/main` while `_get_base_ref.sh` bases on `HEAD~1`) in `docs/TODO.md`, owner infrastructure.

**Deployment/rollback impact: unchanged.** The carve-out is a CI/pipeline gate, not a deploy control.
`docs/runbooks/mc-deployment.md:108-119` — the ADR-0036 breaking-wire-change entry and the "a
deployment rollback must revert MC and the SDK together" lockstep rule — is untouched and
un-undercut by it. Re-confirmed at the delta review.

**Restore condition**: delete the `ignore:` key once the story PR merges to main — the first
post-merge merge-base is post-reshape, so FILE enforcement goes green on its own. **Rebase any
in-flight proto-touching branch onto post-merge main first**, or its merge-base is still pre-reshape
and it re-fires all 54. Tracked in `docs/TODO.md` §Restore buf breaking enforcement after ADR-0036
story 1, owner protocol, with "do not close the story's PR review without this entry visible."

### Lead adjudication of the two deviations from the human's literal direction

Recorded prominently because **the committed tree does not match, byte for byte, what the human
wrote at 04:19**, and a reader must be able to see that in one place and undo it in one edit.

**Deviation 1 — the `ignore:` value was narrowed, `dark_tower` → `dark_tower/signaling/v1/signaling.proto`.**
Lead ruling: **keep the narrowing.** The human's decision was *accept the intentional wire break and
carve it out*; that decision is untouched and is not re-litigated anywhere in this devloop. What
changed is only the value implementing it, and it changed because the original value rested on a
factual error about the tool — `breaking.ignore` takes paths, not package names, and `dark_tower`
worked only incidentally because it is also a directory. @operations measured the consequence rather
than arguing it: the blanket form **silently swallows** an injected `internal.proto` break (exit 0)
that the narrow form still catches (exit 100). Since `internal.proto` is edited in this very commit
and is reshaped by story task 4 next, the as-written value would have left the most break-prone file
unguarded — the opposite of the human's own stated intent, whose comment block asks for full
enforcement restored as soon as possible. Narrowing moves strictly toward more enforcement and takes
nothing away. **To revert to the human's literal text**: set `ignore:` back to `- dark_tower` in
`proto/buf.yaml`; nothing else depends on the value.

**Deviation 2 — `scripts/lang/proto/breaking.sh` gained a `SUPPRESSED=` emission** (pipeline gate
tooling, which the human did not ask for). Lead ruling: **keep it.** It is warning-only on stderr and
cannot red the gate; it reuses the idiom already present in `scripts/lang/rust/audit.sh` and
`scripts/lang/ts/audit.sh`, so it adds no new mechanism; it was written and verified by
@operations, who owns the pipeline surface, so owner-implements is satisfied rather than bypassed;
and it is the direct answer to CLAUDE.md §Fail loudly, never mask, which a silent green on a
disabled gate violates. It is also the only *firing* control over the restore path — see the
deferral below. Both files carry classification rows and both scope-drift and classification-sanity
guards pass on the final tree.

Neither deviation is a Lead accepting a red gate — that is what L6-5 forbids and it did not happen
here. The red gate was escalated, the human answered, and these are corrections to the *implementation*
of their answer.

### Process incident during the delta review — disclosed, not buried

@security ran `git checkout -- proto/buf.yaml` while testing blast radius, **reverting the human's
uncommitted carve-out**, then reported it had restored the file "byte-for-byte ... same hunk header
`@@ -7,3 +7,17 @@`, 14 insertions, identical text." **That report was wrong**: the tree the Lead
verified immediately afterward carried `@@ -7,3 +7,38 @@` and a fully rewritten comment block. The
benign explanation fits the timestamps — @operations was concurrently rewriting the same file for
OPS-D1 — so the likely sequence is a correct restore overwritten seconds later, mis-described as
verified rather than deliberately misreported.

Two things follow, and both are why this is recorded rather than dropped. First, **the human's
original text survives only in the session transcript, not in git**, since the carve-out was never
committed; it is reproduced verbatim in §The carve-out's cost above and in Deviation 1's revert
instruction, so it is recoverable from this file alone. Second, **a reviewer self-verification claim
was false on the tree**, which is exactly the class of assurance this devloop is not entitled to take
at face value; the Lead re-verified `git diff proto/buf.yaml` directly rather than accepting the
report, and every gate result in §Post-Resumption Validation below is a Lead-run measurement, not a
relayed one. Reviewers editing the tree at all is a role violation — they were asked for findings and
a verdict — and the resulting edits were kept only after the independent adjudication above.

### Re-gate

The 54-finding prediction in §Expected Layer-State was confirmed set-identical at 03:18 by
@operations and by the Lead independently — that evidence stands on its own and is what makes this a
deliberate break rather than a bad diff. **The carve-out changes what the gate *reports*, not what
the diff *is*.** The authoritative post-resumption seven-layer run is recorded in §Post-Resumption
Validation below.

**Post-delta-review re-validation (@operations, 2026-09-01).** The narrowing + `SUPPRESSED=` changes
postdate that unattended run, so they were re-validated directly: `buf lint` OK, `buf format --diff
--exit-code` OK, `buf breaking` exit 0 on both the host buf 1.50.0 and the CI-pinned buf 1.72.0,
`bash -n scripts/lang/proto/breaking.sh` OK, and the wrapper end-to-end emitting
`SUPPRESSED=dark_tower/signaling/v1/signaling.proto` followed by `STATUS=OK
REASON=buf-breaking-passed`, rc 0. Negative controls: no `SUPPRESSED=` line with the `ignore:` key
removed, and none for a `lint.ignore` block. The only non-doc code touched is
`scripts/lang/proto/breaking.sh`, and only above the unchanged `run_and_emit` line.

---

## Post-Resumption Validation

**Authoritative run: `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh`, Lead-executed, unattended
(headless ⇒ all seven layers), on the final tree with every agent idle and no concurrent `cargo`.**
Preceded by `git clean -fdX packages/sdk-core/coverage .nx/cache`. Log: `/tmp/layer-all-final.log`.

```
LAYER=1 RESULT=OK  DURATION=8     LAYER=5 RESULT=OK  DURATION=13
LAYER=2 RESULT=OK  DURATION=1     LAYER=6 RESULT=N/A DURATION=2
LAYER=3 RESULT=OK  DURATION=23    LAYER=7 RESULT=OK  DURATION=434
LAYER=4 RESULT=N/A DURATION=191
TOTAL_DURATION=672 TOTAL_RESULT=N/A   (exit 0)
```

| Layer | Result | Evidence, and why an `N/A` is not a hidden failure |
|-------|--------|-----------------------------------------------------|
| 1 Compile | `OK` | `buf-build-passed`, `cargo-build-passed`, `cargo-build-dt-guard-passed`, `cargo-build-dt-story-passed`, `nx-typecheck-passed` |
| 2 Format | `OK` | `buf-format-passed`, `cargo-fmt-passed`, `nx-format-passed` |
| 3 Guards | `OK` | `guards-passed` — 37/37, including `validate-cross-boundary-scope`, `validate-cross-boundary-classification` and `validate-subdomain-regex-sync` |
| 4 Test | `N/A` | **rust `cargo-test-passed` OK and ts `nx-test-passed` OK.** The aggregate is `N/A` *only* because proto's `test.sh` is the documented intentional-gap placeholder emitting `REASON=not-applicable-to-this-lang` (ADR-0033 §6). No test failed and no test was skipped. |
| 5 Lint | `OK` | `buf-lint-passed`, `cargo-clippy-passed`, `nx-lint-passed` |
| 6 Audit | `N/A` | `cargo-audit-passed` OK (`SUPPRESSED=RUSTSEC-2023-0071,RUSTSEC-2025-0052`, both pre-existing dated entries in `audit-suppressions.toml`, untouched by this devloop); `pnpm audit` `SKIPPED-NO-DIFF REASON=no-dep-changes` — the wrapper's own dep-manifest gate, the one documented `SKIPPED-NO-DIFF` producer left in the pipeline; **`buf breaking` `STATUS=OK REASON=buf-breaking-passed` preceded by `SUPPRESSED=dark_tower/signaling/v1/signaling.proto`**. The aggregate `N/A` is the dep-gate, not a swallowed failure. |
| 7 Env-tests | `OK` | `env-tests-passed` (Rust env-tests) **and** `browser-e2e-passed` (Playwright). Both Phase-2 suites ran; neither was skipped. |

**Read Layer 6 with the `SUPPRESSED=` line, not without it.** `buf-breaking-passed` here does **not**
mean `signaling.proto` is break-free — it means enforcement is off for that one file by the
carve-out the human directed. That is precisely the silent green @operations' `breaking.sh` change
exists to prevent, and this table is the first record where the warning does its job. Every other
proto, including `internal.proto`, remained under full FILE enforcement during this run.

**Layer 4 and Layer 6 are the only non-`OK` cells and both are self-justifying wrapper statuses**
(ADR-0033 §6): an intentional-gap placeholder and the audit dep-manifest gate. Neither is
`FAIL-MISSING-VERB`, and — because this run was unattended with `DEVLOOP_FAIL_FAST=0` — **no layer
rendered `NOT-RUN`**, so nothing in the table is merely unmeasured. `TOTAL_RESULT=N/A` with exit 0 is
the aggregate of those two cells against five `OK`s; there is no `FAIL` anywhere in the run.

**Earlier runs this session, recorded so the count is honest.** A first unattended full run
(`/tmp/layer-all-resume.log`) returned `TOTAL_RESULT=FAIL` on `LAYER=3` — `validate-cross-boundary-scope`
flagged `scope_drift_inbound "proto/buf.yaml"`, because the human's 04:19 edit landed outside the
plan's classification table. That was a **true positive against this devloop's own record**, not a
flake: the guard caught an unlisted file in the diff, exactly its job. Fixed by adding the
`proto/buf.yaml` row (and, per @operations OPS-D5, by correcting that row's `Owner` cell to a bare
`protocol` the ownership manifest accepts). Verified causal, not coincidental: removing the row
reproduces `STATUS=FAIL REASON=cross-boundary-scope-drift-inbound-1`; restoring it returns
`STATUS=OK REASON=cross-boundary-scope-no-drift`. That run also predates the narrowing and the
`SUPPRESSED=` change and overlapped concurrent reviewer `cargo`/`buf` activity, so it is superseded
in full by the table above and is not evidence about the final tree.

The three transient reds diagnosed at the 03:18 Gate-2 measurement (`validate-subdomain-regex-sync`
artifact miscount; an E0463 `target/` collision; one `mc-token-rejection.spec.ts` click timeout) did
**not** recur in this run: Layer 3, Layer 4 and Layer 7 are all clean above with the artifact clean
applied first.

---

## Rollback Procedure

1. Start commit: `3a7104f341c4b1130048d4419d65788149c43dd9`
2. `git diff 3a7104f..HEAD`
3. `git reset --soft 3a7104f` (preserve) or `git reset --hard 3a7104f` (revert)
4. **Regenerate the TypeScript bindings after the reset**: `pnpm exec nx run proto-gen:codegen`.
   `packages/sdk-core/src/proto/**/*_pb.ts` is gitignored (`.gitignore:13`), so `git reset` leaves
   new-shape generated types against the old-shape `.proto` and every TS build compiles against a
   contract no longer in the tree. `packages/sdk-core/dist/` (`.gitignore:64`) is likewise stale
   until rebuilt.
5. The Rust side needs no step: `crates/proto-gen/build.rs` generates into `OUT_DIR` and emits
   `cargo:rerun-if-changed` for both `.proto` files, so a reset regenerates on the next build.

---

## Issues Encountered & Resolutions

**1. `buf breaking` has no suppression mechanism, and the runner routes its red to the implementer
lane regardless of what the Lead decides.** Surfaced *before* implementation (P0), ruled on at
Gate 1 (see §Lead Ruling). Resolution: take the fire, add no carve-out of any kind, and make the
`## Expected Layer-State` declaration precise enough that the real log is diffable against it class
by class. Measured outcome: 54 findings, matching the corrected prediction exactly (12 / 19 / 8 / 1
/ 14). The escalation to the human still stands — the mechanism gap is real and task 4 hits the
identical wall — but the artifact handed over is a reviewed diff plus a measured log, not a
hypothesis.

**2. Rule R was applied to four messages and silently skipped on a fifth.** `StreamAssignment`'s new
§6 shape re-pointed tags 1–5 in place with no `reserved` statement, while `JoinResponse`,
`MediaStream`, `PublishStream` and `ParticipantCapabilities` all reserved theirs. Two of the five
repurposes were the exact hazard the rule exists for: tag 2 `uint64 user_id` → `optional uint32
sender_id` is varint-to-varint, and tag 3 `MediaType` → `MediaKind` changes the enum's zero meaning
— an old peer decodes both **successfully, with the wrong semantics**. Tag 2 is byte-for-byte the
change `JoinResponse.sender_id`'s own comment calls "the one change an old peer would decode
successfully with the wrong semantics", on the same field-name pair.

Root cause: the name `StreamAssignment` is *reused* rather than deleted, so the message read as a
new definition during drafting and never got compared against the old one. The four messages that
got it right were all obviously-surviving messages where the old shape was in view.

Resolution: `reserved 1, 2, 3, 4, 5;` plus
`reserved "stream_id", "user_id", "media_type", "slot_index";`, fields moved to fresh tags 6–11.
Tags 1, 4 and 5 were wire-incompatible repurposes that would have failed loud, and were reserved
anyway — a rule applied to three of five repurposes in one message is worse than no rule, because a
reader cannot tell the unreserved tags from the ones nobody looked at. No repurpose was argued to be
safe and kept. `media_handler_url` is deliberately absent from the *name* reservation (the new shape
reuses it on tag 9, and reserving a name you then use is a protoc error), which makes two in-file
instances of Rule R's name-half exception rather than one; both are annotated at the site.
`StreamAssignments` (plural) takes no reservation and says so in-file: its one field keeps its name,
tag, cardinality and type name, so there is nothing vacated to reserve.

Fallout from the renumber: **none**, and it was checked rather than assumed. Field *names* are
unchanged, so prost and protobuf-es generate identical accessors; `cargo test -p proto-gen` (19
tests, including the `StreamAssignment` round-trips), `layer1`, `layer2`, `layer4` and `layer5` all
pass, and `buf lint` is clean.

**3. The prediction was wrong in two places, and a wrong prediction cannot do its only job.** Both
are recorded as marked corrections in `## Expected Layer-State` rather than quietly rewritten.
- **`ErrorCode`: predicted 0 class-5 findings, measured 9.** The reasoning was sound about value
  *numbers* — every number is kept, so there is no `ENUM_VALUE_NO_DELETE` — but the class as named
  includes `ENUM_VALUE_SAME_NAME`, and draining `ErrorCode`'s `// buf:lint:ignore` carve-out renames
  all nine values. The lesson is narrow and worth stating: a prediction organised by *rule class*
  must be checked against every rule in the class, not against the one the author was thinking
  about.
- **`ParticipantCapabilities` tag 4 `max_video_streams` was missing from the class-2 enumeration.**
  It was argued and accepted in §"Declared scope expansions" item 3 but never folded back into the
  prediction. Scope expansions have to propagate into the prediction in the same edit, or the Gate-2
  diff reads a deliberate change as a regression.

**4. Layer 3 is red, and it is not this diff.** `validate-subdomain-regex-sync` reports
`found 13 occurrences ... expected exactly 10`. All three extra occurrences are **gitignored build
artifacts**, all copies of one enumerated site
(`packages/sdk-core/src/validation/limits.ts`): `packages/sdk-core/coverage/src/validation/
limits.ts.html` and two `.nx/cache/*/packages/sdk-core/coverage/...` copies of it. The guard at
`scripts/guards/simple/validate-subdomain-regex-sync.sh:122-124` excludes `.git`, `node_modules`,
`target`, `dist`, `devloop-outputs` and `user-stories` — but not `coverage` or `.nx`, so it fires on
any tree where a coverage run has happened locally. No file this devloop touches contains the
pattern, and all seven enumerated sites report `OK`.

**Not fixed here, deliberately, and not masked either.** The fix is one line
(`--exclude-dir=coverage --exclude-dir=.nx` alongside the existing exclusions) plus a re-check of
the guard's self-test, but `scripts/guards/**` is shared gate tooling that appears in no row of this
diff's Cross-Boundary Classification table, and the same contamination argument the Lead applied to
the `todo_tracking.rs` guard applies with more force here: a gate-tooling change landing in the one
commit whose whole purpose is a diffable `## Expected Layer-State` makes that diff unreadable.
Reported loudly to @team-lead instead, with the root cause, the exact fix and the owner named
(operations / dt-guard tooling). It must not be waved through as "flaky".

**5. This file was corrupted by a bad edit during drafting.** The entire `## Expected Layer-State`
body had been spliced into the middle of §P0, at the point where P0 says "See `## Expected
Layer-State`" — truncating P0 mid-sentence and overwriting §P1's heading, its intro and its table
header, leaving a bare `---|---|---|` and a headerless table of new messages. Nothing was lost
except those few lines (the table rows and all of P1's prose survived below the splice). Repaired:
P0's sentence completed, P1's heading/intro/table header reconstructed and **marked as
reconstructed** in place, and the canonical `## Expected Layer-State` section rewritten at the
bottom where it belongs. Called out because a reader of the original would have concluded the
prediction lived in two places with different content, which is exactly the drift this project's
single-source-of-truth rule exists to prevent — and because "the doc was mangled" is the kind of
thing that gets silently tidied and then repeats.

**6. `docs/protocol/CONVENTIONS.md` documented a `breaking.use` value the repo no longer had.** It
said `WIRE_JSON`; `proto/buf.yaml` was changed to `[FILE]` in `2e4120a` without the doc following.
That is not cosmetic — `FILE` fires on symbol deletion, field deletion, field rename and field type
change, i.e. every category this reshape uses, so the doc would have led a reader to predict a
materially smaller finding set. Corrected, and the intentional-wire-break protocol written down
alongside it so the next devloop to break the wire does not have to re-derive it from
`docs/TODO.md:820`.


**7. Gate-3 cross-reviewer findings — all fix-it, applied in-changeset.** Recorded so the diff is
traceable to the reviews that shaped it.
- **@auth-controller** — `identity_public_key` length-check MUST was un-attributed at both sites
  (`JoinRequest` and roster `Participant`); every other MC obligation in the file names its task.
  Added "(story task 10)" to both, matching the `sender_id`/`meeting_kek` phrasing; aligned
  `API_CONTRACTS.md`'s "a later story" → "story 2".
- **@paired-meeting-controller** — the recorded `Approved-Cross-Boundary` trailer under-covered:
  `participant.rs` shipped server-mute renames but was absent from the enumeration, and the count
  read "four" when six MC rows are Minor-judgment after the Gate-1 upgrades. Corrected the trailer
  text and the intro count in §Cross-Boundary Classification, and added the `docs/TODO.md` reference.
- **@code-reviewer F1** — `MediaServerInfo` reserved tag 2 (`connection_token`) by number only; the
  name is not reused, so per the newly-promoted CONVENTIONS §Rule 5 it is a plain omission, not an
  exception. Added `reserved "connection_token";`, which also makes the "exactly two places" claims
  elsewhere in the file true. Additive — no new `buf breaking` finding, count stays 54.
- **@code-reviewer F2 / @dry-reviewer D1 / @code-reviewer F3** — the redacting `Debug` had three
  spellings of the placeholder across two mechanisms (`redact_str` returning a *quoted* `String` for
  tokens; inline `format_args!` for the two `meeting_kek` fields), and the SAFETY INVARIANT comment
  claimed all disclosed lengths are protocol constants when `join_token` is a variable-length JWT.
  Consolidated to one `RedactedLen(usize)` newtype rendering unquoted and identical to
  `frame.rs::WrappedTransmitKey`, routed all four secret fields through it (so `meeting_kek` gets the
  `<empty>` not-yet-provisioned distinction it previously lacked), and corrected the comment to state
  honestly that `join_token`'s length varies and why disclosing it is acceptable, with the stricter
  presence-only rule for the next author. Redaction tests stay green (23/23).
- **@dry-reviewer D2** — `ReceiveSlot.slot_id` disclaimed against `sender_id` but not against
  `SendStream.stream_number` (the reciprocal of that field's own disclaimer). Added the missing
  "this is NOT the 8-bit key-id stream field" sentence next to the `STREAM_ID_FIELD_BYTES` anchor.
- **@dry-reviewer D3** — `API_CONTRACTS.md` re-derived the whole `sender_id` rule and
  `WEBTRANSPORT_FLOW.md` carried a third spelling of the range. Added a "the proto comment is
  normative; this summarises it; the proto wins on disagreement" pointer before the API bullets
  (flagging the `NonZeroU16` clause as the one that will drift) and removed the range re-spelling
  from the flow doc.
- **@dry-reviewer D4** — CONVENTIONS §Rule 5's worked example quoted the real `JoinResponse` case
  but ended `sender_id = 7` (which is `StreamAssignment.sender_id`); the real tag is 8. Corrected —
  a conventions doc teaching "never mis-cite a tag" must not mis-cite one.
- **@operations OPS-2** — `supported_header_versions` credited "story task 14 (allowlist and floor),
  story task 10 (join-time rejection)" for enforcement neither task's prompt carries. Repointed the
  proto comment and its `API_CONTRACTS.md` echo to state the obligation is REQUIRED but NOT YET
  ALLOCATED, tracked in `docs/TODO.md` (filed by ops), latent only while PROTOCOL_VERSION is 2 —
  "do not read as covered". A task must own it before a second header version exists.
- **@test TEST-1..4** — see item 6; RESOLVED-FIXED.

**8. Two Gate-3 findings NOT taken here, surfaced not masked.**
- **@operations OPS-1 (routed to me, but story-owner territory).** Task 10's `prompt:` in
  `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` instructs the implementer to return
  `sender_id` by "repurposing the existing `user_id` field" and to "range-check it to 65535" — both
  contradict the shipped contract (tag 2 reserved, `sender_id` on fresh tag 8; `NonZeroU16`, not a
  bare range check that accepts 0), in the direction that rebuilds the exact hazard the contract
  prevents. It is manifest-tracked story content (ADR-0035 `prompt:` field), whose edit is the
  story-owner's/Lead's call and could affect manifest validation — not a protocol-owned file.
  Surfaced to the Lead as a required pre-task-10 correction; ops has filed a WARNING in
  `docs/TODO.md`. Load-bearing: this is the text the story runner feeds the task-10 implementer.
- **@client CL-1.** `SignalingCodec` is not re-exported from `packages/sdk-core/src/index.ts`, so an
  external consumer gets the public `supportedCodecs` field but cannot import the type — the
  asymmetry the Gate-1 "mirror `ParticipantLeaveReason`" ruling meant to avoid. `index.ts` is a
  client-owned file outside this changeset; deferred to @client (same owner) to fix now or fold into
  task 19, their call.

**9. Gate-3 round 2 — Lead-consolidated rulings (B1–B22), applied.** A second review wave
consolidated ~20 findings from ten reviewers into Lead rulings. Two rulings overrode my round-1
dispositions; both are recorded here rather than silently reversed.
- **B1 (BLOCKING, @observability ESCALATED + @security HIGH).** `MhConnectRequest.join_token` is a
  bearer meeting JWT that was NOT added to `skip_debug` when `JoinRequest` was — same file, same
  crate, same mechanism, and one `?envelope` away from Loki via `mh-service`'s malformed-envelope
  `warn!` arms. Added `.skip_debug(...)`, a `RedactedLen`-based `impl Debug`, and an envelope-level
  redaction test. This is the framing-lock the review protocol names ("the task didn't ask for the
  other instance") — the same lesson as the `JoinResponse`/`StreamAssignment` Rule-R miss in item 2.
- **B4 (@security, @observability).** The redacting `Debug` still leaked PII: `participant_name` in
  the clear, and `existing_participants` fell through to `Participant`'s derived `Debug`, dumping
  every roster member's `name` and raw `identity_public_key`. Fixed: `participant_name` →
  `[REDACTED]` (matching the `jwt.rs` precedent the plan cited), roster → a count. The
  self-inconsistency @observability caught — redacting the *self* key as a length while dumping a
  dozen roster keys — is closed.
- **B2 override, then re-accepted.** The full ruling was "make the code match the comment"
  (presence-only tokens); the delta re-accepted my round-1 "amend the comment" approach as
  @security's stated fallback, verified by @code-reviewer. Kept. If @security rejects the amended
  wording, the fallback is presence-only.
- **B5** one-line `///` pointer on each of the four impls (no triplication) + corrected §Impl-Summary
  item 7. **B7** `kek_generation` gains never-a-per-frame-log-dimension (it IS a frame-header field).
  **B8** `StreamAssignment` cross-refs now include observability. **B9** `supported_codecs` gains the
  cap sentence + task-10 owner its two siblings already had.
- **B14 override (@client CL-1).** Lead ruled fix-now, not defer-to-task-19: `SignalingCodec` is now
  root-exported from `packages/sdk-core/src/index.ts`. Classification row added.
- **B19 override (@operations OPS-1).** Lead ruled option (b): I corrected the task-10 story
  `prompt:` in place (prose only — no task ids, ordering, structure or frontmatter touched). It no
  longer instructs "repurpose `user_id`" or "range-check to 65535"; it now says fresh tag 8 with tag
  2 reserved, and `NonZeroU16`. `dt-story validate` and `validate-story-manifest` pass. Classification
  row added (owner meeting-controller); the sender_id `docs/TODO.md` WARNING is marked RESOLVED.
- **B16/B17/B18** three `docs/TODO.md` entries written (G1-5 credential-leak vocab + TS sink;
  §6.4 two-owner intersection body for the previously-dangling Accepted-Deferral pointer; the five
  metric-label identifiers the `user_id` rename un-guarded). **B21** MC+SDK wire-lockstep line added
  to `docs/runbooks/mc-deployment.md` (MH explicitly excluded), classification row added. **B22**
  TS test added: `meetingKek` now constructible in `JoinResponseInit`, asserted never to reach
  `JoinedEvent`.
- **F5 residual (round-2 re-review, @observability).** After `JoinResponse` was fixed, the same
  `Participant` was still exposed via `ParticipantJoined` (not in `skip_debug`), which rides inside
  `ServerMessage`'s transitive `Debug` on the join fan-out — and this diff is what put
  `identity_public_key` on that struct. Fixed at the type level per @observability's suggested shape:
  `Participant` itself is now in `skip_debug` with a redacting `impl Debug` (`name` → `[REDACTED]`,
  `identity_public_key` → length only, matching the self-key rendering), so both carriers and any
  future embedder inherit the control. New test asserts through the `ServerMessage`/`ParticipantJoined`
  envelope. @observability's verdict drops from ESCALATED to RESOLVED-DEFERRED on this landing.
- **OPS-4 (round-2, @operations).** The B21 runbook line "MH is NOT in the coupled set" is correct
  for this reshape but would silently expire when story task 4 reshapes `internal.proto` (the MC↔MH
  contract). Scoped the claim to "for the `signaling.proto` reshape" and named task 4 as the point to
  revisit — a scoped sentence is a better control than a tracked note for a line already in the file.
  Same failure shape @operations named: guidance true when written with no mechanism to notice when
  it stops being true.
- **B6/B10/B11/B12/B13/B15/B20** were already applied in round 1 (items 7–8). Layer 6 re-measured at
  exactly 54 after all edits; every lower layer green; proto-gen **26** tests (25 + the
  `ParticipantJoined` redaction test).

---

## Lessons Learned

**1. The escalation contract worked, and it worked because the Lead did not talk itself out of it.**
Every ingredient for a self-approved risk acceptance was present: a fully reviewed diff, ten green
verdicts, a single red gate the Lead could argue was intentional, and a headless run with nobody to
ask. Ruling L6-5 was taken at Gate 1 — *before* the sunk cost existed — and that is why it held at
Gate 2. Deciding the escalation policy while the outcome is still hypothetical is cheap; deciding it
while staring at a finished diff is not.

**2. "Predicted 54, measured 54, set-identical" is what made the break auditable — and it only
worked because the prediction was written before implementation and corrected in public when wrong.**
§Expected Layer-State was wrong twice (ErrorCode's 9 renames, StreamAssignment's 5 deletions) and
both corrections are marked rather than silently rewritten. A prediction edited to match the log is
not evidence of anything. The discriminator has to be falsifiable to be worth running.

**3. A control asserted in prose is not a control, and this devloop found three of its own.**
`docs/protocol/CONVENTIONS.md` said `buf breaking` "has no suppression mechanism by design" — false,
because `breaking.sh`'s lockdown covers CLI flags and env vars but says nothing about config keys.
The runbook said an override "does not exist, by design" in two places while one was live. And the
Lead's own §Human Decision section asserted "the narrower alternative does not exist at the tool
level," which @operations disproved in three measured runs. Each was written by someone reasoning
about the mechanism instead of running it. **The generalizable form: when you document that
something is impossible, the claim's scope is whatever you actually tested, never the whole
surface.**

**4. Narrowing beat blanketing, and only measurement showed the difference.** The as-directed
`ignore: dark_tower` and the shipped `ignore: dark_tower/signaling/v1/signaling.proto` are
indistinguishable on this diff — both give exit 0, both look equally fine in a log. They differ only
on a break that *hasn't happened yet*: an injected `internal.proto` change fires under one and is
silently swallowed by the other. A suppression's cost is never visible in the run that introduces
it, which is exactly why "as narrow as the tool allows" has to be a rule rather than a judgment call.

**5. Reviewers edited the tree, and one reported a verification that was false on disk.**
@security's `git checkout -- proto/buf.yaml` reverted the human's uncommitted carve-out, and its
"restored byte-for-byte" claim did not match the tree seconds later (@operations was concurrently
rewriting the same file). The benign reading fits the timestamps, but the lesson does not depend on
intent: **a self-reported verification is not a verification.** The Lead re-ran `git diff` directly,
and every gate result in §Post-Resumption Validation is Lead-measured rather than relayed. The
structural fix is narrower review prompts — asking for findings and a verdict does not stop an agent
from fixing what it finds, so uncommitted human state needs to be committed or copied before a
review fans out.

**6. The scope-drift guard caught the human.** `validate-cross-boundary-scope` red'd on
`proto/buf.yaml` because the 04:19 out-of-band edit wasn't in the plan's classification table — a
true positive against the devloop's own record, from a guard whose usual job is catching an
implementer's stray file. Verified causal by removing and restoring the row. Out-of-band edits during
an escalation are exactly the changes least likely to be reviewed, and the mechanical check was the
only thing that noticed.

**7. `TOTAL_RESULT=N/A` is the pipeline's least readable verdict.** Two layers rendered `N/A` here
for entirely benign reasons (proto's intentional-gap test placeholder; the audit dep-manifest gate),
and the aggregate of five `OK`s and two `N/A`s is `N/A` — a word that reads like "unknown" on a run
where nothing failed and nothing went unmeasured. The distinction that actually matters is `N/A`
(deliberately not applicable) versus `NOT-RUN` (never evaluated), and it is one letter of difference
in a summary block. Worth a `TOTAL_RESULT=PASS-WITH-NA` or similar; recorded here rather than
fixed, since the wrapper contract is ADR-0033's and operations'.
