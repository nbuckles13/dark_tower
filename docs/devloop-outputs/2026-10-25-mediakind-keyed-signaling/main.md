# Devloop Output: Key every per-kind signaling fact by MediaKind, and enforce it

**Date**: 2026-10-25
**Task**: Replace audio_*/video_* signaling fields with MediaKind-keyed state (ClientMediaState enum, PublishedKind in JoinRequest, MediaKindState on roster/updates, kind-keyed mute requests); derive metric kind labels from MediaKind; add proto per-kind field-name and metric-label literal guards; docs.
**Specialist**: meeting-controller (paired with client)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: ~22h wall clock (2026-10-25 01:44 → 2026-10-26), including idle periods

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c83eaa9123797ed4a2d8414d0b5a968f929b3d74` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (meeting-controller) |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | done (RESOLVED-FIXED) |
| Test | done (RESOLVED-FIXED) |
| Observability | done (RESOLVED-FIXED) |
| Code Quality | done (RESOLVED-FIXED) |
| DRY | done (RESOLVED-DEFERRED) |
| Operations | done (RESOLVED-FIXED) |
| Semantic Guard | not spawned: no checks.md surface (mute state, labels, guards; no credential/key material) |
| Paired Client | done (RESOLVED-FIXED) |
| Protocol (conditional, GSA `proto/**`) | done (RESOLVED-FIXED) |
| Infrastructure (conditional, guard machinery) | done (RESOLVED-DEFERRED) |

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
Key every per-kind signaling fact by `MediaKind` instead of hand-named audio_/video_ field pairs, so state is known at admission with no "unknown" state, and enforce it with a proto guard. Session 1 of 2; metric labels follow in `mediakind-metric-labels`.

### Scope
- **Service(s)**: proto (signaling), mc-service, proto-gen, env-tests, sdk-core, sdk-svelte, web-app, dt-guard; Prometheus rules kustomize; docs (ADR-0036, CONVENTIONS, API_CONTRACTS, story, runbooks, catalog)
- **Schema**: No
- **Cross-cutting**: Yes (GSA `proto/**`; MC and client co-deploy)

### Debate Decision
NOT NEEDED. The human settled the design (2026-10-24/25) and asked for it to be settled in Gate-1 planning; decisions are recorded in the ADR-0036 §5 amendment and CONVENTIONS §6.

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
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment (GSA; security in planning + review per ADR-0024 §6.4) | protocol |
| `crates/proto-gen/src/lib.rs` (redacting `Debug` impls: renamed/new fields) | Not mine, Minor-judgment | protocol |
| `crates/proto-gen/tests/signaling_roundtrip.rs` | Not mine, Minor-judgment | protocol |
| `packages/proto-gen/project.json` (`test` → `test:unit`; `codegen` → `scripts/codegen.sh`) | Not mine, Minor-judgment | infrastructure (machinery; protocol content) — authored by infrastructure, reviewed by operations + observability |
| `packages/proto-gen/scripts/codegen.sh` (new: clean + `buf generate`, the producer) | Not mine, Minor-judgment | infrastructure (machinery; protocol content) — authored by infrastructure, reviewed by operations + observability |
| `packages/proto-gen/scripts/codegen-paths.sh` (new: shared `CODEGEN_OUT_DIR`) | Not mine, Minor-judgment | infrastructure (machinery; protocol content) — authored by infrastructure, reviewed by operations + observability |
| `packages/proto-gen/scripts/verify-codegen.sh` (preamble/header: infrastructure; 2026-10-25 assertions: implementer, protocol reviews) | Not mine, Minor-judgment | infrastructure (machinery; protocol content) |
| `docs/protocol/CONVENTIONS.md` (new rule §6: per-kind facts are MediaKind-keyed) | Not mine, Minor-judgment | protocol |
| `scripts/guards/simple/validate-proto-per-kind-fields.sh` (new `_dt_guard_wrapper.sh` wrapper) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/proto_per_kind_fields.rs` (new; policy content protocol) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/common/proto_lex.rs` (new) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/common/mod.rs` (registration) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/main.rs` (subcommand arm) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/lib.rs` (registration) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/tests/proto_per_kind_fields_e2e.rs` (new; binary/clap-arm e2e tier, test finding 7) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` (guard runbook entry) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/simple/validate-proto-no-any.sh` (one-line duplication comment at `normalise()`) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/simple/validate-internal-proto-no-key-material.sh` (one-line duplication comment at `normalise()`) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mh-deployment.md` (one-line rollback: MC and client deploy and revert together) | Not mine, Minor-judgment | operations |
| `crates/mc-service/src/media_kind.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/participant.rs` | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/webtransport/handler.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/outcome.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/directive.rs` (`pick` removal) | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` (`JoinMedia.declared`) | Mine | — |
| `crates/mc-service/src/errors.rs` (`PublishKindInvalid` doc) | Mine | — |
| `crates/mc-service/src/grpc/mh_client.rs` (test: `&MediaKindLabel::ALL`) | Mine | — |
| `crates/mc-service/src/media_routing/slots.rs` (`ServerMutedKinds::ALL_MUTED`; `set_demand` zip) | Mine | — |
| `crates/mc-service/src/media_signaling/mod.rs` (doc: `receive_slot_caps`) | Mine | — |
| `crates/mc-test-utils/src/media.rs` (shared kind-keyed snapshot readers, DRY F4) | Mine | — |
| `docs/API_CONTRACTS.md` (kind-keyed wire blocks + Superseded 2026-10-25 notes) | Not mine, Minor-judgment | protocol |
| `scripts/lang/_dispatch.test.sh` (authored by infrastructure: lane-orphan test for `test:unit`) | Not mine, Minor-judgment | infrastructure |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/tests/**` (join, media_session rig, server_mute, media_client_signaling, admission, connection_update, otel_webtransport, accept_loop) | Mine | — |
| `crates/env-tests/src/fixtures/mc_session.rs` | Mine | — |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Mine | — |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `packages/sdk-core/**` | Not mine, Domain-judgment (paired; @paired-client implements) | client |
| `packages/sdk-svelte/**` | Not mine, Domain-judgment (paired; @paired-client implements) | client |
| `packages/web-app/**` (incl. `e2e/`) | Not mine, Domain-judgment (paired; @paired-client implements) | client |
| `docs/specialist-knowledge/client/INDEX.md` | Not mine, Minor-judgment (paired; @paired-client edits) | client |
| `docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md` | Not mine, Domain-judgment (paired; @paired-client writes, test reviews) | client |
| `docs/user-stories/2026-10-07-see-each-other.md` (Wire freeze Amendment 6) | Not mine, Minor-judgment | protocol |
| `docs/decisions/adr-0036-media-flow.md` (§5 amendment 2026-10-25) | Not mine, Minor-judgment (security also reviews) | protocol |
| `docs/observability/metrics/mc-service.md` (join error_type, mute rejection reasons, counting-unit note; label-taxonomy section is session 2) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` (rejected panel, mute panel, not_published mapping, error_type regex) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/kustomization.yaml` (authored by infrastructure: prometheus-rules split) | Not mine, Minor-judgment | infrastructure |
| `infra/kubernetes/observability/prometheus-config.yaml` (authored by infrastructure: prometheus-rules split) | Not mine, Minor-judgment | infrastructure |
| `infra/kubernetes/observability/kustomization.yaml` (authored by infrastructure: prometheus-rules split) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/kustomize_configmaps.rs` (authored by infrastructure: prometheus-rules split) | Not mine, Minor-judgment | infrastructure |
| `crates/env-tests/src/fixtures/alert_rules_loaded.rs` (authored by infrastructure: prometheus-rules split) | Not mine, Minor-judgment | infrastructure |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (authored by infrastructure: prometheus-rules split) | Not mine, Minor-judgment | infrastructure |
| `infra/docker/prometheus/rules/client-alerts.yaml` (one-line LOADING comment: `prometheus-rules-client`) | Not mine, Minor-judgment | infrastructure |
| `docs/observability/label-taxonomy.md` (kind-free list wording) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/client.md` (contract-faults row; @paired-client edits) | Not mine, Minor-judgment | client |
| `crates/env-tests/tests/24_join_flow.rs` (replay comment) | Mine | — |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` (`mc_media_mute_requests_rejected_total` in the eager-series list) | Mine | — |
| `docs/runbooks/client-dev-local.md` (`receive_slot_caps` field name) | Not mine, Minor-judgment | client |
| `docs/TODO.md` (session-2 `mediakind-metric-labels` entry; infra §Polyglot entry) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-incident-response.md` (Sc. text on "unknown", replay, not_published) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` (one-line rollback: MC and client deploy and revert together) | Not mine, Minor-judgment | operations |

---

## Planning

### Sizing and planned follow-up (Lead ruling 2026-10-25: agreed split accepted)

**Session 1 (this devloop):**
- the wire;
- the proto per-kind field-name guard, which goes green with the wire;
- MC and client;
- env-tests;
- docs: ADR, CONVENTIONS, story, manual test plan, and the catalog, dashboard and runbook text for the outcomes this changes.

**Session 2, slug `mediakind-metric-labels`** (the planned follow-up, not a deferral). It is recorded in `docs/TODO.md` beside infrastructure's §Polyglot Pipeline Follow-ups entry. Scope:
- the label projection, using observability's option (b): `media_kind` {audio, video} unchanged, plus a co-carried `content_kind` {main, screen}, with `slides` renamed to `screen`;
- a real Rust↔TS↔taxonomy drift test that reads both artifacts and is built on the existing `media_kind_label_matches_the_taxonomy_row` anchor;
- the label-literal guard, extending `metric_labels.rs` and `ts_metric_naming.rs` with the same self-test bar as below;
- GC's `telemetry_filter.rs` audio/video copy, the third consumer;
- the catalog taxonomy section, plus dashboards, alerts and runbooks with series-identity notes;
- porting `validate-proto-no-any.sh` and `validate-internal-proto-no-key-material.sh` onto `proto_lex`.

Session 1 adds **no** new `"audio"`/`"video"` label literal: every new kind-to-label site goes through `MediaKindLabel::from_proto`/`label()` (Rust) or the SDK's existing helper.

### Restated mechanism

Instance: "rename the audio_/video_ pairs". Mechanism: **every per-kind fact on the signalling wire is a function MediaKind → value and must be spelled as one.**

A token sweep of every `proto/**` file (signaling, internal) found 16 hits, all in `signaling.proto`. All are converted below:
- `Participant` 7/8;
- `JoinResponse` 13/14 (the sibling the brief did not list);
- `MuteRequest` 1/2;
- `ParticipantMuteUpdate` 4/5/7/8, plus the message-level `server_muted_by` 6;
- `ServerMuteRequest` 2/3;
- `UnmuteRequest` 1/2;
- `UnmuteResponse` 2/3.

`internal.proto` has none (MH is type-blind), so **MH is untouched**. **The allow-list is empty.** The same mechanism inside MC is re-keyed too: the actor's server-mute bools, `ParticipantInfo` and `RequestUnmute{request_audio, request_video}`. `MediaKindLabel::pick` is deleted together with every call site.

### Wire (protocol FINAL names, Lead-pinned)

```proto
enum ClientMediaState {
  CLIENT_MEDIA_STATE_UNSPECIFIED = 0;   // proto3 zero value ONLY: malformed, never a state
  CLIENT_MEDIA_STATE_NOT_PUBLISHED = 1;
  CLIENT_MEDIA_STATE_LIVE = 2;
  CLIENT_MEDIA_STATE_CLIENT_MUTED = 3;
}
message PublishedKind      { MediaKind media_kind = 1; bool client_muted = 2; }
message MediaKindState     { MediaKind media_kind = 1; ClientMediaState client_state = 2;
                             bool server_muted = 3; string server_muted_by = 4; }
message MuteChange         { MediaKind media_kind = 1; bool muted = 2; }   // requests only; never MediaKindState
message ReceiveSlotCap     { MediaKind media_kind = 1; uint32 max_slots = 2; }
```

| Message | Reserved (number AND name) | New |
|---|---|---|
| `JoinRequest` | 8 `publish_kinds` | `repeated PublishedKind published_kinds = 10;` (9 is `receive_capability`) |
| `JoinResponse` | 13, 14 `max_audio_receive_slots`, `max_video_receive_slots` | `repeated ReceiveSlotCap receive_slot_caps = 15;` |
| `Participant` | 7, 8 `audio_client_muted`, `video_client_muted` | `repeated MediaKindState media_states = 9;` |
| `ParticipantMuteUpdate` | `4 to 8`, with names `audio_server_muted`, `video_server_muted`, `server_muted_by`, `audio_client_muted`, `video_client_muted` (2/3 kept) | `repeated MediaKindState media_states = 9;` |
| `MuteRequest` | 1, 2 `audio_muted`, `video_muted` | `repeated MuteChange changes = 3;` |
| `ServerMuteRequest` | 2, 3 `audio_muted`, `video_muted` | `repeated MuteChange changes = 5;` |
| `UnmuteRequest` | 1, 2 `request_audio`, `request_video` (`participant_id` 3 stays MC-stamped) | `repeated MediaKind media_kinds = 4;` |
| `UnmuteResponse` | **deleted** (dead) | `reserved 10; reserved "unmute_response";` in BOTH `ClientMessage` and `ServerMessage`. The `test_handle_client_message_unhandled_type` fixture moves to another unhandled variant |

**Semantics.** CONVENTIONS §6 "Kind-keyed repeated fields" is the one statement. The proto comments cite it, and §6 cites the guard module for the vocabulary rather than restating it.
- A kind-keyed repeated field is a MAP BY KIND: at most one entry per kind.
- **MC receiving a client message:**
  - It rejects the WHOLE message (generic reply, counted, nothing applied) on any of: UNSPECIFIED, an unknown enum NUMBER (MC is authoritative on what it serves), VIDEO_SCREEN, a duplicate kind (judged on kind only), or an empty request list.
  - It never applies part of a message.
- **Join declaration (human no-compat direction, 2026-10-25):** `published_kinds` directly, no wrapper. Empty means receive-only; the neither-publish-nor-receive rule still applies.
- **Server snapshots** (`Participant.media_states`, `ParticipantMuteUpdate.media_states`, `JoinResponse.receive_slot_caps`):
  - **MC MUST emit exactly one entry per KNOWN non-UNSPECIFIED MediaKind** (AUDIO, VIDEO_CAMERA, VIDEO_SCREEN), with NOT_PUBLISHED explicit.
  - **MC MUST emit VIDEO_SCREEN as NOT_PUBLISHED, `server_muted` false, empty `server_muted_by`, and `max_slots` 0.** This is an MC invariant, unit-tested in the encoder.
  - **There is no unknown state**: MC knows every kind at admission.
- **Client receiving a snapshot:**
  - These are a PROTOCOL ERROR: a MISSING entry for a kind the client knows, a duplicate, a `media_kind` that is UNSPECIFIED or an unknown number, or a `client_state` that is UNSPECIFIED or an unknown number. No compatibility tolerance (human direction).
  - On a protocol error the session TERMINATES: during join, `join()` rejects (`failure_stage="mc_join_response"`), and after join the connection closes (`close_reason=server_error`). A `[dt-signaling-contract]` console WARN carries a bounded reason (`media_states_malformed` | `receive_slot_caps_malformed`); no new metric (observability's lean option). Nothing is applied.
  - The client validates the screen entry like any other kind (present once, known state) and does NOT check its value or render it.
- **Requests:**
  - An absent kind is unchanged, and an empty list is refused.
  - A present `muted=false` is an explicit unmute, which retires the "TAG-COMPATIBLE BUT NOT SEMANTICS-COMPATIBLE" paragraph.
- **Server-mute fields:**
  - `server_muted` is false on every NOT_PUBLISHED kind, because a server mute of an unpublished kind is refused.
  - `MediaKindState.server_muted_by` is per kind and **empty iff `server_muted` is false**. MC never server-mutes without an authenticated requester, so it is never "if any". It carries the moved SECURITY note: relayed identity, 256-byte truncate-before-log, render as text.
- **`max_slots = 0`** is a present 0 (video off, the R-26 lever).
- **Carrier map, one rule:** a snapshot on `Participant`, then full-snapshot deltas on `ParticipantMuteUpdate`.
  - **The post-JoinResponse server-mute replay is retired**, because `MediaKindState` on the roster makes it a duplicate source.
  - Per paired-client's observation, the comment says that a joiner learns its OWN server mute only from its self `ParticipantMuteUpdate`. A fresh join starts server-unmuted. Reconnect-within-grace has no self carrier at join; that is pre-existing, and the SDK has no reconnect.
  - Story-2 S-2 stays reversed.
- **`receive_slot_caps` is REQUIRED** on its own merits: every kind needs a cap (one entry per known kind; empty or incomplete is a protocol error).

### Proto per-kind field-name guard (ADR-0034, infra ruling)

Policy is protocol's; machinery is infrastructure's.
- **Placement:**
  - The module is `crates/dt-guard/src/proto_per_kind_fields.rs`, as subcommand `proto-per-kind-fields`.
  - The wrapper is `scripts/guards/simple/validate-proto-per-kind-fields.sh`, using the ~7-line `_dt_guard_wrapper.sh` pattern and auto-discovered by `run-guards.sh`.
  - The tests are in-module under `cargo test`. There is no `.test.sh` and no `layer3.sh` row.
- **Lexing:** a NEW `crates/dt-guard/src/common/proto_lex.rs`, general enough for both session-2 ports and holding NO policy. Its public API:
  - `//` comment strip;
  - a `/*` occurrence is a typed vacuity error, never a finding;
  - normalised statements, with `{`/`}` emitted as their own scope tokens.
  Its unit tests live in `proto_lex`.
  - A one-line comment goes at `normalise()` in the two bash proto guards: "duplicated by crates/dt-guard/src/common/proto_lex.rs; port tracked in docs/TODO.md §Polyglot Pipeline Follow-ups" (DRY/infra condition 2).
- **Field recogniser (F3):** a statement inside a `message` scope (nested included, with the scope stack giving the fq path) of the form `[optional|repeated] <type|map<K,V>> <name> = <n> [options];`, oneof members included. The recogniser excludes:
  - `option` lines;
  - `syntax` and `package`;
  - `import`;
  - enum bodies (enum values);
  - `reserved`, so the exemption is structural and never a name list;
  - `extensions`/`extend`;
  - `rpc`/`service`.
  Multi-line declarations are joined by the normaliser.
- **Match:** a field fails if any `_`-token of its name, lower-cased, is in the vocabulary. **camelCase decision:** proto field names are snake_case by `buf lint` (FIELD_LOWER_SNAKE_CASE), so a camelCase name is already a Layer-1 failure. The guard additionally splits camelCase humps so that `audioLevel` hits, which keeps the guard honest if lint is ever relaxed. It is fixtured.
- **Vocabulary (F4), in `proto_per_kind_fields.rs`:** DERIVED from the `MediaKind` enum found in `proto/`: the lower-cased tokens of each value after stripping `MEDIA_KIND_`, UNSPECIFIED excluded, which today gives {audio, video, camera, screen}. A typed synonym const adds {mic, microphone, cam} (protocol). If the enum is missing or yields zero tokens, that is vacuity (`vocabulary-empty`), never a pass.
- **Allow-list:** a typed const in the module. Each entry is a fully-qualified `dark_tower.signaling.v1.Message.field` plus a mandatory reason of at least 10 characters. A stale entry fails (`stale-allowlist`), and a missing or short reason fails. It ships EMPTY.
- **Rule ids:** `proto-per-kind-field` (content), `proto-per-kind-scan-vacuous` (vacuity: an empty file set, an empty file, `/*`, a missing enum, or a real-tree positive control not met), and `proto-per-kind-stale-allowlist`.
- **Real-tree positive control (F5):** at least 1 field per scanned file and at least 1 `reserved` in `signaling.proto`, otherwise vacuity. The scanned-field count is reported and asserted > 0.
- **Message (F6):** names the fq field, the CONVENTIONS §6 fix (`repeated <Entry>{MediaKind media_kind; ...}`) and the allow-list route via @protocol.
- No new key in `cross-boundary-ownership.yaml`.

### MC

1. **One per-kind state model in `media_kind.rs`** (DRY: extend it, no second kind enum).
   - `ClientKindState {NotPublished, Live, ClientMuted}` and `ClientMediaStates([ClientKindState;2])`.
     - ONE mapping, `ClientKindState::to_proto() -> ClientMediaState`, in `media_kind.rs`. `handler.rs:media_kind_states` calls it rather than matching inline. MC never decodes `ClientMediaState`, so there is one direction only (DRY-D).
     - `PublishSet` is derived only: `states.publish_set()`, plus a test-only `of` (CQ-4).
   - **Server mute has one store (CQ-2).** `ServerMuteAttribution([Option<String>;2])` holds `Some(requester)` iff that kind is muted, and `ServerMutedKinds` is derived (`attribution.muted_kinds()`).
     - "Muted without attribution" is unrepresentable.
     - `last` (the recency projection) and its tests are deleted.
   - `[T;2]` stays as internal storage behind `index`/`of`/`per_kind`. `pick()` is deleted, and no `[audio, video]` destructure remains outside `media_kind.rs`.
   - Deleted: `ClientMuteFacts`, `KindDecision<Option<bool>>::effective_moved`, `published_only`, `unpublished`, and the `connection.rs` filter path.
     - With no unknown state, "the fact changed but the effective value did not" no longer exists, so the `changed`/`effective` split collapses into one `changed`.
   - The `MediaKindLabel` module header is updated so it does not claim "two kinds" semantics this loop changes. The name stays until session 2.
2. **One parser (CQ-4, DRY-C):**
   ```rust
   fn parse_kind_list<I, T>(items: impl IntoIterator<Item = I>, split: impl Fn(I) -> (i32, T))
       -> Result<[Option<T>; 2], KindListRefusal>
   ```
   - It is built on `served_kind`. `KindListRefusal = Kind(KindRefusal) | Duplicate`, and the existing `KindRefusal` gains no duplicate variants.
   - It serves `PublishedKind` (T = bool), `MuteChange` (T = bool) and bare `MediaKind` (T = ()).
   - It fails on the first bad entry, allocates nothing, and a third valid entry is necessarily a duplicate.
   - The capability parser keeps calling `served_kind`. **Screen classification stays visible at the callers** (DRY-C), with one wire-to-kind switch (`served_kind`):
     - all three request surfaces (client mute, server mute, unmute) map `Kind(KindRefusal::Unsupported)` to a whole-request `not_published` refusal (Lead FINAL). Each mapping is pinned by a per-surface unit test;
     - capability maps it to `media_kind_unsupported`, as `capability.rs` does today. `media_kind_unsupported` stays capability-only.
   - Every reason token comes from ONE exhaustive `fn reason(self) -> &'static str` on the refusal types, shared with the capability spelling. There are no string literals at counter sites.
   - `PublishSet::parse` is deleted. `MuteChange::parse` becomes a thin caller.
3. **Client mute is proven at the boundary (CQ-3).**
   - `MuteChange::against(publish_set) -> Result<PublishedMuteChange, NotPublished>` is the only constructor of `PublishedMuteChange`.
   - The actor's client merge takes ONLY `PublishedMuteChange`, through `ClientMediaStates::apply(PublishedMuteChange) -> KindDecision<ClientKindState>`, applied to the `Live`/`ClientMuted` sub-state. NotPublished is unreachable by type. There is no `From<bool> for ClientKindState`.
   - Server mute runs its publish check once, in the actor, after the target lookup.
4. **Admission.**
   - Order: header version, then kinds (`publish_kind_invalid`), then receive, then neither-nor. Empty `published_kinds` is receive-only.
   - The roster entry is born definite. A joined-muted participant is PAUSED at MH from its first render.
   - **Nothing is enqueued on `ClientMuteFanout` at join**, so `ParticipantJoined` (bounded by admission) is the only per-join broadcast.
   - **Reconnect (CQ-7):**
     - The declaration's kind SET is compared, order-insensitive and kind-only, and must equal the membership's set; otherwise the reconnect is refused.
     - Its `client_muted` values are built into the same `PublishedMuteChange` after the set check and merged by the same `apply`.
     - A change is ENQUEUED on `ClientMuteFanout`, never broadcast directly.
     - A reconnect never resets server mute or attribution.
5. **Refusals, as whole-request refusals.**
   - Client mute runs: shape, then `against` (the publish set; screen counts as unpublished), then the no-op short-circuit, then the token, then the actor.
   - Server mute runs: authority, then shape, then the token, then the actor (authority re-check, target, `not_published`).
     - A non-host gets the one `Forbidden` reply whatever it sends. That already holds, because `reply_mute_refusal` has one text per surface.
     - The pure parse runs first only to pick the counter.
     - A host's `not_published` gets a reply byte-identical to `unknown_target`'s.
     - The requester always comes from `media.participant_id`.
   - `rate_limited` and `actor_unavailable` are reached only after the list check, which keeps observability's partition true.
6. **Unmute.**
   - Run order: shape (the parser), then the publish set on the connection (any named kind the requester does not publish, screen included, refuses the whole request as `not_published`), then the token, then the actor. A refusal goes to `rejected_total{surface="unmute_request"}` with a **rationed generic reply** through a new `unmute_rejection_reply_limiter`, so the suppressed-replies cell is reachable.
   - The actor re-encodes the relay from the parsed set, **restricted to the requester's currently server-muted kinds**, plus the MC-stamped id. It never forwards client bytes.
   - **Mixed case** (test 8): `[AUDIO, VIDEO_CAMERA]` with only the camera server-muted is `relayed` with `[VIDEO_CAMERA]` only.
   - A well-formed request naming only published kinds, none of them server-muted, is `not_server_muted` and is not relayed.
7. **Kept:** `ClientMuteFanout`, its bound, its constants and its tests (see the ledger).
8. **Encoder (CQ-5):** ONE `media_kind_states(&ParticipantInfo) -> Vec<MediaKindState>` in `webtransport/handler.rs`, used by `roster_entry` and the `MuteChanged` encode.
   - It is driven by `fn wire_entry(MediaKind) -> Option<MediaKindState>`, an exhaustive match where `Unspecified => None`. `receive_slot_caps` is built the same way.
   - A test sweeps `MediaKind::try_from(0..64)` and asserts the encoder emits exactly the accepted values minus UNSPECIFIED, once each. The SDK mirrors it (paired-client).
   - `server_muted_by` is set iff `server_muted` (unit-tested).

### Metrics (observability-accepted; amendments acked)

1. **Join refusals: no new token** (no-compat). `publish_kind_invalid` keeps its meaning (UNSPECIFIED, unknown, screen or duplicate kind in `published_kinds`); its field name in the catalog and item 12 becomes `published_kinds`. An empty list is a legal receive-only join. The user sees the generic "Invalid join request".
2. **`mc_media_mute_requests_rejected_total{surface, reason}`.** Its definition: "a request refused WHOLE for its kind list: the list is empty, malformed, or names a kind the subject does not publish. Kind-free because a refused request applied no kind."
   - Surfaces: {`client_mute`, `server_mute`, `unmute_request`}. `unmute_request` names the wire message, and `MuteSurface`'s doc says why.
   - Reasons: {`no_kind_named`, `media_kind_unspecified`, `duplicate_kind`, `not_published`}. `MuteRejection` drops `media_kind_unsupported`, which stays capability-only. Screen counts as `not_published` on all three surfaces, stated once on the row. **Expected-empty on every surface, with both preconditions stated:** the stock SDK names only kinds that are actually published and server-muted, and web-app hides host controls for NOT_PUBLISHED kinds. A non-zero value therefore reads as an SDK regression or a probe. **The dated 2026-10-25 note on `mc_media_unmute_requests_total`:** an UnmuteRequest naming an unpublished kind (screen included) used to answer `not_server_muted`, and now moves to `rejected_total{surface="unmute_request", reason="not_published"}`.
   - Cardinality is the 3×4 cross product, zero-initialised, and all 12 cells are reachable.
   - The decision order is documented per surface, including whether a token was spent and that server-mute `not_published` is actor-side, after authority and lookup.
   - A non-host naming an unpublished kind is `not_permitted` on the per-kind counter, not here. The "not_permitted undercounts non-hosts" note names exactly the reasons that cause it: the shape reasons, which are counted kind-free for non-hosts too.
   - **Expected-empty:** web-app hides the host mute control for a target kind whose `client_state` is NOT_PUBLISHED, which paired-client implements and the Client section records. So `server_mute/not_published` stays expected-empty for a stock client.
   - The per-kind `mc_media_mute_requests_total` loses `not_published`. Its partition statement is now "every MuteRequest whose kind list passes validation lands in exactly one bucket per named kind; whole-request refusals live on rejected_total", and the one-home sentence on rejected_total stays true.
   - Dashboard and runbook rows are updated (see Ops).
3. **`mc_media_refusal_replies_suppressed_total{surface}`** gains a reachable `unmute_request` cell. **`mc_media_unmute_requests_total`** no longer counts every UnmuteRequest:
   - a one-home sentence is added;
   - its kind-free row (~379, "plain bools ... both-false legal") is rewritten;
   - a dated note says an empty list is now refused as `no_kind_named`.
4. **Counting notes**, a 4th dated entry (2026-10-25) in "counting-unit changes, none a traffic change":
   - `applied` no longer rises with join rate (the join declaration carries the initial state);
   - `not_published` moves counters;
   - a request with a valid kind plus an unpublished one used to count `applied` for the valid kind and now counts nothing on the per-kind counter;
   - a reconnect-declaration merge is not a MuteRequest and is not counted there, but does count a fan-out if it changed something (not reachable in prod);
   - **mirrored on `mc_media_server_mute_requests_total`:** a server mute of an unpublished kind used to apply and is now refused whole.
5. **`mc_media_client_mute_fanouts_total`:** joiner states ride `ParticipantJoined`, and a fan-out counts only post-join changes plus reconnect merges. Expected-empty: no.
6. **Retired names:** no metric token names the replay or UnmuteResponse. The only `payload_kind`s are `participant_update_{joined,left,muted}`, and the replay used `participant_update_muted`. The catalog prose at ~1065 ("by the live broadcast and the late-joiner replay") is rewritten.
7. **No client contract-fault metric** (observability's lean option, Lead ruling): a malformed MC snapshot is an MC encoder bug, already covered by the exhaustive encoder, its byte-level unit tests and Layer 7. The client fails loudly (see Wire) on existing labels only (`failure_stage="mc_join_response"`, `close_reason=server_error`).
8. **Presence and zero-init proof:** `validate-counter-zero-init.sh` covers the new `rejected_total` cells and the surface, and `crates/env-tests/src/fixtures/metric_hygiene.rs` (32) checks them present.

### Rollout and rollback (ops; no-compat)

- **Deploy and rollback: MC and client deploy and revert together.** MH is not involved. No skew rows, no carve-out sections (human direction: no old client or MC exists).
- **Runbooks:**
  - `mh-deployment.md` §Rollback layer 2 and `mc-deployment.md` §Story 3 cutover "Rollback" and §How to Rollback: the "one component one version back holds" claim gets that one line, so the runbook does not assert something this commit makes false.
  - `mc-incident-response.md`: item 12's `publish_kind_invalid` wording uses `published_kinds` (empty = receive-only); the `applied` and counting rows and the `not_published` rows are rewritten; the "mic: unknown / camera: unknown" triage paragraph is replaced with "not publishing" semantics (and a malformed snapshot = MC encoder defect, escalate to meeting-controller).
  - A final grep over `docs/runbooks/` and `docs/observability/` for the retired field names, `unknown` and `replay`, plus the `crates/env-tests/tests/24_join_flow.rs` comment.
- **No new alerts** (operations withdrew them after observability's lean option).
- **Dashboards** (unconditional), `mc-media.json`:
  - the rejected panel is retitled "Mute Requests Rejected (by surface, reason)" and its description rewritten ("zero is healthy");
  - the mute panel description drops the post-join applied, "unknown" and the per-kind not_published;
  - the `not_published` value mapping is removed;
  - the join-refusals panel drops its error_type allow-list regex (every token shown, so the next one cannot be hidden).

### Client (@paired-client implements; agreed split, paired-client's text)

- The SDK encodes `JoinRequest.published_kinds` from the frozen publish set and the current MuteState. Receive-only means an empty list. The SDK has no reconnect (a single-use session).
- The post-join re-report is DELETED. Kept:
  - a join-window catch-up: a kind whose mute changed between building the JoinRequest and Joined gets exactly one `MuteChange` after Joined, and a kind toggled back gets none;
  - the publish gate in `bestEffortMuteReport`, now the one toggle send path.
- Kind-keyed `MuteRequest`/`ServerMuteRequest` (`changes`, one `MuteChange` per request). `UnmuteRequest.media_kinds` names exactly the kinds the self copy says are server-muted; if there are none, a typed local error is raised and nothing is sent.
- **Host mute controls are hidden for a target kind that is NOT_PUBLISHED** (observability 2c).
- ONE decoder, `signaling/mediaKindStates.ts`, for `media_states` and `receive_slot_caps`. The known-kind set is derived from the generated enum, and kinds map to tokens via `slotStateMap.ts:mediaKindToken`. It applies the client receive rule above: session-fatal for a missing, duplicate, UNSPECIFIED or unknown kind, or an UNSPECIFIED/unknown `client_state`. A typed `CONTRACT_FAULT` SignalingError, validated before any side effect; a `[dt-signaling-contract]` console WARN with a bounded reason; `failure_stage="mc_join_response"` at join, `close_reason=server_error` after.
- One per-kind cell type, shared by roster and update (CQ-6): `{clientState, serverMuted, serverMutedBy}`, seeded only from roster entries. The old per-kind fields and the tri-state are deleted, with exhaustive `never` switches.
- web-app renders `not_published | live | client_muted`. "unknown" is removed, and server-muted-by is shown per kind. Relayed ids are truncated to 256 bytes before logging and rendered as text (security).
- Fixed in passing: the stale MC home comment in `mediaMetrics.ts`.
- Tests: test items 9–11, plus the client ledger below. The e2e covers `client-mute-roster` (a LATE audio-only joiner; the recorder asserts the exact per-kind sequence from first render, "not publishing" as DOM text) and the server-mute specs. **Joined-muted is unit/component-tier only**: the web-app has no pre-join mute control, so its declaration is always `client_muted=false` and the manual plan cannot exercise it; manual step 4i covers the roster-snapshot half (an observer joining after E muted sees muted at once), and new step 4j covers "not publishing" with no "unknown" anywhere.

### Docs

- ADR-0036 §5, "Amendment (2026-10-25)":
  - (a) the join-carried client mute replaces the post-join report, superseding the 2026-10-24 "absent = unknown" text without editing it;
  - (b) refusal, not ignore, of unpublished-kind and screen mutes;
  - (c) per-kind `server_muted_by`, and the replay retired;
  - (d) S-2 stays reversed;
  - (e) malformed per-kind data fails loudly; no compatibility layer; MC and client deploy and revert together.
- `docs/protocol/CONVENTIONS.md` §6 "Kind-keyed repeated fields" (the protocol's text), citing the guard module.
- Story Design §Wire freeze, "Amendment 6 (human, 2026-10-25)": the types, reservations, the Unmute* fate, the §6 pointer, the one-line rollback note, and `receive_slot_caps` required on its own merits. It supersedes Amendment 5's field rulings.
- Catalog, dashboards and runbooks: as above.
- `docs/TODO.md`: the session-2 entry `mediakind-metric-labels`, written in this loop before Gate 3. It names both bash proto guards to port onto `proto_lex`, the drift test extending the existing `media_kind_label_matches_the_taxonomy_row` anchor, and GC `telemetry_filter.rs`.
- `docs/runbooks/devloop-validation.md` §6.3: a row citing the dt-guard subcommand `proto-per-kind-fields`.

### Tests (MC)

- **Units:**
  - the parser table: empty, each kind, both orders, UNSPECIFIED, unknown, -1, screen, a conflicting duplicate (`{AUDIO,true},{AUDIO,false}`), and a long hostile list;
  - the state model and the derived publish set;
  - `against`/`apply`;
  - `ServerMuteAttribution`, where muted iff `Some`;
  - `media_kind_states`: the 0..64 sweep, screen = NOT_PUBLISHED/false/""/0, `server_muted_by` iff `server_muted`, and the reserved tags unwritten (byte check).
- **Join:**
  - **A table** of refusals (UNSPECIFIED, unknown int, screen, conflicting duplicate), each `publish_kind_invalid`. Each asserts the exact `error_type` cell (MetricAssertion delta 1, others unobserved), **no roster entry, and no `ParticipantJoined` at an existing peer, fenced by a later valid join that is that peer's next frame.**
  - An **ordering table**: {v2 header + bad kind} gives `header_version_unsupported`; {bad kind + invalid receive capability} gives `publish_kind_invalid`.
  - An EMPTY `published_kinds` is admitted receive-only, and a peer's `ParticipantJoined` shows the joiner with every kind (all three entries) NOT_PUBLISHED.
  - `receive_slot_caps` carries one entry per known kind, including a cap of 0 and screen = 0.
- **Roster:**
  - A late joiner sees an audio-only peer as AUDIO LIVE/CLIENT_MUTED and VIDEO_CAMERA NOT_PUBLISHED, and a joined-muted peer as CLIENT_MUTED, with no MuteRequest ever sent.
  - **No-replay fence:** a host server-mutes P before L joins, and L's roster entry for P carries `server_muted` plus `server_muted_by`. After L's JoinResponse the host server-mutes another participant Q, and **L's first post-JoinResponse frame is Q's update** (no replay frame before it).
  - **Joined-muted at MH:** P joins with audio `client_muted`; its first registration snapshot is PAUSED at generation N; P unmutes, and the snapshot shows FORWARD at N+1.
- **Per-surface integration refusals** (client, server, unmute). For each surface: empty gives `no_kind_named`; then duplicate, UNSPECIFIED, screen, and an unpublished kind (screen and unpublished both give `not_published` on all three surfaces). Each asserts:
  - a MetricAssertion delta of exactly 1 on the `{surface,reason}` cell;
  - every other reason, and the per-kind `mute_requests_total` / `server_mute_requests_total` / `unmute_requests_total`, unobserved;
  - the generic reply;
  - no state change (fence).
- **Server-mute ordering:** a host gets `not_published` with a reply byte-identical to `unknown_target`'s; a non-host with a malformed list gets `not_permitted`'s reply and is counted kind-free.
- **Unmute:** the mixed case is relayed with `[VIDEO_CAMERA]` only; a request naming no server-muted kind is `not_server_muted` and not relayed; an unpublished kind or screen is refused whole as `rejected_total{unmute_request,not_published}` and never relayed.
- **Reconnect** (actor level; the SDK has no reconnect, so not wire level):
  - (a) a different kind set is refused, with no state change;
  - (b) the same set with a changed `client_muted` is applied, enqueued, and fanned out exactly once (fence);
  - (c) the same set with an unchanged `client_muted` produces no fan-out (fence: a later real self-mute is the peer's next frame);
  - server mute and attribution survive.
- **Join storm, re-pointed with a positive control:** a join burst enqueues nothing (`emitted` = `deferred` = 0). Then one real self-mute emits exactly once to every peer in the same test.
- proto-gen round-trip for the new messages.
- **Guard (`proto_per_kind_fields.rs` + `proto_lex.rs`, in-module).** Every red fixture asserts the specific finding (fq field, line, rule id), not just failure.
  - Red: prefix, suffix, infix, a bare `audio`, `audioLevel` (camelCase), a oneof member, a map field, a nested message, a multi-line declaration, a synonym (`mic_level`).
  - Green: a reserved name, an enum value `MEDIA_KIND_VIDEO_CAMERA`, a commented-out field, `audiolevel`, `videos`, `microsecond`, and an allow-listed existing field (the positive allow path).
  - Vacuity: a missing enum, `/*`, an empty file set; plus stale-allow and short-reason failures.
  - **Neutered-matcher proof:** the seams are plain function parameters, `check_with(root, &Vocabulary, &[AllowEntry])`, with `run()` passing the derived vocabulary and the const `ALLOW`. There is no env var and no `cfg(test)` branch inside `check`. With an empty `Vocabulary` passed, the red fixtures go green, so the test proves it detects a broken matcher. The allow-listed-field green fixture passes a test `&[AllowEntry]` the same way.
  - A real-tree test: green over `proto/`, with a scanned-field count > 0 asserted.
- env-tests 27/35/24, the `mc_session` fixture and metric hygiene (32) move to the new shapes (Layer 7 is the Lead's).

### R-15 test migration ledger (MC; client rows in paired-client's section)

| Test (f606182d) | Fate | Replacement / note |
|---|---|---|
| `client_mute_fanout.rs`: `subjects_are_served_first_pending_first_…`, `remove_clears_both_structures…`, `pop_ready_respects_the_bucket…`, `churning_subjects_cannot_starve…` | kept | unchanged |
| `meeting.rs` `a_real_client_mute_fans_out_to_everyone_else_and_a_no_op_to_no_one` | kept | the wire shape becomes `media_states` |
| `meeting.rs` `a_first_unmuted_report_fans_out_as_a_fact_and_renders_nothing` | deleted | no unknown state; replaced by the roster "joined-muted / unmuted definite" tests |
| `meeting.rs` `a_client_mute_frame_carries_the_current_server_mute_and_its_imposer` | rewritten | per-kind `server_muted_by` on `media_states` |
| `meeting.rs` `a_client_mute_storm_is_coalesced_per_meeting_and_converges` | kept | shape only |
| `meeting.rs` `a_joiner_during_a_pending_fan_out_gets_the_current_fact_twice_consistently` | kept | shape only |
| `meeting.rs` `a_pending_subject_that_leaves_is_never_fanned_out_after_it_left` | kept | |
| `meeting.rs` `a_server_mute_of_a_pending_subject_supersedes_its_client_fan_out` | kept | |
| `meeting.rs` `a_join_storm_of_first_reports_is_bounded_by_the_meeting_bucket` | rewritten | `a_join_storm_enqueues_no_fan_out_and_a_later_toggle_still_emits_once` (positive control) |
| `meeting.rs` `a_meeting_ending_with_pending_fan_outs_exits_cleanly` | kept | |
| `media_kind.rs` `a_client_fact_decision_separates_the_wire_question_from_the_media_one` | deleted | the split collapsed; replaced by `ClientMediaStates::apply` tests |
| `media_kind.rs` `a_change_restricted_to_the_publish_set_drops_only_unpublished_kinds` | inverted | `against_refuses_any_unpublished_kind_whole` |
| `metrics.rs` `record_client_mute_fanout_counts_each_outcome_on_its_own_cell` | kept | |
| `handler.rs` `a_mute_update_never_emits_the_retired_tags_2_and_3` | rewritten | covers retired 2–8 |
| `handler.rs` `the_roster_entry_carries_client_facts_set_iff_known` | rewritten | `the_roster_entry_carries_one_definite_state_per_known_kind` |
| `server_mute_integration.rs` `a_client_mute_reaches_other_sessions_and_a_repeat_reaches_no_one` | kept | shape only |
| `server_mute_integration.rs` `a_first_unmuted_report_fans_out_and_neither_pushes_nor_advances_the_generation` | deleted | no first report; replaced by joined-muted PAUSED→FORWARD |
| `server_mute_integration.rs` `the_join_roster_carries_client_mute_facts_set_iff_known` | rewritten | definite states, with no MuteRequest sent |
| `server_mute_integration.rs` `a_client_mute_of_an_unpublished_kind_is_ignored_and_counted` (mixed request, ~1385) | **inverted** | `a_client_mute_naming_an_unpublished_kind_is_refused_whole`: audio is NOT applied, the `rejected_total{client_mute,not_published}` cell is delta 1, and the fence is a later valid toggle as the peer's next frame |
| `proto-gen` round-trip R-15 cases | rewritten | new messages |

No fence is replaced by a timed wait or an absence-by-timeout.

### R-15 test migration ledger (client; @paired-client)

| Area | Test | Fate |
|---|---|---|
| SDK signaling | roster entries carry client mute with presence | rewritten (every kind's state) |
| SDK signaling | ParticipantJoined same projection (absent/present) | rewritten (joined-muted at once) |
| SDK signaling | ParticipantMuteUpdate projects client mute with presence | rewritten (full per-kind snapshot) |
| SDK signaling | projects SERVER mute and who applied it | rewritten (per kind) |
| SDK signaling | bounds relayed ids to 256 | kept (per-kind `server_muted_by`) |
| SDK signaling | MuteRequest/ServerMuteRequest names ONLY the changed kind (x8) | rewritten (one `MuteChange`) |
| SDK signaling | unmute request asks for audio | rewritten (named kinds x3) + new local refusal |
| SDK signaling | caps present/absent/0 (x3) | rewritten x2; absent deleted (now a malformed row) |
| SDK signaling | join `publish_kinds` | rewritten as a 4-row `published_kinds` table (receive-only empty, audio-only, both, joined-muted) |
| SDK signaling | (new) decoder malformed table, all carriers + caps, unknown kind number included (incl. "unknown kind IN PLACE OF a known one", the row that pins the known-kind check under mutation) | new |
| SDK signaling | (new) UnmuteRequest relay kinds: decode x3, malformed x5 (session-fatal), positive control | new |
| Session | reports EVERY published kind after join | inverted (declared; the first MuteRequest after join is the explicit toggle) |
| Session | unpublished kind never reported | rewritten (not declared) |
| Session | mute set BEFORE join reaches MC in the post-join report | rewritten (rides the declaration, not re-sent, fenced) |
| Session | per-kind client mute, each report names ONLY that kind | kept |
| Session | toggle of an UNPUBLISHED kind never reported (x2) | kept |
| Session | ABSENT caps read unknown | deleted; "before join caps unknown" |
| Session | (new) join-window catch-up x2; requestUnmute kinds x3 + refusal | new |
| Session | (new) malformed snapshot at join: typed CONTRACT_FAULT, mc_join_response, server_error, bounded WARN | new |
| Store | client mute of others, ABSENT never changes a fact | rewritten |
| Store | client-mute change can never clear a server mute | rewritten (independence per kind) |
| Store | LEAVES forgotten; rejoin unknown | rewritten (rejoin seeded afresh) |
| Store | (new) roster-seeded-before-listed, with positive control | new |
| web-app | participantState tri-state 18-row table | rewritten (3 x 3 x 2 states) |
| web-app | NO slot fallback | kept (unknown half deleted) |
| web-app | arrival order does not matter | deleted (both carriers are full snapshots) |
| web-app | server mute carries resolved name | rewritten per kind |
| web-app | (new) unseeded cell throws; copy exhaustive, no "unknown"; not_published row text + no host control | new |
| e2e | client-mute-roster.spec.ts | rewritten (LATE audio-only; exact sequence; "not publishing" DOM text) |
| e2e | server-mute.spec.ts | tokens updated |

## Expected Layer-State

- Layer 6 `buf breaking`: **0 findings expected**. Field and oneof-variant deletions reserve number and name, and the UnmuteResponse message deletion is covered by the MESSAGE_NO_DELETE except. Any finding is unpredicted.

---

## Gate 1 Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Paired Client | confirmed (no-compat revision) |
| Security | confirmed (re-confirmed after no-compat revision) |
| Test | confirmed (no-compat revision) |
| Observability | confirmed (no-compat revision) |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (no-compat revision) |
| Protocol | confirmed (no-compat revision) |
| Infrastructure | confirmed |

## Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S-1: byte-identical refusal replies tested. Residual: MC and client co-deploy (human direction) |
| Test | RESOLVED-FIXED | 10 | 10 | 0 | R-15 migration with fences intact; codegen oracle green (protocol run) and wired; covers design, Layer 7 pending |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | Scoped join panel restored; TODO and taxonomy text; rejected_total in the env-test presence list. Rules split reviewed clean |
| Code Quality | RESOLVED-FIXED | 11 | 11 | 0 | One type family in media_kind.rs; no silent fallbacks; exhaustive matches; dead paths removed |
| DRY | RESOLVED-DEFERRED | 7 | 7 | 1 | One home per language; deferral: bash proto guards port in mediakind-metric-labels |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | mh-deployment table; receive_slot_caps in 4 docs; rejected_total presence check. Infra hunks clear |
| Protocol | RESOLVED-FIXED | 5 | 5 | 0 | buf lint clean; breaking 0 with positive controls; codegen oracle fixed and wired; API_CONTRACTS; Debug comment; nits |
| Infrastructure | RESOLVED-DEFERRED | 6 | 6 | 1 | Guard fail-opens (single quote, hex) and 4 more fixed; deferral: port the 2 bash proto guards in mediakind-metric-labels. Authored hunks (rules split, codegen wiring, lane-orphan test) are reviewed by ops and obs |
| Paired Client | RESOLVED-FIXED | 1 | 1 | 0 | Client ledger row for the malformed-snapshot session test |

## Lead Notes

- Codegen oracle wiring (proto-gen:test renamed to test:unit), proof of record via `scripts/lang/ts/test.sh --skip-nx-cache` (infrastructure): RED with the pre-fix assertions gave rc=1, STATUS=FAIL, only `proto-gen:test:unit` failing on the missing `publishKinds` symbol. GREEN with the implementer's assertions gave rc=0, `verify-codegen: all checks passed` (144 OK), STATUS=OK. `nx show projects --with-target test:unit` includes proto-gen. Regression guard: `_dispatch.test.sh::test_nx_test_targets_reached_by_a_lane`, 70/70.
- Gate 3 PASSED: 9/9 verdicts (7 RESOLVED-FIXED, 2 RESOLVED-DEFERRED for the same Gate-1 bash proto-guard port deferral).

- Lead fast check #1: TOTAL_RESULT=OK (L1-6, 371s).

- The human settled the design on 2026-10-24/25: no "unknown" state; PublishedKind in JoinRequest, with joins lacking it refused; client and server mute stay independent; every per-kind fact keyed by MediaKind. Observability chooses the label taxonomy at Gate 1.
- Observability taxonomy decision (Gate 1): option (b). media_kind stays {audio, video}; a new content_kind {main, screen} is co-carried wherever media_kind is (MC and the client, never MH); the declared `slides` is renamed `screen`. No proto AUDIO rename is needed for labels. The whole labels set moves together to session 2.
- Lead rulings (Gate 1): (1) `JoinRequest.publish = 10` (tag 9 is `receive_capability`) is a `PublishDeclaration { repeated PublishedKind kinds }` wrapper; presence means declared, so an absent field (old client) is refused loudly and an empty list is receive-only (protocol). (2) `max_audio/video_receive_slots` become a required `receive_slot_caps` (tag 15), one entry per known MediaKind (screen gets 0), which is also the old-MC detector (the new SDK fails the join, `failure_stage="incompatible_controller"`). Snapshot lists carry every known kind, screen always NOT_PUBLISHED. (3) No rollback compatibility across this commit (new client on old MC is silently receive-only); accepted because everything is co-deployed with no production. MC and client revert together; no dual-write.
- Infra ruling (Gate 1, on CQ-1, ADR-0034): the per-kind guard is a dt-guard subcommand `proto-per-kind-fields` (`crates/dt-guard/src/proto_per_kind_fields.rs`) with a thin shell wrapper and a new shared `common/proto_lex.rs`; in-module tests, no layer3.sh pre-step. Porting the two existing bash proto guards onto proto_lex goes to session 2 (docs/TODO.md §Polyglot Pipeline Follow-ups); the wider ADR-0034 bash-guard question (about 10 guards) is recorded there for the human.
- Session-1 additions from Gate 1: the exported `dt_client_media_signaling_contract_faults_total{reason}` (the old-MC skew detector), `failure_stage="incompatible_controller"`, and two warning-only alerts, `ClientSignalingControllerSkew` and `ClientSignalingContractMalformed`.
- Gate 1 PASSED (~01:59): all 10 confirmed; classification guard OK. Final metrics ruling (observability): any unpublished kind, screen included, on any of the 3 surfaces is `rejected_total{reason="not_published"}`; 12 cells, all reachable; `media_kind_unsupported` is capability-only.
- HUMAN DIRECTION (2026-10-25, after Gate 1): backward and forward compatibility is not a concern, since no old clients or MCs exist; use the cleanest design with no compat baggage. This supersedes the earlier Lead rulings (1) and (3) and the skew detector. Dropped: the PublishDeclaration wrapper (`repeated PublishedKind published_kinds = 10` directly; protocol's final name), `publish_declaration_missing`, the unknown-kind carve-out (unknown kinds are malformed), the skew alert, rows and `incompatible_controller`, and the rollback carve-outs (now "MC and client deploy and revert together"). Kept: reserved tags (buf), fail-loud validation, required receive_slot_caps, the screen MC invariant, the guard and refusal semantics. Phase returned to planning for re-confirmation by affected reviewers.
- Note: implementation had already begun under the first Plan approved when the no-compat direction arrived. Because the revision only removes things, the Lead let implementation continue with the trims applied, while affected reviewers re-confirm against the revised plan in parallel (deviation from strict Gate-1 sequencing; recorded).
- prometheus-rules ConfigMap split (infra, in-loop): done before the Lead's "don't split" ruling arrived, then KEPT on its merits. It resolves the TODO "split before the next rules addition" (79.8% of the cap); six per-service generators, projected volume, verified with negative controls. Gate-3 review by operations and observability (the author doesn't review its own hunks). The dropped contract-fault alerts are NOT reinstated.
- Panel: implementer (meeting-controller), paired-client, security, test, observability, code-reviewer, dry-reviewer, operations, protocol (GSA `proto/**`), infrastructure (guard machinery). Semantic guard not spawned.
- INDEX/review-protocol: teammates read the files as their first action, as in the previous loop.
- SPLIT TAKEN (implementer sizing, 2026-10-25; Lead accepted per the human's pre-agreed split). Session 1 (this loop): wire + proto per-kind field-name guard + MC + client + env-tests + docs, also re-keying JoinResponse max_audio/video_receive_slots. Session 2 (follow-up devloop): metric-label projection/rename, dashboards/alerts/runbooks, label-literal guard, catalog taxonomy section. Session 2 needs a TS projection-vs-domain-type split (the SDK uses `audio`/`video` literals as its kind type).

## Pre-Work

None

---

## Implementation Summary

### Wire (`signaling.proto`)
- `ClientMediaState` {UNSPECIFIED=0 malformed, NOT_PUBLISHED, LIVE, CLIENT_MUTED}; `MediaKindState`, `PublishedKind`, `MuteChange`, `ReceiveSlotCap`.
- `JoinRequest.published_kinds = 10` (tag 8 reserved); `JoinResponse.receive_slot_caps = 15` (13/14 reserved); Participant and ParticipantMuteUpdate carry `media_states`; MuteRequest, ServerMuteRequest and UnmuteRequest are kind-keyed; UnmuteResponse deleted. Every retired tag and name is reserved; buf breaking reports 0 findings.
- One entry per known kind, screen always NOT_PUBLISHED (MC invariant); unknown, duplicate or UNSPECIFIED kinds are malformed.

### MC
- State is known at admission; the post-join replay is removed; one kind-list parser; `ServerMuteAttribution` is the single server-mute store; mute requests naming an unpublished kind are refused whole and counted on `rejected_total{surface,reason}` (3×4, zero-initialised).

### Client
- The SDK sends `published_kinds` with initial mute; the post-join re-report is replaced by a join-window catch-up; malformed snapshots are a session-fatal CONTRACT_FAULT with a bounded WARN; "not publishing" replaces "unknown"; host controls are hidden for unpublished kinds; unmute names only server-muted kinds and relays the kinds to the host.

### Additional Changes
- `dt-guard proto-per-kind-fields` on the new `common/proto_lex.rs` (ADR-0034).
- Codegen oracle wired into Layer 4 and CI (`proto-gen:test:unit`), with a lane-orphan regression test in `_dispatch.test.sh`.
- prometheus-rules ConfigMap split per service (infrastructure; resolves a filed TODO).
- Docs: ADR-0036 §5 amendment, CONVENTIONS §6, API_CONTRACTS, story Amendment 6, runbooks (co-deploy and revert line), catalog and dashboards, manual test plan step.

---

## Files Modified

```
 113 files changed, 5720 insertions(+), 2878 deletions(-)
?? crates/dt-guard/src/common/proto_lex.rs
?? crates/dt-guard/src/proto_per_kind_fields.rs
?? crates/dt-guard/tests/proto_per_kind_fields_e2e.rs
?? packages/proto-gen/scripts/codegen-paths.sh
?? packages/proto-gen/scripts/codegen.sh
?? packages/sdk-core/src/signaling/__tests__/mediaKindStates.test.ts
?? packages/sdk-core/src/signaling/mediaKindStates.ts
?? packages/sdk-svelte/src/__tests__/helpers/mediaStates.ts
?? packages/web-app/src/__tests__/helpers/mediaStates.ts
?? scripts/guards/simple/validate-proto-per-kind-fields.sh
```

### Key Changes by File
See the Cross-Boundary Classification table (every file, verified by the scope guard).

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` gave **TOTAL_RESULT=OK** (2196s): L1 12s, L2 5s, L3 179s, L4 251s, L5 10s, L6 6s, L7 1733s (`env-tests-passed`, `browser-e2e-passed`: 30 passed, including `client-mute-roster.spec.ts`). Log: Lead scratchpad `mk-gate2-1.log`.

Earlier: the Lead fast check before review was OK; the codegen oracle RED and GREEN through the TS lane are recorded in Lead Notes.

---

## Code Review Results

See § Gate 3 Verdicts. Totals: 50 findings, 50 fixed, 1 accepted deferral (shared by DRY and Infrastructure). Semantic Guard was not spawned.

---

## Accepted Deferrals

- `docs/TODO.md` §Polyglot Pipeline Follow-ups — port 2 bash proto guards onto proto_lex (session 2)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `c83eaa91`
2. Review all changes: `git diff c83eaa91..HEAD`
3. Soft reset (preserves changes): `git reset --soft c83eaa91`
4. Hard reset (clean revert): `git reset --hard c83eaa91`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: MC and client deploy and revert together (the whole commit plus the browser bundle built from it). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: The human's no-compat direction arrived after Gate 1
**Problem**: the wrapper, the skew detector, the alerts and the unknown-kind carve-out were already planned, and partly built.
**Resolution**: all were stripped, with reviewer re-confirmation; a grep for every dropped token is clean.

### Issue 2: Crossed messages
**Problem**: several rulings crossed in flight (field names, unmute refusal, ConfigMap split), causing rework.
**Resolution**: FINAL pins were sent to all parties, and the Lead checked the tree state before ruling.

### Issue 3: Masked gate
**Problem**: the proto-gen codegen oracle ran in no layer and no CI job, so it sat red unnoticed.
**Resolution**: renamed to `test:unit`, with a lane-orphan regression test.

---

## Lessons Learned

1. State the compatibility posture at the start of a wire change; it decides half the design.
2. Pin names and tags in one Lead message to all parties instead of relaying them.
3. A check that runs nowhere is worse than none; the lane-orphan test now catches the class.

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
