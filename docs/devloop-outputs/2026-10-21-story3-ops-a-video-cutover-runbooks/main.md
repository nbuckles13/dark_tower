# Devloop Output: Story 3 OPS-A — video deploy order, v3 cutover/rollback, video-off lever, send-directive wiring, premise sweep

**Date**: 2026-10-21
**Task**: Story 3 (2026-10-07-see-each-other) OPS-A — runbook-only: R-23 runbook rows, R-25 cutover order + three-layer rollback + skew table, R-26 video-off lever, R-31 send-directive wiring + premise sweep
**Specialist**: operations
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — tier=light (documentation only; manifest tier)
**Branch**: `feature/user-see-each-other`
**Duration**: ~85m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `72a2725c30ebdfd5f2a6ea52dd3aefaf1df68ec4` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `operations` |
| Tier | `light — documentation only (run-story manifest tier=light)` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — docs-only diff, no check surface` |
| Paired | `paired-infrastructure`, `paired-protocol` (task prompt requires infrastructure + protocol + observability review for R-25) |

---

## Task Overview

### Objective
See task prompt (run-story task-4.prompt, OPS-A). Documentation only — no code, manifest, or alert-rule edits.

### Scope
- **Service(s)**: MC, MH runbooks (docs only)
- **Schema**: No
- **Cross-cutting**: Yes (deploy order across config/MH/MC/SDK)

### Debate Decision
NOT NEEDED — R-25 confirmed by user 2026-10-08; story file Design §Wire freeze is the authority.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/mh-deployment.md` | Mine | — |
| `docs/runbooks/mc-deployment.md` | Mine | — |
| `docs/runbooks/mc-incident-response.md` | Mine | — |
| `docs/runbooks/mh-incident-response.md` | Mine (premise sweep: MH 17 scope, MH 19 enforcement and Arm 4) | — |
| `docs/runbooks/client-dev-local.md` | Mine (premise sweep: F18 correction marker only; C-A1 rewrites the entry) | — |
| `docs/runbooks/devloop-validation.md` | Mine (premise sweep: two frame-v2 vector rows scoped to "until P2") | — |
| `docs/devloop-outputs/2026-10-21-story3-ops-a-video-cutover-runbooks/main.md` | Mine | — |

---

## Gate 1 — SKIPPED (tier=light; reason: documentation only)

---

## Planning

Docs-only. The plan was written before any runbook edit. No non-doc file is needed.

### Tree state at start (72a2725c)

- **Landed:** P1 (proto contract, ADR bundle), OBS-A (`media_kind` split), INF-A (profile, keys, retirements, guard).
- **Not landed:** C-v3, P2 (v3 flip), MC-A, MC-B, MH-A1/A2/B, C-A1/C-A2, INF-C. Every runbook statement about their behaviour is therefore written as "once <CODE> lands" or "shipped by <CODE>". This keeps the text true on both sides of the cutover. That matters because OPS-A lands BEFORE the code (R-25).

### INF-A key-name reconciliation (diffed against the tree)

| Prompt name | Tree | Result |
|---|---|---|
| 9 `MEDIA_*` keys and their defaults | `infra/services/media-codec-profile/profile.env` | identical (all nine names and defaults) |
| `MC_MAX_AUDIO_RECEIVE_SLOTS`=8, `MC_MAX_VIDEO_RECEIVE_SLOTS`=4 | `infra/services/mc-service/config.env` L99, L117 | identical |
| `MH_KEYFRAME_REQUEST_RETRY_MS`=500, `_AGGREGATION_MS`=100, `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC`=20, `MH_STREAM_MID_FRAME_STALL_MS`=1000, `MH_EGRESS_STREAM_BACKLOG_BYTES`=262144, `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`=10, `MH_MAX_SOURCE_STREAMS_PER_MEETING`=1024 | `infra/services/mh-service/config.env` | identical |
| `MH_POD_MEMORY_LIMIT_BYTES` | downward-API `resourceFieldRef` in `mh-{0,1}-deployment.yaml` (NOT a config.env key); guard `crates/dt-guard/src/kustomize_resource_field_refs.rs` | name identical; its row cites the Deployment as the source, not a ConfigMap |
| RETIRED: `MC_MAX_RECEIVE_SLOTS`, `MC_AUDIO_{CODEC,MAX_BITRATE_BPS,FRAME_RATE_HZ}`, `MH_STREAM_COST_{AUDIO,VIDEO}_BPS`, `MH_MAX_MUTED_SOURCES_PER_MEETING`, `MH_MAX_STREAMS` | present with RETIRED banners; refs `optional: true` | identical (eight keys; deletion = TODO "Delete the story-3 retired config keys") |

**Differences from the prompt, and how they were reconciled:**

1. **Bounds.** The prompt gives numeric ranges for the MC and MH keys (1..64, 100..=10000, …). The tree's convention (config.env: "cited, never restated") and the prompt's own rule ("Bounds belong to the readers; cite their constants and do not restate numbers") disagree with restating them. The constants do not exist yet: they land with MC-A, MH-A1, MH-A2 and MH-B in `crates/{mc,mh}-service/src/config.rs` (MC's minimum keyframe interval goes in `media_signaling/directive.rs`). **Resolution:** each row cites the file and the landing task, never a number. The design-intent ranges stay in the story file. Infrastructure (paired) concurred.
2. **The aggregation key has a second boot rule** the prompt omits: it must be ≤ `MH_KEYFRAME_REQUEST_RETRY_MS` (config.env). It is included in that row.
3. **The Kind test-profile patch has NOT landed.** The prompt says "Kind patches video to 320×180 / 15 fps / 250000". The tree's `infra/kubernetes/overlays/kind/services/media-codec-profile/kustomization.yaml` says "No patch today", and INF-C lands the patch. **Resolution:** the runbook attributes the patch to INF-C and does not state it as present fact.
4. **Kind rollback across the cutover.** The prompt says "prior content-tagged images survive `prune_superseded_images()`". The tree disagrees. `deploy.sh` REBUILDS every first-party image from the checked-out tree (`build_first_party_images`), and the prune keeps only the deployed ref and the previous rollout-history revision. So image retention is not what makes "check out the previous commit + deploy.sh" work: the rebuild is. ConfigMap generations are never pruned, which is what makes a config-layer `rollout undo` safe. **Resolution:** the runbook states the tree's mechanics. This will be reported to the team lead as a prompt/tree discrepancy (infrastructure paired message).
5. **`deploy.sh` cannot sequence config → MH → MC.** It applies the environment root in ONE `kubectl apply -k`. **Resolution:** the order is stated as the order for a by-hand or non-Kind rollout. On Kind the runbook says plainly that the cutover is one converge, describes the transient window, and says how to avoid it (deploy with no live meetings). It does not invent a flag.

### Token reconciliation (tree vs story file)

- **old SDK → new MC:** the contract is `ParticipantCapabilities.supported_header_versions` (signaling.proto). The story file (MC-A section) fixes the token as `mc_session_join_failures_total{error_type="header_version_unsupported"}`. It is not in the tree yet, so the runbook says "shipped by MC-A".
- **old MH ← v3 frames:** `mh_media_frames_dropped_total{direction="ingress",reason="unknown_version"}`. The token is live (`crates/media-protocol/src/codec.rs`).
- **old MC directs v2 → v3 SDK:** the story file says only "header version refused … counted on an EXPORTED client series" (C-A1). **No series name is fixed in either the story or the tree**, so none is invented. The runbook names C-A1 as the shipper and `docs/observability/metrics/client.md` as where the name will live. This will be reported to the team lead.
- **Withdrawals:** `mc_media_send_directive_withdrawals_total{media_kind, reason}`. `reason="unwatched"` is shipped by MC-A and `reason="server_muted"` by MC-B. The `mc_media_send_directives_total` outcome vocabulary is unchanged. Sources: the story's MC-A/MC-B sections and the `record_send_directive` doc comment in `crates/mc-service/src/observability/metrics.rs`.
- **MH-side A8 and drop reasons** `rejected_transport_inconsistent`, `transport_mismatch` and `unmatched_stream`: shipped by MH-A1 (not in the tree). Story-2 MH today rejects heterogeneous transports as `rejected_invalid` (`routing/mod.rs` `HeterogeneousTransportModes`).

### Edits planned

1. **`mh-deployment.md`:**
   - A new §"Story 3 cutover (see each other): order, skew, rollback". It covers the order (config → MH → MC → SDK bundle) and why MH goes first, the skew table, and the three-layer rollback.
   - The "STOP — MC FIRST" banner becomes a dated correction scoped to pre-story-3 builds. It points to the cutover section and says it never contradicts it.
   - §Cross-service ordering is scoped the same way.
   - §"Operator levers" is rewritten with the video-off lever. The old anchor id is kept via an explicit `<a id>`.
   - §"Rollback for the media path" is rewritten as all-together across the cutover. The "one version-skew detector" paragraph is re-scoped.
   - New subsection §"Mixed-Transport Rollback Hazard".
   - New and retired rows in §Required environment keys.
2. **`mc-deployment.md`:**
   - §0 SDK-first is re-scoped with a dated correction, and the cutover summary is cross-linked to the MH section. That section is the single home, so the MC text is a short restatement plus a link and the skew table is not duplicated.
   - Configuration Reference rows for every new and retired key, and the startup-line checklist.
   - The ConfigMap block, and a video-off lever note.
   - The §Coordination bullets that say "forward MC first" and "SDK first" are scoped.
3. **`mc-incident-response.md`:**
   - The Scenario 15 `transport_mode_mismatch` row is widened to per-stream arms and cross-linked both ways with the hazard anchor.
   - The directive query section is rewritten so that withdrawals read as healthy, with a normal/fault partition.
   - Premise corrections on `source_muted`.
4. **Premise-sweep corrections in place:** `mh-incident-response.md` MH 19 (sender-scoped) and Arm 4 (muted-sources bound), and `client-dev-local.md` F18 (fails loudly → truncation). The full per-kind scenario extensions are OPS-B's scope per the story (MH 19, MC 21, MC 8, F21+), so only the falsified premise is corrected here, with a dated marker.
5. **No edits to** dashboards, alert rules or `docs/observability/**`. Those sites are listed under "Premise sites handed to OBS-B" and sent to @observability.

**Anchor stability:** no `runbook_url` targets either deployment runbook. In mc-incident-response.md, `#scenario-15-media-generation-divergence` is kept unchanged. The renamed MH heading keeps its old id via an explicit anchor.

---

## Pre-Work

None

---

## Implementation Summary

Docs-only. Six runbooks changed, and no code, manifest, alert rule, dashboard or `docs/observability/**` file was touched.

| Acceptance | Where |
|---|---|
| (1) Forward order, three-layer rollback and skew table, with no contradicting forced-order passage | `mh-deployment.md` §"Story 3 cutover (see each other): order, skew detection, rollback" is the single home (order table with gates; why MH goes before MC; Kind = one converge; skew table; three layers). `mc-deployment.md` §"Story 3 cutover (see each other): the MC view" restates the MC-relevant parts and links to it, without duplicating the table. Reconciled passages: MH "STOP — MC FIRST" banner (dated correction, scoped to pre-story-3); MH §Cross-service ordering (scope note; precedence reversed for the story-3 cutover); MH §Rollback ordering "image may roll back alone" (scoped); MH rollback-criteria command and `EndMeeting` note (scoped); MC §Coordination (a new first bullet supersedes the story-2 ordering bullets for the cutover); MC §0 SDK-first (dated correction, does not apply to story 3); MC §How to Rollback and "Rollback (MC half)" (scoped). |
| (2) `#mixed-transport-rollback-hazard`, cross-linked with MC Scenario 15 | `mh-deployment.md` §"Mixed-Transport Rollback Hazard" under §Rollout With Media Flowing: the MC side (paging, per-stream arms, a fork table on the log-line arm), the MH side (A8 `rejected_transport_inconsistent`, `transport_mismatch` vs `unmatched_stream` with their remedies), the story-2 MH whole-registration reject, and consistency with MC Scenario 12's trap. The MC Scenario 15 `transport_mode_mismatch` row was rewritten (widened meaning, fork, link to the anchor), and the hazard links back to Scenario 15. |
| (3) Video-off lever with MC-restart cost, placed before rollback | `mh-deployment.md` §"Operator levers for media: video off, and nothing else" (dated correction; old anchor id kept by an explicit `<a id>`; the two existing bullets kept; server mute extended per kind as reachable by any holder of the service credential; profile-change cost; R-18 host-unmute statement; verification cited to MC-A's integration test and the manual test plan). `mc-deployment.md` §"Video-off lever" carries only the MC procedure (edit on the deployed commit with nothing else changed, confirmation, undo), the R-18 sentence and a pointer. The `mh-deployment.md` section is the single home for what the lever does, its cost, blast radius, ordering and verification (Gate-3, DRY F3). |
| (4) Config Reference row per new and retired key, with names matching the tree | MC: per-kind caps; nine profile keys (in three rows); retired `MC_MAX_RECEIVE_SLOTS` and `MC_AUDIO_*`; ConfigMap block updated; the `media-codec-profile` paragraph (Kind patch attributed to INF-C; no `dev-web.sh` override); startup-line checklist gains a from-MC-A item. MH: `MH_MAX_SOURCE_STREAMS_PER_MEETING`, six stream/keyframe keys (aggregation ≤ retry boot rule included), `MH_POD_MEMORY_LIMIT_BYTES` (downward API, guard cited), five profile keys MH reads; retired `MH_STREAM_COST_*` and `MH_MAX_MUTED_SOURCES_PER_MEETING` moved to the retired table beside `MH_MAX_STREAMS`. Bounds are cited by file and landing task, never restated (see Planning, reconciliation 1). |
| (5) Directive query reads withdrawals as healthy | `mc-incident-response.md` §Client media signalling, question 2: a "WITHDRAWAL IS HEALTHY" banner with the reader-harm statement; every `mc_media_send_directives_total` token partitioned normal/fault; a new `mc_media_send_directive_withdrawals_total{media_kind, reason}` table (`unwatched` from MC-A, `server_muted` from MC-B, both normal); the curl grep includes the new series; no rule written (OBS-B). |
| (6) Premise sweep with every checked site | §Premise Sweep below. |

Security acceptance: the lever's blast radius and R-18 ("a host unmute can resume a participant's camera or mic") are in both deployment runbooks. Server mute is "reachable by any holder of the service credential the registration contract accepts … never impossible", per kind (`mh-deployment.md` levers, MH 19 correction). No step mixes v2 and v3: layer 3 says the window is "an outage, not a mode", and the skew rows describe refusals, not modes. No runbook text claims end-to-end, zero-trust or forward secrecy.

**Disagreements with the prompt, reported to @team-lead** (tree wins; story file not contradicted):
1. The Kind test-profile patch is not landed; it is attributed to INF-C.
2. "Prior images survive prune" is not what makes a cross-cutover Kind rollback work. `deploy.sh` rebuilds from the checked-out tree, and the prune keeps only the deployed revision and the previous one.
3. Bounds are cited, not restated.
4. The client header-version-refused series has no name in either the story or the tree, so it is attributed to C-A1 without a name.

---

## Premise Sweep

Method: case-insensitive grep over `docs/runbooks/*.md`, `docs/observability/**`, `infra/grafana/dashboards/*.json` and `infra/docker/prometheus/rules/*.yaml`, for each premise below. The patterns covered audio-only/datagram-only, sender-scoped mute, fails loudly / `slot_count_over_cap` / over-cap, operator levers, `transport_mode` echo / `EgressStream.transport_mode` / heterogeneous, keyframe interval/cadence, `MH_STREAM_COST_VIDEO`, `SOURCE_MUTED` / `ZERO_REQUESTED`, v2 header / `frame-v2` / 10-byte, `server_muted_sources`, and `MAX_MUTED_SOURCES`. Hits were then read in context. Status: **corrected** (runbook, in place, dated marker), **handed off** (OBS-B file), or **checked — not falsified / already correct**.

### Runbook sites

| Site | Premise | Status |
|---|---|---|
| `mh-deployment.md` §Deployment Procedure "STOP — MC FIRST" banner | forced order MC→MH | corrected (scoped to pre-story-3; cutover banner above it) |
| `mh-deployment.md` §Cross-service ordering + three-service precedence sentence | MC first / MH first back | corrected (scope note; precedence reversed for the cutover) |
| `mh-deployment.md` §Rollback ordering "image may roll back alone" | single-component rollback | corrected (except across the cutover) |
| `mh-deployment.md` §Rollback criteria undo command; §Rollback `EndMeeting` note | MH-only rollback | corrected (scoped) |
| `mh-deployment.md` §Operator levers (title + first sentence) | no operator levers for media | corrected (dated; old anchor kept) |
| `mh-deployment.md` §Scaling MH to zero "no finer-grained lever" | no levers | corrected |
| `mh-deployment.md` §Rollback for the media path (table "Redeploy only", "Partial rollback: Not available") and "one version-skew detector" | MH-only undo; single detector | corrected (two-column table; detector re-scoped to MH + client, agreeing with the skew table) |
| `mh-deployment.md` §Required environment keys `MH_STREAM_COST_*`, `MH_MAX_MUTED_SOURCES_PER_MEETING` rows | cost keys live/operative | corrected (moved to retired table, "until MH-A1") |
| `mh-deployment.md` §Rollout "loses audio" | audio-only | corrected ("and from story 3 video too") |
| `mc-deployment.md` §Coordination ordering bullets (SDK-first, `sender_id` MC-first, three-service order) | story-2 orders | corrected (superseding first bullet) |
| `mc-deployment.md` §0 Roll the browser SDK first | SDK first | corrected (dated scope) |
| `mc-deployment.md` §How to Rollback; "Rollback (MC half)" | MC-only rollback | corrected (scoped) |
| `mc-deployment.md` Configuration Reference `MC_MAX_RECEIVE_SLOTS` ("Over-cap rejects the whole declaration"), `MC_AUDIO_*`; ConfigMap block; startup-line checklist; frame-rate WARN note; "seventeen Required rows" | over-cap fails loudly; MC_AUDIO_* live; restated count | corrected |
| `mc-incident-response.md` Scenario 15 `transport_mode_mismatch` row | single `transport_mode` echo | corrected (per-stream `applied_source_streams`, three arms) |
| `mc-incident-response.md` §Client media signalling Q1 `slot_count_over_cap` | over-cap refused | corrected ("until MC-A and C-A1", truncation to `unavailable`) |
| `mc-incident-response.md` §Client media signalling Q2 | withdrawal read as failure | corrected (R-31 rewrite) |
| `mc-incident-response.md` §Client media signalling Q3 `source_muted` / `zero_requested` | `SOURCE_MUTED` semantics | corrected (flat enum, wire names; `zero_requested` sentence there was already correct) |
| `mc-incident-response.md` Scenario 16 "First fork", Slot-cap rejection bullet | over-cap rejects whole, "hears nobody" | corrected (Gate-3, test F1) |
| `mc-incident-response.md` Scenario 18 arm (b) Over-subscription | over-cap rejected whole; `MC_MAX_RECEIVE_SLOTS`; `VITE_DT_RECEIVE_SLOTS` | corrected (Gate-3, test F2) |
| `mc-deployment.md` §Coordination story-2 task-13 bullet ("whole-declaration rejection stays the enforcer") | over-cap rejects whole | corrected: pre-story-3 history note (Gate-3, test F3) |
| `mc-incident-response.md` Scenario 21 "audio only, today" (Step 3) | audio-only | checked — already correct and self-dated ("becomes the sum across kinds when MC-A …") |
| `mc-incident-response.md` Scenario 21 "Not a mute problem" `source_muted` | `SOURCE_MUTED` | corrected |
| `mc-incident-response.md` Scenario 12 "roll MH FORWARD" trap | — | checked — consistent; hazard section and Scenario 15 now cite it |
| `mh-incident-response.md` Scenario 19 "How it is enforced" | sender-scoped, exact for audio-only | corrected (per declared stream; hop-local unauthenticated uplink id; R-16 in slot terms; no signed stream number; per-kind arms are OPS-B's) |
| `mh-incident-response.md` Scenario 19 Arm 4 `MH_MAX_MUTED_SOURCES_PER_MEETING` | retired bound | corrected |
| `mh-incident-response.md` Scenario 17 "Audio is carried one frame per QUIC datagram" | datagram-only | corrected (scope note) |
| `mh-incident-response.md` refs to "§Operator levers" (Scenario 18 ×2, Scenario 19) | — | checked — heading prefix kept, still resolves by name |
| `client-dev-local.md` F18 | over-cap fails loudly / "media error names the receive-slot cap" | corrected (dated marker; C-A1 rewrites the entry with the env rename) |
| `client-dev-local.md` F14 discriminator table (`source_muted`; row 4 "any token MC does not emit today" → MC defect) | `SOURCE_MUTED`; new avatar/unavailable states read as defects | corrected: dated note above the table, until MC-A/C-A1; C-A1 rewrites the table (Gate-3, protocol F3) |
| `devloop-validation.md` L524, L826 `frame-v2` vectors | v2 header | corrected ("until P2") |
| `devloop-validation.md` L85-113 | v2 incident narrative | checked — historical record, left |
| `devloop-validation.md` L887 `MH_STREAM_COST_VIDEO_BPS` | — | checked — an example of a guard finding, not a premise |
| all runbooks: keyframe interval / cadence / "one-second keyframe"; 10-byte prefix; `EgressStream.transport_mode` | — | checked — no hits |
| `gc-*`, `ac-*`, `TEMPLATE.md` | — | checked — no hits |

### Premise sites handed to OBS-B

**Directive rule `runbook_url` (for OBS-B):** `docs/runbooks/mc-incident-response.md#send-directive-wiring`. This is an explicit `<a id>` directly above the rewritten send-directive block, so a later rename of the parent heading "Client media signalling — where to look (no scenario number yet)" does not break it. `dt-guard alert-rules-policy` does not check fragments, so this id must be kept stable (Gate-3, observability F1).

| File:line | Premise | What is stale |
|---|---|---|
| `infra/docker/prometheus/rules/mc-alerts.yaml:177` (`MCMediaGenerationDivergence` description) | single transport echo | "transport_mode_mismatch = the separate two-ends-must-agree echo failing". From MC-A it is a per-stream `applied_source_streams` disagreement with three arms; the description should name the arm fork and link `docs/runbooks/mh-deployment.md#mixed-transport-rollback-hazard`. |
| `infra/docker/prometheus/rules/mc-alerts.yaml:131-139` (comment) | single transport echo | same widening |
| `docs/observability/alerts.md:665` (inventory mirror of `MCMediaGenerationDivergence`) | single transport echo | "the separate two-ends-must-agree echo"; needs the same per-stream widening (three arms) and the `#mixed-transport-rollback-hazard` link as `mc-alerts.yaml:177` *(added at observability's request)* |
| `infra/docker/prometheus/rules/mh-alerts.yaml:555, 661` (comments) | "§Operator levers" | still resolves (heading prefix kept; old anchor id kept). Informational only. |
| `docs/observability/metrics/mc-service.md:570` (`mc_media_policy_pushes_total` outcome) | single echo | MC-A also owns the catalog update here (its prompt: "Update the catalog definition"); listed for OBS-B's check |
| `docs/observability/metrics/mc-service.md:666, 676, 943` | over-cap rejects | `slot_count_over_cap` as the over-cap signal; from MC-A it is truncation to `unavailable` |
| `docs/observability/metrics/mc-service.md:768` (`applied` mute outcome) | `SOURCE_MUTED` | "re-emitted `SOURCE_MUTED` to the source's holders"; per-kind `AVATAR_*_MUTED` from MC-A *(added at observability's request)* |
| `docs/observability/metrics/mc-service.md:712, 718, 719` | `SOURCE_MUTED` | `source_muted` reachable / "client OR server mute"; not emitted from MC-A, removed by MC-B |
| `docs/observability/metrics/mc-service.md:937` | `MC_MAX_RECEIVE_SLOTS` source | the cap gauge's source becomes the per-kind keys from MC-A |
| `docs/observability/metrics/mh-service.md:280` (`rejected_invalid`) | heterogeneous transport reject | "unspecified or heterogeneous transport mode": from MH-A1 a mixed meeting is legal, and inconsistency is `rejected_transport_inconsistent` |
| `docs/observability/metrics/mh-service.md:476-478` | retired muted bound | `mh_media_muted_sources_per_meeting_limit` / `MH_MAX_MUTED_SOURCES_PER_MEETING` replaced by the source-stream bound (MH-A1) |
| `docs/observability/metrics/mh-service.md:490-492, 497-499` | `MH_STREAM_COST_{AUDIO,VIDEO}_BPS` as enforced cost | both cost gauges (audio row 490-492 added at observability's request); derived from the profile from MH-A1 |
| `docs/observability/metrics/mh-service.md:1097, 1112-1125, 1138-1140` | sender-scoped mute | `server_muted` "spawn-bound sender in the server-muted set"; the 1138 paragraph is already self-dated ("per-kind from MH-A1") — check the 1097/1112 definitions follow it |
| `docs/observability/metrics/mh-service.md:1039`, `docs/observability/metrics/client.md:630, 659, 755`, `docs/observability/label-taxonomy.md:87, 263, 385`, `docs/observability/alerts.md:717`, `infra/docker/prometheus/rules/mc-alerts.yaml:602` (comment) | v2 vectors | `frame-v2.vectors.json` references; P2 replaces the file |
| `docs/observability/metrics/client.md:1211, 1219-1236` (the whole `dt_client_media_receive_slots_rejected_total` entry) | over-cap rejected whole; `MC_MAX_RECEIVE_SLOTS` | "rejects such a declaration WHOLE" (1222-1223) and the retired key (1222, 1232); C-A1 retires the entry |
| `infra/grafana/dashboards/mh-media.json:1478, 1568` | `MH_STREAM_COST_*` enforced cost | panel description/expr on retired cost gauges |
| `infra/grafana/dashboards/mh-media.json:1582, 1683` | retired muted bound | panel on `mh_media_muted_sources_per_meeting_limit` |
| `infra/grafana/dashboards/mc-media.json:461, 543` | over-cap rejects | `slot_count_over_cap` in the client-defect regex/description |
| `infra/grafana/dashboards/mc-media.json:953` ("Receive Slot Fill vs Cap" description) | `SOURCE_MUTED`; retired key; over-cap rejection | "active or source_muted", "publishing MC_MAX_RECEIVE_SLOTS", "shows as slot_count_over_cap" *(added at observability's request)* |
| `infra/grafana/dashboards/mc-media.json:769, 836, 1053` | `SOURCE_MUTED` | `source_muted` override and in the active ratio expr |
| `infra/grafana/dashboards/mc-media.json:1065, 2053` | `MC_MAX_RECEIVE_SLOTS` | legend/description name the retired key |
| `infra/grafana/dashboards/client-media.json:1222, 1301` | over-cap fails loudly | `dt_client_media_receive_slots_rejected_total` panel (C-A1 retires the series; the panel goes with it) |
| `infra/docker/prometheus/rules/mh-alerts.yaml:493` (datagram-drop `impact:` annotation), `docs/observability/alerts.md:1066` | datagram-only? | checked — not falsified: both say "Audio is carried one frame per QUIC datagram", which is audio-scoped and still true in story 3; video rides streams and its drops are separate MH-A2/MH-B reasons. Listed so the absence is shown (Gate-3, test F5) |
| `docs/observability/dashboards.md:217`, `docs/observability/alerts.md:1135`, `docs/observability/metrics/mc-service.md:338-340, 810-820` | — | checked — not falsified (self-dated "until MC-A", or still accurate) |

---

## Code Review Results

Gate 3: all reviewers RESOLVED-FIXED; 23 findings, 23 fixed, 0 deferred.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | R-18 host-unmute statement made unconditional (with client-muted exception) in both runbooks; all three security acceptance criteria met |
| Test | RESOLVED-FIXED | 5 | 5 | 0 | Video-off verification cites story-planned MC-A cap-0 test + T-B manual step (8), marked not-yet-in-tree; premise-sweep gaps closed (MC 16, MC 18 arm b, mc-deployment history bullet, two OBS mirrors listed) |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | Stable `#send-directive-wiring` anchor for OBS-B; `action="mute"` on server-mute cross-read; slot_state label tokens. 73 runbook_urls verified by independent slug sweep |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | Shipped MH has no transitional path (MC-B in release) → Old MC→new MH skew row, gate (b) = presence; both `rejected_transport_inconsistent` arms; slot-state names; `rejected_invalid` is an RPC error (Scenario 12), not `generation_mismatch` |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | Cutover canonical home = mh-deployment §Story 3 cutover; video-off lever single home; config values cited not restated; Scenario 15 remedies live only in hazard table |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | Transitional-path correction (with code-reviewer/protocol); Scaling-MH-to-zero rollback scoped |
| Paired Infrastructure | RESOLVED-FIXED | 3 | 3 | 0 | Kind undo → ErrImageNeverPull for pruned revisions; MEDIA_* defaults cite profile.env; lever edit on currently-deployed commit |
| Paired Protocol | RESOLVED-FIXED | 3 | 3 | 0 | New MC→story-2 MH signal is `mc_register_meeting_total{status="error"}`; slot-state wire/label names; client-dev-local F14 correction |
| Semantic Guard | — | — | — | — | not spawned (docs-only diff, no check surface) |

Lead note: DRY's reading of acceptance (1) — MC runbook carries the order + rollback summary and links to the single skew table in mh-deployment — accepted as the SSoT-correct reading.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Devloop Verification Steps

Gate 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, run-all mode, final tree): `TOTAL_RESULT=OK`.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 4 |
| 2 Format | OK | 4 |
| 3 Guards | OK | 133 |
| 4 Test | OK | 145 |
| 5 Lint | OK | 1 |
| 6 Audit | OK | 1 |
| 7 Env-tests (Rust env-tests + browser E2E) | OK | 815 |

Note: `WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=134 BUDGET=90` (Layer-3 always-run subset over its 90s p95 budget; pre-existing, not diff-caused — docs-only diff).

---

## Rollback Procedure

1. Start commit: `72a2725c30ebdfd5f2a6ea52dd3aefaf1df68ec4`
2. `git diff 72a2725c..HEAD`
3. **Safe-revert unit**: the whole commit (docs-only; reverting re-states story-2 deploy order, which is wrong once story-3 code lands — revert only together with the story).

---

## Issues Encountered & Resolutions

None yet
