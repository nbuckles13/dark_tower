# Devloop Output: Kind video test profile, egress re-size, dev-web.sh video demo (story 3 task 16, INF-C)

**Date**: 2026-10-24
**Task**: Story 3 task 16 (INF-C) — Kind test codec profile patch, Kind MH egress budget re-size against derived ceiling gauge, env-tests (window + profile-equality), dev-web.sh video demo docs. Full prompt: `~/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-16.prompt`
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h (setup 2026-10-24; Gate 2 TOTAL_DURATION=1379s)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e55be8765dd0d142251e38b680a72b37449a1aad` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `not spawned (no check surface: no credential/secret types, Debug impls, or client credential lifetime in scope)` |
| Paired media-handler | `spawned` |
| Paired client | `spawned` |

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

{If debate was needed, link to debate record: `docs/debates/2026-10-24-{topic}.md`}

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
| `infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml` (new) | Mine | — |
| `infra/kubernetes/overlays/kind/services/media-codec-profile/kustomization.yaml` | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/kustomization.yaml` (comment only) | Mine | — |
| `scripts/dev-web.sh` | Mine | — |
| `scripts/dev-web.test.sh` | Mine | — |
| `crates/env-tests/src/fixtures/egress_admission.rs` | Not mine, Domain-judgment | test (paired) |
| `crates/env-tests/src/fixtures/deployment_config.rs` (tree ConfigMap-data reader hoisted from 01) | Not mine, Domain-judgment | test (paired) |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Domain-judgment | test (paired) |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Not mine, Domain-judgment | test (paired) |
| `crates/env-tests/src/fixtures/metrics.rs` (live-pod pinned gauge read, extracted from 01) | Not mine, Domain-judgment | test (paired) |
| `crates/env-tests/src/fixtures/kube.rs` (MH/MC/profile-reader instance lists) | Not mine, Domain-judgment | test (paired) |
| `crates/env-tests/tests/02_mc_deployment_config.rs` (instance-list literal → `kube` constant) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (instance-list literals → `kube` constant) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` (instance-list literal → `kube` constant) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/38_mh_video_forwarding.rs` (backlog-cap read pinned to live pods, agreement required) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/39_mh_video_keyframes.rs` (backlog-cap read pinned to live pods, agreement required) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/*.rs` (Edge-sum header line in every file; `0` where no media edges) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (header wording) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/video/mod.rs` (doc wording) | Not mine, Minor-judgment | test |
| `crates/env-tests/README.md` | Not mine, Minor-judgment | test |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `docs/runbooks/mc-deployment.md` (l.1181 stale "until INF-C" sentence) | Not mine, Minor-judgment | operations |
| `crates/mh-service/src/config.rs` (test only: Kind-budget unit test reads the committed patches instead of a stale literal) | Not mine, Minor-judgment | media-handler (paired) |
| `docs/runbooks/client-dev-local.md` (cross-reference only) | Not mine, Minor-judgment | operations |
| `Cargo.lock` (ctutils 0.4.2 → 0.4.4, RUSTSEC-2026-0385; ambient advisory found by layer-fast, audit remediation is infrastructure-owned) | Mine | — |
| `docs/devloop-outputs/2026-10-24-kind-video-test-profile/main.md` | Mine | — |

---

## Planning

_Lead note: INDEX.md and review-protocol.md were injected by path reference (teammates read them first) rather than inlined, due to size (~128KB of INDEX content)._

### Problem restated in mechanism terms

"A Kind-only value that overrides a base value through a strategic-merge patch, and a capacity figure derived from it" — the class is **every Kind overlay ConfigMap patch** (OTel/CORS patches in AC/GC/MC, the MH egress budget, and now the codec profile). All share the dead-key hazard (a renamed base key leaves the patch key dead and the base value wins). This task closes it for the two patches whose values carry correctness weight (profile: new equality test; budget: existing "overlay did not apply" branch kept). The OTel/CORS patches stay covered only by `docs/TODO.md` §Infrastructure Validation in Devloops (env-config overlay-coverage entry) — task-sized (a static overlay-coverage rule in `dt-guard env-config`), not widened here. Surfaced to reviewers.

### Sizing — from the DEPLOYED derived costs (MH startup log, both pods, 2026-10-23 boot, production profile)

`stream_cost_audio_bps` 112000, `stream_cost_video_bps` 826400 at 800 kbps / 30 fps ⇒ video framing = 880 bits/frame, audio = 1280 bits/frame (matches MH's `*_FRAME_OVERHEAD_BYTES`, 110 / 160). At the Kind test profile (250 kbps, 15 fps): video 263200 bps = 32900 B/s; audio unchanged 14000 B/s; video still binds. These figures are planning-only and are NOT written into any manifest; they are re-confirmed from the gauge and log after the deploy.

Window (per handler, both MHs present):
- lower = max(P×(N+M) demo = 6×(5+4) = 54, planned per-file max 72) + retained floor 39 = **111**
- upper = `max_s9_feasible_ceiling(MC_MAX_AUDIO_RECEIVE_SLOTS = 8)` = (40×8 − 1)/2 = **159** (S9 declares audio slots only)
- width 48 ≥ retained floor 39 ⇒ not empty, not narrower than the floor: no escalation.
- Target mid-window, ceiling **135**: `MH_EGRESS_BUDGET_BPS = 35600000` ⇒ floor(4450000 / 32900) = 135. S9 then needs P = 34 (34×8 = 272 > 270) ≤ 40. Memory boot check comfortably clear (MH refuses loudly anyway).
- **Production-quality switch consequence (operations #1):** at this budget the production profile gives floor(4450000/103300) = 43 < 54. One budget cannot hold both (prod-profile demo needs a ceiling ≥ 54 ⇒ ≥ ~44.6 Mbps ⇒ test-profile ceiling ≥ 169 > 159). Documented honestly, no mechanism: after a switch, compare `mh_media_egress_stream_ceiling` with the meeting's P×(N+M) (dev-web's default 4-participant 3/3 demo = 24 fits; the 6/5/4 size does not); a larger prod-quality demo also edits the budget patch, and BOTH files are restored before Layer 7. Symptom named: `mh_media_stream_admission_total{outcome="rejected_stream_ceiling"}`.

### 1. Kind test profile patch
- New `overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml`: strategic merge on `media-codec-profile`, data = exactly `MEDIA_VIDEO_WIDTH=320`, `MEDIA_VIDEO_HEIGHT=180`, `MEDIA_VIDEO_FRAME_RATE_HZ=15`, `MEDIA_VIDEO_MAX_BITRATE_BPS=250000`. Kustomization gains `patches: - path: …`; header comment there updated (no more "No patch today").
- Header: why (R-28: cohorts of headless tabs encoding/decoding VP8 on a devloop host/CI runner; capacity is evidence by T-B); the ONE place the profile change flows from to MC's directive and MH's derived cost; content-addressed ⇒ rolls mc-0/mc-1/mh-0/mh-1 and the MH roll sheds every media session (cite `mh-deployment.md` §Rollout With Media Flowing, no third prose copy); dev-web.sh demo inherits it; Layer 7's VP8 probe (T-A) reads this deployed profile; values within MC bounds cited by constant name in `directive.rs` (`VIDEO_FRAME_RATE_{MIN,MAX}_HZ`, `VIDEO_BITRATE_{MIN,MAX}_BPS`, geometry bounds), never restated; "patch values only, never add a key" + dead-key explanation + GUARD COVERAGE as in the budget patch; the env-test that enforces it; the AUTHORITATIVE production-quality procedure (remove the `patches:` entry from the sibling kustomization.yaml, or edit values here; deploy with `infra/kind/scripts/deploy.sh` / `dev-cluster deploy`; restore with `git checkout -- infra/kubernetes/overlays/kind/services/media-codec-profile/` + deploy BEFORE Layer 7) plus the budget consequence above and "BUDGET AND PROFILE ROLL BACK TOGETHER". dev-web.sh and the env-test messages point at this file rather than restating the procedure (DRY #8).

### 2. Egress budget re-size
`configmap-egress-budget-patch.yaml`: `MH_EGRESS_BUDGET_BPS: "35600000"`; header rewritten: two-sided window as max(demo, per-file max) + retained floor vs S9's `2 × ceiling < MAX_S9_PARTICIPANTS × audio cap`, citing the fixture constants by name (`egress_admission.rs`) and the enforcing test by path::fn; why under-sizing is dangerous; story-2 retained-edge history kept (grace not shortened, env-tests not serialised); OOM/memory note re-worded for the new position; "BUDGET AND PROFILE ROLL BACK TOGETHER" replaces the stale "INF-C re-sizes it again" paragraph (image-rollback note kept); GUARD COVERAGE kept; ENFORCED paragraph now says the test reads the gauge. No ceiling/cost literal.

### 3. Env-tests (paired with test)
Partial-invariant enumeration of retired-key reads (DRY ask):
- `01` window test — still re-derives the ceiling from profile costs ⇒ **converts** to the gauge.
- `01` `RETIRING_KEYS` rows `MH_STREAM_COST_{AUDIO,VIDEO}_BPS` — INF-A's ref-existence ⇒ **stay**.
- `02:134` `MC_MAX_RECEIVE_SLOTS` — INF-A ref-existence ⇒ **stays**.
- `28` + `fixtures/egress_admission.rs` — already gauge + `MC_MAX_AUDIO_RECEIVE_SLOTS` (MH-A1/MC-A) ⇒ only stale-series pinning and stale figures change.
- No other env-test reads either key (grep of `crates/env-tests`).

a. `fixtures/egress_admission.rs` becomes the ONE home of the lower side: `DEMO_P/N/M` + `required_edges()` MOVED from 01 (ANCHOR to MH's `demo_streams` kept; 01's recommended-min parity check imports them), `MAX_PER_FILE_EDGE_SUM = 72` (planned per-file budget; 27's plan), `RETAINED_EDGE_FLOOR = 39` (story 2 task 20 Gate 2 observation; cause `MC_DISCONNECT_GRACE_PERIOD_SECONDS`), pure `kind_window_lower_bound()` = max(demo, per-file) + floor. Comment: the sums are per handler — conservative, since a file's meetings may spread over both MHs. `kind_sizing_matches_the_recorded_figures` reworded as pure arithmetic (no deployment claim).
b. One pinned live-gauge reader `live_mh_gauge(prom, metric) -> InstanceCounters` in the fixture: per MH instance, live pod IP from kube, `poll_until_pinned_instance` (existing bounded scrape poll, presence predicate); returns exactly one entry per live pod; a live pod with no series panics (absent ≠ zero); stale terminated-pod series are never returned (observability #2). Used by 01's window test and for ALL FOUR gauge reads in 28 (ceiling, edges, registered, limit — same stale-series hazard; completing the invariant in the file).
c. Hermetic Edge-sum check (Layer 4 unit test in the fixture): scans `tests/*.rs` (`CARGO_MANIFEST_DIR`); strict parse of `//! Edge-sum (concurrent Σ P×(N+M)): <n>` — an unparseable or duplicate line fails; structural media detector = the file references `McSession`, `MhSession`, `fixtures::media`, `fixtures::video`, `VideoPublisher` or `VideoSubscriber` ⇒ must carry exactly one line (today this finds `24_join_flow.rs` missing one: it joins with zero declared slots, so I add `Edge-sum …: 0`); non-vacuity: ≥1 media file and ≥1 parsed header, distinct message; every non-exempt sum ≤ `MAX_PER_FILE_EDGE_SUM`; ONE named exemption `28_mh_egress_admission.rs` (S9 exceeds the ceiling by design), asserted still present with its header. 28's header `Edge-sum` updated to the value at the new figures (P=34, S=8 ⇒ 272) and the "ceiling is 100, P=26" illustration reworded to not restate a Kind figure.
d. 01 window test `test_kind_egress_stream_ceiling_meets_demo_requirement` (name kept, cited): keeps the Kind-label precondition, patch≠base, `OVERLAY-NOT-APPLIED` (deployed == base) and deployed==patch branches BEFORE the gauge read; then per live MH pod via `live_mh_gauge`: `WINDOW-LOWER` (ceiling ≥ `kind_window_lower_bound()`, message names instance, value, both bounds and their sources, fix = raise budget) and `WINDOW-UPPER` (ceiling ≤ `max_s9_feasible_ceiling(audio cap)`, fix = lower budget). The local bits/bytes re-derivation is deleted; oracle-vs-gauge parity stays only in `test_running_mh_publishes_the_deployed_stream_ceiling`.
e. NEW 01 `test_kind_media_codec_profile_is_the_committed_test_profile`: the patch is read from the fixed repo-relative path via the tree reader hoisted from 01's `tree_budget()` into `fixtures/deployment_config.rs` (`tree_configmap_data(rel)`: `.env` KEY=VALUE, duplicate key fails; YAML `data` map, every value a string; one reader, `tree_budget` becomes a call). Live generation = `configmap_name_for_all(["mc-0","mc-1","mh-0","mh-1"], PROFILE_CONFIGMAP)` (fails if MC and MH resolve different generations — "rolls together"). Kind-label precondition. Assertions: patch `data` non-empty; for each patch key live == patch (separate `OVERLAY-NOT-APPLIED` when live == base); live key set == base `profile.env` key set (dead added key). Every message names the patch file and says `git checkout -- infra/kubernetes/overlays/kind/services/media-codec-profile/ && dev-cluster deploy` (host: `infra/kind/scripts/deploy.sh`).
f. Hermetic (Layer 4) twin in the fixture: every patch key ∈ base `profile.env` keys, every patch value ≠ base value, no `MEDIA_AUDIO_*` key, non-empty. No key list or value literal in Rust: everything iterates the patch.

### 4. dev-web.sh (paired with client)
- Header: new "VIDEO DEMO" block — N+1 distinct accounts, one per browsing context, separate profiles framed as per-context session isolation (no credential sharing); Kind runs a lower TEST profile ("lower resolution and frame rate than production", values only in the patch, named by path — no numbers, DRY #7/client #6); production quality = pointer to the patch header's procedure (one-line summary: remove/edit patch + deploy; restore with git checkout + deploy before Layer 7; rolls MC+MH and drops live meetings; check the ceiling gauge vs P×(N+M)); `DT_TEST_TONE=1 DT_TEST_PATTERN=1 scripts/dev-web.sh` as the one-machine multi-participant line (build-time `__DT_TEST_PATTERN__`, absent from production bundles, per participant and changing every frame, replaces getUserMedia video in-app — no permission prompt, no fake-device flag; ONE setting per dev server, not per tab); real camera = one participant per physical camera (one webcam generally cannot be opened by several browser profiles at once, notably on WSL2/Windows) — **NOT "several machines"**: the tree's own WHY NOT A SECOND MACHINE block makes a LAN participant impossible (client #1; flagged to Lead as a prompt-vs-tree conflict resolved toward the tree); R-7 loopback self-check described only as "a client debug option, off by default, local only" (C-C2 has not landed; no control named); `MC_MAX_VIDEO_RECEIVE_SLOTS=0` = video disabled cluster-wide, cross-referencing the existing preflight message.
- Banner (behaviour, tested): `test pattern: ON requested … / OFF …` state line mirroring the tone line; "Video for N-way: the test-pattern build (DT_TEST_PATTERN=1 -> __DT_TEST_PATTERN__)" line beside the audio one; a `codec profile:` informational line naming the Kind test-profile patch path and printing the live-read command (printed, never executed — no kubectl run, no ConfigMap values interpolated; least mechanism, consistent with the cap lines).
- `dev-web.test.sh`: pattern ON/OFF, `--help` names `__DT_TEST_PATTERN__`, banner names the patch path, header contains no restated profile figure, help sentinel still last line. `dt-guard no-insecure-browser-flags` stays green (no flag literal; class only).
- `client-dev-local.md`: one cross-reference to dev-web.sh's VIDEO DEMO block; nothing else (ops owns the ladder, OPS-B).

### 5. Other in-tree corrections (trivial, fix now)
`mc-deployment.md:1181` stale "until INF-C" sentence → now true state, pointing at the patch; `mh-service.md` §Egress Admission intro (still names the retired cost keys) and the ceiling / recommended_min Usage lines → name the window test and the profile-equality test (observability to review/draft wording); README edge-sum paragraph, `27` header and `fixtures/video/mod.rs:158` "about 120 / planned 72" → cite `MAX_PER_FILE_EDGE_SUM` and the hermetic check by name.

### Verification (scratch, uncommitted; recorded under Implementation)
1. Deploy committed tree; record MH startup costs + gauge on both pods; L7 targeted: 01 + 28 green.
2. Scratch budget below lower side (ceiling < 111) ⇒ `WINDOW-LOWER`; scratch above upper (ceiling > 159) ⇒ `WINDOW-UPPER`. Each deployed via `dev-cluster deploy`.
3. Scratch removal of the `patches:` entry ⇒ profile test fails naming the patch.
4. Hermetic edge-sum check: scratch-bump a header past 72; scratch-delete 24's header.
5. Revert all; redeploy committed tree; confirm `git status` clean of scratch. After a Layer-7 run, record peak `mh_media_egress_edges` per instance as evidence the retained floor still holds with video edges (media-handler caveat).

### Safe-revert unit
The test-profile patch and the budget re-size are ONE unit (either half reverted alone puts the ceiling outside the window: removing the test-profile patch while keeping the new budget → ~43 < 54; keeping the test profile with the old budget → ~313 > 159). The env-test changes may revert independently only together with that unit.

### Gate-1 amendments (DRY, code-reviewer)
- **Pinned gauge read, one home (DRY A):** `live_mh_gauge(prom, metric) -> InstanceCounters` goes in `fixtures/metrics.rs`, beside `gauge_by_instance_present` and `poll_until_pinned_instance`, reusing the existing `InstanceCounters` type. It is EXTRACTED from the inline live-pod → podIP → pinned-poll code in 01's `test_policy_bounds_reported_by_running_mh_match_configmap`, and that test is converted to call it. It asserts that both MH instances are present, under a distinct PRECONDITION message (test's condition). Every config-published MH gauge read in the two files I edit is converted: 01's window test, ceiling parity, recommended_min and derived stream costs; in 28, the ceiling, the rejection ratio, the threshold, and the edges/registered/limit diagnostics. Only the read location changes; the assertion shape on occupancy gauges stays the same. Reviewed and out of scope: 29, 34, 35, 38, 39, `fixtures/injection.rs` and `fixtures/participant.rs`. They read occupancy or backlog gauges, a different hazard (shared-pod movement, not post-roll staleness), and none of their values is config-published.
- **One patch-vs-base rule (DRY B):** `deployment_config.rs::kind_patch_overrides(patch_rel, base_rel) -> Vec<(key, patch_value, base_value)>` fails on a dead key (absent from base), an unchanged value or an empty patch. The hermetic test and the live test both call it, and the live test adds only the live comparison.
- **Edge-sum, fail-closed (code-reviewer 1, DRY C):** EVERY `tests/*.rs` must carry exactly one `//! Edge-sum (concurrent Σ P×(N+M)): <n>` line, with `0` for files that open no media edges. There is no identifier detector, so a new fixture cannot open a gap. The S9 file carries the exact exempt token `exempt — sized from the live ceiling by fixtures::egress_admission::size_s9_meeting` in place of `<n>`, so no number is restated. The parser accepts `<n>` or that token and nothing else. The check asserts exactly one file carries the token, so there is no filename literal anywhere, including the README. Every `<n>` must be ≤ `MAX_PER_FILE_EDGE_SUM`. Non-vacuity: at least one file is parsed.
- **Strict tree reader (code-reviewer 4):** the `.env` parse rejects `KEY=` with an empty value, a non-comment line without `=`, and duplicate keys. Both the patch side and the base side must be non-empty.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed (after Edge-sum fail-closed + exempt-token amendments) |
| DRY | confirmed (after amendments A/B/C) |
| Operations | confirmed |
| Paired media-handler | confirmed |
| Paired client | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. No GSA paths.
Lead ruling (prompt-vs-tree): item 5's "or several machines" is dropped in favour of dev-web.sh's WHY NOT A SECOND MACHINE (tree is authoritative; not a wire-freeze conflict). Profile figures not restated in dev-web.sh per the task's own one-home acceptance rule.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Deployed derived costs (confirmed after deploying the committed tree, both MH pods, startup log + gauge)
| Item | mh-0 | mh-1 |
|------|------|------|
| `media_video_frame_rate_hz` (live profile) | 15 | 15 |
| `stream_cost_video_bps` | 263200 | 263200 |
| `stream_cost_audio_bps` | 112000 | 112000 |
| `egress_budget_bps` | 35600000 | 35600000 |
| `egress_stream_ceiling` / `mh_media_egress_stream_ceiling` | 135 | 135 |

Window [111, 159]; 135 is mid-window; S9 sizes to P=34, S=8 (printed `S9: ceiling=135 slot_cap=8 -> 34 participants x 8 slots`). With the test-profile patch removed (scratch run 3), the same budget gives a ceiling of 43, which confirms the operations finding.

### Manifests
| Item | Before | After |
|------|--------|-------|
| Kind `media-codec-profile` | production profile (no patch) | `configmap-test-profile-patch.yaml`: 4 video values; header carries the ONE production-quality procedure |
| Kind `MH_EGRESS_BUDGET_BPS` | 82640000 (ceiling 100 at prod profile) | 35600000 (ceiling 135 at test profile) |
| Budget header | audio-only demo window; "INF-C re-sizes it again" | two-sided window naming the fixture constants; BUDGET AND PROFILE ROLL BACK TOGETHER; memory note re-worded |

### Env-tests
- `fixtures/egress_admission.rs` is the ONE home of the window: `DEMO_P/N/M` + `required_edges` (moved from 01), `MAX_PER_FILE_EDGE_SUM`, `RETAINED_EDGE_FLOOR`, `kind_window_lower_bound()`. It also holds the strict `Edge-sum` parser + `check_edge_sums`, and the hermetic test over every `tests/*.rs` (fail-closed: every file carries a line; one exempt token, in 28).
- `fixtures/metrics.rs`: `gauge_on_live_pod` / `gauge_by_live_pod` (extracted from 01's policy-bounds test) pin every config-published MH gauge read in 01 and 28 to the live pods.
- `fixtures/deployment_config.rs`: `LivePod`/`live_pod`; strict `tree_configmap_data` (hoisted from 01's `tree_budget`); the ONE patch-vs-base rule `kind_patch_overrides`, used for the budget patch AND the profile patch; hermetic twin `the_kind_test_profile_patch_overrides_base_keys_only`.
- 01: the window test reads the pinned gauge (`OVERLAY-NOT-APPLIED` / `WINDOW-LOWER` / `WINDOW-UPPER`); the local re-derivation is deleted. NEW `test_kind_media_codec_profile_is_the_committed_test_profile`.
- 28: pinned reads; header carries the exempt token, with no restated Kind figure.
- Every non-media file gains `Edge-sum …: 0` (24 explains why its MC joins install no edge).

### dev-web.sh
VIDEO DEMO `--help` block; banner `test pattern: ON/OFF` state line; `codec profile:` line naming the patch and printing (never running) the live read; "Video for N-way" launch line. `dev-web.test.sh` +11 assertions (234 pass), including a non-vacuous check that no patch value is restated in `--help`.

### Additional Changes
- **Out-of-plan change — `Cargo.lock`**: ctutils 0.4.2 → 0.4.4 (advisory RUSTSEC-2026-0385, published 2026-10-23). Reason: Layer 6 audit gate. This ambient advisory turned layer-fast Layer 6 red. Rust audit remediation is infrastructure-owned (`scripts/lang/rust/audit-remediation.md`), so I applied the preferred real fix rather than a suppression.
- Docs: `mc-deployment.md` stale "until INF-C" sentence; `mh-service.md` Egress Admission intro and ceiling/recommended_min usage; README edge-sum rule; `27` and `video/mod.rs` "about 120 / planned 72" → `MAX_PER_FILE_EDGE_SUM`; `client-dev-local.md` cross-reference only.

### Gate-3 fixes (review round 1)
- **operations F1 / DRY F1**: one restore command. `KIND_SIZING_RESTORE_CMD` / `KIND_SIZING_RESTORE` in `fixtures/deployment_config.rs` restore BOTH patches and replace the budget-only restore in the window messages and the profile-only one. A new clause says that if the profile test also fails, the profile is the cause and should be restored, not re-sized. The hermetic test `the_restore_command_matches_the_patch_header` fails if the patch header's command drifts from the constant.
- **operations F2**: `mh-service` `the_kind_budget_boots_and_clears_the_recommended_min` reads the budget and the test-profile values from the two committed patch files (`include_str!`), not from the stale `82640000`/ceiling-100 literals. It asserts the committed Kind config boots and clears the recommended minimum; the window stays the env-test's job.
- **DRY F2**: `kube::MH_INSTANCES`, `MC_INSTANCES` and `PROFILE_READERS` (with a unit test that readers = MC ++ MH) replace the instance literals in 01, 02, 27, 28 and 29.
- **code-reviewer F2**: `gauge_by_live_pod` now checks for distinct pod IPs (pure `distinct_pod_ips`, unit-tested); the old length check could not fail. **F3**: `PatchOverride` is a named-field struct. **F4 / client F1**: `cap_live_value_cmd` → `mc0_live_value_cmd`, with a comment covering both ConfigMaps. **F5**: header re-wrapped, casts hoisted, `LivePod` imported.
- **observability F1**: a `# After a roll` paragraph on `gauge_by_instance_present`.
- **media-handler**: the 38 and 39 backlog-cap reads use `gauge_by_live_pod` and require the live pods to agree, instead of taking the max.
- **client F2**: a filter-free `--help` check for WxH (x/×, with or without spaces) and the bitrate, with each value read from the patch by key. Scratch run: adding the line "The 320×180 test pattern camera." to the header made it fail with `[help-no-restated-profile-form-320×180] unexpected substring '320×180' WAS present`; reverted, 242/0.

### Scratch verification runs (uncommitted; reverted; committed tree redeployed afterwards)
1. **Budget below the window** (`MH_EGRESS_BUDGET_BPS: "28000000"`, deployed): window test FAILED, all others ok:
   `WINDOW-LOWER: MH instance mh-0 (192.168.61.34) publishes mh_media_egress_stream_ceiling=106, BELOW the Kind window [111, 159] per handler: lower = max(demo P x (N + M) = 54, MAX_PER_FILE_EDGE_SUM = 72) + RETAINED_EDGE_FLOOR = 39; upper = max_s9_feasible_ceiling(MC_MAX_AUDIO_RECEIVE_SLOTS = 8) with MAX_S9_PARTICIPANTS = 40 (all in crates/env-tests/src/fixtures/egress_admission.rs). Under-sizing makes admission reject partway through the largest scenario, … If the budget was edited (e.g. for a production-quality demo), restore the committed patch first: `git checkout -- infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml && dev-cluster deploy`; otherwise RAISE MH_EGRESS_BUDGET_BPS in …`
2. **Budget above the window** (`"45000000"`, deployed): window test FAILED, all others ok:
   `WINDOW-UPPER: MH instance mh-0 (192.168.61.10) publishes mh_media_egress_stream_ceiling=170, ABOVE the Kind window [111, 159] per handler: … the S9 exhaustion test (28_mh_egress_admission.rs) could not drive a registration past it. If the budget was edited …, restore the committed patch first: … otherwise LOWER MH_EGRESS_BUDGET_BPS in …`
3. **Test-profile patch removed from the overlay kustomization** (the `patches:` entry deleted, committed budget, deployed): profile test FAILED, naming the patch and the restore:
   `OVERLAY-NOT-APPLIED: the live profile generation media-codec-profile-2gf9t2ktm7 carries the PRODUCTION value of MEDIA_VIDEO_FRAME_RATE_HZ from infra/services/media-codec-profile/profile.env, not the Kind test profile (infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml). The patch is removed from the overlay or the cluster was deployed without it — a production-quality demo left in place? Layer 7 runs the TEST profile: restore it with `git checkout -- infra/kubernetes/overlays/kind/services/media-codec-profile/` and redeploy (`dev-cluster deploy` in the devloop container, `infra/kind/scripts/deploy.sh` on the host) — …`
   The window test also failed on the same run, `WINDOW-LOWER: … ceiling=43 …`. This is the production-profile ceiling at the committed budget, as operations predicted.
4. **Hermetic edge-sum check** (scratch: 27's header bumped to 73; 24's header line deleted; `cargo test -p env-tests --lib`): FAILED with both:
   `24_join_flow.rs has no `//! Edge-sum (concurrent Σ P×(N+M)): <n>` header line. EVERY env-test file records …` and `27_mc_slot_placement.rs records an edge sum of 73, above MAX_PER_FILE_EDGE_SUM (72): serialise its fns, or raise the cap AND re-check the Kind egress window it feeds`.
5. **Committed tree** (redeployed after reverting 1–4): `01_mh_deployment_config` 8/8 ok, `02_mc_deployment_config` 3/3 ok, env-tests lib 232/232 ok. `git status` holds no scratch edit: the budget is back at 35600000, the profile kustomization carries its `patches:` entry, and 24 and 27 match their intended headers.

---

## Files Modified

```
 Cargo.lock                                         |   4 +-
 crates/env-tests/README.md                         |   4 +-
 crates/env-tests/src/fixtures/deployment_config.rs | 297 ++++++++++++++++
 crates/env-tests/src/fixtures/egress_admission.rs  | 269 ++++++++++++++-
 crates/env-tests/src/fixtures/kube.rs              |  18 +
 crates/env-tests/src/fixtures/metrics.rs           | 117 +++++++
 crates/env-tests/src/fixtures/video/mod.rs         |  11 +-
 crates/env-tests/tests/00_cluster_health.rs        |   2 +
 crates/env-tests/tests/01_mh_deployment_config.rs  | 378 ++++++++++++---------
 crates/env-tests/tests/02_mc_deployment_config.rs  |   8 +-
 crates/env-tests/tests/10_auth_smoke.rs            |   2 +
 crates/env-tests/tests/20_auth_flows.rs            |   2 +
 crates/env-tests/tests/21_cross_service_flows.rs   |   2 +
 crates/env-tests/tests/22_mc_gc_integration.rs     |   2 +
 crates/env-tests/tests/23_meeting_creation.rs      |   2 +
 crates/env-tests/tests/24_join_flow.rs             |   4 +
 crates/env-tests/tests/25_auth_security.rs         |   2 +
 crates/env-tests/tests/27_mc_slot_placement.rs     |  12 +-
 crates/env-tests/tests/28_mh_egress_admission.rs   |  59 ++--
 crates/env-tests/tests/29_mh_meeting_teardown.rs   |   4 +-
 crates/env-tests/tests/30_observability.rs         |   2 +
 crates/env-tests/tests/31_gc_telemetry.rs          |   2 +
 crates/env-tests/tests/32_media_metric_hygiene.rs  |   2 +
 crates/env-tests/tests/33_alert_rules_loaded.rs    |   2 +
 crates/env-tests/tests/38_mh_video_forwarding.rs   |  22 +-
 crates/env-tests/tests/39_mh_video_keyframes.rs    |  22 +-
 crates/env-tests/tests/40_resilience.rs            |   2 +
 crates/mh-service/src/config.rs                    |  56 ++-
 docs/observability/metrics/mh-service.md           |   6 +-
 docs/runbooks/client-dev-local.md                  |   2 +
 docs/runbooks/mc-deployment.md                     |   8 +-
 .../media-codec-profile/kustomization.yaml         |  12 +-
 .../mh-service/configmap-egress-budget-patch.yaml  | 114 ++++---
 .../kind/services/mh-service/kustomization.yaml    |   3 +-
 scripts/dev-web.sh                                 |  88 ++++-
 scripts/dev-web.test.sh                            |  58 +++-
 36 files changed, 1276 insertions(+), 324 deletions(-)
 (new) infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml
```

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
**Status**: PASS (implementer run of `scripts/layer7.sh` on the committed tree, before review; the Lead's layer-all is authoritative)
**Duration**: ~1401s (deploy + Rust env-tests + browser E2E)
**Output**: `STATUS=OK REASON=env-tests-passed`. The full `--features all` suite was green, including 01 (window + profile equality), 28 (S9 at ceiling 135), and the story-2/3 media files at the TEST profile. Browser E2E: `17 passed`, `STATUS=OK REASON=browser-e2e-passed` (the T-A VP8 probe read the deployed TEST profile). Log `/tmp/layer7.log`.
**Rerun after the Gate-3 fixes**: `STATUS=OK REASON=env-tests-passed` (01, 28, 38 and 39 included), browser E2E `17 passed`, `layer7-summary` OK (log `/tmp/layer7b.log`). Peak `max_over_time(mh_media_egress_edges[25m])` per pod: mh-1 120 (the S9 handler, held at or below its 135 ceiling by admission), mh-0 12.
**Retained-edge evidence (media-handler caveat)**: across the run, `max_over_time(mh_media_egress_edges[25m])` per instance was 104 on one handler and 18 on the other. The 104 is the S9 file driving that handler toward its 135 ceiling, as designed. Outside S9, 30-second samples never exceeded 48 on either handler. Both stay below the window's 111 lower side with video edges in the suites, so the 39-edge retained floor plus the per-file cap still covers observed load.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Verified ctutils 0.4.4 checksum/content (RUSTSEC-2026-0385); no-insecure-browser-flags OK |
| Test | CLEAR | 0 | 0 | 0 | Residuals noted (author-written Edge-sum arithmetic; exempt token not name-bound) |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | Stale-series doc on gauge_by_instance_present |
| Code Quality | RESOLVED-FIXED | 5 | 4 | 0 | F1 (Cargo.lock) resolved by justification; F2-F5 fixed |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | One restore cmd; shared instance lists in kube.rs; Cargo.lock documented |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | Window msg restore both paths; MH config.rs test reads patches |
| Semantic Guard | not spawned | — | — | — | no check surface |
| Paired media-handler | RESOLVED-FIXED | 1 | 1 | 0 | 38/39 backlog-cap read pinned per live pod |
| Paired client | RESOLVED-FIXED | 2 | 2 | 0 | mc0_live_value_cmd rename; whole-help no-restatement check |

## Gate 2 — Full Validation (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree, exit 0, no FMT_APPLIED:

```
LAYER=1 RESULT=OK DURATION=5
LAYER=2 RESULT=OK DURATION=5
LAYER=3 RESULT=OK DURATION=158
LAYER=4 RESULT=OK DURATION=153
LAYER=5 RESULT=OK DURATION=1
LAYER=6 RESULT=OK DURATION=6
LAYER=7 RESULT=OK DURATION=1051
TOTAL_DURATION=1379 TOTAL_RESULT=OK
```

Attempts: layers 1-6 = 1, Layer 7 = 1.

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

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `e55be8765dd0d142251e38b680a72b37449a1aad`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the Kind test-profile patch (`overlays/kind/services/media-codec-profile/`) and the budget re-size (`configmap-egress-budget-patch.yaml`) are ONE unit. Either half reverted alone puts MH's ceiling outside the window: removing the test-profile patch while keeping the new budget gives a ceiling of 43 (< 54, measured in scratch run 3); keeping the test profile with the old budget gives about 313 (> 159). The env-test changes revert only together with that unit (they assert it). The `Cargo.lock` ctutils bump is independent and safe to revert alone, though that re-opens RUSTSEC-2026-0385. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
