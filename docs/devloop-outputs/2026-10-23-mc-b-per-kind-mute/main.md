# Devloop Output: Story 3 MC-B — per-kind mute controls at MC, directive withdrawal, superseded proto field deletion

**Date**: 2026-10-23
**Task**: run-story task #15 (MC-B) of `docs/user-stories/2026-10-07-see-each-other.md`; full task text: `~/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-15.prompt`
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~2 sessions (iteration 1 escalated at Gate 2; iteration 2 resumed on the operator's decision)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `69fea9e9895af9dfa9e9c660372c93ff1f156ff0` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

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
| Security | `security` (paired, expanded role) |
| Test | `test` (paired, expanded role) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired | `paired-media-handler` (security, test paired in their panel slots) |
| Conditional | `protocol` (GSA: proto/**), `client` (SDK mute request + packages/proto-gen) |

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
| `crates/mc-service/src/**` | Mine | — |
| `crates/mc-service/tests/**` | Mine | — |
| `crates/mc-test-utils/src/**` | Mine | — |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | Mine (paired with test) | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `proto/dark_tower/internal/v1/internal.proto` | Not mine, Domain-judgment | protocol |
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment | protocol |
| `proto/**` (buf breaking intentional-break declaration, if it lives under proto/) | Not mine, Domain-judgment | protocol |
| `crates/proto-gen/**` (regen + roundtrip tests) | Not mine, Domain-judgment | protocol |
| `packages/proto-gen/**` (regen + verify-codegen.sh oracle) | Not mine, Domain-judgment | protocol |
| `packages/sdk-core/src/**` (single-kind mute report path, slotStateMap; generated `src/proto/**` is gitignored) | Not mine, Domain-judgment | client |
| `docs/protocol/CONVENTIONS.md` | Not mine, Domain-judgment | protocol |
| `proto/buf.yaml` (`MESSAGE_NO_DELETE` → `breaking.except`; human decision 2026-10-23, resume) | Not mine, Domain-judgment | protocol |
| `crates/mh-service/src/**` | Not mine, Domain-judgment (paired) | media-handler |
| `crates/mh-service/tests/**` | Not mine, Domain-judgment (paired) | media-handler |
| `crates/mh-test-utils/src/**` | Not mine, Domain-judgment (paired) | media-handler |
| `crates/env-tests/src/fixtures/**` | Not mine, Domain-judgment (paired) | media-handler |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Domain-judgment (paired) | media-handler |
| `crates/env-tests/tests/**` (27, 29 and any other sweep of deleted fields) | Not mine, Minor-judgment | test |
| `infra/services/mh-service/config.env` (comments only) | Not mine, Minor-judgment | media-handler |
| `docs/decisions/adr-0036-media-flow.md` | Not mine, Minor-judgment | protocol |
| `docs/API_CONTRACTS.md` | Not mine, Minor-judgment | protocol |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (DELIBERATELY NO ALERT comment gains the new kind-free counter) | Not mine, Minor-judgment | observability |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/*/INDEX.md` (protocol, client, media-handler, observability, code-reviewer) | Not mine, Minor-judgment | each INDEX's specialist |
| `docs/TODO.md` | Not mine, Minor-judgment | team-lead |
| `scripts/guards/validate-internal-proto-no-key-material.test.sh` (fixture sweep) | Not mine, Minor-judgment | security |
| `crates/media-protocol/src/frame.rs` (comment-only repoint) | Not mine, Domain-judgment | protocol |
| `packages/web-app/e2e/server-mute.spec.ts` (B goes FLAT under server mute: directive withdrawn) | Not mine, Domain-judgment | client |
| `packages/web-app/e2e/README.md` | Not mine, Domain-judgment | client |
| `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md` ("keeps sending" expectation, if present) | Not mine, Minor-judgment | test |

---

## Planning

Plan v1 (meeting-controller, 2026-10-23). Inputs folded from security (S1-S9), observability (O-1..O-9), test (1-9 + integration asks), dry-reviewer (1-4), semantic-guard (1-6), operations (1-6), paired-media-handler, protocol (P1, P2, re-homing).

### Mechanism restatement
Two rules, each completed everywhere:
1. **"A mute request names kinds; only named kinds move."** One parser (`MuteChange`), used by BOTH request paths, carried as-is into the actor, which merges against roster state (never against a connection cache). Today the fold lives in `webtransport/connection.rs::named_kinds` (`unwrap_or(false)`) and in the actor signatures `handle_self_mute(audio: bool, video: bool)` / `handle_server_mute(.., audio_muted: bool, video_muted: bool)` that overwrite both flags. (`ServerMuteAction::from_request` is already gone; OBS-A replaced it with per-kind `from_muted`.)
2. **"Every SUPERSEDED -- deleted by MC-B marker is gone, with its readers and writers."** Six wire items, not five: the prompt's four internal/signaling items plus `RegisterMeetingResponse.transport_mode = 5` (story Design wire freeze item 18, internal.proto "Field 5 is deleted ... by MC-B"; the story wins over the prompt). Acceptance grep: `git grep -nE 'MC-B'` returns only history docs and the GC "MC instance B" false positive; each deleted name appears only on `reserved` lines and in history.

### A. Per-kind request semantics (R-15, R-17)
- New leaf type `media_kind::MuteChange` (private fields `[Option<bool>; 2]`). The only constructor is `MuteChange::parse(audio: Option<bool>, video: Option<bool>) -> Result<MuteChange, NoKindNamed>`, so a both-absent request cannot be represented past the connection (S2, DRY-2). Accessors: `get(kind) -> Option<bool>` and `named() -> iterator of (kind, bool)`. `Debug` holds bools only (SG-2). `named_kinds` / `named_value` are deleted.
- **Client mute, connection** (`handle_mute_request`). Order: parse, then the no-op short-circuit, then the limiter, then the actor.
  - Both-absent: rejected before the limiter (no token spent), counted on the generic counter (below), and answered with a generic `ErrorMessage{INVALID_REQUEST, "Mute request refused"}` rationed by a new per-connection `mute_rejection_reply_limiter`. A suppressed reply is counted as `mc_media_refusal_replies_suppressed_total{surface="client_mute"}` (new `RefusalReplySurface::ClientMute`).
  - Short-circuit cache becomes per kind, `[Option<bool>; 2]`, and short-circuits only when EVERY named kind equals a KNOWN cached value (S3). It is seeded from the actor's reply, never defaulted to false.
  - `UpdateSelfMute` gains `respond_to`. The actor returns `MuteDecision { changed: [bool; 2], state: [bool; 2] }`. Per-kind `applied`/`unchanged` comes from the actor's decision, so the counts are correct after a reconnect within grace, when the cache is unknown.
- **Client mute, actor** (`handle_self_mute(participant, MuteChange)`). Merges only named kinds against the roster, is idempotent per kind, and runs no reconcile when nothing moved, so the generation does not advance. A moved kind is a structural change: reconcile (`Affected::HoldersOf`, as today) re-renders the disposition FORWARD<->PAUSED (`slots.rs` already maps client-only mute to PAUSED and never to DROP_AT_INGRESS), publishes to the latest-wins push workers, and flushes AVATAR_CLIENT_MUTED (or AVATAR_SERVER_MUTED if the server mute is also on) to the holders.
- **Server mute, connection.** Order: (1) authority (`is_host` from the token; requester = `media.participant_id`), (2) shape (both-absent), (3) limiter, (4) actor, which re-checks authority and then resolves the target (S1). A non-host gets the one generic refusal whatever it sends.
  - Counting: per named kind as today, and a both-absent request (host OR non-host) counts ONLY on the generic counter, never on a per-kind cell. That gives every request exactly one home (SG-3, O-2).
  - The both-absent reply is the existing `SERVER_MUTE_REFUSED_MESSAGE` via the existing rationed reply limiter. No new client-visible text.
- **Server mute, actor** (`handle_server_mute(target, requester, MuteChange)`). Authority comes first, then the target. Only named kinds are written. `server_muted_by` is set to the requester only when this request MUTES a kind (false->true). An unmute of one kind leaves it alone while the other kind stays muted, and it is cleared when neither kind is server-muted (S4). It remains the single writer of `*_server_muted` and `server_muted_by`. `ServerMuteDecision` keeps per-kind `changed`.
- **Generic counter (O-1 option a)**: `mc_media_mute_requests_rejected_total{surface="client_mute|server_mute", reason="no_kind_named", key_custody}`.
  - Kind-free on purpose: the request names no kind.
  - Built from bounded enums (`MuteSurface`, `MuteRejection`) with `ALL` and `label()`, and zero-initialised.
  - Ships with a recorder test, an mc-service.md row (CLIENT-INFLATABLE: never a ratio denominator, no alert) and a basic mc-media.json panel.
- `MuteOutcome::AppliedNoRecompose` is DELETED, together with its `ALL` entry, zero-init cell, catalog row and panel success-set text (O-3). The success set becomes `{applied}`.
- **Logs (S8, SG-2)**: the "Server mute decision" INFO and the refusal WARN log per-kind `Option`s (an absent kind shows as absent, not false). No new log line for self-mute transitions.

### B. Directive withdrawal on server mute (R-16, ADR §5 boundary)
- `directive.rs` gains a type `ServerMutedKinds` (a newtype over `[bool; 2]`). Its only constructor takes the server flags (`ServerMutedKinds::from_server_flags([bool; 2])`). `build_send_directive(..., server_muted: ServerMutedKinds)` emits EMPTY targets for a server-muted kind and leaves the stream listed with its encoding.
  - directive.rs still imports nothing that reaches `SourceMute`/`SourceMuteView`/client flags (S6). The caller in `meeting_media::flush_one` projects `entry.mute.server`.
  - The module doc is rewritten to "server mute and watch state, never client mute", and the matching sentence in `assignments.rs` is updated.
  - The edges stay in the render as DROP_AT_INGRESS (MC-A), so MH keeps counting `server_muted` drops for a patched client.
- `Affected` gains `PublisherAndHoldersOf(sender)`, used by server mute, so the TARGET is dirtied for its directive change. Client mute keeps `HoldersOf`, since its directive never changes.
- **Withdrawal counting (O-5)**: `withdrawn_kinds` stays a per-transition (non-empty -> empty) diff, counted on DELIVERY. Reason = `server_muted` if that kind is server-muted on the publisher at flush, else `unwatched`.
  - A server mute of an already-empty (unwatched) stream counts nothing.
  - A reconnect's fresh view has no previous directive, so it counts nothing.
  - Unmute counts nothing.
  - `withdrawals{reason="server_muted"}` is already zero-initialised (ops-6, confirm). Stale "emitted by MC-B" wording is updated in the catalog, panel, the `WithdrawalReason` doc and the recorder doc.
- `mc_media_send_directives_total` stays kind-free. Server mute is added to the routine `emitted_empty_targets` causes (O-6).
- **Server unmute** restores targets in the same structural change REGARDLESS of client mute. The disposition returns to PAUSED if client-muted, else FORWARD (S7).
- The mute lives on the roster entry. A re-push (next render restates it), an MH restart (the next snapshot restates it), and a reconnect within grace (the roster is kept, and `reset_view` re-dirties) all keep it. A fresh rejoin starts unmuted (story 2).

### C. Ordering (R-18)
- In `handle_server_mute` the self `ParticipantMuteUpdate` goes to every participant, the target included, via `send_update` BEFORE `reconcile_media`/`flush_views`. Same `ParticipantActorHandle` mailbox, so FIFO holds (S5). No path emits the directive ahead of the broadcast.
- The integration test reads the TARGET's outbound sequence. For mute and unmute and both kinds it asserts MuteUpdate(K state) strictly before the SendDirective whose K targets change.
- `server_mute_replay` is unchanged (per-kind flags already).

### D. Census (O-7, DRY-4)
`publish_mute_census` already counts per kind from the roster flags. It stays the single source. New unit test: video-only gives [0,1], adding audio gives [1,1], unmuting video gives [1,0], unmuting both gives removal ([0;2]).

### E. Actor-blocking (SG-4)
"Confirm the echo" stays the push worker's job (`HandlerPusher` latest-wins + `confirm.rs`). The actor publishes and never awaits MH or the echo. The churn bound follows: at most one push in flight and one pending per handler.

### F. Superseded-field deletion (R-25), split by owner
- **protocol** implements proto edits and owns the following:
  - reserves, each with a "Was:" line: EgressStream `reserved 6; "transport_mode"`; RegisterMeetingRequest `reserved 7; "server_muted_sources"`; RegisterMeetingResponse `reserved 5; "transport_mode"`; JoinResponse `reserved 12; "max_receive_slots"`; SlotState `reserved 2; "SLOT_STATE_SOURCE_MUTED"`; message `MutedSource` deleted;
  - re-homing the R-16 slot-terms block onto `SourceStream`;
  - making `source_streams` stand alone;
  - rewriting the mute-section comments;
  - inlining the per-session rule into `max_audio_receive_slots`;
  - the CONVENTIONS.md correction note;
  - `crates/proto-gen/src/lib.rs` Debug redaction, both roundtrip tests and `verify-codegen.sh` absence asserts (new message-scoped helper with positive controls);
  - the `validate-internal-proto-no-key-material.test.sh` re-anchor;
  - comment-only repoints in `crates/media-protocol/src/frame.rs`.
  - No `skip_debug` entry is removed from `crates/proto-gen/build.rs` (SG-1). I will confirm whether `MutedSource` has one before the regen; if it does, that line goes with the deleted message, which I will state explicitly.
- **paired-media-handler** implements the MH side, per its inventory:
  - delete the transitional reading, `TransitionalStory2`, the union-muted branch, `legacy_transport_echo*` and four `PolicyRejection` variants;
  - make `parse_declared` unconditional;
  - drop field 5 in `respond()`;
  - make `mh-test-utils::register_request` MC-shaped (declares its candidates), with a declared muted builder;
  - sweep the tests;
  - repoint the `forward.rs` comment to the `SourceStream` home;
  - edit the env-test fixtures `injection.rs`/`mh_grpc.rs` and `26_mh_quic.rs`;
  - MH docs/runbook comment rows.
  - NEW pinned test: egress present + empty `source_streams` returns the gRPC error AND `mh_media_policy_applies_total{outcome="rejected_transport_inconsistent"}` +1 AND the prior generation stays live (ops-2). Boundary positive controls: an empty registration still applies; an egress with zero candidates and no sources is still legal.
- **meeting-controller (me)**:
  - delete `MutedSource` writes (`mh_client.rs::build_muted_sources`), `HandlerAssignment::server_muted_sources` and its render in `slots.rs`, plus `generation.rs`, `confirm.rs` (`the_legacy_transport_echo_is_no_longer_compared` goes with field 5), the `JoinResponse.max_receive_slots: None` write, the `SlotState::SourceMuted` label/zero-init (`source_muted` leaves the `slot_state` domain, O-8), `mc-test-utils/mock_mh.rs` and MC tests;
  - rename the MC-internal config field `max_receive_slots: [u8; 2]` to `receive_slot_caps` so the acceptance grep is clean (an internal name, not the wire field);
  - 27/29 env-test sweeps if needed.
- **client**:
  - regen `packages/sdk-core/src/proto/**`;
  - switch `sendMuteRequest`/`sendServerMuteRequest` (`muteRequest.ts`, `MeetingSession.ts`) to send only the changed kind, with a test that a single-kind request leaves the other kind's state untouched;
  - restore the exhaustive `Record<SlotState, …>` in `slotStateMap.ts`.
  - Before regen I re-run the zero-reference grep of `packages/` (non-generated) for SOURCE_MUTED / maxReceiveSlots / max_receive_slots / MutedSource / serverMutedSources. If it is non-zero, we stop and report.
- **Layer 6 expectation (protocol P2), ESCALATION FOR THE LEAD**: buf breaking reports exactly ONE finding, `MESSAGE_NO_DELETE` on `MutedSource`. Field/enum deletions with reserves produce none. Per CONVENTIONS no `except`/`ignore` and no tombstone; a headless Lead escalates to the human as P1 did. I will record it under `## Expected Layer-State` in main.md; any other finding is a regression.

### G. ADR-0036 §5 (R-36)
New dated blockquote `Amendment (2026-10-23, story 3 MC-B)` under §5. It keeps the 2026-10-13 text visible and supersedes "Client mute MAY stay directed". It states:
- four controls compose;
- server mute withdraws the directive (empty targets, capture stops, camera light off) AND MH drops at ingress as the backstop;
- client mute keeps the directive, is enforced at capture, and is never an ingress drop (PAUSED);
- the directive never depends on client mute;
- plainly: a host's server unmute can resume a participant's camera or microphone unless they have client-muted, and the participant is notified at that moment;
- the mic stays open on audio mute of either cause; only a video mute stops the camera.
Placement is agreed with protocol (P1 bundle). §7 "Server mute is enforced at MH ingress" gets a one-line pointer if it still names `MutedSource`.

### H. Docs/metrics/runbooks
- mc-service.md: counting-unit text rewritten to the steady state (one per NAMED kind; rates roughly HALVE vs the interim, a counting-unit change and not traffic); the new rejected row; the AppliedNoRecompose removal; `source_muted` out of the slot_state domain; withdrawals and directives rows.
- mc-media.json:
  - slot_state regex/descriptions;
  - Mute Requests / Server Mute Requests descriptions;
  - the new panel;
  - the withdrawals panel text.
- label-taxonomy.md and mh-service.md rows.
- dashboards.md.
- Runbooks per ops-4:
  - Scenario 21 "How server mute works", the counting example, the benign "unmuted but client-muted" arm, and the withdrawal evidence;
  - past-tense MC-B in mc-incident-response;
  - mh-deployment:743;
  - client-dev-local source_muted rows as history.
- mc-incident-response.md, operations follow-up (a)/(b):
  - delete the `applied_no_recompose` row at ~2165, or turn it into dated history;
  - the success set at ~2176 becomes `applied` only;
  - reword the `source_muted` discussion at ~2124-2128 as history (the series is now absent, not flat);
  - carry the "rates roughly halve at MC-B (counting unit, not traffic)" note into Scenario 21 Step 1 and the client-mute row.
- Observability N-1..N-3:
  - catalog and panel state that `server_mute/no_kind_named` counts both-absent requests from ANY requester, so `not_permitted` undercounts non-hosts by exactly those requests;
  - integration pin: non-host both-absent gives generic +1 and all per-kind `not_permitted` +0;
  - `refusal_replies_suppressed{surface}` gains `client_mute` (zero-init, catalog, panel);
  - the new panel uses `sum by (surface, reason)`, no ratio, and the "zero healthy / absent = pipeline fault" sentence;
  - check that env-test 32 picks the new series up.
- Test additions:
  - R-18 also asserts a non-target holder's order (MuteUpdate before StreamAssignments with AVATAR_SERVER_MUTED/ACTIVE);
  - churn counts every push from the first toggle to quiescence, with a ≥1 push positive control.
- Security Gate-3 checks:
  - non-host both-absent reply bytes are identical to the named-kind refusal;
  - the R-18 test is proven to fail with the broadcast moved after reconcile (local swap);
  - MH reject positive control.
- Code-reviewer:
  - CQ-1: `SourceMute`'s fields become distinct newtypes, `server: ServerMutedKinds` and `client: ClientMutedKinds`, both built only in the roster->`SourceMute` projection.
    - `ServerMutedKinds` is defined in directive.rs; `ClientMutedKinds` lives in slots.rs.
    - `build_send_directive` takes `ServerMutedKinds`, so passing client flags is a TYPE error, not just a missing import.
    - Pinned by a rustdoc `compile_fail` doctest (no trybuild in the crate) and stated in the module doc.
  - CQ-2: one shared per-kind decision type, `KindDecision`, with `MediaKindLabel`-keyed `changed(kind)`/`state(kind)`/`is_unchanged()` and no public arrays. It is used by both self-mute and server-mute.
  - CQ-3: complete the rename `max_receive_slots` -> `receive_slot_caps` at every Rust site; env var names unchanged.
  - CQ-4: one shared both-absent rejection helper, parameterised by `MuteSurface`; the new limiter constants follow the const+doc pattern.
- DRY asks:
  - one home for the surface tokens: `RefusalReplySurface::{ClientMute,ServerMute}` labels delegate to `MuteSurface::label()`, with a unit test pinning them equal;
  - `mute_rejection_reply_limiter` is another `ClientWorkLimiter`, reusing the server-mute reply constants rather than restating literals.
- API_CONTRACTS.md, the specialist INDEXes, and the TODO.md §Media Path Obligations (d) repoint to `applied_source_streams`.
- My INDEX: per-kind mute paths.

### I. Tests
**Unit (MC)**:
- `MuteChange::parse` (each kind alone, both, both-absent rejected);
- directive: server-muted kind gives empty targets with stream/encoding intact, the other kind byte-identical; a self-mute flip gives a byte-identical directive (`ServerMutedKinds` has no client input — compile-level);
- disposition: client-only gives PAUSED never DROP; server+client gives DROP; server unmute while client-muted gives PAUSED and restored targets;
- server-wins slot state;
- `server_muted_by` attribution (S4);
- census sequence (O-7);
- withdrawal classification (server_muted vs unwatched; already-empty counts 0);
- MeetingActor: per-kind actor merges (absent unchanged).

**Integration** (`server_mute_integration.rs` + siblings, cross-kind tests per arm comparing whole structs: directive stream, `SourceStreamPlan`, edges, slot state):
- R-15:
  - host-only per kind;
  - non-host with a NON-EXISTENT target id gets the same generic refusal, `not_permitted` per named kind, and `unknown_target` stays 0;
  - both-absent from host and non-host: generic message + generic counter +1, no per-kind cell observed;
  - rate bound.
- R-16:
  - video server mute empties video targets and sets video DROP_AT_INGRESS while audio stays byte-identical, and the reverse;
  - exactly +1 `withdrawals{video,server_muted}` and +0 audio;
  - survival across a re-push (another participant's join), across a mock-MH restart (re-registration restates the DROP), and across a reconnect within grace.
- R-17 churn:
  - mock MH gate held, N toggles across 3 participants: in-flight per handler ≤ 1;
  - on release, pushes per handler ≤ 1 + 1 (derived from the pusher's in-flight+pending structure, not hand-typed);
  - the final applied snapshot's dispositions equal the final mute state;
  - an idempotent toggle does not advance the generation or push, and does not consume the limiter (the next real toggle is admitted).
- R-18: the target's mailbox order for mute and unmute, both kinds.

**Env-tests** (`35`, paired with test, using test's design):
- one file-level `static tokio::sync::Mutex` serialises every server_muted-producing fn (S3 audio, video server mute, patched publisher);
- the patched publisher asserts per variant (label 1 uni gives `server_muted` delta ≥ K on ITS MH instance; label 1 datagram gives `transport_mismatch`; relabel to 0 gives audio slots only) with pre/liveness/traffic-bounded-window controls;
- client video mute is evidenced by a `KeepSendingWhileMuted` publisher's label-1 frames still reaching the video slot (PAUSED signature), plus AVATAR_CLIENT_MUTED at every holder, plus an unchanged non-empty directive;
- cross-kind both directions with frames;
- host unmute restores targets with the R-18 pair observed on the target's path;
- the storm checks convergence and that the final directive is unchanged, while the numeric push bound is proven in-process (header says so);
- `server_mute()` helper takes per-kind `Option<bool>`;
- header edge sum over concurrent fns ≤ 46, with arithmetic.

**MH**: the reject pin above. **SDK**: single-kind request test (client).

### J. Client additions (client input, folded)
- Layer-7 break: `packages/web-app/e2e/server-mute.spec.ts` asserts B KEEPS SENDING under server mute. Under MC-B the directive is withdrawn, so B goes FLAT.
  - Flip both `sentProbe(b,'advance')` to `'flat'` (S3 and the compose test, where B stays flat after client-unmute because the server mute still withholds the directive).
  - Rewrite the header and comments, the e2e README row, and the manual test plan if it says "keeps sending".
  - Optionally assert that B's bus shows empty audio targets.
  - Owner: client.
- SDK report path narrows to `(kind, muted)`:
  - `muteRequest.ts` single-kind builder (both-absent unconstructible);
  - `SignalingClient.sendMuteRequest(kind, muted)` / `sendServerMuteRequest(target, kind, muted)`;
  - `AudioPipeline`/`VideoPublisher`/`mediaStart`/`mediaWiring`/`MeetingSession` report callbacks;
  - delete `signalingMediaState.serverMuteRequest`;
  - wire-level tests decode the sent bytes and assert the unchanged field is ABSENT;
  - a session-level test that a toggle names only its kind.
  - The `muteChanged` event keeps the full snapshot. Owner: client.
- Codegen oracle: member-level absence matchers with positive controls (agreed by protocol and client; never `assert_not_generated` on a field or enum member).
- MC rate-limit comment (`connection.rs` client-mute suppression) is rewritten truthfully. Under MC-B a dropped report drives PAUSED/AVATAR, so a dropped UNMUTE leaves the participant shown as muted until the next SAME-kind report. That is accepted: burst 8 / 4 per second is beyond human reach. The cache is per kind (section A), so a limited audio report never short-circuits a later video one.
- The self `ParticipantMuteUpdate` keeps reaching the TARGET itself (broadcast includes the target; replay unchanged).

## Expected Layer-State

Declared per `docs/protocol/CONVENTIONS.md` "Intentional wire breaks" (Lead ruling 2026-10-23: no tombstone, no `proto/buf.yaml` edit; the Lead escalates this single red to the human at Gate 2):

| Layer | Rule class | File | Symbol | Why |
|-------|-----------|------|--------|-----|
| 6 (proto breaking, `buf breaking`) | `MESSAGE_NO_DELETE` | `proto/dark_tower/internal/v1/internal.proto` | `MutedSource` | Superseded by `SourceStream`, deleted in-story by MC-B (R-25, one coordinated cutover). Field/enum deletions are reserved by tag AND name and produce no finding. |

Any other buf finding is a regression. Every other layer is expected green.

**Superseded by the human decision (2026-10-23, resume):** `MESSAGE_NO_DELETE` is now in `proto/buf.yaml` `breaking.except`, so Layer 6 is expected **green**. No other finding was ever predicted.

### Safe-revert unit
The whole commit. A partial proto revert fails to compile (loud). Reverting only the MH legacy-path deletion is QUIET and brings back an accepting path the runbook says never ships, so it is forbidden. At deployment MC-B cannot be rolled back alone; rolling back across the story is all components together (mh-deployment.md §Rollback layer 3).

### Sequencing
1. protocol lands the proto edits first (everything compiles against them).
2. MH (paired-media-handler) and MC (me) work in parallel on disjoint trees.
3. client regen + SDK.
4. env-tests.
5. docs.

`./scripts/layer-fast.sh` green before "Ready for review".

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired media-handler | confirmed |
| Protocol (GSA owner) | confirmed |
| Client (conditional) | confirmed |

Classification-sanity guard: `STATUS=OK` (after Lead set `frame.rs` Owner to the single manifest owner `protocol`; media-handler co-review via paired-media-handler).

Lead rulings at Gate 1:
- `RegisterMeetingResponse.transport_mode = 5` deletion in scope (story Design §Wire freeze wins over the prompt).
- `buf breaking` `MESSAGE_NO_DELETE` on `MutedSource`: declared under Expected Layer-State; no tombstone, no `buf.yaml` edit; escalated to the human at Gate 2 (CONVENTIONS "Intentional wire breaks" — headless Lead may not accept).
- `packages/web-app/e2e/server-mute.spec.ts` + README + manual-test-plan edits in-loop (client reviews).
- Authorship: implementer writes MC, proto/proto-gen and TS edits (protocol and client stay independent Gate-3 reviewers); paired-media-handler writes the MH half (paired collaborator role), reviewed at Gate 3 by the implementer-independent panel (security, code-reviewer, test, protocol).
- `packages/sdk-svelte/src/stores/MediaStore.svelte.ts` per-kind server-mute display: task 17 (C-C1) scope (story R: four indicators per participant incl. server video); no UI path to a video-only server mute before C-C1. Hand-off recorded in `docs/TODO.md`.

---

## Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 2 | 2 | 0 | F1 typed SourceMute halves; F2 per-kind ServerMuteAttribution |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 | env-35 window ends/liveness, PAUSED discriminator, storm during==1, unwatched-mute test, MH same-input control |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | vacuous deltas → vocabulary-domain tests; "flat drops = normal" caveats |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 | CR-1 (via security F2), error-context split, test/doc nits |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | stale refs to deleted items |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | OPS-R1 runbook flat-drops reading |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 1 | 1 | 0 | error-context-preservation (shared with CR-2) |
| Paired media-handler | RESOLVED-FIXED | 3 | 3 | 0 | contract text, stale MC comments |
| Protocol (GSA owner) | RESOLVED-FIXED | 2 | 2 | 0 | INDEX wording, comment wrap; independent buf measurement matches declaration |
| Client (conditional) | RESOLVED-FIXED | 2 | 2 | 0 | server-mute.spec same-page mover; muteState.ts wording |

---

## Gate 2 — Full Validation (attempt 1, 2026-10-23)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final reviewed tree (log `/tmp/gate2-1.log`):

| Layer | Result |
|-------|--------|
| 1 Compile | OK |
| 2 Format | OK |
| 3 Guards | OK |
| 4 Test | OK |
| 5 Lint | OK |
| 6 Audit | **FAIL — exactly the declared finding**: `proto/dark_tower/internal/v1/internal.proto:1:1: Previously present message "MutedSource" was deleted from file.` (`MESSAGE_NO_DELETE`). cargo/pnpm audit OK. No unpredicted finding. |
| 7 Env-tests | OK (`env-tests-passed`, `browser-e2e-passed`) |

`TOTAL_RESULT=FAIL` solely on the declared Layer-6 break. Per `docs/protocol/CONVENTIONS.md` "Intentional wire breaks", a headless Lead may not accept that red: **escalated to the human** (`.devloop-escalation.json`, reason `reviewer-escalated`/human decision). Options (protocol): **(a)** accept the declared red for this commit; **(b)** add `MESSAGE_NO_DELETE` to `proto/buf.yaml` `breaking.except` as a policy change (message names are not on the binary/JSON wire; no `Any` in the protos). A tombstone `MutedSource` is not endorsed. The tree is complete and uncommitted; after the decision, re-run Gate 2 and commit.

Earlier (non-gate) Layer-7 probe on a mid-fix tree had 2 browser failures (`join-happy-path.spec.ts:148`, `kek-rotation.spec.ts:78`), diagnosed by @client as environmental: a concurrent `verify-codegen.sh` deleted live `signaling_pb.ts` under the Vite dev server. Gate 2's L7 was green. Tooling hazard recorded in `docs/TODO.md` §Devloop Container Resource Hygiene & Build Isolation.

---

## Human Review (Iteration 2)

**Feedback** (operator restart directive, run-story `--restart`): "policy change: add MESSAGE_NO_DELETE to proto/buf.yaml breaking.except — record it in CONVENTIONS §Enforcement. Keeping a dead MutedSource tombstone is not endorsed."

**Done (Lead, operator-directed config delta)**:
- `proto/buf.yaml`: `MESSAGE_NO_DELETE` added to `breaking.except`; header comment states the wire rationale (message names on neither binary nor proto3 JSON wire; no `Any`; fields typed by a deleted message are caught by type/reserve rules).
- `docs/protocol/CONVENTIONS.md` §Enforcement: except list and rationale updated (incl. "revisit if `Any` is introduced"); the 2026-10-23 correction note gains a Resolution paragraph recording the decision.
- `./scripts/layer6.sh`: `buf-breaking-passed`.

### Gate 3 — iteration 2 (GSA delta: protocol owner + security)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Protocol (GSA owner) | RESOLVED-FIXED | 1 | 1 | 0 | stale "message-deletion detection" claim in buf.yaml header fixed; positive controls C0-C7 on scratch copies (recorded in CONVENTIONS Resolution note) |
| Security | RESOLVED-DEFERRED | 1 | 0 | 1 | no break class only `MESSAGE_NO_DELETE` caught; low: "no `Any`" is a prose tripwire → `docs/TODO.md` §Guard Coverage Gap — `MESSAGE_NO_DELETE` exception |

The rest of the panel's iteration-1 verdicts stand: the iteration-2 delta is `proto/buf.yaml` + CONVENTIONS/TODO/main.md prose only, with no code change.

## Gate 2 — Full Validation (attempt 2, 2026-10-23, iteration 2)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree (log `/tmp/gate2-2.log`): Layers 1-7 all `OK`, `TOTAL_RESULT=OK` (L6 `buf-breaking-passed` under the human-decided `MESSAGE_NO_DELETE` exception; L7 1043s).

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Per-kind mute semantics (R-15, R-17)
| Item | Before | After |
|------|--------|-------|
| Request parse | `named_kinds` folded absent → `false`, both kinds always acted on | `media_kind::MuteChange::parse` — the ONE parser for both requests; absent = unchanged; both absent unrepresentable (`NoKindNamed`) |
| Actor merge | `handle_self_mute(a, v)` / `handle_server_mute(.., a, v)` overwrote both flags | both take `MuteChange` and merge against the ROSTER (`MuteChange::apply` → `KindDecision`); `UpdateSelfMute` now replies with the per-kind decision |
| Client-mute short-circuit | `last_reported_mute: Option<(bool,bool)>` defaulting to false | `[Option<bool>; 2]` seeded only from the actor's confirmed state; unknown never short-circuits |
| Both-absent | applied as unmute-both | refused generically (server: same `FORBIDDEN` text as every refusal; client: `INVALID_REQUEST` "Mute request refused"), rationed reply, counted on new kind-free `mc_media_mute_requests_rejected_total{surface,reason}`, no token spent |
| Server-mute order | authority → limiter → actor | authority → shape → limiter → actor (non-host always gets the one refusal) |
| `server_muted_by` | overwritten by any request touching either kind | set only when a request MUTES a kind; cleared only when neither kind is muted |
| `MuteOutcome::AppliedNoRecompose` | unreachable, kept | deleted |
| Legacy `SignalingPayload::MuteUpdate` | a dead both-kinds second self-mute writer | deleted (one entry: `UpdateSelfMute`) |

### Directive withdrawal + ordering (R-16, R-18; ADR §5 boundary)
- `build_send_directive(.., server_muted: ServerMutedKinds)`: a server-muted kind is listed with EMPTY targets; edges stay (DROP_AT_INGRESS). `SourceMute` now holds distinct `ServerMutedKinds` / `ClientMutedKinds` types, so passing client mute is a type error (compile_fail doctest + byte-identical positive-control doctest).
- `Affected::PublisherAndHoldersOf` dirties the target for its directive on server mute; client mute keeps `HoldersOf`.
- Withdrawal reason = current cause at delivery (`server_muted` if that kind is server-muted, else `unwatched`).
- The self `ParticipantMuteUpdate` broadcast stays BEFORE reconcile/flush on the same mailbox; pinned by an integration test reading the target's and a holder's message order (proved to fail with the broadcast moved after the reconcile).

### Superseded-field deletion (R-25)
- Proto: deleted + reserved by tag and name — `EgressStream.transport_mode` (6), `RegisterMeetingRequest.server_muted_sources` (7) + message `MutedSource`, `RegisterMeetingResponse.transport_mode` (5, story wire-freeze item 18 — Lead ruling), `JoinResponse.max_receive_slots` (12), `SLOT_STATE_SOURCE_MUTED` (2). R-16 slot-terms block re-homed on `SourceStream`; UNSPECIFIED-is-a-mismatch rule re-homed on `applied_source_streams`; mute-section comments rewritten; CONVENTIONS correction note.
- MC: `HandlerAssignment::server_muted_sources` + its render, `build_muted_sources`, the field-5 echo test, the tag-12 write, `source_muted` slot-state label/domain removed; `max_receive_slots` → `receive_slot_caps` (internal config field; env vars unchanged).
- MH (paired-media-handler): transitional reading deleted; egress + empty `source_streams` → `rejected_transport_inconsistent` (pinned with counter + prior-generation assertions and positive controls).
- Oracle: member-scoped field-absence helper with positive controls (proven red against HEAD output).

### Additional Changes
- SDK: single-kind `sendMuteRequest(kind, muted)` / `sendServerMuteRequest(target, kind, muted)`; `(kind, muted)` report path; wire-decoded absence tests; `signalingMediaState.serverMuteRequest` deleted; exhaustive `slotStateMap`.
- Browser S3 spec: B's send goes FLAT under server mute (directive withdrawn), compose window too; README + story-2 manual plan text.
- Env-test 35 (fork, test design): video server mute + patched publisher per variant, cross-kind, client video mute PAUSED, R-18 pair, storm convergence; file-level mutex; edge sum 14.
- ADR-0036 §5 dated amendment (2026-10-23); catalog, dashboard (new panel 190 + descriptions), alerts/no-alert comment, runbooks (Scenario 21 incl. Step 1b + benign arm), API_CONTRACTS, INDEXes, TODO (d) repoint + C-C1 MediaStore hand-off.

---

## Files Modified

```
 crates/env-tests/src/fixtures/injection.rs         |  69 +-
 crates/env-tests/src/fixtures/mh_grpc.rs           |   3 +-
 crates/env-tests/tests/26_mh_quic.rs               | 129 +--
 crates/env-tests/tests/29_mh_meeting_teardown.rs   |   8 +-
 .../env-tests/tests/35_mc_server_mute_teardown.rs  | 970 ++++++++++++++++++++-
 crates/mc-service/src/actors/meeting.rs            | 511 ++++++++---
 crates/mc-service/src/actors/meeting_media.rs      |  33 +-
 crates/mc-service/src/actors/messages.rs           |  66 +-
 crates/mc-service/src/actors/participant.rs        |   5 +-
 crates/mc-service/src/config.rs                    |  14 +-
 crates/mc-service/src/grpc/gc_client.rs            |   4 +-
 crates/mc-service/src/grpc/mh_client.rs            |  49 +-
 crates/mc-service/src/main.rs                      |   4 +-
 crates/mc-service/src/media_kind.rs                | 212 +++++
 crates/mc-service/src/media_routing/assignment.rs  |  65 +-
 crates/mc-service/src/media_routing/confirm.rs     |  12 -
 crates/mc-service/src/media_routing/generation.rs  |  31 +-
 crates/mc-service/src/media_routing/slots.rs       | 183 +---
 .../mc-service/src/media_signaling/assignments.rs  |  65 +-
 crates/mc-service/src/media_signaling/directive.rs | 300 ++++++-
 crates/mc-service/src/media_signaling/mod.rs       |  11 +-
 crates/mc-service/src/media_signaling/outcome.rs   | 156 +++-
 crates/mc-service/src/observability/metrics.rs     | 147 +++-
 crates/mc-service/src/webtransport/connection.rs   | 659 ++++++++------
 crates/mc-service/src/webtransport/server.rs       |   2 +-
 crates/mc-service/tests/common/media_session.rs    |  86 +-
 crates/mc-service/tests/common/mod.rs              |  47 +-
 crates/mc-service/tests/gc_integration.rs          |   2 +-
 .../tests/media_client_signaling_integration.rs    |  12 +-
 .../tests/media_policy_push_integration.rs         |   3 +-
 .../tests/otel_grpc_outbound_integration.rs        |   2 +-
 crates/mc-service/tests/server_mute_integration.rs | 944 ++++++++++++++++----
 .../mc-service/tests/slot_placement_integration.rs |   8 +-
 crates/mc-service/tests/video_slots_integration.rs |  10 +-
 crates/mc-test-utils/src/mock_mh.rs                |  51 +-
 crates/media-protocol/src/frame.rs                 |   2 +-
 crates/mh-service/src/config.rs                    |   7 +-
 crates/mh-service/src/grpc/mh_service.rs           | 117 ++-
 crates/mh-service/src/main.rs                      |   3 +-
 crates/mh-service/src/media/forward.rs             |   5 +-
 crates/mh-service/src/observability/metrics.rs     |  17 +-
 crates/mh-service/src/routing/mod.rs               | 107 +--
 crates/mh-service/src/routing/sources.rs           | 331 ++-----
 crates/mh-service/src/routing/tests.rs             | 228 +----
 crates/mh-service/src/session/mod.rs               |   2 +-
 crates/mh-service/tests/auth_layer_integration.rs  |   1 -
 crates/mh-service/tests/common/media_rig.rs        |   6 +-
 .../tests/media_attribution_integration.rs         |  15 +-
 .../tests/media_server_mute_integration.rs         |  10 +-
 crates/mh-service/tests/otel_grpc_integration.rs   |   2 -
 .../tests/register_meeting_integration.rs          |   1 -
 crates/mh-test-utils/src/media_policy.rs           | 114 ++-
 crates/proto-gen/src/lib.rs                        |   8 +-
 crates/proto-gen/tests/internal_roundtrip.rs       | 101 +--
 crates/proto-gen/tests/signaling_roundtrip.rs      | 103 +--
 docs/API_CONTRACTS.md                              |  30 +-
 docs/TODO.md                                       |   4 +-
 docs/decisions/adr-0036-media-flow.md              |  29 +-
 docs/observability/alerts.md                       |   2 +-
 docs/observability/dashboards.md                   |   3 +-
 docs/observability/label-taxonomy.md               |   3 +-
 docs/observability/metrics/mc-service.md           |  62 +-
 docs/observability/metrics/mh-service.md           |  36 +-
 docs/protocol/CONVENTIONS.md                       |  13 +
 docs/runbooks/client-dev-local.md                  |   7 +-
 docs/runbooks/mc-deployment.md                     |   5 +-
 docs/runbooks/mc-incident-response.md              |  99 ++-
 docs/runbooks/mh-deployment.md                     |   2 +-
 docs/runbooks/mh-incident-response.md              |  10 +
 docs/specialist-knowledge/client/INDEX.md          |   4 +-
 docs/specialist-knowledge/code-reviewer/INDEX.md   |   2 +-
 docs/specialist-knowledge/media-handler/INDEX.md   |   4 +-
 .../meeting-controller/INDEX.md                    |  10 +-
 docs/specialist-knowledge/observability/INDEX.md   |   4 +-
 docs/specialist-knowledge/protocol/INDEX.md        |   6 +-
 .../2026-09-21-hear-each-other-manual-test-plan.md |   7 +-
 infra/docker/prometheus/rules/mc-alerts.yaml       |  11 +-
 infra/grafana/dashboards/mc-media.json             | 128 ++-
 infra/grafana/dashboards/mh-media.json             |   4 +-
 infra/services/mh-service/config.env               |   7 +-
 packages/proto-gen/scripts/verify-codegen.sh       | 102 ++-
 .../sdk-core/src/media/lifecycle/AudioPipeline.ts  |  23 +-
 .../sdk-core/src/media/lifecycle/VideoPublisher.ts |   9 +-
 .../lifecycle/__tests__/VideoPublisher.test.ts     |  15 +-
 .../lifecycle/__tests__/audioPipeline.test.ts      |   6 +-
 packages/sdk-core/src/media/lifecycle/muteState.ts |  16 +-
 packages/sdk-core/src/session/MeetingSession.ts    |  23 +-
 .../src/session/__tests__/meeting-session.test.ts  |  19 +-
 packages/sdk-core/src/session/mediaStart.ts        |   3 +-
 packages/sdk-core/src/session/mediaWiring.ts       |  15 +-
 .../sdk-core/src/session/signalingMediaState.ts    |  13 -
 packages/sdk-core/src/signaling/SignalingClient.ts |  44 +-
 .../src/signaling/__tests__/mediaSignaling.test.ts |  80 +-
 .../signaling/__tests__/signaling-client.test.ts   |   2 +-
 .../src/signaling/__tests__/slotStateMap.test.ts   |  21 +-
 packages/sdk-core/src/signaling/muteRequest.ts     |  34 +-
 packages/sdk-core/src/signaling/slotStateMap.ts    |  22 +-
 packages/web-app/e2e/README.md                     |   4 +-
 packages/web-app/e2e/server-mute.spec.ts           |  32 +-
 proto/dark_tower/internal/v1/internal.proto        | 334 ++-----
 proto/dark_tower/signaling/v1/signaling.proto      | 149 ++--
 ...validate-internal-proto-no-key-material.test.sh |   2 +-
 102 files changed, 4652 insertions(+), 2861 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/mc-service/src/media_kind.rs` | `MuteChange`, `KindDecision`, `ServerMutedKinds`, `ClientMutedKinds` + unit tests |
| `crates/mc-service/src/webtransport/connection.rs` | per-kind client/server mute handlers, `reject_no_kind_named`, `reply_mute_refusal`, per-kind cache, new reply limiter |
| `crates/mc-service/src/actors/meeting.rs` | per-kind actor merges, `server_muted_by` imposition rule, R-18 comment, unit tests |
| `crates/mc-service/src/actors/meeting_media.rs` | server-mute input to the directive, `PublisherAndHoldersOf`, withdrawal reason |
| `crates/mc-service/src/media_signaling/directive.rs` | `ServerMutedKinds` input, module doc + doctests, unit tests |
| `crates/mc-service/src/media_signaling/outcome.rs` | `MuteSurface`, `MuteRejection`, `RefusalReplySurface::Mute(..)`, `AppliedNoRecompose` removed |
| `crates/mc-service/src/observability/metrics.rs` | `record_mute_request_rejected`, zero-init, `source_muted` out of the domain |
| `crates/mc-service/tests/server_mute_integration.rs` | rewritten: per-kind R-15/R-16/R-17/R-18, churn bound, census, both-absent |
| `crates/mc-service/tests/common/{mod,media_session}.rs` | push hold gate; per-kind frame helpers; `join_shaped` |
| `proto/dark_tower/{internal,signaling}/v1/*.proto` | deletions + reserves + re-homed comments |
| `crates/mh-service/**`, `crates/mh-test-utils/**` | (paired-media-handler) legacy path deleted, reject pinned |
| `packages/sdk-core/src/**` | single-kind mute requests, exhaustive slot-state map |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | MC-B scenarios |
| `docs/decisions/adr-0036-media-flow.md` | §5 amendment 2026-10-23; §7 pointer |

---

## Devloop Verification Steps

Implementer `./scripts/layer-fast.sh` (2026-10-23): L1-L5 OK; L6 FAIL with exactly the declared finding (`MESSAGE_NO_DELETE` on `MutedSource`, see Expected Layer-State) — nothing else.

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

- `docs/TODO.md` §Guard Coverage Gap — `MESSAGE_NO_DELETE` exception — no guard for its no-`Any` premise

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
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. A partial proto revert without MC/MH fails to compile (loud). Reverting only the MH legacy-path deletion is QUIET — it re-admits an accepting path the runbook says never ships (`mh-deployment.md` §Forward order) — so it is forbidden. At deployment MC-B is not rolled back alone: rolling back across story 3 is all components together (`mh-deployment.md` §Rollback layer 3). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
