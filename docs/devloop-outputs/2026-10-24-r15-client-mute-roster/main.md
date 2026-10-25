# Devloop Output: Story 3 R-15 fix — client mute state on the roster for every participant

**Date**: 2026-10-24
**Task**: MC broadcasts ParticipantMuteUpdate on real self-mute transitions (reversing story-2 S-2 per human decision 2026-10-24); join roster carries current client mute; web app derives client mute from roster facts, not slots.
**Specialist**: meeting-controller (paired with client)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: ~2h50m (2026-10-24 22:04 → 2026-10-25 ~00:55)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `07bf04a7968f64654e6772dfb4ee0a59c524d2d6` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (CLEAR) |
| Test | `test` (RESOLVED-FIXED) |
| Observability | `observability` (RESOLVED-FIXED) |
| Code Quality | `code-reviewer` (RESOLVED-FIXED) |
| DRY | `dry-reviewer` (RESOLVED-FIXED) |
| Operations | `operations` (RESOLVED-FIXED) |
| Semantic Guard | not spawned — diff touches no `scripts/guards/semantic/checks.md` surface (mute state, no credential/key material) |
| Paired Client | `paired-client` (RESOLVED-FIXED) |
| Protocol (conditional, GSA `proto/**`) | `protocol` (RESOLVED-FIXED) |

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
Every client shows every participant's client mic and camera mute state, not only for participants one of its slots carries (manual test 2026-10-24: user 5 showed "unknown" to user 1). This reverses story-2 security judgment S-2 per the human decision of 2026-10-24.

### Scope
- **Service(s)**: mc-service, proto (signaling), sdk-core, sdk-svelte, web-app; dt-guard (one threshold); docs (ADR-0036, story file, runbooks, metrics catalog, dashboard)
- **Schema**: No
- **Cross-cutting**: Yes (wire GSA `proto/**`, MC + client)

### Debate Decision
NOT NEEDED. The human decided the S-2 reversal (2026-10-24); the remaining design choices were settled at Gate 1 by protocol (GSA owner), security and the Lead.

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
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` (doc comments) | Mine | — |
| `crates/mc-service/src/webtransport/handler.rs` | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/tests/server_mute_integration.rs` | Mine | — |
| `crates/mc-service/tests/common/media_session.rs` | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment (GSA; security in planning + review per ADR-0024 §6.4) | protocol |
| `docs/decisions/adr-0036-media-flow.md` | Not mine, Minor-judgment (security also reviews the amendment) | protocol |
| `packages/sdk-core/**` | Not mine, Domain-judgment (paired; @paired-client implements) | client |
| `packages/web-app/**` (incl. `e2e/`; `e2e/README.md` includes prettier cleanup) | Not mine, Domain-judgment (paired; @paired-client implements) | client |
| `docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md` | Not mine, Domain-judgment (paired; @paired-client writes, test reviews) | client |
| `crates/mc-service/src/actors/client_mute_fanout.rs` (new) | Mine | — |
| `crates/mc-service/src/client_work_limiter.rs` (new; moved from `connection.rs`) | Mine | — |
| `crates/mc-service/src/lib.rs` | Mine | — |
| `crates/mc-service/src/actors/mod.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/mod.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/outcome.rs` | Mine | — |
| `docs/specialist-knowledge/client/INDEX.md` | Not mine, Minor-judgment (paired; @paired-client edits) | client |
| `crates/mc-service/src/actors/participant.rs` (doc) | Mine | — |
| `crates/mc-service/src/media_kind.rs` (`ClientMuteFacts`, generic `MuteChange::apply`) | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine (doc + new recorder; catalog policy observability) | — |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | Mine | — |
| `crates/proto-gen/tests/signaling_roundtrip.rs` | Not mine, Minor-judgment | protocol |
| `crates/proto-gen/src/lib.rs` (redacting `Debug` for `Participant`: two new fields) | Not mine, Minor-judgment | protocol |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment | observability |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` (check) | Not mine, Minor-judgment | operations |
| `docs/user-stories/2026-10-07-see-each-other.md` (Wire freeze Amendment 5) | Not mine, Minor-judgment | protocol |
| `docs/TODO.md` | Not mine, Minor-judgment | operations |
| `scripts/layer7.test.sh` (flake fix: bounded wait for the stale-forward stand-in's exec before the reap case) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/ts_retained_credentials.rs` (`MAX_DECL_BLOCK_LINES` 2400 -> 3000; guard policy content, widened threshold) | Not mine, Minor-judgment | security |
| `packages/sdk-svelte/**` | Not mine, Domain-judgment (paired; @paired-client implements) | client |

---

## Planning

### Restated mechanism

Instance: "user 1 sees user 5 as mic: unknown". Mechanism: **a client's knowledge of another
participant's client mute depends on whether one of its slots carries that participant**, because
slot state is the only wire carrier of client mute to others. The fix decouples it. Client mute
becomes a roster fact: a snapshot on the roster entry, then deltas via `ParticipantMuteUpdate`.
Server mute already works this way, with a snapshot via the replay and deltas via the broadcast.
Wider class checked: server mute already reaches everyone (no gap), and reachability is
per-subscriber by design (a different class). No other same-owner sibling found.

**Corrected premise (Lead, 2026-10-24):** the brief's "existing bound (R-17)" does not cover a
roster broadcast. R-17's flush bound splits slot-view work across turns, and the per-connection
`mute_limiter` bounds only each sender. A direct broadcast would cost O(N²) per meeting, and MC
has no per-meeting participant cap. The bound below is a new per-meeting instance of the existing
`ClientWorkLimiter` type.

### Wire (GSA; protocol confirmed, security accepts)

- `Participant`: `optional bool audio_client_muted = 7; optional bool video_client_muted = 8;`.
  Present iff MC holds a fact from that participant's own report (Lead ruling 1). Absent = no
  fact; consumers MUST NOT render absent as unmuted and render "unknown" instead. The value is the
  snapshot as of the actor turn that built the entry, and any later `ParticipantMuteUpdate`
  supersedes it. Client mute only. Set on `JoinResponse.existing_participants`, on
  `ParticipantJoined`, and on the (unwired) reconnect result, always from retained roster state,
  never reset.
- `ParticipantMuteUpdate`: FRESH `optional bool audio_client_muted = 7; optional bool
  video_client_muted = 8;`, with `reserved 2, 3; reserved "audio_self_muted", "video_self_muted";`
  and a comment at the site. Fresh tags are used because tags 2/3 change meaning (CONVENTIONS §5),
  so an old MC's stale snapshot can never read as a fact.
  - Set iff known (`Some`) in EVERY update MC emits, the server-mute broadcast and replay
    included. A server-mute frame about a participant with no client report carries them absent.
  - Every message is a full snapshot. A no-op emits nothing.
  - Server-mute changes go to everyone, the subject included. Client-mute changes go to everyone
    except the subject.
  - The subject MUST NOT treat 7/8 on its own copy as a capture command, because client mute is
    enforced locally.
  - Client-mute fan-out is coalesced latest-state-wins under the per-meeting bound (named by
    ADR-0036 §5 amendment 2026-10-24, not by number). Clients MUST NOT infer or count intermediate
    transitions.
  - A rejoin is a new participant. Its fields are absent until its post-join report lands, so the
    window is display-only ("unknown"); capture is local.
  - The R-18 self-copy paragraph is unchanged.
- **Carrier map stated ONCE**, in the Mute-state section header. Client mute: `Participant`, then
  `ParticipantMuteUpdate`. Server mute: the post-JoinResponse replay, then `ParticipantMuteUpdate`
  (`server_muted_by`, R-11). The other comments point there.
- Story file Design §Wire freeze: "Amendment 5 (user, 2026-10-24)". ADR-0036 §4 ("no key
  material on the roster") was checked: mute flags are not key material, so no edit.

### MC (meeting-controller)

1. **`ClientMuteFacts`** (in `media_kind.rs`, beside `ClientMutedKinds`; `[Option<bool>; 2]` laid
   out by `MediaKindLabel::index`): per-kind `Option<bool>` replaces `audio_self_muted`/`video_self_muted`
   on the actor's `Participant` (renamed to `client_muted`; "self" survives only where an
   existing API name requires it). `None` until that participant's own report.
   - Media policy (PAUSED disposition, slot state) reads it through ONE method,
     `ClientMuteFacts::effective() -> ClientMutedKinds` (the only `unwrap_or(false)`), called from
     `Participant::source_mute`. That is today's behaviour.
   - `ParticipantInfo` / `MuteChanged` carry `[Option<bool>; 2]`-shaped fields.
2. **No-op first, at one site, through the ONE merge.** `MuteChange::apply` / `KindDecision` are
   made generic over the per-kind state (`[bool; 2]` for server mute, `[Option<bool>; 2]` for
   client mute), so the unnamed-kind rule (R-17) has one implementation. "Wire transition"
   (`is_unchanged()` on the fact decision) and "media moved" (the `effective()` projection
   differs) are two queries on that one decision. If the fact is unchanged, `handle_self_mute`
   returns before any enqueue or render.
   - `None -> Some(false)` is a wire transition (enqueued for fan-out) and a media no-op (no
     re-render).
   - Media re-renders only when the `unwrap_or(false)` projection moved.
3. **`ClientMuteFanout`**, a new module `actors/client_mute_fanout.rs` (code-reviewer):
   - Holds the per-meeting `ClientWorkLimiter`, a `VecDeque` plus membership set (FIFO; a subject
     already pending keeps its place, with no duplicate), and the wake deadline.
   - API: `enqueue(id)`, `remove(id)`, `pop_ready(now) -> Option<id>`, `next_wake()`. The
     queue/set invariant lives only here.
   - `ClientWorkLimiter` moves from `connection.rs` into a shared `pub(crate)` module, unchanged
     apart from a `next_token_at()` accessor. Both users take `tokio::time::Instant`, so paused
     time drives the tests.
   - Constants (`CLIENT_MUTE_FANOUT_BURST`, `CLIENT_MUTE_FANOUT_REFILL_INTERVAL_MS`) sit beside
     `MUTE_WORK_*`, with a one-line rationale: a security cap with no tuning story, matching the
     file's constants convention. Docs cite them by name.
4. **`flush_client_mute()`** is a thin caller. While `pop_ready` yields a subject, it composes
   `mute_changed(current roster entry)` AT FLUSH TIME (all four facts plus `server_muted_by`) and
   sends it to every OTHER participant present AT FLUSH TIME (so a participant who joined while
   the subject was pending receives it). Sending goes through the ONE existing helper,
   `broadcast_update`, widened to `except: Option<&str>`. The server-mute inline loop becomes
   `broadcast_update(None, ..)` and Joined/Left/Reconnected pass `Some(id)`, leaving one loop
   over the roster.
   - It is called from `handle_self_mute` and from a new select arm on `next_wake()` (the
     `settle_wake`/`kek_wake` pattern).
   - Teardown: the same lifecycle as `settle_wake`/`kek_wake`. The wake is a `sleep_until` future
     built per loop iteration inside `select!`, and the queue is actor state, with no spawned
     task. A meeting end (the `end_cause` break) or graceful shutdown (the cancel arm) leaves the
     loop and drops both, discarding the pending subjects. Nothing can wake after exit. A test
     ends a meeting with a pending queue and asserts a clean exit with no further frames.
   - Await: kept on `send_update` (Lead withdrew ruling 2 for this path). The flush doc says why
     it is accepted. The participant actor's `handle_update` only `try_send`s to the wire stream,
     with drops counted on `mc_participant_outbound_messages_dropped_total
     {payload_kind="participant_update_muted"}`. So the awaited meeting-to-participant hop (mpsc
     200) stalls only on a full participant mailbox (pre-existing), and the per-meeting bucket caps
     what a non-host can drive. No mailbox-hop drop is possible, so no new drop counter is needed.
     A wire-hop drop has no re-sync until that subject's next transition or the observer's rejoin.
5. **Removal sites.**
   - Leave: through `remove_and_broadcast_left`.
   - Server-mute broadcast of S removes S from the queue. The comment states the dependency: the
     server-mute frame carries S's current client facts (a pending S has `Some` for the moved
     kind), so removal loses nothing. Trimming that frame breaks this.
6. **Why not the slot-view flush bound** (ops 1): it splits work across turns but never
   coalesces. A self-mute adds one subject per turn, and the flush runs every turn, so the set
   never grows and the cost stays O(N) per toggle.
7. **Visibility of lag** (ops 2; observability ruling): `mc_media_client_mute_fanouts_total
   {outcome="emitted"|"deferred", key_custody="operator"}`, backed by an `ALL` + `label()` enum
   and zero-initialised. It is kind-free (one fan-out carries all four flags) and goes in the
   catalog's kind-free table. No meeting, participant or sender labels, and no alert.
   - Counting unit: one `emitted` per subject flush, and one `deferred` per subject per flush it
     is still pending. Never per recipient.
   - It does NOT partition transitions (as with `slot_view_emissions`, mc-service.md:887), and it
     does not partition `mc_media_mute_requests_total{applied}`.
   - `emitted` means composed and handed to the awaited sends, not delivered. Per-recipient loss
     is on `payload_kind="participant_update_muted"`.
   - Both outcomes are success. Sustained `deferred` means the bound is delaying facts, not
     losing them.
   - Catalog entry plus a "Client Mute Fan-outs" panel next to the Client Mute panel. The R-30
     rewrite at 780-781 and the panel text name it as the bound's edge.
8. **Ordering** (ops 6): the fan-out is enqueued BEFORE `reconcile_media`/`flush_views`, so the
   roster fact precedes the slot re-emit in the same turn **only when not deferred**. A deferred
   fact can arrive after the slot state, which is harmless because clients take client mute from
   the roster only.
9. **One roster builder.** `roster_entry(&ParticipantInfo) -> Participant` in
   `webtransport/handler.rs`, the module of the one encoder. `connection.rs` already imports from
   `handler.rs`.
10. `#[instrument(skip_all, fields(meeting_id = %self.meeting_id))]` on `handle_self_mute`. No
    per-toggle or per-recipient log; no identity fields.
11. Also fixed in passing: the `mc_messages_dropped_total` catalog label list says `connection`
    where the code has `participant` (observability).

### Old-judgment text sweep (by relationship: S-2, fan-out, mute limiter, `mc_media_mute_requests_total`)

MC (mine):
- `meeting.rs`: `handle_self_mute`, `server_mute_replay`, `mute_changed` and the `handle_server_mute`
  fan-out-cost section.
- `messages.rs:719-726` (`MuteChanged`).
- `participant.rs:551-553`.
- `handler.rs`: the `PAYLOAD_KIND_PARTICIPANT_UPDATE_MUTED` and `encode_participant_update` docs.
- `connection.rs`: ~700 (replay) and ~1601 ("what a dropped report costs": the staleness is now
  room-wide).
- `observability/metrics.rs:1125-1153` (`record_mute_request`).
- `tests/media_client_signaling_integration.rs` ~565.

Finally, a `grep -rn "S-2"` sweep over `crates/mc-service packages/web-app packages/sdk-core proto
docs/decisions` before Gate 2. `meeting_media.rs:551` and the mh-service S-2 are different findings
and stay as they are.

Others (rows in the table):
- `docs/observability/metrics/mc-service.md`: 55-61 (label fix), 772, 780-781, 786, 1048, plus the
  new counter row.
- `infra/grafana/dashboards/mc-media.json`: ~1979 (Client Mute panel), ~1222
  (`participant_update_muted`), plus the new panel.
- `docs/runbooks/mc-incident-response.md`: ~2205, ~2265-2272, ~2290, ~2321-2324, ~3618/~3701-3704.
  The rewrite says: an applied client mute fans out to the roster, bounded per connection by the
  bucket and per meeting by the fan-out bucket. Under churn, indicators LAG and do not diverge,
  with the lag visible on `outcome="deferred"`.
- `docs/runbooks/mh-deployment.md:487`, checked.
- ADR-0036 §5: an "Amendment (2026-10-24, story 3 R-15 fix)" after the 2026-10-23 one. It covers
  S-2's original rationale in one sentence, the human decision, the per-meeting bound, and three
  caveats:
  - (a) members in another's `unreachable_sender_ids` learn that sender's mute timing, a coarse
    voice-activity signal for push-to-talk users;
  - (b) a limiter-dropped report's staleness is now room-wide;
  - (c) under churn, facts are DELAYED (bounded by FIFO plus bucket rate), never lost.
  It also records: no identity-bearing per-toggle log or metric.
- REVERSED marker — **lands in two follow-up commits by the Lead, one historical main.md per commit** (Lead ruling 2026-10-24: a diff touching more than one devloop main.md fails `validate-cross-boundary-scope` with `multi-devloop-collision`; a single-main.md commit is the guard's `effective-diff-all-exempt` record-keeping path). Patches: `/tmp/claude-1000/-work/4a4befac-a19a-495e-8563-7f4939000f11/scratchpad/reversed-marker-0903.patch`, `/tmp/claude-1000/-work/4a4befac-a19a-495e-8563-7f4939000f11/scratchpad/reversed-marker-0927.patch`. Security's exact text, originals byte-preserved:
  `2026-09-03-mc-client-media-signaling/main.md` (before the :1722 row, and at the start of the
  :1830 S-2 sentence), and `2026-09-27-mc-server-mute-teardown/main.md` (:300, :456 O-14).
- `docs/TODO.md`: "MC has no per-meeting participant cap; the per-meeting cost of client-driven
  slot-view re-emission is bounded only per connection". Owner meeting-controller + operations;
  first observed in S-2, 2026-09-03. This tracks pre-existing out-of-diff work and is not a
  deferral of this diff.
- Client (paired-client):
  - `SignalingClient.ts` ~720 and the `events.ts` header;
  - `participantState.ts` header;
  - `ParticipantRow.svelte:5-7` and `InMeeting.svelte` ~29/355;
  - the `joinMeeting.test.ts:235` name;
  - `server-mute.spec.ts:105`, which now expects the real fact.

### Client (@paired-client implements; agreed)

- **SDK.** ONE decode mapper for 7/8 on the roster and the update. ONE home: the
  `ParticipantCell` in `sdk-svelte` `MediaStore.svelte.ts`, written through one setter
  (`setClientMute`), seeded from `existingParticipants`/`participantJoined` and updated by
  `participantMuteChanged`. There is no second map in `signalingMediaState.ts` or on
  `RosterParticipant`.
- **Fact rule.** The latest PRESENT value wins; absent = no change; none yet = `unknown`.
  `ParticipantLeft` clears the fact.
- **Post-join re-report.** On entering Joined, the session sends one `MuteRequest` per
  currently-muted kind.
- **web-app.** `clientMuteFrom(fact | undefined) -> token` is pure, with no slot input; the
  `avatar_client_muted` branch is removed. The client indicator shows the real fact even while
  that kind is server-muted. Server indicators come only from the server fields, and slot copy is
  unchanged.
- **e2e and manual test plan.** A new `client-mute-roster.spec.ts`, plus the manual test plan
  step mirroring the 5-participant repro (user 5 as seen by user 1); I review it.
- No client metric derives client mute from slot state.

### Tests (MC): ordering and fences only, no sleeps or wall-clock bounds

1. **`client_mute_fanout.rs` unit tests:**
   - FIFO order;
   - a re-toggle keeps its place (no duplicate);
   - remove-on-leave and remove-on-server-mute clear both structures;
   - `pop_ready` respects the bucket;
   - `next_wake` equals the next token.
2. **Starvation (security 1):** A and B churn continuously and C toggles once. C is emitted within
   ≤ len(pending) pops. This test goes red against BTreeSet ordering.
3. **Actor test (replaces `no_self_mute_report_is_broadcast_to_other_participants`):**
   - a no-op on A, then a real audio change, then a video-only change;
   - B's FIRST `MuteChanged` is the real one (`Some(true)`, video unchanged/`None`), and the next
     is the video-only change with audio unchanged;
   - fence: B then self-mutes, and the first message A receives after its baseline is B's update,
     so A got no copy of its own.
4. **Wire (`server_mute_integration.rs` rig):**
   - (a) a `MuteRequest` from A produces an update at B with the client fields set, the server
     flags false, and `server_muted_by` empty;
   - (b) an identical repeat (`outcome=unchanged`) then a fence: B's next update about A is the
     fence's;
   - (c) security 2: the host server-mutes A's audio, then A client-mutes video, and B's frame
     carries `audio_server_muted=true`, `video_client_muted=Some(true)` and the right
     `server_muted_by`;
   - (d) a self-mute and a server-mute in consecutive turns: the last frame reflects both;
   - (e) `MuteRequest` has no participant field, so the subject is always the connection's
     participant (pinned).
5. **Churn / bound, at ACTOR level** (`MeetingActorHandle` plus test channels, `tokio::time::pause`;
   no rig and no QUIC, so auto-advance cannot fire I/O timeouts):
   - K subjects toggle above the meeting bucket;
   - at each observer, updates per subject number STRICTLY fewer than the real transitions
     written (coalescing happened; this goes red by itself under burst = `u32::MAX`), and ≤
     `CLIENT_MUTE_FANOUT_BURST` before any refill;
   - `deferred` > 0 under the real burst and = 0 under burst = `u32::MAX`;
   - starvation-free: every observer receives ≥ 1 update for EVERY subject;
   - no loss: observer channel capacity exceeds the burst, and each channel receives every
     emitted frame (no drops);
   - convergence: final frames are reached by advancing paused time to `next_wake`, then a fence,
     and each subject's LAST frame at every observer equals MC's final state;
   - perturbation: with the burst at `u32::MAX`, confirm the edit landed (grep plus file hash),
     see the test go red, restore it, and confirm `git diff` is clean. Reported at review.
6. **Join during pending coalesce (ops 3):** S is pending, then J joins. J's roster snapshot
   carries S's current fact, and the later flush reaches J (sent to participants present at flush
   time) with the same state.
6a. **Leave while pending (actor level):** with the bucket exhausted, A is pending, A leaves, then a
    fence. At B, nothing about A arrives after its `ParticipantLeft`.
6b. **Server-mute dedupe:** with S pending, the host server-mutes S, then the clock advances past
    `next_wake` and a fence. No client-only frame for S follows the server-mute frame, and S's last
    frame reflects both halves.
7. **Join storm (security 6):** each joiner's post-join `None -> Some` report goes through the
   per-meeting bucket (`deferred` ≥ 1 under a join burst above the bucket).
8. **Roster:**
   - a late joiner sees muted A as `Some(true)`/`Some(false)` and reported-unmuted B as
     `Some(false)`/`Some(false)`;
   - a never-reported participant is ABSENT on `existing_participants` and on `ParticipantJoined`;
   - a first `false` report (wire level, so the connection cache's default is exercised and the
     report reaches the actor) fans out to B, and is a media no-op: no MH push and no
     `policy_generation` advance before a fence (the `an_identical_client_toggle` pattern).
9. **Teardown:** a meeting ends with a pending queue, and the actor exits cleanly.
10. **`handler.rs` unit:** `roster_entry` and the `MuteChanged` encode carry `Some` and absent
    faithfully, and never emit tags 2/3 (byte check).
11. **`crates/proto-gen/tests/signaling_roundtrip.rs`** (mine; protocol reviews): a `Participant` and a
    `ParticipantMuteUpdate` with `Some(false)`, `Some(true)` and `None`, where `Some(false)`
    survives as `Some(false)`.

---

## Gate 1 Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Paired Client | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Protocol | confirmed |

Lead rulings (2026-10-24): (1) the roster carries no unmuted default before the participant's own report, and the SDK re-reports mute after join; (2) bound: the brief's premise ('existing bound R-17') was wrong, because no existing bound covers a per-meeting roster broadcast. I first ruled for a plain broadcast plus a TODO deferral, then reversed that the same day once security and ops accepted the implementer's per-meeting coalesced fan-out (existing `ClientWorkLimiter` type, FIFO pending queue, latest-state-wins). The non-blocking-send requirement was withdrawn for this path; (3) wire shape is protocol's call: fresh optional tags 7/8 on Participant and ParticipantMuteUpdate, tags 2/3 reserved.

## Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Conditional on the follow-up marker commits landing exactly the checked patches (sha256 recorded in the Lead scratchpad) |
| Test | RESOLVED-FIXED | 3 | 3 | 0 | Leave-while-pending and teardown positive controls; e2e video re-check. Covers the design of the browser specs, not a Layer-7 pass |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | Scoped never-lost; applied counting shift; fan-out wording; roster-miss WARN. not_published verified |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | Roster-miss assert+warn; single counter increment; retired-tags test through the encoder; imports |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | Derived fan-out constants; ClientMuteFacts cache; one e2e attribute map |
| Operations | RESOLVED-FIXED | 5 | 5 | 0 | metrics.rs doc; rollback item 7; "unknown" triage; never-lost scoped; counting-shift note |
| Protocol | RESOLVED-FIXED | 4 | 4 | 0 | Snapshot wording; Amendment 5; post-join obligation; publish-kinds text + MC not_published gate. buf lint/breaking/format OK |
| Paired Client | RESOLVED-FIXED | 1 | 1 | 0 | The post-join report now covers only the published kinds (an unpublished kind stays unknown) |

## Lead Notes

- INDEX/review-protocol injection: teammates were told to read `docs/specialist-knowledge/{name}/INDEX.md` and `.claude/skills/devloop/review-protocol.md` as their first action, not given the contents pasted into the prompt (a deviation from SKILL.md Step 3, made to save context; same content).
- Gate 1: all 8 confirmed; classification guard passed after Lead fixed the proto row Owner cell (`protocol`; security noted in the Classification column).
- Lead fast check #1 (L1-6 attempt 1/3): FAIL at Layer 3, `scripts/layer7.test.sh` `[mh-reap-reported]`. Not diff-caused (no scripts in the diff) and passed 2/2 reruns; the cause is a race between the backgrounded `exec -a kubectl` and the reaper's /proc scan. Routed to the implementer as an in-tree fix (fix-don't-defer).
- Lead fast check #2: TOTAL_RESULT=OK (L1-6), after the layer7.test.sh race fix.
- Scope note (not a finding deferral): a kind the participant does not publish (an audio-only camera, or both kinds for a receive-only participant) reads 'unknown' to peers. That is truthful, since there is no fact. Showing 'not publishing' needs the publish set on the roster (`Participant.streams` is sent empty today), which is a separate, task-sized MC + client + wire-semantics change. Left for the human to schedule.
- Panel: implementer (meeting-controller), paired-client, security, test, observability, code-reviewer, dry-reviewer, operations, protocol (conditional + GSA owner for `proto/**`). Semantic guard not spawned.

## Pre-Work

None

---

## Implementation Summary

### Perturbation of the per-meeting bound (recorded per security/test Gate 1)

`a_client_mute_storm_is_coalesced_per_meeting_and_converges` (actor level, paused time):
1. Edit `CLIENT_MUTE_FANOUT_BURST = 16` -> `u32::MAX` in `actors/client_mute_fanout.rs`; landed confirmed by grep plus sha256 (`7c64a0c2…` -> `f2e1326e…`). Test RED on its positive control: "positive control: the per-meeting bound engaged".
2. Same edit, with the positive-control assert also neutralised: test RED on the coalescing assert by itself: "o1 got 11 updates for s1's 11 transitions: nothing coalesced".
3. Both files restored from byte copies; sha256 back to `7c64a0c2…`; the full suite is green afterwards.

### Wire (`proto/dark_tower/signaling/v1/signaling.proto`)
| Item | Before | After |
|------|--------|-------|
| `Participant` client mute | absent | `optional bool audio_client_muted = 7`, `video_client_muted = 8`, set iff MC knows |
| `ParticipantMuteUpdate` self-mute | `audio_self_muted = 2`, `video_self_muted = 3` (informational) | tags/names 2, 3 reserved; fresh optional `*_client_muted = 7/8` |
| Client obligation | none | report every kind in `publish_kinds` after join, explicit `false` included |

### MC
- Real self-mute transitions fan out to every other participant through a per-meeting `ClientMuteFanout` (`actors/client_mute_fanout.rs`): FIFO pending queue, latest-state-wins composed at flush, burst/refill derived from `MUTE_WORK_*`. `ClientWorkLimiter` moved to `client_work_limiter.rs` (shared).
- Client mute is held per kind as `ClientMuteFacts` (unknown until reported); one `roster_entry` serves the join response and `ParticipantJoined`; one `broadcast_update(except)` loop; generic `MuteChange::apply`.
- A report for an unpublished kind is ignored before the token and counted `not_published`.
- New metric `mc_media_client_mute_fanouts_total{outcome=fanned_out|awaiting_token}`; `not_published` outcome on `mc_media_mute_requests_total`.

### Client
- The SDK decodes the new optional fields (absent stays absent); it reports published kinds after join and gates every report on the publish set (`bestEffortMuteReport`).
- Client mute lives in one `ParticipantCell` (`setClientMute`); `participantState.ts` reads only that cell, and the slot derivation is deleted.
- New e2e `client-mute-roster.spec.ts`; `server-mute.spec.ts` expectation updated; manual test plan step 4i.

### Additional Changes
- ADR-0036 §5 dated amendment (2026-10-24) keeping prior text; story Design §Wire freeze Amendment 5; runbook, catalog and dashboard sweep of the old S-2 claims; `docs/TODO.md` entry for the missing per-meeting participant cap (pre-existing slot-view gap).
- `dt-guard` `MAX_DECL_BLOCK_LINES` 2400 → 3000 (security-approved; `MeetingSession` grew to 815 lines).
- `scripts/layer7.test.sh` race fix in `[mh-reap-reported]` (found by the Lead's fast check).
- REVERSED 2026-10-24 markers on S-2 in the 2026-09-03 and 2026-09-27 outputs land as two follow-up commits (scope guard: one main.md per diff).

---

## Files Modified

```
 crates/dt-guard/src/ts_retained_credentials.rs     |   6 +-
 crates/mc-service/src/actors/meeting.rs            | 788 ++++++++++++++++-----
 crates/mc-service/src/actors/messages.rs           |  32 +-
 crates/mc-service/src/actors/mod.rs                |   1 +
 crates/mc-service/src/actors/participant.rs        |  12 +-
 crates/mc-service/src/lib.rs                       |   1 +
 crates/mc-service/src/media_kind.rs                | 152 +++-
 crates/mc-service/src/media_signaling/mod.rs       |   8 +-
 crates/mc-service/src/media_signaling/outcome.rs   |  54 +-
 crates/mc-service/src/observability/metrics.rs     |  88 ++-
 crates/mc-service/src/webtransport/connection.rs   | 169 +----
 crates/mc-service/src/webtransport/handler.rs      | 199 ++++--
 crates/mc-service/tests/common/media_session.rs    |  30 +-
 .../tests/media_client_signaling_integration.rs    |  43 +-
 crates/mc-service/tests/server_mute_integration.rs | 178 ++++-
 crates/proto-gen/src/lib.rs                        |   4 +
 crates/proto-gen/tests/signaling_roundtrip.rs      |  59 ++
 docs/TODO.md                                       |   1 +
 docs/decisions/adr-0036-media-flow.md              |  47 ++
 docs/observability/metrics/mc-service.md           |  43 +-
 docs/runbooks/mc-incident-response.md              |  72 +-
 docs/runbooks/mh-deployment.md                     |   3 +
 docs/specialist-knowledge/client/INDEX.md          |   4 +-
 .../meeting-controller/INDEX.md                    |   5 +-
 .../2026-10-07-see-each-other-manual-test-plan.md  |  32 +-
 docs/user-stories/2026-10-07-see-each-other.md     |   6 +
 infra/grafana/dashboards/mc-media.json             | 179 ++++-
 packages/sdk-core/src/index.ts                     |   1 +
 packages/sdk-core/src/session/MeetingSession.ts    |  33 +-
 .../src/session/__tests__/meeting-session.test.ts  | 126 +++-
 packages/sdk-core/src/session/mediaWiring.ts       |  16 +-
 packages/sdk-core/src/signaling/SignalingClient.ts |  51 +-
 .../sdk-core/src/signaling/__tests__/helpers.ts    |  50 +-
 .../src/signaling/__tests__/mediaSignaling.test.ts |  82 ++-
 packages/sdk-core/src/signaling/events.ts          |  51 +-
 .../sdk-svelte/src/__tests__/MediaStore.test.ts    | 103 +++
 .../helpers/CellGranularityHarness.svelte          |   2 +-
 .../sdk-svelte/src/stores/MediaStore.svelte.ts     |  24 +-
 .../sdk-svelte/src/stores/MeetingStore.svelte.ts   |   5 +-
 .../sdk-svelte/src/stores/mediaCells.svelte.ts     |  42 +-
 packages/web-app/e2e/README.md                     |  84 ++-
 packages/web-app/e2e/fixtures.ts                   |  88 +++
 packages/web-app/e2e/receiveEvidence.ts            |   5 +-
 packages/web-app/e2e/server-mute.spec.ts           |   6 +-
 packages/web-app/src/__tests__/inMeeting.test.ts   |   4 +-
 packages/web-app/src/__tests__/joinMeeting.test.ts |  23 +-
 .../web-app/src/__tests__/participantState.test.ts | 213 ++++--
 packages/web-app/src/lib/participantState.ts       |  75 +-
 packages/web-app/src/views/InMeeting.svelte        |   4 +-
 packages/web-app/src/views/ParticipantRow.svelte   |   4 +-
 proto/dark_tower/signaling/v1/signaling.proto      |  82 ++-
 scripts/layer7.test.sh                             |  14 +
 52 files changed, 2648 insertions(+), 756 deletions(-)
?? crates/mc-service/src/actors/client_mute_fanout.rs
?? crates/mc-service/src/client_work_limiter.rs
?? packages/web-app/e2e/client-mute-roster.spec.ts
```

### Key Changes by File
See Implementation Summary and the Cross-Boundary Classification table (every file listed there).

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → **TOTAL_RESULT=OK** (1816s). L1 OK 6s, L2 OK 5s, L3 OK 163s, L4 OK 154s, L5 OK 1s, L6 OK 5s, L7 OK 1482s (`env-tests-passed`, `browser-e2e-passed`: Playwright 30 passed, including `client-mute-roster.spec.ts` and all `server-mute.spec.ts` cases). Log: Lead scratchpad `gate2-1.log`.

Earlier: Lead fast check #1 FAILED at L3 (`layer7.test.sh` race, fixed); fast check #2 OK.

The browser run logged a pre-existing `PeriodicExportingMetricReader ... 401` console error from the e2e telemetry exporter. It is not a test failure and not from this diff.

---

## Code Review Results

See § Gate 3 Verdicts (table) for counts. Summary:
- **Security**: CLEAR, 0 findings; conditional on the two marker commits applying exactly the checked patches.
- **Test**: RESOLVED-FIXED, 3/3 (leave-while-pending and teardown positive controls; e2e rendered-video re-check).
- **Observability**: RESOLVED-FIXED, 4/4 (scoped never-lost; applied counting shift; fan-out wording; roster-miss WARN).
- **Code Quality**: RESOLVED-FIXED, 4/4.
- **DRY**: RESOLVED-FIXED, 3/3. Extraction opportunities: None (nothing added to docs/TODO.md).
- **Operations**: RESOLVED-FIXED, 5/5.
- **Protocol**: RESOLVED-FIXED, 4/4; buf lint, breaking and format OK.
- **Paired Client**: RESOLVED-FIXED, 1/1 (post-join report limited to published kinds).
- **Semantic Guard**: not spawned (no checks.md surface in the diff).

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: partial reverts are SAFE along the client/server seam, UNSAFE inside MC's fan-out.
   - **MC alone or client alone: safe.** A new client on an old MC sees the optional fields absent and shows "unknown" (the pre-fix behaviour). An old client on a new MC ignores tags 7/8, and its self-mute frames carry unchanged server flags. Capture enforces client mute either way, so there is no privacy impact.
   - **UNSAFE partial: the fan-out without its bound.** Reverting `actors/client_mute_fanout.rs` / the per-meeting bucket while keeping the self-mute fan-out in `handle_self_mute` rebuilds the O(N²) unbounded fan-out S-2 objected to. The fan-out, its bucket and the `flush_client_mute` wake are ONE unit and revert together, or not at all.
   - **Degraded but safe: the SDK post-join report alone.** Reverting it while MC keeps None-until-reported leaves joiners "unknown" to peers until their first toggle.
   - The two REVERSED-marker follow-up commits are record-only and revert independently. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Brief premise "bounded by an existing bound (R-17)" was false
**Problem**: no existing bound covers a per-meeting roster broadcast, so per-connection limiting alone gives O(N²).
**Resolution**: a per-meeting `ClientMuteFanout` built on the existing limiter type. After a Lead reversal the same day; see the Lead rulings.

### Issue 2: Wrong "unmuted" at join
**Problem**: a definite default on the roster before the participant's own report would show "mic on" wrongly.
**Resolution**: set-iff-known optional fields, and the SDK reports published kinds after join.

### Issue 3: Crossed Lead messages on the dt-guard cap
**Problem**: the extraction and the cap raise were both applied and reverted while messages crossed.
**Resolution**: the tree was frozen at cap 3000 with the method in-class; security approved.

### Issue 4: Multi-main.md scope-guard collision
**Problem**: the REVERSED markers in two historical outputs tripped `cross-boundary-multi-devloop-collision`.
**Resolution**: each lands as its own main.md-only follow-up commit (the guard's record-keeping path).

---

## Lessons Learned

1. Check a brief's "existing bound" claim in code at Gate 1: here it was wrong, and security caught it.
2. Lead rulings sent while teammates are already acting cross easily; check the tree and freeze its state rather than issuing a counter-ruling.
3. "Unknown until a fact arrives" needs a gate at every write path (MC, the SDK post-join report, the SDK toggles) or a stray default reappears.

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
