# Devloop Output: MC KEK lifecycle — leave-debounced rotation, reconnect re-issue, sender-id rotate-and-reissue

**Date**: 2026-09-26
**Task**: story 2 (hear-each-other) task 9 — KEK lifecycle in MC (R-12..R-16, R-26; ADR-0036 §4)
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present; paired-with client + protocol (S-1 ruling A')
**Branch**: `feature/hear-each-other`
**Duration**: ~6h (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9e416233c3ddd3f520bb9c9210a768477f7ad7aa` |
| Branch | `feature/hear-each-other` |
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
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

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
{What was the goal of this task?}

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
| `crates/mc-service/src/media_admission/kek.rs` | Not mine, Domain-judgment | security |
| `crates/mc-service/src/media_admission/epoch.rs` | Not mine, Domain-judgment | security |
| `crates/mc-service/src/media_admission/rotation.rs` | Mine | — |
| `crates/mc-service/src/media_admission/mod.rs` | Mine | — |
| `crates/mc-service/src/media_admission/sender_id.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/participant.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |
| `crates/mc-service/src/errors.rs` | Mine | — |
| `crates/mc-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mc-service/src/media_routing/generation.rs` | Mine | — |
| `crates/mc-service/src/webtransport/handler.rs` | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/config.rs` | Mine | — |
| `crates/mc-service/src/main.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/src/grpc/media_coordination.rs` | Mine | — |
| `crates/mc-service/tests/**` | Mine | — |
| `crates/mc-test-utils/src/kek.rs` | Mine | — |
| `crates/mc-test-utils/src/lib.rs` | Mine | — |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` | Not mine, Minor-judgment | test |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | operations, observability |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (comment only, retired-precedent citation) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/src/session/mod.rs` (comment only, @media-handler's own edit: reissue residual rewritten to the `HandedOutBindings` mechanism, Gate-3 F-1) | Not mine, Minor-judgment | media-handler |
| `infra/grafana/dashboards/mc-overview.json` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `scripts/guards/semantic/checks.md` | Not mine, Minor-judgment | semantic-guard, security |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `infra/services/mc-service/configmap.yaml` | Not mine, Minor-judgment | infrastructure |
| `docs/TODO.md` | Mine | — (shared: @paired-client adds the roster-removal delivery residual, owner meeting-controller + client) |
| `docs/user-stories/2026-09-21-hear-each-other.md` | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `packages/sdk-core/src/media/frame/receivePath.ts` | Not mine, Domain-judgment | client, security |
| `packages/sdk-core/src/media/setup/kekSource.ts` | Not mine, Domain-judgment | client, security |
| `packages/sdk-core/src/media/setup/rosterKeys.ts` | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/frame/sframe.ts` (invariant comment) | Not mine, Domain-judgment | client, security |
| `packages/sdk-core/src/media/frame/keyId.ts` (doc comment only) | Not mine, Minor-judgment | client |
| `docs/observability/metrics/client.md` (`wrap_generation_conflict`) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/frame/__tests__/**` (vitest) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/**` (vitest) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/setup/__tests__/**` (vitest) | Not mine, Domain-judgment | client |
| `docs/decisions/adr-0036-media-flow.md` (§4 monotonicity) | Not mine, Domain-judgment | protocol, security |
| `proto/dark_tower/signaling/v1/signaling.proto` (comment only, :429, :430, :483, :715/:720) | Not mine, Domain-judgment | protocol |
| `proto/dark_tower/internal/v1/internal.proto` (comment only, :1241) | Not mine, Domain-judgment | protocol |
| `crates/media-protocol/src/frame.rs` (doc comment only, ~:608) | Not mine, Domain-judgment | protocol |
| `crates/media-vector-gen/src/kid.rs` (doc comment only, :47) | Not mine, Minor-judgment | protocol |
| `docs/devloop-outputs/_template/main.md` (§Rollback: safe-revert-unit prompt, Lead edit per OPS-Q follow-up) | Not mine, Minor-judgment | infrastructure |
| `docs/decisions/adr-0011-observability-framework.md` (@observability's own edit: ADR-0031 supersession note on the alert-ownership row) | Not mine, Minor-judgment | observability |
| `docs/devloop-outputs/2026-09-26-mc-kek-lifecycle/evidence/**` (@paired-client's red-before-green mutation evidence) | Not mine, Mechanical | client |

GSA note (proto rows): the Owner column lists the manifest owner (`protocol`); @security co-signs both `proto/**` rows under the ADR-0024 §6.4 intersection rule (crypto-invariant content) and was in planning for both.

GSA note: `kek.rs` gains a NEW call site of an ADR-0027-approved primitive (`ring::rand::SystemRandom` via `MeetingKeyState::rotate`). ADR-0024 §6.4's crypto-primitive criterion is path-independent, so that row is Guarded regardless of path. A GSA edit is at least Domain-judgment (§6.4; review-protocol worked example #3 — GSA disallows Mechanical and is stronger than Minor-judgment), so the row is **Domain-judgment, owner `security`** (upgraded from Minor at Gate 1 on @code-reviewer's catch; a Minor→Domain monotonicity upgrade per §6.2). The bar is already met: security is on the panel, was in planning (S-1, the §Security Decisions table) and reviews the diff. `configmap.yaml` edits are comment corrections only (task-9 premises the file states and this diff falsifies; no key/value change) — owner `infrastructure`, not on the panel; the Lead ruled Minor-judgment, review-only, with operations sufficient.

Under the Lead's (A') ruling the sdk-core rows are implemented by @paired-client and the ADR/proto rows by @paired-protocol, both in-loop (ADR-0024 §6.3 — cross-boundary is done in-loop, not spun out). `proto/**` is a GSA, but the edit there is a COMMENT encoding of the monotonicity invariant — no field, tag, or semantic change — so the wire contract is untouched. `docs/decisions/adr-0036-media-flow.md` §4 is the decision record the receiver comment cites, which is why it is Domain-judgment with security co-owning.

---

## Planning

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
| Paired Client | confirmed |
| Paired Protocol | confirmed |

Lead rulings: S-1 → (A') (client-only receiver scoping, no wire change; R-16 ships whole); Scenario 8 root-cause 11 → (a) minimal dated marker now, full rewrite in task 18; configmap.yaml comments → Minor-judgment, operations review sufficient. Classification guard: `STATUS=OK` (proto Owner cells normalised to manifest owner `protocol`, security co-sign recorded in GSA note).


### Restated mechanism

Instance framing: "rotate the KEK on leave". Mechanism: **every change to the set of entitled KEK holders must end, within a bounded time, with every remaining holder on a key the departed holders never saw. It must also be visible when that stops being true.** Same-owner siblings that this wider class covers are the reconnect path (an entitled holder re-entering must get the CURRENT key, `ReconnectResult`) and the join path (W on `JoinResponse`). Both are already in scope. The class also names one sibling outside MC's reach: the receiver-side per-sender state that `sender_id` reissue invalidates (security S-1). Under the Lead's (A') ruling that sibling is completed in this devloop by a paired `client` specialist rather than held — so the invariant lands whole, across both services.

### S-1 resolution: (A'), ruled by the Lead — R-16 ships whole

Security S-1 (agreed, and narrowed): after a KEK-epoch reset a reissued `sender_id` X is bound to a new joiner, but every incumbent still holds per-(sender, stream) replay state for X from X's **departed** previous holder — the transmit-key generation high-water mark in sdk-core `ReplayWindow`, which `purgeSender` deliberately does not touch and only `clear()` at teardown resets. The new holder starts at transmit-key generation 0, so incumbents would silently drop its frames as replays, permanently.

Narrowing verified with @security, who withdrew S-1 points 2 and 3: `SignalingClient.ts:585` calls `rosterKeys.left(...)` on `ParticipantLeft` and `rosterKeyFeed.ts:51,75` call `sink.remove(...)`, so a reissued id is a **first binding**, not a rebind. The rebind path and the `rebind` counter's reads-zero-forever contract are NOT falsified. **Point 1 is the live defect** and is the whole of what (A') fixes.

**The Lead ruled (A'): client-only receiver scoping, no wire change.** Per-sender receiver state is scoped by the KEK generation the cached transmit key was **unwrapped under**, not by `sender_id` alone. Work is split so it fits: @paired-client owns the sdk-core half, @paired-protocol owns the ADR-0036 §4 amendment and the `signaling.proto:429` comment, and I keep MC, telemetry, alerts, the env-test and the full-space integration test.

**Soundness, as I verified it** (@security reached the same conclusion independently). A replayed frame's key id and wrap are both covered by the signature, so it always resolves to the generation it was minted under. It lands in the **old** scope, whose bitmap still holds it; once that generation leaves retention it is rejected earlier still as `kek_generation_stale`. No gap. Ordering is self-correcting: a new joiner wraps under the NEW generation, and an incumbent that has not yet installed that KEK cannot unwrap at all (`no_kek_for_generation`, a counted transient), then lands in the new scope once it installs.

**MC↔client contract: NO proto change, verified.** The Lead asked me to stop and escalate if reissue visibility needed one. It does not. `ParticipantJoined` carries a full `Participant`, which already has `sender_id` and `identity_public_key` (`signaling.proto:654`), and `rosterKeyFeed.joined()` already handles a participant reappearing under a different sender id (`:51`). The generation bump arrives on the existing `MeetingKekUpdate`. Nothing new on the wire.

**Binding conditions from the Lead, all in this diff:**
- **Provenance is structural.** The scope generation is stored ON the cached transmit key at unwrap time, never recomputed from "current" at admit time. (@paired-client)
- **C-1, STRENGTHENED (@paired-client's finding, accepted — "among live members" was too weak).** The needed invariant is: *within one KEK generation, a `sender_id` is bound to at most one identity EVER* — not merely among concurrently-live members. Their counterexample is real: a carried-over member holding id 7 leaves mid-epoch, its rotation is debounced up to W so the generation does not advance, and a liveness-**re-checked** skip set would let the cursor reissue 7 under the same generation — the new holder lands in bucket (g, 7) whose high-water still holds the old holder's txgen. That is S-1 again, reintroduced by the fix.

  So: **the skip set is captured ONCE at reset (every id bound at that moment, live AND grace) and is PERMANENT for the epoch — never re-checked for liveness as the cursor passes.** The reciprocal comment names a liveness query in the allocation path as the **PROHIBITED** implementation rather than only describing the correct one (@security's D-1 supersession), because the failure it causes is not the S-1 blackout but **wrap-nonce reuse**: a new holder's transmit generation 0 reusing the departed holder's key ids under the SAME KEK. Combined with the cursor staying monotonic within the epoch, no id bound at reset time and no id released during the epoch is ever reissued before the next generation bump. Incumbents keep their ids; only ids that were already free at reset time are reused. An MC test covers exactly their case: a carried-over member leaves after the reset, the cursor passes its id, and the id is NOT reissued.

  This also answers @dry-reviewer's MC-side finding, which read a stale draft: the allocator is NOT "a fresh allocator starting at 1". Roster-wide reissue is neither required nor safe — @paired-protocol confirms no message carries a live participant its own new `sender_id` (`JoinResponse.sender_id` fires only at join), so renumbering incumbents WOULD be a wire change and an escalation. Skip-set-plus-monotonic-cursor satisfies C-1 with no wire change.
- **C-2** is pinned in three places: the atomic type's rustdoc, the ADR-0036 §4 amendment (@paired-protocol), and a reciprocal receiver comment. `receivePath.ts:240-257` is **rewritten, not amended** — its stated job (survive LRU eviction ACROSS generations) moves to KEK retention (≤30 s), a tighter but different mechanism. Within a scope the high-water keeps its original job. (@paired-client)
- **C-3:** scope disposal hangs off `MeetingKekHolder`'s single expiry event (`kekSource.ts:281`), never a parallel timer. @security's refinement: disposal is "never earlier AND never later" — never earlier is the security half (a scope dropped while its KEK is retained admits a replayed in-retention frame into an empty window); never later is the memory half (one leaked scope per W is a slow exhaustion any participant can drive by leaving repeatedly). (@paired-client)
- **`rotate()` saturates fail-closed at `u16::MAX`, never wraps.**

**Consequence: all 13 `sender_id_space_exhausted` encodings retire in this diff**, per the original task prompt. See §Retirement sweep.

### Four residual items on the allocator (@dry-reviewer, via the Lead)

1. **`sender_id.rs:23-29` — the high-watermark rationale names a dead remedy.** It currently argues the warning is "**Not** an action threshold: the remedy at 90% and at 100% is identical (end and restart the meeting), so acting early only disrupts a meeting that still works." Under (A') 100% is no longer a remedy at all — it is an automatic epoch reset. Rewritten: the watermark now *precedes a recoverable reset*, so it becomes a genuine leading indicator whose action is to investigate the driver (flapper or scripted join loop), not to end the meeting. It stays a compile-time constant selecting no behaviour. This is the same fact @operations had me state in the Scenario 8 marker, so the two must agree.

2. **`SenderIdSpaceExhausted` is REWRITTEN, not deleted** — D-3 keeps it reachable. The epoch reset fails closed if the skip set leaves nothing allocatable, and that is the one surviving path to this error. So its doc changes meaning entirely: it no longer means "the meeting consumed its namespace and is terminal" but "an epoch reset could not find an allocatable id". Reachability is worth stating honestly: it needs all 65,535 ids simultaneously bound (live + grace), which `MC_MAX_PARTICIPANTS` makes unreachable in practice — so it is a defensive fail-closed rather than an operational condition, and the doc says so rather than implying an incident an operator might see. The name still reads correctly for that meaning and is kept; renaming it would churn the error taxonomy for no gain.

3. **What `issued()` / `remaining()` / `mc_meeting_sender_ids_issued_max` measure now.** `remaining()` derives from the cursor, and after a reset the cursor advances *past* skipped ids without issuing them — so `issued()` counts skipped ids too. It is **cursor consumption, not a count of admissions.** For the gauge's actual purpose (namespace pressure: how close this meeting is to needing another reset) cursor consumption is exactly the right quantity, because a skipped id genuinely is not available. So **the computation stays** and the docs stop overclaiming: the rustdoc on both methods, and the catalog entry for the gauge, say cursor consumption including ids the epoch-reset snapshot skipped — not admissions, and therefore not comparable to a join count. The high-watermark threshold inherits the same meaning, which is correct for a pressure signal.

4. **Test seam gains a skip set.** `SenderIdAllocator::resuming_from(next)` takes no skip set today, so D-3's fail-closed path would be untestable (@dry-reviewer's question). It gains a skip-set parameter under the same `test-seams` gate, which is what lets the D-3 test construct an allocator with nothing allocatable. Same release guard as the existing cursor seam; the seam's rustdoc gains the skip set to its list of bypasses.

### Client half (@paired-client owns; scope recorded here because the invariant is joint)

@dry-reviewer's sweep found that "scope by (kek_generation, sender_id)" reaches **four** sites, not one. Recorded so the client half is not planned against the one-line framing of the ruling. The provenance seam already exists — `keys.kekForGeneration(wrapped.kekGeneration)` at `receivePath.ts:557`, `WrappedTransmitKey::kek_generation()` on the wire, and ADR-0036 §4:413 means **audio carries the wrap on every frame** — which is the ruling's premise, and it holds.

1. `ReplayWindow.#generationHighWater` (`receivePath.ts:265`) — the site the ruling names.
2. `ReplayWindow.#bySender` contexts (`:313`), keyed `${stream}:${generation}` — also unscoped. Missing it lets the new holder inherit the old holder's sliding bitmap, so its early frames read as **duplicates** rather than below-high-water: different symptom, same cause, and it would look like a separate bug.
3. `ReplayWindow.admit(parts, streamSequence)` (`:289`) — the signature changes, and the ordering was the real problem: `admit` is called at `:537`, before `const wrapped = frame.wrappedTransmitKey` at `:542`, so the provenance is not yet in scope.

   **DECISION (@paired-client, ruled by @security): REORDER — `admit` moves AFTER scope resolution**, recorded at the site and not only here, because a reordering here looks like a refactor and is a security change. The DoS trade does not actually cost anything: `openVerifiedFrame` only accepts a `VerifiedFrame`, so Ed25519 verification against a roster key has already happened and an attacker cannot reach the unwrap without being a rostered member — the ratio does not move. The added work on the uncached path is ONE AES-GCM open over a 48-byte block, negligible beside the Ed25519 verify already spent on that frame, and the common path does not regress at all (a cached key id resolves its scope by map lookup, with no AEAD before `admit`). What is bought is an unambiguous scope, which is what closes S-1.

   **Consequence for MC-side test expectations:** a replayed frame whose generation has left retention now drops as `kek_generation_stale` rather than `replay_detected` — refused before any replay state is consulted. Same frame, still dropped, different token.
4. `TransmitKeyCache.#bySender` (`:383`), keyed by the **raw key-id bytes**, which carry no KEK generation (`sender(16)|stream(8)|generation(40)`). After a reissue the departed and new holders of one id produce byte-identical key ids and collide. The rustdoc's claim ("two distinct wire key ids can never collide") stays true as written but stops being *sufficient*, because the two key ids are no longer distinct.

**The `set()` zeroization question, which needs @security explicitly.** `TransmitKeyCache.set()` (`:458-466`) has a no-zero-on-replace optimisation justified by "the same key id is the same transmit key, so the replaced buffer holds identical bytes", resting on a caller obligation of one transmit key per key id — and the comment **names @security and tells a future author not to "correct" the asymmetry**, so changing it is a co-signed edit, not a cleanup. (A') falsifies the premise: across an epoch one key id maps to two different transmit keys, so `set()` would overwrite a departed participant's key buffer with a live one without zeroizing. Note the failure mode is NOT the "(key, nonce) repeat" the comment predicts — the KEKs differ — it is a **zeroization leak**. @dry-reviewer's trace suggests the functional path self-heals for audio (the `matchesCachedWrap` fast path misses on a colliding id, falls through to `kekForGeneration`, unwraps correctly and calls `set()`), leaving the un-zeroized departed key as the residue — quieter than a deafened sender, and correspondingly easier to ship without noticing. A second encoding of the same invariant sits at `sframe.ts:379`.

**One ordering exception I found while answering @paired-client's Q1, which revises a contract three reviewers signed off.** For any reissued id X, X was free at reset time, so its old holder left strictly BEFORE the reset; every removal goes through `remove_and_broadcast_left` (remove, then broadcast), and per recipient ParticipantLeft and ParticipantJoined travel the same mailbox then the same `stream_tx`, so they are FIFO. The reset's W-exemption cannot outrun the leave broadcast, because the reset can only reissue ids whose leaves already happened. **But a ParticipantLeft can be DROPPED**: `handle_update` uses `try_send`, and a full or closed stream channel drops it, counted as `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update"}`. An incumbent that lost that frame never learns of the removal and will later see X bound to a new identity with no prior removal — which the client classifies as an R-18 rebind. @paired-client verified the NORMAL path holds (`rosterKeyFeed.left()` → `RosterIdentityKeys.remove` → `forgotten` → `AudioPipeline.purgeTransmitKeys` → `cache.purgeSender`, so the later ParticipantJoined is a first binding with no rebind counted) and corrected my citation: `rosterKeyFeed.ts:51` is the MIRROR case (one participant id under a new sender id); the operative mechanism for this case is `left()`'s `remove`. The exception is orthogonal to that verification and still stands — if the ParticipantLeft frame is dropped, `left()` never fires, so `remove` never happens.

So under (A') `dt_client_media_roster_key_rebinds_total{outcome="rebind"}` is **no longer a pure MC-defect signal**: it can also fire after a lost ParticipantLeft under outbound backpressure. The `rosterKeys.ts` and `mediaMetrics.ts` contract comments must say so.

**The correlation, after @observability ruled the `payload_kind` split** (this supersedes my earlier "plausible but unconfirmed" wording, which recorded the opposite of the decision — @paired-client caught that it had gone stale). With `participant_update_left` as its own value, non-zero on that value over the same window makes the dropped-leave cause **SUPPORTED**, and zero **rules it out**.

One precision that survives the upgrade (@paired-client's, and correct): the counter is fleet-wide and per-mailbox, so it still cannot attribute a specific lost frame to THIS client's sender. The causes are separable as *explanations*; an individual rebind increment is not traceable to a dropped leave. So the ceiling is "supported, not confirmed for any individual increment" — a real upgrade from "plausible", and short of a claim the label domain cannot carry.

**Not a security regression, and the comments will say why** (@paired-client's trace): if ParticipantLeft is lost, old X's roster entry survives with its key bytes, so the new holder's ParticipantJoined hits `upsert` with `hadKey = true` and different bytes — the `rebind` arm — which fires the SAME transmit-key purge `forgotten` would have. The departed holder's keys are zeroized either way, via a different arm and one counter tick. So this is a counter-contract revision, not a hygiene hole.

**Option NOT taken:** splitting `payload_kind` per update variant would make the correlation actually diagnostic, but it changes an existing series' label domain and belongs to @observability. Raised with them rather than done unilaterally; the honest wording above holds either way. Raised with @security too, since both signed the original contract.

**@paired-client's vitest plan: ~25 cases, each recording the MUTATION it discriminates against** (so the list can be checked rather than trusted), across `receivePath.test.ts`, `receivePathIntegration.test.ts`, `kekSource.test.ts` and a new `media/lifecycle/__tests__/` atomicity spec. It covers @test's five and adds four they did not name, one of which is a second-order risk created by the fix itself and worth recording here:

- **Cross-scope EVICTION independence.** Flooding scope g+1 must not evict the same sender's scope-g state. `ReplayWindow`'s per-sender context budget is documented as the security boundary that closed the F1 attack (a member minting unbounded frames across its own triples to walk a global LRU until another sender's window is evicted). Adding a generation dimension **re-opens that shape with `generation` in place of `stream`** unless the budget accounts per scope: a sender could flood its own g+1 to evict its own retained g state and then replay a captured g frame. The fix must not recreate the attack it is adjacent to.
- The **F-8 purge-across-every-scope** case, asserted against the OLD scope — an assertion on the current scope alone passes the buggy narrow purge.
- The **RetentionGuard fail-safe splice** as a fourth generation-dropped path, the one that would otherwise drop a generation silently and leak a scope (C-3's "never later" half).
- The **two S-1 cases themselves** — a reissued id at txgen 0 accepted, and a captured-frame replay after rotation rejected with the token differing by whether g is still retained. The task's reason for existing, and not among @test's five.

@paired-client's planned files are in the classification table. Their `lifecycle/AudioPipeline.ts` and `teardown/teardown.ts` rows and the `RetentionGuard` fail-safe are theirs; C-3 requires the disposal hang off `MeetingKekHolder`'s single expiry event, never a parallel timer.

**Two findings from @paired-protocol, both now owned in-loop:**
1. **A latent defect in the shipped client, made reachable by (A') — NOT a live replay path in shipped code** (corrected at the Lead's direction; the shipped replay window is unscoped, so the overwrite has no fresh bucket to drag a replay into, which the red-before-green run below demonstrates: B2 is GREEN against shipped code). `cache.set` at `receivePath.ts:589` is unconditional, so a sender re-wrapping an existing transmit key under a newer KEK generation overwrites the entry and drags its provenance forward. Once replay state is keyed by that provenance, a replayed frame with no wrapped key resolves to a bucket that never saw it and is admitted — valid signature, valid tag. The ruling did not create this; it made it reachable. @paired-client is fixing it as first-set-wins, refusing the wrap before any unwrap.
   **Test bar for this fix (@test, held against @paired-client's vitest list). Corrected: this involves TWO frames, and my first wording conflated them** (@paired-client's catch — taken literally it would send a reader looking for the conflict counter on the wrong frame). (a) The **re-wrap frame** is refused by first-set-wins, and `wrap_generation_conflict` is counted **on that frame**. (b) The **replayed frame carrying no wrapped key** then resolves to the ORIGINAL scope and is dropped as **`replay_detected`** — not as a wrap conflict. Each fact is asserted on the frame it belongs to (@paired-client's B2 and B6). The case must also be a real regression test. Because the defect is latent in shipped code, "red against the pre-fix unconditional `cache.set`" means red against the SCOPED implementation that still overwrites — not against today's code. That mutant-kill evidence is recorded under "sdk-core red-before-green evidence" in §Tests (the Lead accepted this form).
2. **The proto's "never recycled while a KEK is live" is misleading, not merely incomplete.** Receivers retain the previous generation for `min(W/2, ceiling)`, so a reissued id genuinely coexists with a still-usable old KEK. The correct scope is per-generation: "never reissued within one KEK generation." Fixed at every site, not two: `signaling.proto` :429/:430, :483 and :715/:720, `internal.proto` :1241, plus `kek_generation`'s "sender allocation and key rotation are unrelated" (now: the two BOUNDS are independent, the EVENTS are coupled — exhaustion triggers a rotation). See the §4 amendment and the seven-site enumeration under "A third falsified claim" below (@paired-protocol).

**Settled client-side rulings, recorded so there is one name in the record:** the new `WrapOutcome` value is `wrap_generation_conflict` (@observability; @security withdrew approval of `wrap_scope_conflict`), and @security ruled NO alert on that counter pair — the closed label set cannot attribute a firing to a sender, so an alert could not support its own runbook. @security F-8 (A'-introduced): scoping the transmit-key cache makes "purge this sender" ambiguous, so `purgeSender` purges EVERY scope for that id and ADR invariant 3 reads "a departed holder's keys are purged across every scope on removal". MC holds no transmit-key cache and restates that invariant nowhere, so F-8 has no MC-side site.

**Pre-existing residual filed to `docs/TODO.md` by @paired-client** (owner meeting-controller + client): roster removals ride `try_send` and are droppable, and the client's roster IS its frame-verification authorization list, so it can silently diverge from MC's until teardown or LRU eviction. Not exploitable today (MH forwards only per MC's authoritative roster); task-sized ("reliable delivery semantics for roster removals"); not introduced by (A'). This is the same mechanism as the dropped-ParticipantLeft exception above, and the two entries cross-reference each other.

### Epoch reset (R-16)

`MeetingKeyState::reset_epoch(rng, bound: &HashSet<SenderId>) -> Result<(MeetingKeyState, SenderIdAllocator), KekRotationFailed>` — an **atomic** constructor returning the new key state and the fresh allocator TOGETHER, so "reissue before rotation" is unrepresentable in the types. There is no setter that produces a fresh allocator without a new key state, and none that rotates without reseating the allocator — the generation bump and the reseed are **atomic** in the one constructor (@security D-2), so no interleaving can expose a reseeded allocator under the old generation. The allocator is seeded to permanently skip `bound`, satisfying the strengthened C-1. `bound` is taken from the **roster** map `self.participants`, which includes grace-period members (@security D-1) — never from a connected-only view, because a grace member reconnecting onto an id a new joiner also holds would put two live senders on one id under one generation, which `media_admission/sender_id.rs` records as AES-GCM authentication-key recovery rather than a confidentiality loss. It **fails closed** if the skip set leaves nothing allocatable (@security D-4).

- Trigger is `sender_space_exhausted`, IMMEDIATE and exempt from the W debounce (waiting would stall admission for up to W). It is counted under its own trigger value and never summed with leave-triggered rotations.
- It is driven from `handle_join`'s allocation path: on `SenderIdSpaceExhausted` from the allocator, the actor performs the reset, pushes `MeetingKekUpdate` to existing members, then allocates the joiner's id from the fresh allocator. Admission never fails.
- Because the trigger now has a real emitter, `sender_space_exhausted` IS zero-initialised — the false-clean concern that applied when the reissue was held does not apply once the emitter exists.

### Design

**`MeetingKeyState::rotate(&mut self, rng) -> Result<(), KekRotationFailed>`** (`kek.rs`):
- Generates a fresh 32-byte KEK from the `SystemRandom` passed in, into a `SecretBox`.
- Increments the generation with `checked_add`. At `u16::MAX` it fails closed (`KekRotationFailed::GenerationExhausted`) and never wraps, because a wrapped generation would collide at the client's holder (same generation, different bytes).
- On success it REPLACES the `Arc<MeetingKek>`. The old handle drops here; transient clones held by in-flight pushes or joins drop within one delivery, and zeroize runs on the last drop.
- MC keeps no previous-KEK field (S-5), and no retention is derived in MC. **F-4, stated honestly in the same register as the `JoinResponse` encode-buffer residual:** "the old handle drops here" is true of the actor, but `KekPush { kek: Arc<MeetingKek>, … }` travels in participant mailboxes, so a push queued before a rotation keeps that generation's `Arc` alive until the mailbox drains — zeroize is deferred past the rotation, not coincident with it. Bounded by the mailbox depth and accepted; the design does not change, but the comment must not imply otherwise.
- **F-2: the `u16::MAX` generation seed is `#[cfg(test)]`, never a production constructor and not `feature = "test-seams"`.** A generation seed is MORE dangerous than `SenderIdAllocator::resuming_from`'s cursor seed: a caller who can set the generation can make two DIFFERENT KEKs share one generation, and the client's holder cannot tell — its install is idempotent on an already-held generation, so a colliding generation makes the client **refuse** the new key and keep using the old one. That is a silent downgrade to a KEK a leaver still holds, i.e. the exact property rotation exists to remove. The test lives in `kek.rs`'s own module, so plain `#[cfg(test)]` is available and is strictly stronger than a feature flag, needing no release guard.
- RNG failure maps to `KekRotationFailed::Rng`. There is no fallback RNG and no default key (ADR-0002).
- The docs asserting "generation never advances in this story" and "no KEK-push message ships" are rewritten.

**`KekRotationDebounce`** (`media_admission/rotation.rs`, pure, `tokio::time::Instant`):
- `record_removal(now)`: if nothing is pending, `oldest = now, leaves = 1`; otherwise `leaves += 1`, and `oldest` is **never** touched.
- `due_at() = oldest + W`.
- `take_due(now) -> Option<Coalesced { leaves, oldest }>`.
- `defer_after_failure(now)`: keeps `oldest` and `leaves` and sets `next_attempt = now + W`. This avoids a hot loop on a failed rotation while pending age keeps growing, so the overdue page fires.
- Non-collapse is recorded at the type (DRY B): `media_routing/connectivity.rs` anchors its window on the LAST settle, while this anchors on the OLDEST un-rotated removal. Lifting that shape here would reintroduce exactly the restarted-timer defect.

**Hooks in `actors/meeting.rs`**:
- `remove_and_broadcast_left` (the single choke point, which removes before it broadcasts) calls `debounce.record_removal(now)` and publishes `pending_since` to the registry.
- A sibling `sleep_until(next wake)` arm in `run()`'s `select!`, the same idiom as `next_settle_wake` (DRY A6), calls `rotate_due()`, which:
  1. calls `media_keys.rotate(&SystemRandom::new())`;
  2. clears pending and publishes the cleared state to the registry. Pending age resets AT ROTATION, not on push success (S-3b);
  3. calls `push_kek_update()` over `self.participants`, which holds exactly the members present after every removal. The leaver is absent from the map, so it is unreachable by construction, with no filter.
- Graceful shutdown DROPS a pending rotation, stated not assumed: the KEK is non-persisted and dies with the actor, so an MC restart or actor teardown is itself a rotation for every future joiner. The W bound is unaffected. **F-5, confirmed and made a requirement:** the drop is tied to the **exit path only**. Nothing else — no select-arm restructure, no shutdown flag, no drain window — may cancel the rotation arm while the actor still serves participants. If it could, the W bound would lapse for that window AND pending state would have been cleared, so pending age would not grow and nothing would page: a silent gap in the one control. The `is_shutting_down` flag deliberately does not gate the rotation arm.

**Push, one outcome per recipient**:
- Each rostered participant gets exactly one outcome. With a live connection, it is sent `ParticipantMessage::KekUpdate { push: KekPush { kek: Arc<MeetingKek>, generation, w_seconds }, respond_to: oneshot }`; with none, it is recorded as `participant_gone`.
- The participant actor encodes a `ServerMessage{MeetingKekUpdate}` at `try_send` time. The KEK leaves the `Arc` only there, and never as a `SignalingPayload::Raw` `Vec` travelling through a mailbox. It then replies with `delivered` or `dropped_outbound`. `dropped_outbound` is also counted on `mc_participant_outbound_messages_dropped_total{payload_kind="meeting_kek_update"}`, a third const, with the "two sites" rustdoc updated.
- A dropped oneshot, or a closed mailbox, maps to `actor_unavailable`.
- Outcomes are collected by a detached, span-instrumented task. It holds oneshot receivers only, never key material, so the meeting actor never awaits a participant actor; that avoids the meeting↔participant mailbox deadlock a join-all inside the actor turn could produce.
- **The collector's wait is BOUNDED** (@observability P1 — a real defect in my first draft). An actor that is alive with an open mailbox but wedged (the condition `MCHighMailboxDepthCritical` exists for) never answers, so an unbounded wait would leave that receiver pending forever and emit NOTHING: no duration observation, no INFO log, and none of the per-recipient outcomes — **including the `delivered` ones already collected**. With pending age already cleared at rotation (S-3b), the overdue page would not fire either, so a rotation that mostly succeeded would report as nothing at all, indistinguishable from "no rotation occurred". That is a masked failure (CLAUDE.md §Fail loudly) and worse than a missing metric because the surrounding series look healthy. So: a named `KEK_PUSH_OUTCOME_TIMEOUT = 2s` deadline, non-responders **emitting an outcome** (not merely no longer waited on), and the histogram and log emitted **unconditionally**. Derivation at the const: comfortably above p99 fan-out (a non-blocking `try_send` plus one oneshot round-trip per recipient) and far below the W floor of 30 s, so a stalled collector can never overlap the next rotation. The histogram's top buckets (1.0, 2.5) sit at or above it, so a fully-timed-out rotation lands in a real bucket instead of saturating `+Inf` — otherwise the histogram silently excludes exactly the slow rotations it exists to measure.
- Wedged-but-alive gets its own fifth token, **`timed_out`**, rather than folding into `actor_unavailable`, whose documented meaning ("mailbox closed or exited before answering") EXCLUDES it — a responder reading that doc would fork wrong. `MCKekPushFailureRate`'s negated selector picks the new token up automatically, which is the wanted behaviour.
- When every outcome has arrived or the deadline expires, the collector records the per-recipient outcomes, the duration histogram (rotation start → last outcome) and the INFO lifecycle log (target `mc.kek.lifecycle`). The log carries `trigger`, `coalesced_leaves`, `members`, `generation`, `duration_ms`, the per-outcome counts and `key_custody = KEY_CUSTODY_OPERATOR` (DRY A1 const). It carries no key bytes and no participant id; meeting_id comes from the span.
- S-3a residual, stated at the emission site: a member whose push is not delivered never rotates its transmit keys, so a leaver keeps opening THAT member's media past W. `mc_meeting_kek_pushes_total` + `MCKekPushFailureRate` are the only control.
- Outcome vocabulary: `KekPushOutcome` enum with `label()` + `ALL`, which states at its definition that it is NOT `PolicyPushOutcome` (DRY A4).
  - `delivered` = handed to the connection's outbound stream, not a client ack.
  - `participant_gone` = rostered but no live connection (inside grace). It is benign, because a returning participant gets the current KEK (reconnect re-issue or fresh join).
  - `dropped_outbound` = the outbound stream channel was full or closed.
  - `actor_unavailable` = the participant actor's mailbox was closed or it exited before answering. Explicitly NOT the wedged-but-alive case.
  - `timed_out` = handed to the mailbox and the collector deadline elapsed with no reply. **Names the observable, not an inferred cause** (@observability, citing `label-taxonomy.md` §Frame reject reason and the `wrap_key_id_mismatch` lesson): MC does not observe that the actor is wedged — a lost oneshot, scheduler starvation or a merely slow turn all produce the same one bit — so `unresponsive` or `wedged` would assert more than MC can see and would be triaged on the name. Correlate with `mc_actor_mailbox_depth`. `actor_unavailable` (closed mailbox / exited actor) points at `MCActorPanic` territory; this points at `MCHighMailboxDepthCritical` territory — different fault, different remedy, so the taxonomy requires a distinct value rather than folding.
  - The five values are disjoint, and each names a distinct remedy. `participant_gone` is the only benign one.

**Reconnect (R-15, S-7)**: `ReconnectResult` gains `meeting_kek: Arc<MeetingKek>`, `kek_generation: u16` and `kek_rotation_debounce_seconds: u32`, the CURRENT values. Reconnect triggers no rotation and does not touch the debounce. Wire-level reconnect is still unwired (existing TODO); this closes the actor-side obligation that `docs/TODO.md` consequence (c) states.

**W on the wire (R-14)**: `JoinResult` and `ReconnectResult` carry `kek_rotation_debounce_seconds`, read from the SAME `Duration` the actor's timer enforces. `build_join_response` fills it (replacing the hardcoded `0` and its comment), and every `MeetingKekUpdate` carries it too. MC does no retention derivation or validation.

**Config**:
- `MC_KEK_ROTATION_DEBOUNCE_SECONDS` is required through `bounded()`, with `MIN = 30`, `MAX = 300` (security's recorded figures, derivation at the consts) and `MissingEnvVar(...)` kept on one line for `dt-guard env-config`. The existing tests are extended: missing → error, both ends rejected, non-numeric rejected.
- The overdue multiplier is `KEK_ROTATION_OVERDUE_MULTIPLIER: u32 = 2`, a named Rust const with a `>= 2` compile-time assert (OPS-E). At 1, healthy pending age reaches the threshold just before every rotation and the page could fire on a healthy fleet.

**Fleet gauges**:
- `KekLifecycle` (`rotation.rs`), passed as `Arc` like `PolicyGenerations`, holds W, the overdue threshold, and a `tokio::sync::RwLock<HashMap<meeting_id, {pending_since, sender_ids_issued}>>`. `RwLock`, not `Mutex`, so the `PolicyGenerations` citation is TRUE rather than near-miss (@dry-reviewer): `generation.rs:76-79` records the reason verbatim for this access pattern — it is the in-tree idiom for a shared map read on an async path, matching `redis/client.rs`, and a `std::sync::Mutex` would need poison handling that cannot use `unwrap`/`expect` under the workspace lints. The pattern here is read-mostly in exactly RwLock's shape: N meeting actors writing on change, one sampler reading every 5s.
- Meeting actors write to it on change. It is evicted on actor exit AND in `controller::remove_meeting` (both teardown paths).
- A sampler task in `main.rs` recomputes MAX(pending age) and MAX(issued) across live meetings every `KEK_GAUGE_REFRESH_INTERVAL = 5s`. That is well inside the 15s scrape interval and the `for: 2m`, and because it samples `Instant`s written by the actors, a WEDGED actor still shows a growing age (observability item 1, S-4).
- Zero-initialised at boot; window and threshold are published once at boot from the values the timer enforces.

**DRY D**: `messages.rs` is edited, so its trigger fires. `LeaveReason::ALL` and `DisconnectCause::ALL` are hoisted, and `metrics.rs`'s `LEAVE_REASONS`/`DISCONNECT_CAUSES` and their witnesses are replaced by them.

### Telemetry (every new series carries `key_custody=operator`; no meeting, participant, sender or generation label)

| Metric | Type | Labels | Notes |
|---|---|---|---|
| `mc_meeting_kek_generated_total` | counter | `trigger` (`meeting_created`, `participant_left`, `sender_space_exhausted`), `key_custody` | extended; **all three** zero-initialised (each has a real emitter under A'); catalog "identically meeting-creation count" rewritten, cardinality 1 → 3 |
| `mc_meeting_kek_pushes_total` | counter | `outcome` (4), `key_custody` | once per recipient |
| `mc_meeting_kek_rotation_coalesced_leaves` | histogram | `key_custody` | observability's spelling (unit last) adopted over the prompt's `_leaves_coalesced`; buckets 1,2,3,5,10,25,50,100; `Matcher::Full` |
| `mc_meeting_kek_rotation_duration_seconds` | histogram | `key_custody` | rotate → last outcome; buckets 0.0005, 0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.0, 2.5; `Matcher::Full` |
| `mc_meeting_kek_rotation_pending_age_seconds` | gauge | `key_custody` | MAX across live meetings, sampled |
| `mc_meeting_kek_rotation_window_seconds` | gauge | `key_custody` | W, set once from the enforced Duration |
| `mc_meeting_kek_rotation_overdue_threshold_seconds` | gauge | `key_custody` | W × 2 |
| `mc_meeting_sender_ids_issued_max` | gauge | `key_custody` | MAX across live meetings, sampled |
| `mc_meeting_kek_rotation_failures_total` | counter | `reason` (`rng`, `generation_exhausted`), `key_custody` | @observability P3; answers my (c) with NO |
| `mc_participant_outbound_messages_dropped_total` | counter | `payload_kind`: `signaling_raw`, `participant_update_joined`, `participant_update_left`, `meeting_kek_update` | @observability ruled the `participant_update` SPLIT lands in this diff; see below |

**`payload_kind`: `participant_update` is SPLIT into `participant_update_joined` / `participant_update_left`** (@observability's ruling, adopted). It is non-breaking — I verified independently that the only consumer anywhere is `sum by (payload_kind)` at `mc-overview.json:5172`, with no equality selector on `payload_kind="participant_update"` in any panel or rule, so the panel just grows a series. The artifact is already open (this diff already adds `meeting_kek_update`, already edits `OUTBOUND_PAYLOAD_KINDS`, already rewrites the "exactly two `try_send` sites" rustdoc, already touches the catalog row), so fix-later cost is identical LoC plus tracking overhead. Merged, the signal is nearly useless for its stated purpose: joins and leaves broadcast on the same path and drop together under backpressure, so "plausible" would be true whenever the counter moves at all. And the two have different client-visible consequences — a dropped `Joined` means a slot never fills; a dropped `Left` means a stale roster entry AND a legitimate reissue miscounted as an R-18 rebind.

**Exact-match consumer sweep** (@paired-client's point: the stem preserves regex selectors, not equality ones, and a flat-going panel ships unnoticed). `grep -rn 'participant_update'` over `crates/ docs/ infra/ scripts/ crates/env-tests/` finds no rule, dashboard, recording rule or env-test selecting the old value by equality. Every exact-match site is in files this diff already edits: the const (`participant.rs:45`), the two unit-test `with_labels` assertions (`participant.rs:660, :678`), the zero-init list (`metrics.rs:1579`), the catalog row (`mc-service.md:709`), and **one the obvious set would miss: `docs/runbooks/mc-incident-response.md:2058`**, a triage comment enumerating `payload_kind ∈ {signaling_raw, participant_update}`. That file is already open for Scenarios 17 and 19, so it is updated here rather than left asserting a value set that no longer exists.

The `participant_update` **stem is kept** so `payload_kind=~"participant_update.*"` recovers the old merged series and the existing panel, saved queries and historical comparisons survive; a stemless rename would orphan the old name.

**Implementation shape, which is the load-bearing part:** the label is NOT two literals at the `handle_update` call site. It is derived at `encode_participant_update` — the single place that decides wire-visibility — via a `label()` on the variant, so making a third variant wire-visible **forces** a label decision at the same site instead of silently inheriting whichever literal the call site holds. That is the difference between a bound and a convention. The `metrics.rs:993` rustdoc's "bounded by there being exactly two `try_send` sites" is already inaccurate once `meeting_kek_update` lands and is rewritten to the real bound: bounded by `encode_participant_update`'s `Some` arm, plus one const per `try_send` site.

**The rule being established is narrow, and the catalog says so to stop it being over-read:** the test is **demonstrated consumer need, not message-type taxonomy.** `signaling_raw` also covers several message types and **stays merged**, because no consumer needs the distinction today. The catalog entry records both halves — `participant_update` split because a named downstream consumer (the client's R-18 rebind correlation) cannot do its job without it, and `signaling_raw` staying merged is the same rule returning the other answer, not an unfinished half of this one. The `Cardinality:` numeral is dropped rather than bumped, for the same drift reason @paired-client is applying one file over.

Consequence: the client comments carry the **stronger** wording — non-zero on `participant_update_left` makes the dropped-leave explanation *supported*, not merely plausible. Told to @paired-client so they upgrade rather than ship the hedge.

**Why the failures counter, reversing my own (c)** (@observability P3, accepted): the ERROR-log-plus-overdue-page argument fails on two independent grounds. (1) `GenerationExhausted` at `u16::MAX` is permanent and unrecoverable for that meeting, so the overdue page fires and never clears — reintroducing the NOT-SELF-CLEARING shape this very diff retires from `MCSenderIdSpaceExhausted`, with no metric to identify it. At W MIN = 30 s, 65,535 rotations is roughly 23 days of steady churn in one long-lived meeting, so it is not hypothetical. (2) The two causes have OPPOSITE remedies and the page cannot tell them apart: `rng` is transient and self-heals on the retry at W, `generation_exhausted` requires ending the meeting. Without the counter the responder's first move is a log hunt. Same reasoning the tree already applies to `mc_media_generation_divergence`, whose catalog entry says the gauge is not the detection signal.

`trigger` is added to `label-taxonomy.md` §Service-local labels. Catalog, dashboard panels (ADR-0029 shapes) and `MetricAssertion` coverage for every name land here (guards `metric_no_catalog`, `metric_no_dashboard`, `uncovered_metric`).

### Alert thresholds (cross-cutting review required; ADR-0031)

| Metric | Condition | For | Severity | Runbook |
|--------|-----------|-----|----------|---------|
| `mc_meeting_kek_rotation_pending_age_seconds` | `mc_meeting_kek_rotation_pending_age_seconds > mc_meeting_kek_rotation_overdue_threshold_seconds` | 2m | page | `#scenario-19-kek-rotation-stalled` |
| `mc_meeting_kek_pushes_total` | `(sum(rate(mc_meeting_kek_pushes_total{outcome!~"delivered\|participant_gone"}[10m])) / sum(rate(mc_meeting_kek_pushes_total[10m]))) > 0.01 and sum(rate(mc_meeting_kek_pushes_total[10m])) > 0` | 10m | warning | `#scenario-16-missing-key-material` |
| `mc_meeting_kek_generated_total` | `(sum(rate(mc_meeting_kek_generated_total{trigger="participant_left"}[10m])) / sum(avg_over_time(mc_meetings_active[10m]))) > 2 / scalar(max(mc_meeting_kek_rotation_window_seconds)) and sum(avg_over_time(mc_meetings_active[10m])) > 0 and on() sum(mc_meetings_active) > 0` | 10m | warning | `#scenario-17-kek-rotation-storm--flapping-participant` |
| `mc_meeting_kek_generated_total` | `increase(mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}[15m]) > 0` | 0m | info | `#scenario-17-kek-rotation-storm--flapping-participant` |

Anchors follow @operations' routing, which corrected my single-stub proposal. Three destinations, not one: **Scenario 16 already exists** (line 2275) and a failed KEK push IS missing key material at the client, so `MCKekPushFailureRate` belongs there — a real ladder beats a stub, and pointing it at a flapping-participant section would misdirect a delivery failure. **Scenario 17** uses the story's reserved heading verbatim ("KEK Rotation Storm / Flapping Participant", story line 609) so task 18 expands it in place without having to edit three `runbook_url`s in a rules file. **Scenario 19: KEK Rotation Stalled is new** and gets a real body, not a stub, because it carries a page and the condition is the OPPOSITE of Scenario 17's — nothing rotating versus too much. A responder paged at 3am onto a page headed "Rotation Storm" would apply storm guidance to a stall. 19 is free (task 18 reserved MH 18/19 and MC 17; MC 18 is task 6's); the allocation is recorded in the story notes so task 18 does not collide.

Provenance:
- **Overdue**: healthy pending age is at most W plus rotation latency, which is below 2W, so healthy steady state cannot satisfy the rule. The rule depends on label-set identity: both gauges carry exactly `key_custody` plus the scrape labels, and a later label added to one silently empties the comparison (@observability P4). Demonstration: stall the rotation timer and watch pending age cross. **Detection latency is the accepted cost and goes in `impact`, not only the comment block** (@operations OPS-I): at W = 300 the page arrives ~12 minutes after the oldest un-rotated departure, against an R-12 promise of 5 — so it does NOT mean "W is about to be missed", it means "W has already been exceeded, by roughly 2x, for at least 2 minutes". `impact` also states what the exposure IS: a departed participant still holding a working KEK, a confidentiality fact, which reads very differently at 3am from the availability-shaped alerts around it. Per OPS-L the `rng` / `generation_exhausted` fork is named in `description`, `rng` first — at the W floor, exhausting 65,535 generations takes ~22.7 days of uninterrupted rotation in one meeting and the KEK dies with the actor on any restart, so `generation_exhausted` is effectively unreachable and an unreachable arm listed first costs 3am minutes.
- **Push failure**: the selector is negated, so a future outcome value fails closed. `participant_gone` is excluded as benign. 1% is the story's figure, not SLO-derived. The bounded collector plus the `timeout` token are what make this rule sound at all (@operations OPS-M): with an unbounded wait a wedged recipient is absent from BOTH numerator and denominator, so the alert gets *quieter* as more actors wedge — a control whose sensitivity falls as the fault spreads, reading green while degrading, and it is the only control for the S-3a security residual. One outcome per rostered recipient, no exceptions, and the duration histogram still gets a sample in the timeout case so slow rotations are not silently excluded.
- **Storm**: the denominator is `sum(avg_over_time(mc_meetings_active[10m]))`, matching the numerator's window (@operations OPS-H — my first draft divided a 10m rate by an *instantaneous* count). Those disagree exactly when the fleet drains: 1000 meetings each rotating once and then ending together leaves the numerator carrying ~1.67/s while the denominator collapses to the few remaining, firing by ~25x with nothing wrong. K = 2 covered only a 2x collapse, not a synchronized drain. Both sides now describe the same ten minutes and the drain case is neutral.

  **The rule carries TWO conjuncts doing two different jobs, and they must not be merged** (@observability's addition to OPS-H). `and sum(avg_over_time(mc_meetings_active[10m])) > 0` is the §Non-Zero-Denominator guard and sits on *the same expression that forms the denominator* — guarding a different expression is the inversion that section exists to prevent. `and on() sum(mc_meetings_active) > 0` is a separate **sanity conjunct** ("there is a live meeting to act on right now"), closing the residual the first leaves: a fully drained fleet still has rotations inside the 10m window and a non-zero windowed average, so without it the rule could fire with nothing to page about, clearing only as the window ages out. In-tree precedent for the second shape is `MCLowConnectionCount` at `mc-alerts.yaml:235`, which `alert-conventions.md` §Idioms that look like this guard but are not lists explicitly as a sanity conjunct and NOT the non-zero-denominator guard. Each conjunct's job is named in the ADR-0031 block so a later reader does not delete one as duplication.

  **The denominator is `sum(avg_over_time(mc_meetings_active[10m]))`, NOT a subquery** (@operations' amendment to their own OPS-H, adopted). @observability's `avg_over_time(sum(...)[10m:])` would have been the repo's first PromQL subquery — I verified: no `[W:]` syntax exists in any rule file. That matters because `dt-guard application-metrics` check 4 extracts metric names from rule expressions and **fails open**, so a name its parser missed would leave a rule referencing a nonexistent metric passing silently — a control that reads as present and can never fire. `avg_over_time` over a **raw range vector** needs no new parser capability: the metric sits in exactly the `metric[window]` position the extractors already handle, identical to `rate(mc_meeting_kek_generated_total[10m])` in this rule's own numerator, and the form is already proven in-tree at `gc-alerts.yaml:161` (`absent_over_time(gc_telemetry_ingest_total[15m])`), which I verified. This is preferring an extraction that structurally cannot go wrong over a detector for it going wrong.

  **Semantics of the swap, recorded so the asymmetry is not rediscovered:** `sum(avg_over_time(per_pod))` and `avg_over_time(sum(per_pod))` are equal while all pods report throughout the window, and differ only when a pod appears or disappears mid-window — the chosen form averages each pod over just the samples it has and then sums at full weight, slightly OVER-estimating the denominator and so making the rule slightly **less** sensitive. That is the conservative direction for a warning whose failure mode we are eliminating is a false positive. It still fixes the drain case, which is the point: 1000 meetings declining to 10 gives a denominator near the mean of the decline rather than the final 10.

  The three checks are still run rather than reasoned about (`dt-guard alert-rules`' `for:`-floor extraction, `application-metrics` check 4, and `crates/env-tests/tests/33_alert_rules_loaded.rs` for live-cluster load) — but against an expression that does not need a new parser capability to be safe. `scalar(max(mc_meeting_kek_rotation_window_seconds))` makes the rule silently non-evaluating if that gauge is ever absent (`2/NaN` is NaN, `vector > NaN` is false) — acceptable because MC sets it at boot and the exporter has no `idle_timeout`, but recorded, because a reader's instinct is that a config gauge is inert and this one gates whether the rule can fire (@observability P5). The prompt's "keyed on the trigger label" is satisfied by the positive selector plus the documented partition rather than by `by(trigger)` — see the storm-selector note above for why a per-trigger series would have been false coverage for the exhaustion arm.
- **Exhaustion info rule** (`MCKekEpochResetOnSenderIdExhaustion`): the prompt's required replacement for the retired `MCSenderIdSpaceExhausted`. Now has a real emitter, so it is not a permanently-zero series. The 15m / `for: 0m` shape is deliberately the RETIRED rule's own, whose existing justification at `mc-alerts.yaml:321-324` still applies ("the 15m increase window already smooths, the condition cannot flap") — but the ADR-0031 block **inverts the conclusion that comment draws**: the old rule argued a `for:` delay "would only postpone paging on an already-irreversible state", and under (A') the state is NOT irreversible, which is exactly why severity drops to `info` and nobody is paged. Recording the inversion is the cheapest way to show the retirement was reasoned rather than mechanical. `for: 0m` clears the guard floor via the expr-window branch, not `for:`. Annotation also carries the prompt's stated reason for retiring the old rule: its `sender_id_space_exhausted` join-failure label is never emitted again, and its "NOT SELF-CLEARING" annotation would become the **opposite of true** — an annotation that inverts is worse than a missing one, because a responder acts on it (@security F-6).
- **Storm selector is POSITIVE (`trigger="participant_left"`), not negated** — @observability reversed their own earlier guidance and I have adopted it, including the residual it costs. The rule's semantic is "the debounce is not applying", and `2/W` is derived entirely from the debounce. Exhaustion-triggered rotation is **exempt from the debounce by design** and bounded by 65,536 admissions, so its rate can never approach `2/W`: a `sum by(trigger)` series would sit permanently under a threshold that cannot fire for it, which is worse than not covering the arm because it READS as coverage. The arm's real visibility is the info rule above, which needs no threshold judgement. **This is the one place in this diff where fail-closed is traded for semantic honesty**, so it is paid for with documentation rather than left implicit: the rule comment names the full **partition** — `participant_left` → storm rule, `sender_space_exhausted` → info rule, `meeting_created` → neither — and states that a new `trigger` value must be classified into one of them or it is silently unalerted. `RotationTrigger::ALL` keeps the vocabulary in one place and the catalog entry carries the same partition.
- **Sender-id headroom: NO alert, deliberately.** @operations' OPS-J asked for a warning at 80% of the space and @observability seconded it — both **explicitly conditioned on (B)**, where exhaustion stays terminal and the gauge is the only pre-cliff signal. Under the Lead's (A') ruling there is no cliff: exhaustion self-heals into an epoch reset and the meeting keeps admitting. A threshold alert on a limit that no longer terminates anything would fire on a healthy condition, which is the provenance failure both reviewers spend most of their findings avoiding (@security F-6 states this as a prohibition, and @observability's own ruling says "dropped, not re-tuned" under A'). `mc_meeting_sender_ids_issued_max` reverts to the flapper-visibility role the prompt assigns it — a dashboard read, not a rule — and the exhaustion **info** rule above is what (A') brings in its place. Recorded here so the substitution reads as reasoned rather than as an omission, and so nobody re-adds it from the B-shaped thread.

### Retirement sweep: `sender_id_space_exhausted`, all 13 encodings (@dry-reviewer's charge)

Under (A') the token becomes **unreachable by construction** — the allocator's exhaustion is absorbed by the epoch reset and admission never fails. So the rule is "this label value is never emitted again", and shipping it for the alert file alone would be the rule-everywhere-except-X case. All 13 retire here:

**Production code (mine):**
1. `media_admission/sender_id.rs` — `SenderIdSpaceExhausted` stays as the allocator's internal signal (the epoch reset consumes it) but its "terminal for the meeting / no in-place remedy" rustdoc is rewritten, as is the "Exhaustion is a reject, never a wrap" block.
2. `media_admission/mod.rs:48` — re-export kept; it is now an internal signal, not a join-failure.
3. `errors.rs:117, 217, 262, 297` — `McError::SenderIdSpaceExhausted` variant, the code-7 mapping, the `error_type` label and the classification arm all removed: the join path can no longer produce it.
4. `actors/meeting.rs:905` — the emit site becomes the epoch-reset call.
5. `observability/metrics.rs:1539` — the `JOIN_FAILURE_ERROR_TYPES` zero-init entry removed (@observability item 10). Left in, MC would stand up `mc_session_join_failures_total{error_type="sender_id_space_exhausted"}` at 0 forever for a condition that can no longer occur — a permanent zero row reading as "checked, clean" with nothing left to check. Plus `:1908, :1925` in tests. **CORRECTED 2026-09-26 at Gate 2 by @observability, who authored the claim this replaces.** The original text called this "the trap" and asserted that `JOIN_FAILURE_ERROR_TYPES` is the one MC vocabulary with no drift control, so removing the variant "compiles clean and would ship the stale entry silently". **That is false.** It is true that `dt-guard counter-zero-init` validates at metric-NAME level only, and true that this const has no `slot_*` witness — but it has a *different* drift control, and a stronger one: the TEST-F1 test at `observability/metrics.rs:2012`, whose `join_reachable` helper is a **wildcard-free exhaustive match on `McError`** plus a set-equality assertion against the const. Removing `McError::SenderIdSpaceExhausted` therefore fails to COMPILE there, and discharging that also reds the assertion unless the const is updated. So this vocabulary is among the best-protected string vocabularies in MC, not the worst, and `docs/TODO.md`'s GUARD-1 entry already records it as the mitigated case. **The removal was still correct on its own merits** — a permanently-zero series for a condition that is unreachable by construction is a false affordance on the Join Failures panel whether or not a guard would have forced the edit — but it was a guard-backed edit, not a silent-drift near-miss. The error was inferring "no drift control" from "no `slot_*` witness" without checking for another mechanism: the same shape as the two claims @observability corrected in others this loop, recorded here rather than quietly dropped.
6. `media_routing/generation.rs:33` — the cross-reference whose "bounded namespace, terminal" premise changes.

**Alerts / docs:**
7. `infra/docker/prometheus/rules/mc-alerts.yaml:318` — `MCSenderIdSpaceExhausted` retired, replaced by an **info** rule on `mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}` with its ADR-0031 block (the prompt's requirement, now with a real emitter).
8. `mc-alerts.yaml:164` — the comment citing it as the `for: 0m` precedent, re-pointed at `MCActorPanic`.
9. `infra/docker/prometheus/rules/mh-alerts.yaml:240` — an **MH-owned** comment citing it as its `for: 0m` precedent. Cross-boundary, comment-only; fixed in-loop rather than left dangling. Classified below.
10. `docs/observability/metrics/mc-service.md:178` — the token dropped from the `error_type` list; `:191` — the bullet restating "MC refused the admission rather than wrapping onto a live id" replaced with what now happens (epoch reset + fresh allocator, admission never fails).
11. `docs/TODO.md:530` — the counter-visibility entry lists `MCSenderIdSpaceExhausted (mc-alerts.yaml:314)`; both the alert and the line reference die. Per @operations OPS-K the replacement citation is **name-based**, not a re-pinned line number that will drift again.
12. `docs/TODO.md:1392` — consequences (a), (b) and (c) are ALL discharged: the namespace now has a reclaim path, leave-debounced rotation ships, and `ReconnectResult` carries the KEK. Entry closed.
13. `docs/TODO.md:1390` — the join-rate-limit / `jti`-replay threat. Its secondary trigger, "the first `MCSenderIdSpaceExhausted` firing", becomes unreachable and is **re-expressed** against `MCKekRotationStorm{trigger="sender_space_exhausted"}` and `mc_meeting_sender_ids_issued_max`, with @security. The threat itself stays deferred — I am not implementing a join rate limit — but the entry must not stay keyed on an artifact this diff deletes. @security's note: under (A') the threat genuinely drops in severity (permanent unrecoverable denial becomes a self-repairing rotation storm) rather than merely going silent.

**Scenario 8 root cause 11 — Lead ruling (a): a dated correction marker lands in THIS diff** (the file is already open for Scenarios 17 and 19, so fix-don't-defer applies). The full rewrite stays in operations' task 18, and operations reviews the marker.

**Convention**, read from the artifact rather than invented (@operations): `mc-incident-response.md` has no dated-correction precedent, so the marker follows the sibling runbooks' form — lead with the **corrected fact in bold**, then a "previously said" clause naming the wrong conclusion (`client-dev-local.md:824`, `gc-deployment.md:1637`).

**Root cause 11 has FIVE falsified claims** (@operations enumerated four; @paired-client found the fifth): `:1060-1061` "MC refused the admission"; `:1075-1078` "Fix: End the meeting… the only remediation… the KEK-epoch reset… is deferred with all KEK rotation" (the dangerous one — it names the deferral (A') closes); `:1078-1079` "Do not attempt to 'reset' the allocator" (MC now does precisely this, and **"live"** is the operative word that makes it safe); `:1093-1094` "In both cases the meeting is permanently broken"; and the headline of `:1071` — "**Ids are never recycled**".

**A reviewer conflict on `:1071-1073`, resolved by splitting the sentence rather than picking a side.** @operations told me to keep that paragraph as "still true as written"; @paired-client flagged it as false. Both were partly right, because the sentence carries four clauses with different dispositions: "the namespace is consumed by cumulative lifetime admissions" — **true** (per epoch); "Ids are never recycled" — **FALSE** under (A'), now "never reissued within one KEK generation"; "because reusing one under a live KEK collides two senders on one key id… on one AES-GCM nonce" — **true**, and it is **the reason the epoch reset is sound** (the reset changes the KEK, so a reissued id is never under the KEK its previous holder used); "`MC_MAX_PARTICIPANTS` does not bound this" — **true**. So the marker supersedes the headline clause only, and explicitly keeps its attached reason. Striking the whole paragraph (as a strict reading of @paired-client would) deletes the mechanism and leaves a reader with "we used to refuse this, now we don't"; keeping it whole (as my first draft did) leaves the false headline standing under a banner saying "everything else stands". Same class as the `signaling.proto` "never recycled while a KEK is live" phrase @paired-protocol fixed. And the driven/accidental security split at `:1080-1094` survives: a deliberate namespace burn is still worth a security response, and there is still no join rate limit and no `jti` replay check. Only the *availability* consequence dies — permanent unrecoverable denial becomes a self-repairing rotation storm — which is the same move as the `TODO:1390` trigger re-expression, and the two are kept consistent.

**The triage command at `:1062` MUST be fixed, not just annotated** (@operations' catch, and I verified it). `grep "sender_id space exhausted"` targets the WARN at `actors/meeting.rs:901`, which lives inside the terminal reject's `map_err` — under (A') that record is **gone**, so the command matches nothing, and nothing reads as "not this cause". That is a false clean, and it is the exact trap the same paragraph already warns about for a missing `--tail`.

So the reset's own WARN is deliberately spelled to share a grep stem with the surviving watermark record: **`"sender_id namespace exhausted; KEK epoch reset and reissue"`**, so `grep "sender_id namespace"` finds both the leading indicator and the event.

Three hardening requirements on that stem (@operations), because a runbook-to-log-string coupling has nothing enforcing it:
- **The stem must be contiguous on ONE source line.** I verified `:901`'s existing literal is `\`-continued across three lines; if the new WARN is continued and the break falls inside `"sender_id namespace"`, the compiled message is correct but **the grep matches nothing** — the `\`-continued-literal vacuity mechanism, which a length check cannot catch because the bad needle is long. The stem sits at the start of the first fragment. `:936` is already a single short literal (verified).
- **The runbook command names the COUNTER as well as the grep.** The grep is precise but fail-open: nothing enforces the runbook/log-string pair, so a future reword silently returns nothing and reads as "not this cause" — the exact failure this whole exchange removed. `mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}` cannot vanish that way (catalog, zero-init and dashboard guards; a rename fails CI). Division of labour: the counter is the fail-closed signal for *whether it happened*, the grep tells you *which meeting*. That is what makes the grep's fragility tolerable rather than load-bearing.
- **A one-line note at BOTH WARN sites** (`:901`'s replacement and `:936`) recording that the runbook greps this stem. Not a guard, and not claimed as one — a note where the person rewording the message will actually be looking, which is the story's "prose citing its source cannot drift silently" rule applied to a grep instead of a value. I also verified the watermark record survives and *improves*: `watermark_warned` is a per-allocator field initialised `false`, and the reset installs a fresh allocator, so `"sender_id namespace past high watermark"` re-arms every epoch. With the cliff gone it is now a genuine leading indicator rather than forensics-only, and the marker says so in one clause.

**Marker text** (blockquote under the `11.` heading; body left intact for task 18):

> **Corrected 2026-09-26 (story 2 task 9, R-16) — sender-id exhaustion is no longer terminal. DO NOT act on the Fix below: ending the meeting destroys a session that is already recovering.** MC now performs an immediate KEK-epoch reset and reissues ids from a fresh namespace, so admission succeeds and the meeting repairs itself.
>
> **Five claims below are superseded, and only these five.** (1) that MC "refused the admission" — it now admits; (2) the **Fix** ("end the meeting… the only remediation", and that the epoch reset "is deferred with all KEK rotation") — that deferral is closed; (3) "do not attempt to 'reset' the allocator" — MC now does exactly that, and **live** is the operative word: it reissues only ids whose holders have already left; (4) "the meeting is permanently broken"; (5) the bare claim "**Ids are never recycled**" — ids ARE now reissued, but only after a reset that bumps the KEK generation, and **never within one KEK generation**. A reader trusting (2) would have **destroyed a healthy meeting and every participant's session**, and would have waited for `MCSenderIdSpaceExhausted`, which is retired (replacement: the info rule on `mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}`).
>
> **Everything else below stands — including the REASON attached to claim (5).** "Because reusing one under a live KEK collides two senders on one key id… on one AES-GCM nonce" is still true, and it is exactly **why the reset is sound**: the reset installs a new KEK, so a reissued id is never under the KEK its previous holder used. Likewise "the namespace is consumed by cumulative lifetime admissions" and "`MC_MAX_PARTICIPANTS` does not bound this" still hold, per epoch.
>
> **Triage commands changed.** Start with the counter, not the log: `mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}` is the fail-closed answer to *whether this happened*. The `"sender_id space exhausted"` record is no longer emitted, so for *which meeting* grep `"sender_id namespace"` (with `--tail`, as below), which matches both the reset — `sender_id namespace exhausted; KEK epoch reset and reissue` — and the one-shot `sender_id namespace past high watermark`. **That watermark WARN now fires before EACH reset rather than once before a permanent break**, so it is the closest thing this scenario has to a pre-event signal. Also read `mc_meeting_sender_ids_issued_max` and Scenario 17.
>
> **The driven/accidental split below still applies**, and so does the security response: a deliberate namespace burn is still cheap (no join rate limit, no `jti` replay check — `docs/TODO.md` §Media Path Obligations). What changed is the consequence — a self-repairing rotation storm rather than a permanently broken meeting. Full rewrite: story 2 task 18.

**Handoff, stated as a prohibition (@operations):** beyond the dated marker above, my sweep does NOT rewrite `mc-incident-response.md` Scenario 8 — the full correction is operations' task 18, conditioned by story line 591 on R-16 having landed. Under (A') R-16 DOES land, so task 18 **must** make the Scenario 8 correction. (Had the Lead ruled B, task 18 must NOT have made it; recording both arms so the condition is not re-derived.)

### The claim this diff DOES falsify: "KEK rotation is deferred"

A second, independent claim also becomes false: **KEK rotation is no longer deferred.** That is this diff's own consistency rule, and the story's named prose-premise hazard class, so it is swept exhaustively rather than per-site. `grep -rn "rotation is deferred\|with all KEK rotation\|never advances in this story\|Always 0\|ships no KEK"` over `crates/mc-service/src/` gives **seven** encodings, in three dispositions:

**Falsified outright — the claim is that rotation has not shipped:**
1. `media_admission/kek.rs:82-83` — "This story ships no KEK-push message and no re-attach delivery path; both land with KEK rotation in a later story."
2. `media_admission/kek.rs:117` — `MeetingKeyState`'s "Generation starts at 0 and never advances in this story".
3. `media_routing/generation.rs:13` — the three-generations table row `| JoinResponse.kek_generation | … | KEK rotation (deferred; always 0 today) |`. Both halves false. **Not the same line as the `:33` cross-reference my 13-site list calls unchanged** — `:33` cites `SenderIdSpaceExhausted` and stays true under B. Naming the file once as "unchanged" is how `:13` gets missed (@dry-reviewer's catch).
4. `webtransport/connection.rs:1720-1721` — the `kek_generation` field comment's "Always 0 in this story — rotation is deferred" clause. A **separate comment on a separate field** from the `kek_rotation_debounce_seconds: 0` hardcode four lines below that is already on the list. Only that clause is dropped: the rest ("0 is a LEGAL first generation, not a sentinel … never gate on `kek_generation != 0`") stays true and load-bearing.
5. `actors/messages.rs:424` — `JoinResult::kek_generation`'s "Always 0 in this story — rotation deferred." **Found by this sweep, not in @dry-reviewer's list** — the fourth falsified site, and the reason the sweep was run as a grep rather than site-by-site.

**Conclusion still true, stated reason now false — corrected, not deleted:**
6. `actors/meeting.rs:889` — "The KEK-epoch reset that would reclaim the namespace is deferred **with all KEK rotation**, so there is no in-place operator remedy." Under B the conclusion holds (no in-place remedy) while the reason does not. A right conclusion resting on a now-wrong reason is worse than a plainly false comment, because the next reader infers "rotation has not shipped" and that inference is now wrong. Re-pointed at the held reissue task.
7. `media_admission/sender_id.rs:128` — "is deferred with all KEK rotation." Same correction.

`sender_id.rs:34` — "this story ships no KEK-epoch reset, so there is no in-place operator remedy" — was the one site true under B. Under (A') it is **false too** and is rewritten with `:128`. (Recording the flip because the B-shaped analysis had them in opposite dispositions in the same file, and that is exactly the kind of near-miss a shape change reintroduces.) Also confirmed non-issues: `generation.rs:74` (§8 re-assert cadence, not KEK), `tests/media_admission_integration.rs:205` ("first generation is 0"), `join_tests.rs:1101` and `media_policy_push_integration.rs:97` (policy generation), and `signaling.proto:429` — the sender-id monotonicity claim, the fourth encoding of @dry-reviewer's ADR finding. Under (A') it IS in scope and is @paired-protocol's (comment-only).

### A third falsified claim: the pre-A' `sender_id` recycling rule (@security F-7)

Seven sites across four files assert the pre-(A') rule; three are mine, four are @paired-protocol's (`signaling.proto:429/:430, :483, :715/:720` and `internal.proto:1241` — security found `:715`, @paired-protocol found `:1241` on a re-grep). Correcting only half is the partial-invariant shape, and worse than usual because the uncorrected half reads as agreement. Verified all three of mine in tree:

1. `media_admission/sender_id.rs:107-108` — "*a departed participant's id is never reissued to a later joiner*". A **direct falsification**: (A') makes exactly that the defined behaviour. The paragraph's consequence goes with it ("Because ids are never recycled the namespace is consumed by cumulative lifetime admissions") — that framing is what made exhaustion terminal, and it no longer is.
2. `media_admission/sender_id.rs:79` — "the R-35 never-recycled invariant a property of the type system rather than of reviewer vigilance". The structural claim SURVIVES and is still the point, so it is restated, not deleted: what the types now guarantee is no reissue **within a generation**, with the epoch reset as the only reissue path and the generation bump atomic with it.
3. `media_admission/kek.rs:126` — "a never-recycled-while-a-KEK-is-live invariant", in the passage distinguishing the KEK-generation ceiling from `SenderId`'s. That distinction stays correct and is kept; only the invariant's scope is fixed.

Also on my side, at @paired-protocol's suggestion and because it is where a reader would look: `sender_id.rs`'s rustdoc states that the skip set is a **snapshot fixed for the epoch and never liveness-re-checked** — the mechanism that makes "at most one identity EVER within a generation" true rather than merely "unique among live" — and names the liveness-query optimisation as the prohibited one.

**Why this is a finding and not a nit, carried into the corrections themselves:** "never recycled while a KEK is **live**" does not merely overstate — it names the **wrong scope key**. A reader who scopes receiver state on KEK *liveness* rather than *generation* collapses `(G, X)` and `(G+1, X)` into one bucket during the retention window, and **S-1 reappears exactly**. So the stale text actively misdirects toward the defect this fork exists to close. Each correction states that **retention is why the scope key is the generation and not KEK liveness.**

TODO :1392 (a), (b) and (c) are all discharged. DRY D, the `LeaveReason::ALL` / `DisconnectCause::ALL` hoist, is taken because `messages.rs` is edited.


### Deployment safety under (A') — @operations OPS-N and OPS-O

**OPS-N: (A') creates an MC↔SDK version skew that NEITHER END CAN DETECT.** This is the finding I most wish I had caught myself. (A') fixes S-1 client-side with no wire change, and that is exactly what makes the skew invisible:
- **New MC + old SDK bundle:** MC reissues a `sender_id` after an epoch reset; an incumbent on pre-(A') sdk-core still scopes replay state by `sender_id` alone, so the reissued sender's frames hit the departed previous holder's high-water mark and are dropped as replays — **silently and permanently, for that sender, to that incumbent.** That is S-1 happening in production, triggered by version skew instead of by design.
- **Old MC + new SDK:** benign; the finer scope is simply never exercised.

So the ordering is strict and one-directional: **SDK first, then MC.** Same direction as the existing task-20 bullet and for the same browser-cached-bundle reason, but strictly worse, because task 20's skew is *detectable* (an empty `connection_id` lands on `mc_mh_notifications_without_connection_id_total`) and this one has **no field, no negotiation and no counter**. "Roll either side first" is safe for task 20's MH coupling and is NOT safe here. The `mc-deployment.md` §Coordination bullet states the ordering, the reason — *explicitly* that no wire change means no detector, because the absence of a field otherwise reads as "nothing to coordinate" — and the stale-bundle tail that persists until users reload regardless of deploy order.

The triage half matters because the symptom lands somewhere unhelpful: a replay drop is NOT a key-material drop, so `MCMediaMissingKeyMaterial` does not cover it (the frame opens fine and is then refused). The presenting symptom — one sender inaudible to incumbents while every MC-side signal reads green — is Scenario 18. So the Scenario 17 stub gains one line: if an epoch reset occurred and a single sender went inaudible to incumbents only, suspect a stale SDK bundle before anything server-side, and cross-reference Scenario 18. Proportionality, agreed: the skew only bites in a meeting that has actually exhausted 65,536 admissions, i.e. flapper-or-driven territory, so it is one §Coordination bullet and one runbook line — no version gate, no counter.

**OPS-P: WITHDRAWN by @operations, no security clause in the bullet.** Recorded because the reasoning is load-bearing for how OPS-N's bullet is written, and because three of us verified it independently rather than deferring to each other. @operations proposed that the stale-bundle population also retains a newly-reachable replay-admission path (the `receivePath.ts:589` provenance drag), making the SDK-first ordering security-relevant on a second ground. They explicitly invited correction on the reachability, and I verified it inverts:

Today's `ReplayWindow.admit()` keys on `parts.senderId`, `parts.stream` and `parts.generation` — **all three read from the KEY ID**, where `generation` is the 40-bit transmit-key generation. The **KEK** generation appears nowhere in it; it lives only in the wrap. So on a pre-(A') bundle, replay admission has zero dependence on provenance, and the unconditional `cache.set` drag **cannot** affect it. The bug is real but **inert** there.

The drag becomes exploitable only once replay state IS provenance-keyed — i.e. only in a bundle that has the scoping **without** the first-set-wins fix. Both land together in this diff, so that bundle is never shipped.

**Sequencing constraint, therefore (@security, reached independently and binding on @paired-client's half): the scoping and the first-set-wins fix must land in the SAME commit.** The intermediate state — scoping first, first-set-wins second — is strictly **worse than either endpoint**, because it opens a replay-admission path that neither the old nor the final code has. Irrelevant to a single reviewed diff, and exactly relevant to a future bisect, revert or cherry-pick landing on it and treating it as a safe waypoint. A one-line note at the scoping site records that, so the constraint survives the commit boundary it is about.

**Severity bound on that window, recorded so it is not over-read** (@security): even in the vulnerable configuration the precondition cannot be created by a third party. The drag needs a frame carrying a wrapped key under a newer generation for an already-cached key id, and that block sits in the **signature-covered publisher region** (ADR-0036 §4: "MH can neither attach, strip, nor replay it"). Only the holder of the sender's own identity signing key can produce it, and what it buys is replay of that sender's *own* media. Not unauthenticated, not a confidentiality loss, and it cannot reach another sender's streams. The stale-bundle population therefore carries the deafening, not the replay path.

**The ordering stays non-negotiable on OPS-N's own ground, and the register distinction is an AVAILABILITY one — not a security one** (@operations' corrected wording, adopted). The reason this tail must not inherit the task-20 bullet's reassuring tone is not that it is insecure: it is that **task 20's degradation is transient and self-correcting, whereas this one is permanent for the affected sender until the bundle refreshes, and invisible to both parties.** That is the distinction an operator weighing a forced refresh actually needs. @security's point that audibility is the stronger claim standing alone holds: it needs no adversary and is a deterministic consequence of the two versions meeting, so the ordering rests on something unconditionally true rather than on a caveated threat.

**Why the bullet's silence on security is a FINDING rather than an omission**, stated so a reader trusts it instead of re-deriving it: the skew is audibility-only *because* every property whose failure would be a security failure is enforced at MC, not at the receiver. @security verified that a pre-(A') receiver **drops rather than mis-attributes**, that verification always uses the current roster key, and that the one-`sender_id`-to-one-identity-per-KEK-generation invariant holds server-side regardless of client version. @security reviewed this independently and reached the same conclusion, confirming OPS-N stays scoped to **audibility** — which they note is the stronger claim anyway, because it needs no adversary: a new MC reissuing a `sender_id` into a pre-(A') receiver leaves that sender silently and permanently inaudible, deterministically. So no security clause goes in that bullet and no extra work lands from it.

**OPS-O: OPS-G is fully live again under (A'), and my handoff line covered only the required-key half.** `infra/docker/prometheus/rules/` is content-hashed through `configMapGenerator` in `infra/kubernetes/observability/kustomization.yaml`, so it **self-rolls**; the MC ConfigMap does not. Retiring `MCSenderIdSpaceExhausted` makes that divergence unsafe in the **rollback** direction: roll the MC image back to a pre-(A') build while the rules stay new, and a meeting that exhausts its id space refuses admissions permanently **with nothing firing at all** — the terminal reject is back, its detector is gone, and the replacement info rule keys on a trigger that build never emits. A rollback that silently removes a control is worse than one that fails loudly. One §Coordination bullet: the rules are a **fourth artifact** in this release's coupled set; retire with-or-after the image, and restore with-or-before any image rollback.

### Tests

**Determinism: no integration assertion waits out a real wall-clock W** (@test 1; ADR-0028 zero-retry). "Small W injected at construction" was a real-time race — both S8 leaves would have to land inside a small real window or they silently stop coalescing under CI load. Instead `MeetingSeams` (already the gated home for `sender_id_cursor` and `slot_view_flush_batch`) gains `kek_debounce_manual: bool`. When set, the actor's wake arm never auto-fires, and a test-only `MeetingMessage::ForceKekRotation { respond_to }` drives `rotate_due(now)` after the removals are recorded. Coalescing then holds **structurally** — both removals are in the debounce before anything fires — rather than by timing luck. This also de-flakes S6, the grace case and joiner-after-rotation, which would each otherwise wait out a real window. The pure `rotation.rs` struct stays `now`-parameterised, so its unit tests need no seam.

- **Unit** (`rotation.rs`):
  - debounce-from-oldest-never-reset (a steady trickle of removals every W/2 still fires at oldest + W);
  - coalescing (k removals → one `Coalesced{leaves:k}`);
  - defer-after-failure keeps oldest;
  - registry MAX and eviction, with the **discriminating** fixture (@test 2): TWO live meetings, the YOUNGER `pending_since` written LAST, asserting the OLDER age is published. A single-meeting fixture, or one writing the older last, passes identically for a correct pod-max and for the broken last-write-wins implementation, so it could not tell them apart — the `mc_media_generation_divergence` defect class, on a gauge that is paged on.
- **Unit** (`kek.rs`): rotate changes the bytes, increments the generation, keeps the redaction, drops the old Arc (weak ref dead), and fails closed at `u16::MAX` (test-only generation seed).
- **Integration** (`tests/kek_rotation_integration.rs`, real WebTransport, W = a small `Duration` injected through `KekLifecycle::new`; assertions are counter- and frame-based, never on elapsed wall-clock):
  - S6: A, B, C join; C leaves; A and B each receive `MeetingKekUpdate{gen 1, W}`; C's stream receives none; the counters `participant_left` +1 and `delivered` +2.
  - S8: two leaves inside one W → exactly one rotation, asserted as ONE event. **As implemented, the positive control is a SECOND `force_kek_rotation()` returning `false`**, not a histogram value: `MetricAssertion` histograms expose observation counts, not values, so `coalesced == 2` is not readable off the metric. The second force is the stronger discriminator anyway — if the second departure had been left pending rather than folded in, it would return `true`. Together with `participant_left` delta exactly 1 and each remaining member receiving exactly ONE update, that proves coalescing with no timed negative wait. The `coalesced == k` value itself is pinned at unit level (`rotation.rs::departures_in_one_window_coalesce_into_one_rotation`). Recorded against @test's original ask.
  - A grace-period member yields `participant_gone`.
  - `dropped_outbound` is driven behaviourally (@test 4), since it is `MCKekPushFailureRate`'s numerator and would otherwise exist only as a zero-init render. Direct precedent: `actors/participant.rs`'s `bare_actor_with_stream(1)` + overflow, the shape `outbound_signaling_drop_is_counted_not_merely_warned` already uses — a one-slot outbound channel, filled, then a KEK push, asserting `outcome="dropped_outbound"` and `payload_kind="meeting_kek_update"` together.
  - **The strengthened-C-1 test (@paired-client's case)**: a carried-over member leaves AFTER the epoch reset, the cursor passes its id, and the id is NOT reissued — proving the skip set is permanent rather than liveness-re-checked. Plus the reset itself with ≥2 live participants, **one of them in grace** (@security D-4) — a single-joiner test is vacuous against D-1.
  - The full 16-bit-space reissue test via the `test-seams` cursor: admission never fails, the rotation and reissue both occur.
  - A deliberately NON-responding participant actor — alive, open mailbox, never answers (@test 5, the discriminating fixture): the duration histogram AND the INFO log still emit, the already-collected `delivered` outcomes are still recorded, and the non-responder is counted `timed_out`. The happy-path S6 test passes identically with and without the bounded wait, so only this case distinguishes them.
  - `reason="generation_exhausted"` asserted on the existing `u16::MAX` fail-closed unit test (@test 6) — one added assertion, no new fixture. The `rng` arm is not driven deterministically.
  - `actor_unavailable` is covered at unit level (a dropped oneshot sender; an unanswered receiver past the deadline) rather than through a real actor teardown race, which would be timing-dependent for one label value. Flagged to @test as the proportionality call.
  - JoinResponse carries W; a joiner after a rotation receives gen 1.
- **Actor-level**: reconnect returns the current KEK and generation plus W, and no rotation happens (counter unchanged, debounce untouched).
- **Config**: bounds, required, non-numeric.
- **Env-test** `crates/env-tests/tests/34_mc_kek_rotation.rs`: two users join, B closes; A reads until `MeetingKekUpdate`, with a deadline derived from the `mc_meeting_kek_rotation_window_seconds` gauge plus a margin (never a literal W). It asserts generation = join generation + 1 and W = the gauge, then that Prometheus shows `trigger="participant_left"` and `outcome="delivered"` increased (≥ 1, since the cluster is shared). Deadline expiry is a HARD failure (@test 3): the frame receipt is the positive control and is a `require`, never an `if received { check }` falling through to a soft or skipped assertion.

#### sdk-core test cases (@paired-client)

Node-tier vitest only; no browser/e2e change. Every expiry is driven through `MeetingKekHolder`'s injected clock and timer seams, never wall-clock (ADR-0028 zero-retry). Each case names the **mutation** it goes red against, so the list can be checked rather than trusted. Every rejection case has an accepting twin one field apart, because a suite of rejections alone passes against a receiver that rejects everything (the discipline `vectors.conformance.test.ts` already states for `replay_same_stream_sequence`). No case primes the cache through `set()` where a production path exists; B3 and B5 are the deliberate exceptions, because they test `set()` itself.

Files: `media/frame/__tests__/receivePath.test.ts` (unit), `media/frame/__tests__/receivePathIntegration.test.ts` (composed, real signed frames), `media/setup/__tests__/kekSource.test.ts` (holder events), new `media/lifecycle/__tests__/` spec (pipeline atomicity).

**The Lead's Gate-1 items, mapped to cases:** (1) cross-scope replay both directions → D1 (admitted) with A2 and D2 (dropped), plus A1/A3; (2) first-set-wins → B1, B1′ and B2; (3) `#bySender` bitmap scoping → A4; (4) retention, not LRU, governs cross-generation admission → C6; (5) `wrap_generation_conflict` increments on the refusal → B6; atomic discard tied to the single expiry event, including the `RetentionGuard` splice → C1, C3 and C4.

**A. Replay scoping (unit)**

| Case | Asserts | Red against |
|---|---|---|
| A1 | Same `(sender, stream, txgen, seq)` admitted in scope g, then offered in g+1 → ACCEPTED | No scoping |
| A2 | Then re-offered in g → REJECTED | Scoping by clobbering instead of partitioning |
| A3 | Reverse order: admit in g+1, then g → ACCEPTED; g again → REJECTED | Scope taken from the *current* generation instead of provenance (one bucket for both directions) |
| A4 | In g, admit seqs 5..10 at txgen T; in g+1, admit txgen T at seq 5 → ACCEPTED | High-water scoped but `#bySender` bitmap not; the new holder's early frames read as **duplicates** |
| A5 | In g, admit txgen 100; in g+1, admit txgen 0 → ACCEPTED | The shipped code (unscoped high-water); S-1 at unit level. Different failure signature from A4 |
| A6 | Within one scope, the high-water still survives LRU context eviction | Scoping weakening the survive-eviction property @security required |
| A7 | Per-sender insider-eviction (F1), unchanged | Regression of the existing boundary |
| A8 | Flooding scope g+1 for one sender does not evict that sender's scope-g state | Budget shared across scopes. That recreates F1 with `generation` in place of `stream`: flood your own g+1, evict your retained g, replay a captured g frame |

Budget accounting is per `(scope, sender)`. That does not grow without bound, because a bucket exists only for a generation that was **held** when the bucket was created, and every scope is discarded when its generation leaves retention. Live scopes per sender are therefore at most two, and memory is bounded at `2 × maxContextsPerSender`. The LRU stays a plain memory bound, not a security boundary.

**B. One entry per key id, plus F-8**

| Case | Asserts | Red against |
|---|---|---|
| B1 | Entry cached in scope g; a wrap for the **same key id** announcing held g+1 → REFUSED: no unwrap (spy on `kekForGeneration`), one entry, scope still g, key bytes unchanged, outcome `wrap_generation_conflict` | Scoping with the pre-E-1 unconditional `cache.set`, which reports `cached` and drags provenance to g+1. "No unwrap" is asserted through its consequences (same scope, same key object), not a `kekForGeneration` spy: the refusal legitimately QUERIES the announced generation to fork conflict from not-held, so a lookup count cannot separate the two |
| B1′ | Accepting twin, two variants: (i) a **new sender's** first key id under held g+1 → ACCEPTED and cached with scope g+1; (ii) the **same sender** rotating to its next txgen (a new key id) under held g+1, while its g entry is still cached → ACCEPTED, second entry with scope g+1, the g entry untouched. No `wrap_generation_conflict` in either | (i) catches "refuse any wrap newer than the cached maximum", which breaks rotation outright. (ii) catches "refuse a newer wrap for a *sender* with any cached entry", which (i) cannot see because a new sender has no entry. It also breaks every honest R-13 rotation. Only "refuse a re-wrap of a *key id* that already has an entry" passes all three of B1, B1′(i) and B1′(ii) |
| B2 | Composed: a frame without a wrapped key is admitted in g; a key-bearing frame re-wraps the same key id under held g+1 (refused, B1); the first frame is replayed → `replay_detected` | Scoping with the pre-E-1 overwrite: the replay resolves to a bucket that never saw it and is ADMITTED with a valid signature and tag. Composed rather than a cache assertion, because B1's cache state can hold while the admit path still consults the wrong bucket. GREEN against shipped code by construction (latent defect); RED against the scoped-overwrite mutant — see the evidence below |
| B3 | F-8: populate (g,X), (g+1,X), (g,Y); `purgeSender(X)` → both X scopes gone and both buffers zeroized (captured by reference beforehand); Y untouched | The narrow "purge the current scope" reading. An assertion against the current scope alone PASSES that bug, so this one targets the **old** scope |
| B4 | After `purgeSender(X)`, a frame previously admitted in (g,X) is still rejected | A purge that also clears replay state, which re-opens rebind-back replay |
| B5 | Same generation, different wrap bytes: identical unwrapped key → scope kept, and `wrap_generation_conflict` does **not** increment; different unwrapped key → old buffer zeroized | Over-triggering the conflict counter on a benign re-set; the zeroization leak. The different-bytes branch is foreclosed by MC invariant 1 and kept as defence in depth, which the case says so it is not deleted as dead code |
| B6 | Pipeline level: the refusal increments `dt_client_media_key_wrap_outcomes_total{outcome="wrap_generation_conflict"}` by exactly 1, and the drop counter does not move | The value missing from `ReportableWrapOutcome`, or the refusal counted as a drop, which breaks R-25 `received = accepted + sum(drops)` |

**C. C-3 atomicity**

| Case | Asserts | Red against |
|---|---|---|
| C1 | `generation-dropped(gen)` fires exactly once, with the dropped generation, from each of the four paths: expiry timer, lazy `#expireIfDue` backstop, demotion dropping the old previous, and the `RetentionGuard` fail-safe splice (reusing the existing fabricated over-retained list) | A path that drops a generation silently. The splice is the likeliest to be missed, and it leaks a scope |
| C2 | Inside retention: no event fires, and a frame under the previous generation still opens | Discarding **earlier** than retention |
| C3 | Pipeline: a real frame under g leaves both a key entry and replay state in scope g; the injected clock passes retention; in one observation both are gone, and a frame under g now drops `kek_generation_stale`, not `replay_detected` | Discarding **later**, or separately. The token proves the drop happens before replay state is consulted (the `admit()` reorder), not just that state was eventually discarded |
| C4 | The holder's timer seam fires once per demotion, and the pipeline registers no timer of its own | A parallel expiry path, which C-3 forbids and C3 alone cannot see |
| C5 | Teardown clears every scope | Scopes surviving teardown (ADR-0028 §5) |
| C6 | Retention, not LRU, decides cross-generation admission: flood g's context budget until g's bitmaps are LRU-evicted, then replay a g frame while g is still retained → REJECTED (the high-water survives); after g leaves retention → `kek_generation_stale` | Scope lifetime driven by an LRU over generations ("keep the N most recently seen"). That drops g under pressure while g is still retained, or keeps it after g has expired |

**D. The S-1 cases (composed)**

| Case | Asserts | Red against |
|---|---|---|
| D1 | The incumbent holds (g,X) with a high txgen from the departed holder; the epoch resets to g+1; the **new** holder of id X sends txgen 0 under g+1 → ACCEPTED | The shipped code, which rejects it permanently. This is the fix stated as a test |
| D2 | A frame captured from the departed holder under g is replayed after the rotation: while g is retained → `replay_detected`; after g expires → `kek_generation_stale` | Scope recomputed from the *current* generation at admit time. The frame then lands in a fresh bucket and is admitted. Asserting the specific token is what proves provenance: "was dropped" alone passes an implementation that drops it for the wrong reason |

#### sdk-core red-before-green evidence (@paired-client)

Captured output: `evidence/client-red-before-green.txt`; the mutants are generated by `evidence/client-mutants.py`, which rewrites `receivePath.ts` from the fixed version. Each mutant was applied, run, then reverted, and the fixed file was byte-compared after restore. Form accepted by the Lead and @test: red against SHIPPED code for the S-1 cases, and red against the MUTANT each case exists to kill for the E-1 cases, because those are latent in shipped code.

| Run | Red (×) | Green (✓) |
|---|---|---|
| Shipped `receivePath.ts` (git HEAD) | D1 (`replay_detected`: S-1 reproduced), C7 (the reordered previous-generation frame dropped by the unscoped high-water) | B2, B1′(i), B1′(ii). B2 is green by construction: the defect is latent. B1′ cases accept by construction |
| Mutant `scoped_overwrite`: scoping, but the pre-E-1 overwrite and no refusal arm | B2, B1 | B1′(i), B1′(ii) |
| Mutant m1: refuse any wrap newer than the newest cached scope | B1′(i), B1′(ii) | B2, B1 |
| Mutant m2: refuse a newer wrap for a sender with any cached entry | B1′(ii) only, which shows (i) cannot see m2 | B2, B1, B1′(i) |
| Fixed | none | D1, B2, B1, B1′(i), B1′(ii), C7, C7 twin/D2, D2 expired |

Case changes during implementation, each for a stated reason:
- **Two cases added after implementation** for @security's Gate-3 checks, in `receivePathIntegration.test.ts`: a frame whose scope expires mid-processing reports `kek_generation_stale` (not `no_transmit_key`, and not `decrypt_failed`), and such a refused frame leaves no bucket behind in the just-discarded scope.
  - **@security's hazard (a) was reframed, not fixed as stated.** The zeroize-then-open sequence is NOT reachable through `MeetingKekHolder`: its lazy expiry runs BEFORE the lookup, so it never returns a usable KEK for a generation whose entry the listener just zeroized. The first draft of the test faked the opposite order; the fake was unfaithful, not the code. What WAS live: a wrapless frame carries no generation, so the cached scope is the only thing that can name one, and re-reading it after the lookup (finding the entry gone) downgraded a retention expiry to `no_transmit_key`. The scope is now captured before the check and used for the drop. The re-read remains as defence for any other `ReceiverKeys` implementation reaching the seam.
  - **A claimed narrowing of the re-wrap residual was WITHDRAWN.** @paired-client proposed that failing closed on a key id bound to an expired generation narrows it; @security approved the BEHAVIOUR and declined the narrowing, and the reason is @paired-client's own frame-driven-expiry change: the expired entry is discarded at the FIRST frame after expiry, so the window closed is about one frame wide. After that the key id is unbound, a wrap under the live generation is a FIRST BINDING rather than a conflict, and a fresh entry and bucket are created — exactly the residual. **The residual stands unchanged, with a better justification than the original bounded-impact one: it is STRUCTURAL.** Once the old binding is discarded, nothing distinguishes a sender legitimately using a new key id from one re-wrapping a key id it used before — the binding WAS the memory. Closing it requires remembering expired key ids indefinitely, i.e. the unbounded per-sender state the context budget exists to prevent. Accepted because closing it costs an unbounded set and its impact is confined to a non-conforming sender's own media, NOT because it is small. Recorded at `receivePath.ts`'s expiry arm so the code cannot be read as closing it.
  - **One mechanism, three dispositions** (@paired-client's generalization, corrected by @security before recording). *Discarding state necessarily discards the ability to tell a legitimate new binding from a returning one — the binding WAS the memory.* Three sites in this diff share that mechanism and resolve DIFFERENTLY, and the difference is the point:
    1. the re-wrap residual — **ACCEPTED**: closing it costs unbounded per-sender state, so the blind spot stays and the bounded-impact argument is what makes it tolerable;
    2. `setup/rosterKeys.ts`'s forget path — **MITIGATED, not accepted**: the same blind spot, neutralized by purging the transmit keys so it is harmless ("a re-add is indistinguishable from a first binding and the rebind check would be blind to it", therefore forget the keys too);
    3. `TransmitKeyCache` eviction — **SAFE BY CONSTRUCTION**: nothing that matters is lost, because re-deriving the key requires a signed key-bearing frame from the sender, and an attacker who can produce one never needed the cache.
    @paired-client first wrote this as "all accepted on the same ground"; @security declined that flattening, because a reader meeting a FOURTH instance would take the pattern as "we accept binding-loss blind spots" and skip the analysis that produced three different answers. **The pattern is a prompt to ask the question, not a precedent for the answer** — and the fact that two of the three were NOT accepted is what makes accepting the first a cost judgement rather than a habit. Recorded at the `receivePath.ts` expiry arm, pointing at the other two sites.
  - **@security's check (b)**, listener non-reentrancy, is structural and documented at the call site: two synchronous map deletions, no await, no emit, no path back into the ingress, registered exactly once.
- **B1′(ii) tail.** It now asserts cache state instead of opening an old-key frame, because opening one would couple it to cross-generation admission, which C7 owns.
- **C7 added.** A previous-generation frame reordered after the first next-generation frame is ACCEPTED while retained. It is red on shipped code. @security reframed this: today's drop was the anomaly, because the unscoped high-water negated what ADR-0036 §4 buys retention for.
- **C7 negative twin added** at @security's request: the same frame replayed is still `replay_detected`.
- **Pipeline-level cases** (D1, C3, C6, B6) live in `audioPipeline.multiSender.test.ts`, not a new spec, to reuse its receiver harness rather than duplicate it.
- **The RetentionGuard splice path (C1)** is asserted on `evaluate`'s returned generations. No honest holder path over-retains, so the holder's one-line forwarding of that return is covered by structure, not by driving it.

### Docs and handoffs
- `mc-deployment.md` (OPS-B, OPS-G): W row; seventeen Required rows; sample ConfigMap line; §Coordination required-key bullet; post-deploy echo check.
- `configmap.yaml` (OPS-C): three stale task-9 passages corrected.
- `checks.md`: the credential-leak items 11-13 scope names the rotation, push-encode, lifecycle-log and reconnect sites.
- `alerts.md`: the complete-page-set line gains `MCKekRotationOverdue`. Inventory entries remain task 16's; the page-list line is fixed now because it would otherwise be false.
- TODO: close :1298; rewrite :1392 so (a), (b) and (c) are ALL discharged; re-express :1390's secondary trigger against `MCKekRotationStorm{trigger="sender_space_exhausted"}` and `mc_meeting_sender_ids_issued_max`, leaving its primary trigger and the deferred join-rate-limit threat intact (@security F-6); add @paired-client's roster-removal delivery residual.
- Story notes plus this record: the flapper no-eviction decision and residual, as shipped under (A'). The debounce bounds leave-triggered rotation to one per W. Exhaustion-triggered rotation is immediate — an epoch reset that admits the joiner — but self-limited to once per 65,536 admissions. Nothing bounds a flapper's join-path cost: O(N) recompute plus a push per cycle, with leverage that grows with meeting size. Visibility comes through `mc_meeting_sender_ids_issued_max`, deliberately without an alert because exhaustion self-repairs.
- Operations task 18 MUST make the full Scenario 8 correction (R-16 lands, so story line 591's condition is met); this diff carries only the dated marker.
- No metric, log, doc or annotation calls rotation forward secrecy, end-to-end or zero-trust.

### Security Decisions

| Decision | Choice | Rationale |
|---|---|---|
| RNG source for rotated KEK | `ring::rand::SystemRandom` (ADR-0027), same as meeting creation | OS CSPRNG; no fallback RNG, no default key (ADR-0002) |
| RNG failure during rotation | Fail closed: no key change, rotation stays pending, retried after W, ERROR log, `mc_meeting_kek_rotation_failures_total{reason="rng"}`; overdue page fires | Never a predictable or reused KEK. **F-3: the EXPOSURE, not just its detectability, is stated at the site** — while rotation keeps failing every departed participant retains a KEK that opens all current media with no forward bound, so the W bound is **suspended, not delayed**, and the overdue page is the only control. Terminating the meeting instead would be a worse DoS |
| Generation overflow | `checked_add`; at `u16::MAX` rotation fails closed, `reason="generation_exhausted"` | A wrapped generation collides at the client holder — and under (A') receivers scope replay state by (kek_generation, sender_id), so a wrap would alias that scope. Permanent for the meeting actor, so the overdue page cannot clear until the meeting ends; the F-3 exposure sentence covers this arm too |
| Epoch-reset skip set | Captured once from the roster (live + grace), permanent for the epoch, never liveness-re-checked; fails closed if nothing is allocatable | Strengthened C-1: within one generation a `sender_id` is bound to at most one identity EVER (@paired-client). A grace member reconnecting onto a reissued id would put two live senders on one id under one KEK — authentication-key recovery, not a confidentiality loss |
| Old KEK lifetime in MC | Replaced `Arc` dropped at rotate; zeroize-on-drop via `SecretBox`; no previous-KEK field | Retention is client-side only (R-14) |
| Recipient set | `self.participants` after removal at the choke point | Leaver unreachable by construction |
| KEK transit inside MC | `Arc<MeetingKek>` in the participant mailbox; bytes materialised only at encode | No plain `Vec` of KEK bytes in a mailbox; the encode-buffer copy is the same stated residual as `JoinResponse` |
| Undelivered push | Counted per recipient; residual stated at the emission site | A non-rotating member extends a leaver's exposure past W (S-3) |
| `sender_id` reissue (R-16) | SHIPS whole under the Lead's (A') ruling, paired with client-side receiver scoping | Reissue without a receiver-side reset silently and permanently deafens incumbents (S-1); (A') closes it with no wire change |
| Reconnect | Current KEK, generation and W re-issued; no rotation | R-15; TODO consequence (c) |
| Telemetry | No key bytes, no generation label, no participant or meeting label; `key_custody=operator` | ADR-0036 §11 |

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

MC half (meeting-controller). The sdk-core half is recorded by @paired-client in §Tests and the evidence directory; the ADR / proto comment half by @paired-protocol.

### KEK lifecycle (R-12, R-14, R-15)
| Item | Before | After |
|------|--------|-------|
| `MeetingKeyState` | generated once, generation always 0 | `rotate()`: fresh CSPRNG KEK, generation +1 via `checked_add`, atomic and fail-closed (`KekRotationFailed::{Rng, GenerationExhausted}`), old `Arc` released |
| KEK + `sender_id` allocator | two independent actor fields | one `AdmissionEpoch`; the allocator can be reseated only by its atomic epoch reset |
| Leave handling | `remove_and_broadcast_left` removed + broadcast | also records the departure in `KekRotationDebounce` (anchored on the OLDEST un-rotated removal, never reset) and the fleet registry |
| Rotation | none | sibling `sleep_until` arm in `MeetingActor::run` → `rotate_due` → `push_kek_update` over the post-removal roster; not gated on `is_shutting_down` (F-5) |
| KEK push | none (no push message was sent) | `ParticipantMessage::KekUpdate { Arc<MeetingKek> }` → `handle_kek_update` encodes `MeetingKekUpdate`; one outcome per recipient, collected by a detached task with a 2 s bound |
| `JoinResponse.kek_rotation_debounce_seconds` | hardcoded `0` | W from `Config::kek_lifecycle` — the same value as the timer and the window gauge |
| `ReconnectResult` | no KEK | current KEK, generation and W; no rotation, debounce untouched |
| `MC_KEK_ROTATION_DEBOUNCE_SECONDS` | in the ConfigMap, unread | required, `bounded()` 30..=300 |

### Sender-id rotate-and-reissue (R-16)
| Item | Before | After |
|------|--------|-------|
| Exhaustion | terminal reject, `McError::SenderIdSpaceExhausted`, join fails | immediate KEK-epoch reset; the joiner is admitted |
| Reset allocator | n/a | starts at 1 and permanently excludes every id bound at reset (live + grace); no liveness input by construction; fails closed if nothing is allocatable (D-3) |
| Ordering | n/a | incumbents are pushed the new KEK BEFORE `ParticipantJoined` of the joiner (asserted on the wire) |
| `sender_id_space_exhausted` | error type, zero-init entry, alert, catalog row | retired at all 13 sites plus a 14th found in `connection.rs` |

### Telemetry (R-26)
New: `mc_meeting_kek_pushes_total{outcome}` (5 values), `mc_meeting_kek_rotation_failures_total{reason}`, `mc_meeting_kek_rotation_coalesced_leaves`, `mc_meeting_kek_rotation_duration_seconds`, `mc_meeting_kek_rotation_{pending_age,window,overdue_threshold}_seconds`, `mc_meeting_sender_ids_issued_max`. Extended: `mc_meeting_kek_generated_total{trigger}`, and `mc_participant_outbound_messages_dropped_total{payload_kind}` split into `participant_update_{joined,left}` plus `meeting_kek_update`, with the label derived at `encode_participant_update`. Every new series carries `key_custody=operator`. Alerts: `MCKekRotationOverdue` (page), `MCKekPushFailureRate` and `MCKekRotationStorm` (warning), `MCKekEpochResetOnSenderIdExhaustion` (info); `MCSenderIdSpaceExhausted` retired. A KEK Lifecycle dashboard row, catalog entries, and Scenarios 17 (stub) and 19 (full), plus the Scenario 8 marker.

### Additional Changes
- **DRY D**: `LeaveReason::ALL` / `DisconnectCause::ALL` hoisted; the in-domain arrays and `slot_*` witnesses in `metrics.rs` are removed.
- **Test W home**: `mc-test-utils/src/kek.rs`. The `src/` fixture passes the `Duration` across, not the type, because `src/` unit tests see a second copy of `mc_service` through `mc-test-utils`.
- **Histogram buckets** use `Matcher::Prefix` on each histogram's FULL name. I first wrote `Matcher::Full` (as observability's Gate-1 P6 suggested), the bucket guard reported both histograms unconfigured, and I initially called that a guard false positive. **It is not** — corrected by the Lead: `histogram_buckets.rs`'s module doc records Prefix-only as a deliberate @observability S2 decision (no equality requirement, no `Matcher::Full`). So Prefix-on-the-full-name is the correct compliance, not a workaround; it still gives each histogram its own matcher, which was the point of P6. No TODO.
- **`MetricAssertion` drain-on-read**: every snapshot read drains ALL histograms, so at most one histogram query per snapshot reads correctly and it must come first. This is recorded at the top of `kek_rotation_integration.rs`, and cost two false failures to learn.

---

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

### Gate 2 (Lead) — attempt 2 (final tree, after Gate-3 fixes incl. F-1 `HandedOutBindings`)

PASS, exit 0 (log `/tmp/gate2-run2.log`). L1 OK · L2 OK · L3 OK · L4 cargo-test + nx-test OK · L5 OK · L6 audits + buf-breaking OK · L7 env-tests-passed + browser-e2e-passed (1245s). No FAIL/PRECONDITION lanes. `run-guards.sh` re-run after the Lead's main.md verdict edits: 0 violations.

### Gate 2 (Lead) — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, attempt 1

PASS, exit 0 (log `/tmp/gate2-run1.log`). L1 OK 6s · L2 OK 5s · L3 OK 119s · L4 cargo-test OK + nx-test OK (aggregate N/A from proto placeholder) 272s · L5 OK 7s · L6 cargo/pnpm audit + buf-breaking OK (aggregate N/A) 3s · L7 env-tests-passed + browser-e2e-passed 1424s. New env-test `34_mc_kek_rotation` passed (118.8s). TOTAL_RESULT=N/A = worst-child aggregate of the documented `not-applicable-to-this-lang` proto placeholders; no FAIL/PRECONDITION lanes.


Implementer self-check: `./scripts/layer-fast.sh` (layers 1-6) over the WHOLE diff — all three halves (MC, sdk-core, ADR/proto) — **EXIT 0, 48 `STATUS=OK`, zero non-OK/N/A**. Layer 7 is the Lead's Gate 2.

### Layers 1-2: check / format
**Status**: PASS — `cargo check --workspace --all-targets`, with and without `test-seams`; `cargo fmt`; prettier / `buf format` (paired halves).

### Layer 3: Guards
**Status**: ALL PASS. Directly re-run after the gate: cross-boundary-scope (no drift), cross-boundary-classification, application-metrics, metric-coverage, histogram-buckets (27 configured), dashboard-panels, counter-zero-init, metric-labels, alert-rules (4 files loadable), env-config (4 services / 6 workloads — `MC_KEK_ROTATION_DEBOUNCE_SECONDS` now has rule-1 coverage), knowledge-index, todo-tracking, doc-citation guards, gsa-sync.

### Layers 4-5: tests
**Status**: PASS — `cargo-test-passed`, `nx-test-passed`. MC lib 479; `kek_rotation_integration.rs` 8; `media_admission_integration.rs` incl. the rewritten exhaustion-admits test; every other mc-service binary green. sdk-core 871 vitest (@paired-client).

### Layer 6: lint / audit / breaking
**Status**: PASS — `cargo-clippy-passed` (`-D warnings`), `nx-lint-passed`, `buf-lint-passed`, `buf-breaking-passed`, cargo/pnpm audit. Note @paired-protocol's evidence that `buf breaking` is vacuous for the two edited protos (`breaking.ignore`); no-wire-change was proven by byte-identical descriptors instead.

### Layer 7: Env-tests
**Status**: Lead's Gate 2. New: `crates/env-tests/tests/34_mc_kek_rotation.rs` (`flows`; compiles and clippy-clean; deadline derived from the W gauge, frame receipt a hard requirement; 2 AC registrations).

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 9 (F-1..F-9) + S-1 | 9 | 0 | S-1 → Lead ruling (A'); r1 accepted design property (closing needs unbounded state) |
| Test | CLEAR | 0 | 0 | 0 | re-verified after F-1/F-2 |
| Observability | RESOLVED-FIXED | 6 (P1-P5, payload_kind) | 5 | 0 | P4 withdrawn (premise removed by A') |
| Code Quality | RESOLVED-DEFERRED | 1 | 0 | 1 | `#[allow]`→`#[expect]` test-module sweep, ~138 sites / 4 crates |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | extraction-opportunity TODO entry updated (not a deferral) |
| Operations | RESOLVED-DEFERRED | 2 (OPS-R, OPS-S) | 2 | 1 | records the dropped-KEK-push no-re-push gap (paired-client's) |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |
| Paired Client | RESOLVED-DEFERRED | 1 | 0 (operator-facing part fixed) | 1 | dropped `MeetingKekUpdate` never re-pushed |
| Paired Protocol | RESOLVED-DEFERRED | 2 (F-1, F-2) | 2 | 1 | F-1 residual: lost MH `Disconnected` leaks an id, no reconciliation |
| Media Handler (GSA co-owner, Gate-3 pull-in) | RESOLVED-FIXED | F-1 (retracted initial CLEAR) | 1 | 0 | mh-service session/mod.rs comment rewritten |

Attribution: Gate-3 F-1 (MH binding outlives roster removal → reissued id refused at MH) was **@paired-protocol's** finding; @implementer implemented `HandedOutBindings`; @media-handler confirmed the unbind-before-notify ordering. Lead ruled it fix-in-loop.

Lead ESCALATED-path handling: @media-handler's interim ESCALATED on F-1 converged to RESOLVED-FIXED after the in-loop MC fix; no Lead override was needed.

Deferral assessment (Lead): all three deferrals are task-sized with cross-service/contract components (MC↔client delivery semantics; MC↔MH reconciliation contract, a new `proto/**` GSA change; a four-owner mechanical sweep whose partial form would be a half-applied invariant). Each interim state is fail-safe and visible (alert/runbook/TODO trigger).

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

**This devloop's pointers:**

- `docs/TODO.md` §Media Path Obligations — dropped KEK push / roster removal never re-sent (droppable channel)
- `docs/TODO.md` §Media Path Obligations — MC handed-out-binding ledger only grows on lost MH Disconnected
- `docs/TODO.md` §Code Quality — codebase-wide test-module `#[allow]`→`#[expect]` sweep (~138 sites)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **THE SAFE-REVERT UNIT IS ALL THREE HALVES — MC, the SDK, and the alert rules. Revert MC first, or revert everything together.** (@operations OPS-Q, revised on @security's catch; stated as one directional rule rather than three warnings, so a reader who holds the direction can derive the cases instead of matching a list.)
   - **Never revert the SDK half alone** while MC is still reissuing `sender_id`s. That reproduces the §Coordination MC↔SDK skew, arrived at by revert instead of by deploy order, and it is **permanent and undetectable** for the affected sender. This is the likelier partial revert by far — the natural reflex when a client bug surfaces is to roll the SDK back and leave the server — which is why it leads.
   - **Reverting the MC half alone is safe for the client interaction, and the asymmetry is the non-obvious part.** Verified: `MeetingKeyState.generation` is a `u16` that pre-task-9 MC never advances, so a client-only-forward fleet scopes everything as `(0, sender_id)` — degenerate but correct. First-set-wins does not misfire there either: audio carries the wrap on every frame, but under an old MC every re-wrap for a given key id is byte-identical (one transmit key wraps to one ciphertext within a generation), so `matchesCachedWrap` hits and first-set-wins never sees a conflicting second wrap.
   - **But an MC-alone revert MUST take `infra/docker/prometheus/rules/mc-alerts.yaml` with it** — the OPS-O hazard in the revert direction. Reverting MC re-arms the terminal `SenderIdSpaceExhausted` reject while `MCSenderIdSpaceExhausted` has already been retired from the rules, so an exhausted meeting refuses admissions permanently **with nothing firing at all**. `git reset --hard {start_commit}` is safe because the rules live in-tree and it takes all three; a *selective* MC revert is not, because the rules self-roll from a separate content-hashed artifact.
   - **Never cherry-pick the receiver-scoping change without first-set-wins** (@security's same-commit constraint): the intermediate is strictly worse than either endpoint. Lower in the list than it first appeared, because it requires splitting one logical change inside one file, which nobody reaches for.
   - §Rollback Procedure and §Coordination are therefore **the same constraint read in opposite directions.** The code-site note stays as well as this section: the code site serves the bisect case, where nobody opens main.md at all. Different reader, different moment.

---

## Issues Encountered & Resolutions

### Gate 3 — implementer resolutions (meeting-controller half)

| Finding | From | Resolution |
|---|---|---|
| **F-1**: a reissued id is refused at MH because a departed holder's MH binding outlives its roster removal | @paired-protocol (confirmed with @media-handler; Lead: fix here) | **FIXED.** New `HandedOutBindings` (`media_admission/epoch.rs`), held as `MeetingActor.mh_bindings`: recorded where MC ANSWERS a binding (`handle_media_connected`, even if connectivity is refused), released FIRST in `handle_media_disconnected` before the roster lookup. The epoch reset's exclusion set is now roster ∪ `mh_bindings.held()`. It fails TOWARD exclusion (MH's `Disconnected` is best-effort), so a lost notification leaks one id, never a refused joiner. A departed holder's close that releases a binding now counts as applied, not `UnknownConnection`. Test `an_id_still_bound_at_a_media_handler_is_not_reissued_until_the_handler_releases_it` — **confirmed red with the fix disabled** (C was handed 65535), green with it. Reconciliation against MH's authoritative state is task-sized (contract change) → `docs/TODO.md` §Media Path Obligations. MH's `session/mod.rs` comment rewritten by @media-handler to the same mechanism. |
| **F-2**: `window_seconds()` saturated a narrowing, so a W larger than the enforced one could reach the wire | @paired-protocol | **FIXED.** `KekLifecycle` holds W as the wire's own `u32` seconds; `Config` parses it as `u32` through `bounded()`; the `Duration`, the threshold and the gauge are all WIDENINGS of that one number. No narrowing exists. The test W's home is `TEST_KEK_ROTATION_WINDOW_SECONDS: u32`. |
| Dropped `MeetingKekUpdate` is never re-sent | @paired-client | **Runbook line added** (Scenario 16 + `MCKekPushFailureRate` `impact`: no re-push; recovery only at the next rotation or a reload; indefinite in a quiet meeting). Recovery itself **deferred**, task-sized (non-droppable control lane / close-on-overflow, MC + client halves) → @paired-client's `docs/TODO.md` entry. RESOLVED-DEFERRED. |
| OPS-R: `mc-deployment.md` restated the Required count a second time ("sixteen") | @operations | **FIXED** by deleting the copy: the §3 warning now points at §Configuration Reference, the one place the count lives. |
| OPS-S: `mh-alerts.yaml` comment cited the new info rule on the retired rule's irreversibility ground | @operations | **FIXED**: the shared ground is stated as the smoothing only; irreversibility is scoped to MH's own rule. |
| DRY 1: stale (B)-shaped text in this record | @dry-reviewer | **FIXED** (the leftover conditional and the "exhaustion is still terminal" bullet). |
| DRY 2: the test W claimed to BE the ConfigMap value | @dry-reviewer | **FIXED**: documented as representative and deliberately untied, safe because no test waits on it. |
| DRY 3: `KekLifecycle::evict` vs `PolicyGenerations::remove_meeting` | @dry-reviewer | **FIXED**: renamed to `remove_meeting`. |
| `#[allow]` vs ADR-0002's `#[expect(…, reason)]` in test modules | @code-reviewer | **Spun out**, as the reviewer offered. The reviewer's count (5 sites) was re-measured before choosing: **66 outer + 72 inner** `allow(clippy::unwrap_used…)` across four crates against one `expect`. Neither the three new files nor mc-service alone can be converted without a partial invariant → `docs/TODO.md` §Code Quality. |
| Histogram buckets: I called `dt-guard`'s Prefix-only rule a false positive | Lead | **Withdrawn — my error.** It is a documented @observability S2 decision; Prefix-on-full-name is the correct compliance. Comment and record corrected. |

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

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
