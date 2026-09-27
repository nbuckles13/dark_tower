# Devloop Output: MH server mute at ingress + multi-party forwarding proof (story 2 task 10)

**Date**: 2026-09-26
**Task**: MH server mute (R-9), multi-party proof (R-4/R-5/R-33), env-test speed, Kind handler capacity — see task-10 prompt
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/hear-each-other`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9c664bb4e3b26c61bdd5f50883beec7b47a615dd` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (spawned)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `CLEAR` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `CLEAR` |
| DRY | `RESOLVED-FIXED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |
| Paired meeting-controller | `RESOLVED-FIXED` |
| Protocol (GSA owner, conditional) | `CLEAR` |

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
MH server mute at ingress (R-9), the multi-party forwarding proof (R-4/R-5/R-33: unit, integration S10c, env-tests S4/S5/edge churn), env-test speed (Kind scrape 5 s, one derived settle, JWT serial-group investigation), and Kind handler capacity (egress budget bump).

### Scope
- **Service(s)**: mh-service (code); env-tests; Kind infra (Prometheus config, MH budget overlay); docs
- **Schema**: No
- **Cross-cutting**: Yes (proto comment, Prometheus config, env-test fixtures shared with MC suites)

### Debate Decision
NOT NEEDED — the design is fixed by ADR-0036 §7/§9 and `internal.proto` field 7; the two prompt conflicts found (vectors-file token, 10x budget) are resolved by recorded rulings/arithmetic below and escalated to the Lead for sign-off, not debated.

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
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/routing/mod.rs` | Mine | — |
| `crates/mh-service/src/media/forward.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/tests/media_server_mute_integration.rs` (new) | Mine | — |
| `crates/mh-test-utils/src/admission.rs` | Mine | — |
| `crates/mh-test-utils/src/media_policy.rs` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `proto/dark_tower/internal/v1/internal.proto` (comment-only, `MutedSource` + field 7 doc; GSA) | Not mine, Domain-judgment | protocol |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/dashboard-conventions.md` | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` (15 s cadence statement) | Not mine, Minor-judgment | observability |
| `docs/TODO.md` (D1 update) | Not mine, Minor-judgment | infrastructure |
| `infra/kubernetes/observability/prometheus.yml` | Not mine, Minor-judgment | infrastructure |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/configmap.yaml` (comments only: "running image does not parse the muted-source field") | Not mine, Minor-judgment | infrastructure |
| `crates/env-tests/src/fixtures/metrics.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mh_grpc.rs` (new, hoisted from test 29) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/media.rs` (hoisted frame/bind helpers from test 27) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/egress_admission.rs` (new, S9 sizing one home) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/participant.rs` (new, `Participant` vocabulary + triage literals hoisted from test 27) | Not mine, Minor-judgment | test |
| `crates/env-tests/Cargo.toml` (`media-protocol`, `tonic` dev -> normal deps for the lib fixtures) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/eventual.rs` (scrape derivation comment/budget) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` | Mine | — |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Mine | — |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` | Mine | — |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Mine | — |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (import hoisted helpers; no behaviour change) | Not mine, Minor-judgment | meeting-controller |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` (settle constant) | Not mine, Minor-judgment | meeting-controller |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` (one stale config-file citation in a doc comment) | Not mine, Minor-judgment | observability |
| `crates/mc-service/src/media_admission/rotation.rs` (doc comment citing the scrape key, no code) | Not mine, Minor-judgment | meeting-controller |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` (pointer to the hoisted participant fixture) | Not mine, Minor-judgment | meeting-controller |
| `scripts/layer7.test.sh` (fake dev-cluster learns the `deploy` verb) | Not mine, Minor-judgment | infrastructure |
| `packages/web-app/e2e/mcMetrics.ts` (comments naming the 15 s scrape only) | Not mine, Minor-judgment | client |
| `infra/grafana/dashboards/mh-media.json` (panel description text only) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-overview.json` (panel description text only) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` (stale cadence sentence) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/prometheus.yml` (reciprocal ANCHOR comment only) | Not mine, Minor-judgment | infrastructure |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (one description sentence) | Not mine, Minor-judgment | observability |
| `scripts/layer7.sh` (apply the Kind observability overlay / a service's manifests when their paths change, bounded rollout wait) | Not mine, Minor-judgment | infrastructure (reviewed by operations) |
| `docs/runbooks/devloop-validation.md` (Layer-7 step note, if §6.7 lists steps) | Not mine, Minor-judgment | infrastructure (reviewed by operations) |

---

## Planning

### Problem restated in mechanism-language

MH binds a publisher (`SenderId`) to each connection at spawn, after the JWT gate and MC's `NotifyParticipantConnected`. The forward path re-reads the per-meeting routing snapshot on every frame. Server mute is therefore one more property of that snapshot: a meeting-scoped set of spawn-bound senders whose ingress frames are dropped before any decode or fan-out. It is installed, swapped, re-asserted, released and forgotten exactly as edges are (one `ArcSwap` store), so re-assert survival, restart re-learning and one-generation unmute follow from the existing snapshot lifecycle and need no new mechanism.

The wider class is **per-meeting ordinals must never be resolved outside their meeting** (the `by_sender` invariant). The muted set is a new instance of it. It lives inside `MeetingRoutes`, keyed by `SenderId`, and is reachable only through a `MeetingKey`-taking accessor. It is pinned by a two-meeting test (security S-2).

### Resolved prompt conflicts (need Lead sign-off; each deviates from the prompt's literal words)

1. **`server_muted` is NOT added to `proto/test-vectors/frame-v2.vectors.json` `reject_reasons`** (Option A; @protocol ruled as owner, @security concurs).
   - That array has a single home for codec, crypto and client tokens. It is set-equality pinned in both directions against the client's `rejectReason.ts`, and the client must EMIT every `drops_frame` token. Adding the token would force the browser to declare a drop it can never observe.
   - `media_metrics_integration.rs::no_mh_local_drop_reason_collides_with_a_cross_language_reject_token` would also go red.
   - `label-taxonomy.md:86` requires one home per family. `server_muted` belongs to the relay family, which lives in `MediaDropReason`, alongside `no_policy`, `no_subscriber` and the other relay tokens.
   - The token's full set of homes is `MediaDropReason::ServerMuted` + `ALL` + the catalog entry. `label-taxonomy.md` is NOT edited.
2. **The Kind egress budget goes up 2.5x, not ~10x** (@operations F-A, verified; Lead ruling R2 as amended).
   - The derived ceiling is `budget / 2_500_000` (video cost).
   - Env-test 28 needs `P*S > 2*ceiling` with `S <= MC_MAX_RECEIVE_SLOTS = 8` and `P <= MAX_PARTICIPANTS = 40`. The largest reachable `P*S` is 320, so any ceiling >= 160 turns 28 into a PRECONDITION panic.
   - A 10x ceiling of 400 would need about 101 participants. That breaks the per-meeting bound (512 < 808) and AC's 100-per-minute registration limit.
   - A 400-stream ceiling would also risk an OOM on the 1Gi/1000m pod.
   - Choice (Lead R2): `MH_EGRESS_BUDGET_BPS` 100_000_000 -> **250_000_000**. The ceiling goes 40 -> **100** (in the hundreds; 2.5x the observed 39/40 peak).
   - Test 28 sizes itself to **P = 26 participants x 8 slots = 208 streams > 2*100**, with `MAX_PARTICIPANTS` unchanged at 40 and no logic change. The per-meeting worst case of 208 stays within 512.
   - Other test-28 preconditions still hold, and test 28 asserts them from the live ConfigMap:
     - `MH_MAX_EGRESS_STREAMS_PER_MEETING` 512 > 208, so `rejected_invalid` cannot fire first;
     - `MH_MAX_TOTAL_EGRESS_EDGES` 65536 > 100, so MH does not refuse to boot;
     - 100 is 3.3x the N=5 demo floor of 30.
   - Test 28's cost goes from 11 to 26 AC registrations, MC sessions and 52 MH sessions. Its wall clock will be recorded before and after (the two changes push suite time in opposite directions).
   - The larger budget of 375M (ceiling 150) was rejected: it needs P = 38 of 40, leaves no slack, and costs about 3x the test-28 wall clock.
   - Headroom beyond this would need a different design for test 28 (multi-meeting fill or direct `RegisterMeeting`). That is task-sized, so this plan does not attempt it.

### Server mute (R-9)

**Config** (`config.rs`)
- New REQUIRED read `MH_MAX_MUTED_SOURCES_PER_MEETING` into `PolicyLimits.max_muted_sources_per_meeting`. It uses the same statement-level raw binding + `parse_bounded` + `bound_to_usize` as the other §8 bounds, with `PRE_ALLOCATION_CONSEQUENCE`.
- Hard ceiling `MAX_MUTED_SOURCES_PER_MEETING_CEILING = MAX_EGRESS_STREAMS_PER_MEETING_CEILING`, derived rather than written as a literal. It must be at least the egress ceiling, because the ConfigMap invariant is `muted >= egress`: any egress value MH accepts must leave room for a legal muted value.
- Zero is refused by `parse_bounded`.
- No new ConfigMap key and no new `configMapKeyRef`; both already landed with task 8 (@operations F-H). The code read is the rollback-safe direction. Startup validation fails loudly, naming the key and its bound, through the fielded `ConfigError` from `parse_bounded`. The required read builds `ConfigError::MissingEnvVar` with the literal key inline, so `dt-guard env-config` can see it.
- `for_tests()` goes from five keys to six, and the `mh-test-utils::admission::fixture_policy_limits` twin gets the same field.
- The startup event logs `max_muted_sources_per_meeting`. Env-test 01's `LOGGED_POLICY_BOUNDS` gains that row, completing the "running pod reports what the ConfigMap says" invariant for the new key.

**Parse** (`routing::MeetingPolicy::from_request`)
- The muted-count bound joins the existing "counts, before any per-element work" block, BEFORE the duplicate-detection `HashSet::with_capacity`.
- Then each element goes through `SenderId::from_wire`, which rejects 0 and out-of-range values through the existing `IdError` arms.
- Then duplicates are rejected.
- New `PolicyRejection::{TooManyMutedSources, DuplicateMutedSource}`, with static reasons and no request data. `ALL` goes from 11 to 13.
- A muted sender that is not a candidate on this handler, or not yet connected, is accepted and held (the proto rule). There is no cross-check against edges.
- `MeetingPolicy.server_muted: Vec<SenderId>` is kept in request order.

**Snapshot**
- `MeetingRoutes` gets a private `server_muted: HashSet<SenderId>` (security S-2).
- New `RoutingSnapshot::is_server_muted(&MeetingKey, SenderId) -> bool` takes the meeting and the sender together. It is borrowing and non-allocating, and it returns false when the meeting is absent.
- The equal-generation divergence detector in `session::handle_config_apply` also compares the muted set, and still logs counts only. A mute change re-sent at an unchanged generation is exactly the MC contract violation that `internal.proto` field 7 calls out.

**Enforcement** (top of `forward_one`, before the sampler and decode)
- Check `snapshot.is_server_muted(parts.meeting, parts.sender)` against the spawn-bound sender, never anything read from the frame (S-5).
- On a match: `dropped(ServerMuted).increment(1)` through the cached handle, then return `ForwardOutcome::rejected(ServerMuted)`.
- No hop is consumed, nothing is forwarded, and there is no `per_frame_trace`, log, span or macro (S-6).
- `ServerMuted.direction()` is Ingress, so `forwarded{ingress} + dropped{ingress reasons} = received` holds. It is placed in `ALL`, so the series is present at zero.
- Site comments will state:
  - enforcement is sender-scoped and exact for audio-only;
  - per-stream ingress mute is FORECLOSED for video: the only stream indicator readable at ingress is the relay region, which the muted publisher writes, so keying on it is self-selectable evasion. Per-kind mute moves to MC's egress edge set (S-7);
  - no policy means no mute set, but `no_policy` forwards nothing either, so the absent-policy case is fail-closed by construction;
  - the security qualifier (S-8), in two halves. A patched client changes nothing observable: the sender is bound at spawn and there is no client input to the decision. The scope-only authorization half points at the `docs/TODO.md` §Media Path Obligations entry, which carries the on-path clause. That half also records the triage asymmetry: a foreign-`mc_id` mute leaves the takeover WARN, while an own-credential mute leaves only the counter.
- The existing "re-read per frame, cache nothing" comment in `ingress.rs` is kept intact.

**Proto**
- Comment-only amendment to `MutedSource` / field 7. It is a GSA edit classified Domain-judgment, so **@protocol implements it** (owner-implements) and @security reviews.
- Content: replace "per-kind mute keys on TRANSPORT" with the foreclosure reasoning (the only ingress stream indicator is the relay region, which the muted publisher writes, so keying on it is self-selectable evasion) and state that per-kind mute moves to the egress edge set.
- Also add a pointer to the enforcement site.
- No wire change and no `proto-gen` regeneration beyond doc comments.

**Catalog**
- `server_muted` entry in `mh-service.md` §Media Forward Path: ingress direction, the saturation-or-input group (not should-read-zero), no alert (a nonzero rate is healthy moderation), and present at zero because MH Scenario 19 reads a zero.
- No runbook edit; Scenario 19 belongs to task 18 (@operations F-G).
- The mh-media dashboard's `sum by(reason)` ingress panel picks the series up with no edit. It is NOT added to the invariant-violations stat panel. `label-taxonomy.md` is NOT edited (one home).

### Tests

**Unit, `routing` tests**
- Parse into set; empty set.
- Count bound fires before duplicate detection (over-limit AND duplicated fires `TooManyMutedSources`).
- Duplicate rejects whole; `sender_id` 0 and out-of-range reject.
- Muted-but-not-a-candidate is accepted.
- `ALL`/reasons are distinct.
- **Two-meeting pin**: sender 5 muted in A is not muted in B.

**Unit, `config` tests**
- Missing key, zero, and over-ceiling each fail.
- The ceiling is at least the egress ceiling (const assert).

**Integration, new `tests/media_server_mute_integration.rs`, through the real session actor apply path**
- **Drop while a route exists** (MC-paired A.3): the muted sender is still a live candidate with a connected subscriber. Result: `server_muted` rises, the subscriber queue stays empty, and no hop is consumed.
- **Mute is source-only** (A.4): the muted sender's own inbound edge (it is the subscriber) keeps delivering.
- **Re-assert** (A.6): the same generation re-sent is a no-op and the mute stays live; a new generation still carrying it survives; a new generation without it resumes forwarding.
- **Two-meeting** forward-path both-arms.
- **S10c**, using MC's real shape (B.1–B.6):
  - A connected {H1}, B {H1,H2}, C {H2}; H1 = {A->B, B->A}, H2 = {B->C, C->B}.
  - `egress_stream_id = (subscriber << 8) | ordinal`; one candidate at stream 0, priority group 1, datagram; slot ids distinct from the sender-id space.
  - Split-pair variant: B->A on H1 and A->B on H2.
  - Each handler's snapshot holds exactly its own edges. C's frame into H1 and A's frame into H2 forward to nobody (`no_subscriber`). B's frame on H1 reaches A only.
  - With B muted in both sets, each handler drops B independently.
- **Series present at zero** before any mute: rendered `server_muted` = 0, distinct from absent.

**Env-tests** (`26_mh_quic.rs`, all in `#[serial(mh_notifications)]` because they open MH sessions and do MC joins)
- **S4** (`test_server_muted_sender_is_dropped_at_mh_ingress_and_survives_reassert`)
  - Setup: a real GC/MC meeting with A and B, both on both handlers.
  - Mute injection: MC sends no mute until task 12, so the test registers directly on BOTH MH pods through the Layer-7 gRPC forwards with MC's AC credential (the test-29 mechanism). It uses the meeting's real `mc_id` and MC gRPC endpoint from GC's join response, so there is no ownership takeover. The policy is edges A->B and B->A plus `server_muted = [A]`, at a generation far above MC's, so an MC push arriving concurrently is refused as stale and cannot undo the mute.
  - Evidence:
    - the synchronous `applied_generation` echo;
    - `server_muted` rising past its own baseline on EACH pinned pod (`poll_until_pinned_instance`), with A sending on both of its sessions;
    - per-receiver: B never receives an A-marked frame carrying a post-mute sequence number, while A keeps receiving B's frames (the mute is source-only, and this is the positive liveness control).
  - Re-assert: the same generation is re-sent (mute still enforced; the counter keeps rising), then a new generation still carrying the mute (enforced).
  - Unmute: a new generation without A, after which B receives A's post-unmute frames within one generation.
  - Cleanup: a `Drop` guard runs `EndMeeting` on both pods before the sessions close, so the high generation never wedges the id.
  - Per @test and the README evidence rule, **the "forward counter stays flat" wording is replaced by per-receiver non-delivery**. A pod-wide shared counter staying flat cannot prove anything under concurrent suites.
- **S5** (`test_two_concurrent_senders_into_one_receiver_are_never_misattributed`)
  - Setup: a real MC meeting R, A, C, all connected only to one handler, so every edge sits on one transport. R declares two slots (non-ordinal, one > 255). No test-injected policy; MC programs it.
  - A and C send concurrently. EVERY frame R receives must carry the marker/slot pair that R's own `StreamAssignments` names (A's marker only on A's slot, C's only on C's), with relay-only rewrite and the payload/signature byte-identical.
- **Edge churn** (`test_edge_churn_at_a_new_generation_does_not_stall_unrelated_egress`)
  - Uses the same direct-registration mechanism: A->B streams continuously while generation N+1 adds C->B and generation N+2 removes it.
  - After each synchronous apply echo, B receives an A frame with a post-swap sequence number.
  - Everything is send-until-received under a rig deadline. No latency assertion and no wall-clock gate.
- **DRY hoists**:
  - `audio_datagram` / `bind_until_received` / `assert_relayed_on_slot` move from test 27 into `fixtures/media.rs` (27 becomes an import-only change).
  - Test 29's AC-credential, `handlers()`, `connect`, `authed` and `ReleaseOnDrop` move into `fixtures/mh_grpc.rs`.

### Env-test speed

- `infra/kubernetes/observability/prometheus.yml`: `global.scrape_interval` 15s -> 5s. This file serves only Kind (the one overlay is a labels passthrough), so it is edited in place. A `behavior: replace` overlay would need a second full copy of the file, which is an SSoT break.
  - Add an explicit `scrape_timeout: 10s` on the `otel-collector` job. Otherwise its effective timeout drops to the new global 5s (@operations F-C).
  - `evaluation_interval` stays at 15s; alert `for:` clauses are unaffected.
- **Settle (Lead R3; @observability O4; @dry-reviewer P1/P3; @operations F-D)**
  - ONE in-tree constant in `fixtures/metrics.rs`: `PROMETHEUS_SCRAPE_INTERVAL` (5 s, with an `ANCHOR (DRY):` to `prometheus.yml` `global.scrape_interval`) and `SCRAPE_SETTLE = PROMETHEUS_SCRAPE_INTERVAL + SCRAPE_SETTLE_MARGIN` (1 s).
  - It is a copy of a YAML value, so a FAIL-LOUD runtime precondition binds it. `assert_settle_exceeds_live_scrape_interval(&prom)` reads `/api/v1/status/config` once per process and takes the max effective `scrape_interval` over the ac/gc/mc/mh service jobs (a per-job override wins over global).
  - Unless `SCRAPE_SETTLE > live`, it panics with its own distinct message: "settle no longer exceeds one scrape interval; this wait would prove nothing — apply the Kind observability overlay or raise the constant, never shorten it".
  - Every stability wait that takes `SCRAPE_SETTLE` calls it first. A cluster still scraping at 15 s therefore fails LOUDLY; it never passes vacuously.
  - `poll_until_stable` keeps `settle` as a REQUIRED parameter with no default. Its `DEFAULT_SETTLE` paragraph is amended to distinguish a reasoned shared value passed explicitly from a default nobody reasoned about.
  - All FOUR former 16 s sites use the constant: 26's two waits and 34's `COUNTER_SETTLE`. Each keeps its own why-it-waits paragraph, pointing at the constant.
  - Budgets are NOT scaled by the interval ratio (@observability O4). Each site's budget is written as `rounds * SCRAPE_SETTLE + load allowance`, with the decomposition stated at the site and no shared default budget. `eventual.rs` `MetricsScrape` keeps 30 s (it is a ceiling) and gets a corrected derivation.
  - Every "15 s" cluster claim is rewritten to cite the key rather than a number:
    - 26 (all sites), 28:63, 34:59, `metrics.rs`, `eventual.rs`;
    - the four `mh-media.json` panel descriptions and the one in `mh-overview.json`, plus `dashboards.md:238` and `mc-service.md:593` (already wrong);
    - the e2e TS comments.
  - The stale `prometheus-config.yaml` citations are corrected.
  - Synthetic parser fixtures (`metric_hygiene.rs:430`, `alert_rules_loaded.rs:262`, `dt-guard/src/alert_rules.rs:1634`) are left alone.
- **Prometheus config** (@operations F-C, @observability O2)
  - The `prometheus.yml` comment states that `evaluation_interval` stays at 15 s on purpose, and that MC/GC now inherit 5 s. That is finer than the compose file's 10 s and is not a regression to "restore".
  - Explicit `scrape_timeout: 10s` on the `otel-collector` job, with the derivation rule stated.
- **Docs** (@operations F-E, @observability O2)
  - `dashboard-conventions.md`: the §Media-path cadences cells are rewritten, the "deployed cadence is 15 s, and that is a gap" subsection and its inversion blockquote are replaced, and the stale `prometheus-config.yaml` pointer is fixed.
  - Compose file `infra/docker/prometheus/prometheus.yml`: a reciprocal `ANCHOR (DRY):` comment pair states the deliberate divergence.
  - `docs/TODO.md` D1 is narrowed. The deployed-cadence instance is closed. The mechanism (two configs, no sync guard, owner infrastructure) remains as a TODO because it is task-sized: a new guard with its own design.
- **Alert and catalog** (@observability O5, O6)
  - The `MHIngressDatagramsNeverRead` description gains one sentence: a sustained server mute enters the ingress denominator and dilutes the ratio, which is the fail-quiet direction.
  - The catalog adds a THIRD MH-local group, *policy tokens*, for `server_muted`. It gives the healthy reading in both directions and says absent is not zero.
  - The catalog carries the security qualifier, including the on-path clause.
  - The catalog states MC's blind spot: there is no server-mute-specific MC signal, and `slot_state=source_muted` covers both mutes, so `server_muted` is the only fleet evidence.
- **JWT serial-group investigation. Result: LEAVE forged and oversized serialized.** The evidence is from the code path, not an observed zero:
  - The oversized JWT is rejected in `read_framed_message` (Step 3) and the forged one at `validate_meeting_token` (Step 4). Both return `Err` from `handle_connection` before Step 5.
  - The only `notify_participant_*` call sites are `connection.rs:690`, `:933` and `:1114`, all at Step 5 or later. So neither JWT test produces an MH->MC notification.
  - However, both tests run a real GC->MC join (`join_with_registered_mh`). That makes MC push `RegisterMeeting`, which increments `mc_media_policy_pushes_total{outcome="match"}`, and `test_mc_programs_live_handler_with_confirmed_forwarding_policy` in the same group attributes that same counter's rise to its own join. De-serializing the two tests would make that attribution unsound (a false pass). They stay in the group, and the serial comment is corrected to name both reasons.
- **Wall-clock** for the `mh_notifications` group, before and after, will be recorded here if a Kind cluster is reachable from this container. Otherwise the Lead's Gate 2 run is the "after" measurement.

### Kind handler capacity

- `configmap-egress-budget-patch.yaml`: 250_000_000 (ceiling 100). The formula comment names the two-sided window (at least the N=5 demo requirement of 30; `2*ceiling < MAX_PARTICIPANTS * slot_cap`, the test-28 feasibility bound) without restating a ceiling literal.
- The comment records that this is dev headroom, not a capacity model. The cause is disconnected participants holding edges through MC's 30 s reconnect grace window while Layer-7 suites run in parallel. A lower Kind grace period was considered and rejected: it would make Kind diverge from the grace behaviour the reconnect tests exercise (@operations F-F).
- Env-test 01 gains an UPPER bound (F-B, mandatory). It asserts `2 * ceiling < MAX_S9_PARTICIPANTS * slot_cap` from the live ConfigMaps, with a message naming both bounds and the knob to turn: ceiling <= the test-28 feasibility bound, derived from ONE home (`fixtures::egress_admission::{MAX_S9_PARTICIPANTS, size_s9_meeting}`, moved out of 28). This fails in seconds with a message naming both bounds.
- The bump keeps the ceiling far below `MH_MAX_TOTAL_EGRESS_EDGES` (65536), so there is no refuse-boot risk.
- 28's stale literals are fixed: "Kind (ceiling 40, S 8): P = 11" becomes "ceiling 100, S 8: P = 26 (208 > 200)", and the "scraped every 15 s" literal is replaced by a citation of the key. The run also records that the pod genuinely serves about 100 concurrent egress streams on a 1Gi/1000m limit during this test.

### Gate-1 fold-ins (after the first reviewer round)

**@observability F-OBS-1: per-job cadence, not global.**
- Add `scrape_interval: 5s` to each of the ac/gc/mc/mh-service jobs. `global.scrape_interval` stays at 15s. The `otel-collector` job stays untouched (it keeps its own 10s, and no `scrape_timeout` patch is needed).
- This closes D1's *instance* on D1's own terms (its Fix is per-job overrides).
- It deviates from the prompt's literal "lower the global to 5s", with the same effect on every job the tests read. The kubelet, node-exporter, kube-state-metrics and prometheus jobs keep 15s and their timeouts.
- For the four service jobs, the effective timeout becomes the interval (5s). The config comment states this as a decision, per @security's note.
- The config comment says MC is 5s on purpose, not D1's original 10s, because the `mh_notifications` group gates on MC counters.
- The live precondition's per-job branch is therefore the path that actually runs.

**@observability F-OBS-2: the 16s sites, enumerated.**
- Literals: `26_mh_quic.rs:289` and `:930`, and `34_mc_kek_rotation.rs:60`.
- Derived prose in `fixtures/metrics.rs`:
  - `:502-503` (value plus the stale `prometheus-config.yaml` citation);
  - `:554` (redo the "~106s" ceiling arithmetic);
  - `:483` (the lookback-delta residual grows from about 19x to about 50x the settle; the paragraph says shortening the settle moved it, and that this is still structural).
- The constant states the margin's basis: 1s covers absolute scrape duration plus ingestion. It does not shrink with the interval and must not be scaled.
- All descriptive prose cites the key, never a numeral (`mc-service/src/observability/metrics.rs:444` precedent). Only the constant's definition states the number.

**@security G1-1 and @operations O-1: the high-generation wedge.**
- Every S4 and churn meeting id is a fresh GC meeting, so ids are unique per run.
- The test first issues an idempotent `EndMeeting` at SETUP (an unknown id is an acknowledged no-op), then again via `ReleaseOnDrop`.
- Generation: read MC's installed generation G from a stale probe once MC's view has converged, then use G + a small margin (+16), never "far above".
- The module doc states the residual. If cleanup is skipped (SIGKILL, harness kill), the id is wedged stale on both pods and holds its edges and its registered-meeting slot until the pod restarts. Recovery is `EndMeeting` on both pods through the Layer-7 gRPC forward with the MC credential, or `kubectl rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower`.

**@security G1-2: `fixtures/mh_grpc.rs` is test-only.**
- It is a test-target helper only: no service-crate dependency, no bin or CLI.
- A comment names it as the deliberate exercise of the ownership gap, pointing at `session/mod.rs` `Ownership` and `docs/TODO.md` §Media Path Obligations.
- The bearer token never reaches an assertion message, a panic payload or a log.

**@security G1-3: boot-time relation check.**
- New fielded `ConfigError::MutedSourceBoundBelowEgressBound { muted, egress }`, following the `EgressStreamCeilingExceedsEdgeBound` precedent. MH refuses to start when `MH_MAX_MUTED_SOURCES_PER_MEETING < MH_MAX_EGRESS_STREAMS_PER_MEETING`.
- The const assert and env-test 01 are kept.

**@security G1-4: fail-open arm documented.**
- `is_server_muted` documents that `false` means "not muted OR no installed policy", which is safe only because the sole caller's fan-out also fails closed (`no_policy`), and what any future caller must pair it with.

**@operations O-2: the settle panic names the literal command.**
- The settle panic message includes `kubectl apply -k infra/kubernetes/overlays/kind/observability/`.

**@code-reviewer A: where the check sits.**
- The mute check is the first statement of `forward_one`, before the sampler, decode and fan-out. `ingress.rs` only calls `forward_one` after the per-frame re-read.
- A drop consumes no hop, and `server_muted` is ingress-direction, so the accounting identity holds.

**@protocol: `MutedSource` edit done.**
- The foreclosure amendment in the `MutedSource` doc is done, and that block is its single home. The field-7 doc is not restated.

**@paired-meeting-controller: MC's floor-adoption gate is closed after the first confirm.**
- MC does NOT tolerate a higher echo after its first confirmed push. It records `generation_mismatch` and raises the divergence gauge, a paging-class footprint (`mc-service/src/grpc/mh_client.rs:84-85`).
- Option 1, a meeting MC never programs, is impossible: MH's sender binding requires MC to know the participant, and MC programs every meeting it admits.
- So the injection is constrained as follows:
  - a DEDICATED fresh meeting per test;
  - injected only after every participant's view AND directive have converged;
  - the whole assertion window is free of structural changes (no joins, leaves or declarations; sessions stay open);
  - generation G + 16, never near `u64::MAX`;
  - the test's own re-asserts are what "survives a re-assert" means in S4: the same generation re-sent, then G+17 still muted, then G+18 unmuted;
  - cleanup: `EndMeeting` on both pods BEFORE any session closes, so MH forgets the injected generation. MC re-programs a pod afterwards ONLY if that pod's snapshot changes (the carrying pod does, as sessions close); a pod with an unchanged (empty) MC snapshot stays unregistered for the rest of this dedicated, disposable meeting. It is also issued via `ReleaseOnDrop` (setup-time issue dropped — see Deviations #6).
- The cleanup is ASSERTED, not assumed. After the final `EndMeeting`, a `RegisterMeeting` at generation 1 with an empty policy must echo `applied_generation == 1` (MH forgot the injected generation). A final `EndMeeting` then releases that probe too.
- A comment at the injection site says this deliberately creates an MC-side `generation_mismatch` risk if MC pushes mid-window, cites `mh_client.rs:84-85`, and says the test's own footprint must not be triaged as an MC bug.

**@test D, ordering.** Covered by the MC fold-in above. MC issues no push during the window because nothing structural changes. The re-asserts in S4 are the test's own, not MC's. No wait is a wall-clock "hope MC is done": injection is gated on converged views and directives.

**@test C, sizing.** Lead ruling R2 (amended) fixes a ceiling of 100. The arithmetic: 100_000_000 -> 250_000_000 raises the ceiling from 40 to 100. Against the observed peak of 39 this is 2.5x headroom, and against about 12 edges per suite it covers about 8 concurrent suites' worth of held edges. 10x (ceiling 400) is infeasible for test 28 (P about 101 > `MAX_PARTICIPANTS`, and 808 streams > the 512 per-meeting bound) and risks an OOM on the 1Gi pod. @operations signed the sizing.

**@test B.** An in-process test in `media_server_mute_integration.rs` asserts that a muted frame increments `dropped{server_muted}` and leaves `forwarded{ingress}` unchanged, through `MetricAssertion`.

**@test E.** The precondition reads the DEPLOYED interval via `/api/v1/status/config`. The `eventual.rs` `MetricsScrape` value stays 30s, with its derivation text corrected.

**Lead addendum: Layer 7 must apply the observability overlay.**
- In `scripts/layer7.sh`, when the diff touches `infra/kubernetes/observability/` or `infra/kubernetes/overlays/kind/observability/`, run `kubectl apply -k infra/kubernetes/overlays/kind/observability/` after rebuild and before the observability-ready gate.
- Wait for the Prometheus rollout with a bound (`kubectl rollout status deployment/prometheus -n dark-tower-observability --timeout=...`).
- A failure is a loud `precondition_fail`.
- Add a matching note in `docs/runbooks/devloop-validation.md` if §6.7 lists the Layer-7 steps.

### Baseline measurement (before)

Measured standalone against the devloop Kind cluster, 2026-09-26, `cargo test -p env-tests --features all --test 26_mh_quic`, `ENV_TEST_ORG_SUBDOMAIN=devtest`:
- 8 passed, 2 ignored.
- Test time 127.0s; wall clock 142s.
- The binary is almost entirely the `mh_notifications` serial group plus two independent tests.

The test-28 baseline will be recorded before its budget change lands.

**@dry-reviewer F2: the constant is scoped to its job class.**
- It is named `SERVICE_JOB_SCRAPE_SETTLE`. Its doc says it is valid ONLY for series from the ac/gc/mc/mh service jobs, and names the classes it is NOT valid for: `otel-collector` (its own per-job interval) and the jobs on the global interval (prometheus, kube-state-metrics, node-exporter, kubelet).
- The live precondition resolves the max effective interval over those four service jobs, never `global`.
- The parse of `/api/v1/status/config`'s `data.yaml` fails CLOSED. If a service job or its interval cannot be found or parsed, it panics with a reason token (`scrape-config-unreadable`) distinct from the settle-too-small token (`settle-not-above-scrape-interval`). It never defaults and never skips.

**Budgets (@dry-reviewer, @observability O4, @test).**
- The 90s/60s wall-clock budgets KEEP their values; only their derivation prose changes. With the smaller settle, the same 90s buys more convergence rounds.
- Each site states its decomposition: rounds x settle + a load allowance.
- This deviates from the prompt's "scale the 90s/60s budgets": three reviewers agree that shrinking a failure-only budget buys zero green-path wall clock and converts load variance into flakes.

**@dry-reviewer F1.** Test 28's worked example is restated relationally, with the Kind figures (ceiling 100, S 8, P 26: 208 > 200) given as an illustration derived at run time. The shared `fixtures/egress_admission.rs` keeps `MAX_S9_PARTICIPANTS` (an economic feasibility limit) textually distinct from the `p*S > 2*ceiling` correctness bound.

**@operations follow-ups.**
- The cadence comment names the observable failure mode of the clamped 5s timeout: the job goes `up == 0` and every Prometheus-gated env-test fails with a symptom that does not mention scraping.
- In §Periodicity, the MC/GC intended cells state the deliberate divergence (compose 10s vs deployed 5s), matching the ANCHOR comments.

**Test-28 baseline (before).** Ceiling 40, 11 participants x 8 slots; test time 16.1s, wall clock 20s.

**@observability O5 addendum: the catalog row names the task-12 boundary.**
- (1) Enforcement is live at MH as of this task; the drop path and the counter are real.
- (2) No production producer sets `server_muted_sources` until story 2 task 12 (`crates/mc-service/src/grpc/mh_client.rs`). In a deployed cluster the series is therefore zero by construction until then, and that zero proves nothing. After task 12 the same zero becomes informative.
- (3) As of this task MC has no server-mute signal. `mc_media_server_mute_requests_total{action,outcome}` is owned by task 12. The row points at @paired-meeting-controller's `docs/TODO.md` §Media Path Obligations entry for the residual (a request-event count is not a muted-state count) rather than restating it.
- No MC metric is added in this task.

### Remaining host-side actions (cannot be done from this container)

`kubectl apply -k infra/kubernetes/overlays/kind/observability/` rolls Prometheus through the hash. The MH overlay apply plus `rollout restart deployment/mh-0 deployment/mh-1` is performed by `setup.sh` on a ConfigMap change. Until both run, the live-derived settle keeps the env-tests correct.

---

## Pre-Work

None — dependencies (protocol contract, MH config surface, egress admission) already landed on the branch.


---

## Implementation Summary

### Server mute (R-9)
| Item | Before | After |
|------|--------|-------|
| `MH_MAX_MUTED_SOURCES_PER_MEETING` | in ConfigMap/Deployments, no code read | REQUIRED read into `PolicyLimits.max_muted_sources_per_meeting`; ceiling `MAX_MUTED_SOURCES_PER_MEETING_CEILING` (= egress ceiling, const-asserted); boot refusal `ConfigError::MutedSourceBoundBelowEgressBound` when below the egress bound; logged at startup (env-test 01 `LOGGED_POLICY_BOUNDS` row) |
| `MeetingPolicy::from_request` | ignored `server_muted_sources` | count bound in the counts-first block (before the dedup `HashSet`), `SenderId` width rejects, `DuplicateMutedSource`; `PolicyRejection::ALL` 11 -> 13 |
| Snapshot | — | private `HashSet<SenderId>` inside `MeetingRoutes`; `RoutingSnapshot::is_server_muted(&MeetingKey, SenderId)` (fail-open absent arm documented) |
| `forward_one` | — | first statement: spawn-bound sender muted -> `dropped(ServerMuted)`, `ForwardOutcome::rejected`, no hop, no trace; site comment carries sender-scope, foreclosure, who-can-set-it (pointer to TODO for the on-path clause), fail-closed no-policy |
| `MediaDropReason` | 13 | 14 (`ServerMuted`, ingress, MH-local — NOT in the vectors file) |
| Equal-generation divergence WARN | edges + transport | also the muted set (counts only) |

### Tests
- Unit: routing (parse, bound-before-dedup, bound-before-egress-walk, duplicate, width, held-not-rejected, two-meeting pin, absent-meeting arm, unmute, set equality); config (boot refusal, zero, ceiling, required key); metrics (`server_muted` present at zero, with anchor).
- Integration `tests/media_server_mute_integration.rs`: drop with live edges + source-only + `forwarded{ingress}` unchanged (MetricAssertion); malformed frame from a muted sender is `server_muted`; re-assert through the real session actor (same gen no swap, unmute at same gen is a no-op, new gen keeps it, next gen resumes with hop 0); S10c canonical, split-pair, muted-on-both; meeting-scoped mute on the forward path.
- Env-tests (`26_mh_quic.rs`, `mh_notifications` group): S4, S5, edge churn — all green on the rebuilt Kind MH.

### Env-test infrastructure
- `fixtures/metrics.rs`: `SERVICE_JOB_SCRAPE_SETTLE` (5 s + 1 s, class-scoped), `service_job_scrape_settle()` bound to the LIVE `/api/v1/status/config` (distinct tokens `scrape-config-unreadable` / `settle-not-above-scrape-interval`), pure parser with FIRE fixtures; all 16 s sites converted (26 x2, 34); budgets kept, derivations rewritten; prose cites keys.
- Hoists (second consumers): `fixtures/mh_grpc.rs` (from 29, test-only note), `fixtures/participant.rs` (from 27), `fixtures/media.rs` (frame + bind loops, plus `bind_until_received_each`), `fixtures/egress_admission.rs` (S9 sizing, one home for 01 + 28).
- Prometheus: per-job `scrape_interval: 5s` on ac/gc/mc/mh (global unchanged); compose file ANCHOR; D1 narrowed; §Periodicity rewritten.
- Layer 7: re-applies the Kind observability overlay (Phase 1e2, token `observability-apply-failed`) and a service's manifests (`dev-cluster deploy <svc>`, Phase 1c0) when the diff touches them — without the latter, `rebuild-all` never applies a ConfigMap change and env-test 01 fails on this very diff's budget patch.

### Kind capacity
- `MH_EGRESS_BUDGET_BPS` 100_000_000 -> 250_000_000 (ceiling 40 -> 100); env-test 01 two-sided (demo floor + S9 feasibility, `max_s9_feasible_ceiling` = 159 at slot cap 8); test 28 sizes to P = 26.

### Deviations from the task text (all recorded above with owner rulings)
1. `server_muted` NOT added to `proto/test-vectors/frame-v2.vectors.json` (Lead R1; @protocol owner ruling).
2. Budget 2.5x, not ~10x (Lead R2 amended; test-28 feasibility + 1Gi OOM risk).
3. Scrape cadence per-job on the four service jobs, not global (@observability F-OBS-1).
4. 90 s / 60 s budgets kept (failure-only ceilings), not scaled (@observability O4, @dry-reviewer, @test).
5. "Forward counter stays flat" replaced by per-receiver non-delivery (env-tests evidence rule).
6. **No SETUP-time `EndMeeting`** (@security G1-1 fold-in): every injected meeting is a FRESH GC meeting created by the test, and MC has already registered it by the time the injection opens — an `EndMeeting` at setup would tear down MC's own programming and close the test's sessions, while a leaked wedge from a PREVIOUS run lives under a different id that this run never touches. The residual (SIGKILL skips `ReleaseOnDrop`) is stated at `Injection`'s docs with the recovery; cleanup is asserted (post-release generation must drop below the injected one).

### Measurements (evidence, not gates) — standalone runs against the devloop Kind cluster
| Run | Before | After |
|-----|--------|-------|
| `26_mh_quic` binary (≈ the `mh_notifications` group) | 8 tests, 107–127 s test time (142 s wall standalone) | 11 tests (+S4, S5, churn), 67.6 s standalone / 79.4 s in-suite |
| `28_mh_egress_admission` | ceiling 40, P = 11: 16.1 s (20 s wall) | ceiling 100, P = 26: 24.2 s |
| `34_mc_kek_rotation` | 139.8 s | 89.8 s |
| `29_mh_meeting_teardown` | 78.4 s | 28.4 s |
| Full `--features all` suite | 427 s wall | 269 s wall (all green), vs Layer 7's 600 s envelope |

Cluster state for the "after" runs: observability overlay applied by hand (`kubectl apply -k infra/kubernetes/overlays/kind/observability/`), MH rebuilt (`dev-cluster rebuild mh`) and MH manifests applied (`dev-cluster deploy mh`) — exactly what the new Layer-7 steps do.

## Devloop Verification Steps

Self-check (implementer): `./scripts/layer-fast.sh` — TOTAL_RESULT=N/A (L1 OK, L2 OK, L3 OK, L4 N/A,
L5 OK, L6 N/A). Also `scripts/guards/run-guards.sh` exit 0; `scripts/layer7.test.sh` 277 passed / 0
failed; `cargo test -p mh-service -p mh-test-utils` all green (301 lib + integration binaries incl. the
7 new `media_server_mute_integration` tests); `cargo test -p env-tests --lib` 110 passed.

Layer 7 (by hand against the devloop Kind cluster, after applying the observability overlay and
`dev-cluster rebuild mh` + `deploy mh`): `cargo test -p env-tests --features all` — all binaries green,
269 s wall (see §Measurements). Gate 2 proper is the Lead's.

---

## Code Review Results

See §Gate 3 — Verdicts below for the per-reviewer table. Finding details were exchanged directly between reviewers and the implementer; every finding was fixed in-diff.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `9c664bb4e3b26c61bdd5f50883beec7b47a615dd`
2. Review all changes: `git diff 9c664bb4..HEAD`
3. Soft reset (preserves changes): `git reset --soft 9c664bb4`
4. Hard reset (clean revert): `git reset --hard 9c664bb4`
5. No schema changes.
6. Infrastructure: a tree revert changes NOTHING on a running cluster. Re-apply
   `kubectl apply -k infra/kubernetes/overlays/kind/observability/` and
   `dev-cluster deploy mh` (which restarts MH on a ConfigMap change) — or let Layer 7's
   Phase 1e2 / 1c0 do it, since a revert touches those paths.
7. **Safe-revert unit**: **Answer**: the whole commit is safe. Two partial-revert couplings,
   with their safe direction:
   - (a) **Prometheus per-job cadence + `SERVICE_JOB_SCRAPE_INTERVAL_SECS` are ONE unit.**
     Reverting `infra/kubernetes/observability/prometheus.yml` (back to 15 s) while the constant stays
     at 5 makes `service_job_scrape_settle` panic `settle-not-above-scrape-interval` on every
     Prometheus-gated env-test (fail-closed, loud). Revert them together, or the constant FIRST.
     The opposite order (a 16 s settle against a 5 s cadence) is merely slow.
   - (b) **The Kind egress-budget revert is SILENT.** 100M (ceiling 40) and 250M (ceiling 100) both
     sit inside env-test 01's two-sided window (30..=159), and 01 reads the LIVE ConfigMap, so a
     tree-only revert leaves the cluster on 250M with every test green and nothing pointing at the
     mismatch. After reverting, re-apply (`dev-cluster deploy mh`) and confirm
     `mh_media_egress_stream_ceiling` reads the reverted value.
   - The MH code (server mute) reverts cleanly on its own: MC sends an empty muted set until
     story 2 task 12, so no deployed behaviour depends on it; the ConfigMap key stays (it predates
     this task), which is the rollback-safe direction.

---

## Issues Encountered & Resolutions

### Issue 1: Task text contradicted recorded SSoT
**Problem**: The prompt asked for `server_muted` in the frame-v2 vectors `reject_reasons` and a ~10x Kind budget raise. The first breaks the one-home rule and a pinned collision test, and the second makes env-test 28 infeasible.
**Resolution**: Lead rulings R1 (Option A) and R2 (250M, ceiling 100), recorded as deviations.

### Issue 2: Layer 7 never applied manifest or observability-config changes
**Problem**: `rebuild-all` applies no ConfigMaps, so the budget and scrape changes would never reach the cluster.
**Resolution**: new Layer-7 phases 1c0 (service deploy on manifest change) and 1e2 (observability overlay re-apply).

---

## Lessons Learned

1. Arithmetic checks on a sizing ask (e.g. "10x") belong in planning. The task text had not been checked against the tests that consume the value.
2. A fail-loud precondition makes a stale environment loud. Closing the deploy gap makes it not happen. Both are needed.

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

## Gate 1 — Plan Confirmations

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
| Protocol (GSA owner) | confirmed |

Lead rulings: R1 (no vectors-file edit; Option A), R2 (Kind budget 250M → ceiling 100, not 10x — test-28 feasibility/cost), R3 (derived settle + fail-loud live precondition), R4 (Scenario 19 is task 18), Layer-7 observability re-apply addendum. Classification guard: STATUS=OK.

## Gate 2 — Validation (Lead, attempt 1)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → TOTAL_RESULT=N/A (no FAIL), 1651s. L1 OK 5s · L2 OK 4s · L3 OK 114s · L4 N/A (aggregate; proto intentional-gap placeholder) 251s · L5 OK 2s · L6 N/A (audit aggregate; proto placeholder) 3s · L7 OK 1272s.

Post-review re-validation (Lead, Gate 2 attempt 2 on the post-fix tree): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → TOTAL_RESULT=N/A (no FAIL), 1502s; L1–L3 OK, L4 N/A (proto placeholder), L5 OK, L6 N/A, L7 OK 1119s.

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | check-order claim; classification rows |
| Test | CLEAR | 0 | 0 | 0 | |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | 3 stale prometheus-config.yaml citations |
| Code Quality | CLEAR | 0 | 0 | 0 | |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | kind-context helper; inert const-assert comment; count restatement |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | 1e2 trigger covers all overlay bases; rollback filled |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |
| Paired meeting-controller | RESOLVED-FIXED | 2 | 2 | 0 | vacuous probe precondition; recovery claim |
| Protocol (GSA owner) | CLEAR | 0 | 0 | 0 | Ownership Lens: comment-only internal.proto, owner+security present |

No accepted deferrals. Task-12-triggered TODO filed by meeting-controller (§Media Path Obligations — server-mute state signal) is a scoping record, not a deferred finding.
