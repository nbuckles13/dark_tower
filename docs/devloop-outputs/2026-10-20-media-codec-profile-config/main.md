# Devloop Output: Story 3 INF-A — media-codec-profile config surface

**Date**: 2026-10-20
**Task**: Story 3 task 3 (INF-A): shared media-codec-profile generator base, MC/MH config keys, retirements step 1, dt-guard cross-base resolution + downward-API limit rule, docs mirrors, env-tests
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h (setup 2026-10-20 → commit 2026-10-21)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0db5b8b4dce5f1a6c8a132737a8ea12dcdfa8f84` |
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
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned (no checks.md surface: config/manifests/guard machinery, no credential types or Debug impls)` |
| Paired Media Handler | `paired-media-handler` |
| Paired Meeting Controller | `paired-meeting-controller` |

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
Land story 3's media configuration surface before the code tasks that read it: the shared `media-codec-profile` generator base (read by MC and MH), MC per-kind caps, MH stream/keyframe keys and the downward-API memory limit, retirement step 1 for eight superseded keys, dt-guard cross-base resolution + the downward-API limit rule, docs mirrors and env-tests. Behaviour unchanged when deployed alone.

### Scope
- **Service(s)**: infra (MC/MH manifests), dt-guard, env-tests, docs
- **Schema**: No
- **Cross-cutting**: Yes (MC + MH config, guard machinery)

### Debate Decision
NOT NEEDED - design fixed by the story's Design §Wire freeze / §Deployment; prompt-vs-story conflicts ruled by the Lead (below).

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
| `infra/services/media-codec-profile/kustomization.yaml` (new) | Mine | — |
| `infra/services/media-codec-profile/profile.env` (new; bound citations are reader-owned) | Not mine, Domain-judgment | meeting-controller, media-handler (paired) |
| `infra/kubernetes/overlays/kind/services/kustomization.yaml` | Mine | — |
| `infra/kubernetes/overlays/kind/services/media-codec-profile/kustomization.yaml` (new) | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` (one-line note; value untouched) | Mine | — |
| `infra/services/mc-service/config.env` (cap/bound/banner comments) | Not mine, Domain-judgment | meeting-controller (paired) |
| `infra/services/mc-service/mc-0-deployment.yaml` | Mine | — |
| `infra/services/mc-service/mc-1-deployment.yaml` | Mine | — |
| `infra/services/mh-service/config.env` (bound/banner comments) | Not mine, Domain-judgment | media-handler (paired) |
| `infra/services/mh-service/mh-0-deployment.yaml` | Mine | — |
| `infra/services/mh-service/mh-1-deployment.yaml` | Mine | — |
| `crates/dt-guard/src/env_config.rs` | Mine | — |
| `crates/dt-guard/src/common/kustomize_generators.rs` (generator-only predicate, shared generator index) | Mine | — |
| `crates/dt-guard/src/common/services.rs` (base enumeration `infra_service_bases`) | Mine | — |
| `crates/dt-guard/src/common/pod_spec.rs` (resourceFieldRef walk) | Mine | — |
| `crates/dt-guard/src/kustomize.rs` | Mine | — |
| `crates/dt-guard/src/kustomize_content_addressing.rs` | Mine | — |
| `crates/dt-guard/src/kustomize_resource_field_refs.rs` (new; rule policy content) | Not mine, Domain-judgment | operations |
| `crates/dt-guard/src/lib.rs` (module registration) | Mine | — |
| `crates/env-tests/src/fixtures/deployment_config.rs` (new) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Mechanical | test |
| `crates/env-tests/src/fixtures/kube.rs` (`live_pods_json` made pub, Gate-3 DRY) | Not mine, Mechanical | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/02_mc_deployment_config.rs` (new) | Not mine, Minor-judgment | test |
| `docs/TODO.md` (MH_MAX_STREAMS entry, new step-2 entry, local-run-blocks entry status) | Not mine, Minor-judgment | operations |
| `docs/DEVELOPMENT.md` (MC/MH local-run blocks) | Not mine, Minor-judgment | operations |
| `.env.example` (MC/MH sections) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` (§6.3 failure-map rows for the new guard tokens) | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-10-20-media-codec-profile-config/main.md` | Mine | — |

---

## Planning

### Problem restated as mechanism
"Two services must agree on one encoding contract, and the agreement must be *derived* (one value, many readers) and *roll together* when it changes." The mechanism is a content-addressed generator in a base of its own, referenced cross-base. Wider classes this surfaces, all handled in-loop:
1. **Cross-base generator references** — today every guard assumes a workload's ConfigMaps live in its own base: env-config check 2/3 (resolution, orphans) AND kustomize R-21 (per-base render sees an unhashed, unresolved ref). Verified by scratch render: adding one profile ref to mc-0/mh-0 turns `dt-guard kustomize` red with 8 R-21 findings on the mc/mh base and overlay targets, while the environment-root render rewrites all refs to `media-codec-profile-<hash>`. Both guards are taught the shape (below); no allowlist.
2. **Generator-only bases** — R-21's "every rendered ConfigMap is referenced" positive control cannot hold in a render with no pod template; R-16 and build targets come from a hand list (`kustomize.rs:SERVICE_BASES`) the new base is not in.
3. **`resourceFieldRef` without a declared resource** — the downward-API rule covers the whole class: env `valueFrom.resourceFieldRef` AND `downwardAPI` volume items (plain and projected), `limits.*` and `requests.*`, containers and initContainers.

### Manifests
- **New base** `infra/services/media-codec-profile/{kustomization.yaml,profile.env}`: one `configMapGenerator` `media-codec-profile`, `namespace: dark-tower`, nine keys at the task's values, no resources. Header: membership rule (wording pending @team-lead ruling, see Open question 1), coordinated MC+MH roll + MH roll sheds every media session (ADR-0036 §11), why the R-26 lever is NOT here, INTERIM cost model citing the ADR-0036 addendum adaptive-capacity line by location (no figures), cost formula in words citing `EgressAdmission` (no frame-format version, no byte counts), type-blind consequence (raising video bitrate/frame rate lowers every MH's ceiling). Per key: what, readers BY CODE (MC-A; MH-A1 / MH-A2 / MH-B per @paired-media-handler's map), "the current images ignore this key until <code> lands", bounds cited by constant name only (forward refs worded as "added by MC-A"), where an operator sees it (series, no values), do-not-unify lists. The audio keys carry the substance of today's `MC_AUDIO_*` comments, corrected: MH-A1 refuses to boot outside its supported bitrate band and pins frame rate to its compiled frame duration (equality), so the deployable value is the intersection with MC's 25..50 bound. Min keyframe interval: both readers cited; retry >= interval coupling written at BOTH keys.
- **Kind root**: new `infra/kubernetes/overlays/kind/services/media-codec-profile/kustomization.yaml` (house shape: `resources: [../../../../../services/media-codec-profile]` + `environment: kind` label; INF-C's test-profile patch lands here) listed in `overlays/kind/services/kustomization.yaml`.
- **mc-0/mc-1**: nine hard `configMapKeyRef`s to `media-codec-profile`; hard refs to `MC_MAX_AUDIO_RECEIVE_SLOTS`, `MC_MAX_VIDEO_RECEIVE_SLOTS`; `optional: true` on `MC_MAX_RECEIVE_SLOTS`, `MC_AUDIO_CODEC`, `MC_AUDIO_MAX_BITRATE_BPS`, `MC_AUDIO_FRAME_RATE_HZ`. Stale "resources:, not configMapGenerator:, so no content hash" sentences in the blocks I edit are corrected (fix-don't-defer; ADR-0038 made them false).
- **mh-0/mh-1**: five profile refs; seven new hard MH keys; `MH_POD_MEMORY_LIMIT_BYTES` via `resourceFieldRef {containerName: mh-service, resource: limits.memory, divisor: "1"}`; `optional: true` on `MH_STREAM_COST_AUDIO_BPS`, `MH_STREAM_COST_VIDEO_BPS`, `MH_MAX_MUTED_SOURCES_PER_MEETING`, `MH_MAX_STREAMS` (its banner rewritten: step 1 done). "Today that is this key alone" corrected in both files; the reconciliation sentence with "DO NOT flip them" written once, where that warning lives (MH files; MC files point to it). Both files kept byte-identical modulo instance names.
- **mc config.env**: the two caps with truncation text (excess slot -> `SLOT_STATE_UNAVAILABLE`, value 9, never a join failure, supersedes story 2 R-1), bounds cited to `crates/mc-service/src/config.rs`, operator surface `mc_media_receive_slot_cap{media_kind}` (video 0 confirms the lever); lever + blast radius at the video cap citing `docs/runbooks/mc-deployment.md`. RETIRED banners for the four MC keys, stating the current image still enforces/validates them exactly as today (reject-whole, `slot_count_over_cap`; bad audio value fails startup) until MC-A removes the read. Stale story-1 "task 19 / no observable effect" note dropped.
- **mh config.env**: seven keys with media-handler's bounds cited to `crates/mh-service/src/config.rs` (by role, not symbol, where MH-A1 may rename), do-not-unify points; RETIRED banners for the three MH keys; `MH_MAX_CONNECTIONS` note -> "MH (from MH-A2) checks this relationship at boot"; `MH_EGRESS_BUDGET_BPS`: delete the false sentence, state only the checkable property (below `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` so the nudge fires, above `MIN_EGRESS_STREAM_CEILING`), cite `base_placeholder_budget_is_below_recommended_min`, INTERIM note, the memory-check coupling; every live reference to retiring keys in the file re-pointed (block header "all six"/"two per-kind costs", the `MC_MAX_RECEIVE_SLOTS` P figure in the egress-streams block rewritten as a CORRECTION without a restated P, the direction paragraph -> `MH_MAX_SOURCE_STREAMS_PER_MEETING`). No derived figure anywhere.
- **Kind egress patch**: one-line note only; value untouched.
- NOT touched: dashboards, alert rules, metric catalogs, runbooks (true for current images; ops owns runbook prose). No new metric, label or log field.

### dt-guard (machinery mine; downward-API policy content operations)
- `common/kustomize_generators.rs`: `infra_service_bases(repo_root)` (read_dir of `infra/services/`; a subdir without `kustomization.yaml` is still yielded so R-16's "missing kustomization.yaml" stays loud) and `is_generator_only(doc)` (≥1 `configMapGenerator`, no `resources:`). ONE enumeration replaces `kustomize.rs:SERVICE_BASES` (R-15/R-16 build + orphan targets now derive) and feeds env-config's sibling-base discovery. No new parser: generator keys still via `config_map_generators` + `generator_keys` (projection of `generator_data`).
- `env_config.rs`: two-pass. (a) index generators of every generator-only base; a name declared by two bases (two shared bases, or shared + a canonical service's own) is a hard error, new rule id `configmap_name_ambiguous_across_bases` (precedence beside `duplicate_configmap_name`; shared detail-builder, no copied body). (b) per service, resolve local-first, then the shared index; refs to a non-generator-only sibling base's generator stay `configmap_not_found` (a workload base's generator is that workload's private config). (c) after the loop, orphan check for shared generators over refs from EVERY canonical workload plus KeyRef refs from workloads in non-canonical bases (walked via `pod_spec`); any envFrom/volume/projected ref naming a shared generator -> `unsupported_env_source` (loud); if any service set `suppress_all_orphans` or named the shared map ambiguously, shared orphan checks are suppressed too (run is already red — same soundness argument as today, stated). OK token counts shared maps. Module doc (check 2 text, Scope boundary) + `configmap_not_found` detail updated. Helpers extracted; `analyze` does not grow.
- `kustomize_content_addressing.rs` (R-21), two scoped changes, each with a test that fails before the change: (i) in a NON-root target, an unhashed ref absent from the render whose name is a generator-only-base generator is "resolved at the environment root" (no finding); the env-root target never defers, so a profile not included in the root still fails there; (ii) the unreferenced-ConfigMap positive control applies only to renders with at least one pod template; the root keeps its vacuity check, and the profile must be referenced there.
- New `kustomize_resource_field_refs.rs` (rule `resource_field_ref_undeclared`, plus distinct `resource_field_ref_unknown_container` and `resource_field_ref_malformed`): walk in `pod_spec.rs` (`pod_resource_field_refs` -> container, origin env/volume, env name, containerName, resource). Target = `containerName` if set else own container (searched across containers + initContainers); an absent/empty `resources` counts as missing; divisor not policed (a wrong divisor fails closed in MH). Runs on the rendered base targets AND the environment root (catches an overlay patch stripping a limit). Build-dependent, but cannot pass silently: no kustomize tool already fails the run via R-21's `content_addressing_unverifiable`. Doc comment carries the reason (absent limit -> Kubernetes resolves to node allocatable -> MH's R-21 check passes vacuously). Fixtures: own-container pass, named-container pass, MH-shaped `limits.memory` dropped FAIL, named sibling lacking the limit FAIL, unknown containerName FAIL, missing `resource` FAIL, requests.* variant, initContainer, downwardAPI volume item, unrelated fieldRef untouched; asserts on rule id + detail fields.
- 6c: generator-only base trips neither R-16 nor env-config `no_workload_manifest` (it is not a canonical service); fixture for each.

### env-tests (test-owned)
- New pure fixture `crates/env-tests/src/fixtures/deployment_config.rs`: `check_key_ref(pod, container, key, expected_live_cm, expect_optional) -> Result<(), String>`, `check_resource_field_ref(...)`, `quantity_to_bytes` (Ki/Mi/Gi/Ti, k/M/G, integer; else Err), value shapes (PositiveInteger, NonNegativeInteger, Token, UnitRatio), with unit tests (Layer 4).
- `01_mh_deployment_config.rs`: table becomes `(key, base, shape)`; profile keys resolved per (instance, base); `RETIRING_KEYS` asserts present on both, key==name, `optional == Some(true)`, key STILL in the live generation and parseable; relationships across live generations (retry >= min interval, aggregation <= retry, source streams >= egress — media-handler's floor rule cited; muted >= egress kept); `MH_POD_MEMORY_LIMIT_BYTES` structure (resource, containerName, divisor 1/absent, named container DECLARES limits.memory, own message) — process-side value is not observable (distroless, today's image doesn't read it), recorded as an honest limitation. CONFIG_GAUGES, Kind ceiling test, `28_*`, `egress_admission.rs` unchanged.
- New `02_mc_deployment_config.rs`: mc-0/mc-1 wiring of 9 profile + 2 cap keys, retiring MC keys, and ONE `media-codec-profile` generation across mc-0/mc-1/mh-0/mh-1 via `kube::configmap_name_for_all`. Positive controls (tables non-empty, each instance matched). No polling/sleeps for static reads.

### Docs
- `docs/TODO.md`: `MH_MAX_STREAMS` entry -> step 1 done, step 2 pending (points at the new entry); ONE new step-2 entry for all eight keys (delete key + refs in one change; trigger MH-A1 AND MC-A images deployed and proven AND the story-3 cutover rollback window closed (R-25); precondition INF-C moved the live `MC_MAX_RECEIVE_SLOTS` readers off it; owner infrastructure with operations; per-key site inventory incl. env-tests lists, `CONFIG_GAUGES`, `28_*`, `fixtures/egress_admission.rs`, `scripts/dev-web.sh` + test, `.env.example`, `docs/DEVELOPMENT.md`, runbook rows, dashboards/catalog rows). Local-run-blocks entry: MC block status update. "under-costs audio" entry left for MH-A1.
- `docs/DEVELOPMENT.md` + `.env.example`: MC/MH blocks gain every new key (profile keys in both, `MH_POD_MEMORY_LIMIT_BYTES` noted as downward-API in-cluster / set by hand locally), retiring keys kept and marked retired, `ANCHOR (DRY)` names `profile.env` as the profile keys' home.
- Runbooks: not edited; final key list sent to @operations for the OPS-A Configuration Reference rows.

### Verification
Scratch (not committed), recorded here with hashes: `kubectl kustomize infra/kubernetes/overlays/kind` renders one `media-codec-profile-<hash>` referenced from mc-0/mc-1/mh-0/mh-1; a profile value change changes all four pod templates; a `MC_MAX_VIDEO_RECEIVE_SLOTS` change changes mc-0/mc-1 only. Then `./scripts/layer-fast.sh`.

### Safe-revert unit
The whole commit, and reverting it alone is safe ONLY while MC-A/MH-A1/MH-A2/MH-B are not deployed — once any of them is, these keys are required reads and a revert crash-loops that service.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (S1-S4 accepted) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed (A-C accepted) |
| Operations | confirmed |
| Paired Media Handler | confirmed |
| Paired Meeting Controller | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Plan approved by Lead 2026-10-20.

Story-level notes for MH-A1 (raised by paired-media-handler, non-blocking for INF-A): (a) story ceiling 4096 for `MH_MAX_SOURCE_STREAMS_PER_MEETING` is below the 8192 egress-streams ceiling, so "source ≥ egress" makes some accepted egress values unpairable; (b) MC-A sends every publisher's source streams to every handler (no edge filter), so the story-2 rationale for source ≥ egress no longer holds — MH-A1's doc/error text must not claim the floor guarantees a registration fits.

### Lead rulings and prompt-vs-story deviations (recorded at Gate 1)
- **Membership rule**: story wording wins over the prompt's "and both read it" clause (four of the nine keys are MC-read only). Header uses "values both services must agree on" + the encoding-contract justification, states that changing an MC-only-read profile key still rolls MH, and gives the reader's test ("would MH's derived costs or validation be wrong if this changed without MH rolling?").
- **Memory-check owner**: the prompt says MH-A1; the story's media-handler design puts it in MH-A2. Committed text says MH-A2 everywhere.
- **`MH_POD_MEMORY_LIMIT_BYTES` env-test**: the running container's value is unobservable (distroless, no exec; today's image neither reads nor publishes it). The env-test asserts the live wiring (resourceFieldRef containerName/resource/divisor) and that the container declares `limits.memory`; Kubernetes then resolves exactly that limit. Honest limitation; the closing gauge is filed in `docs/TODO.md` §Observability Debt for MH-A2.
- **Scope additions approved in-loop**: R-21 cross-base refs, derived service bases, downwardAPI volume items in the limit rule.

### Gate-1 reviewer refinements carried into implementation
- One `shared_generator_index` (common/kustomize_generators.rs) consumed by env-config AND R-21; R-21 never resolves an ambiguous name (dry-reviewer A, security S2).
- R-21 deferral coupled in code to a successful environment-root render in the same run; exact-name match; tests for root-missing/failed and profile-excluded-from-root (security S2).
- Base enumeration (`infra_service_bases`, in `common/services.rs`) gated through `resolve_cited_path`; symlink-escape fixture (security S1, code-reviewer a).
- `is_generator_only`: `resources: []` and `resources:` (null) both mean "no resources", fixture each (code-reviewer c).
- Downward-API rule: SOUNDNESS note at both sites + a test that a no-tool run is red (code-reviewer b); fixtures incl. requests-but-not-limits FAIL (security S4); cross-workload-base ref stays `configmap_not_found` fixture (security S3).
- env-tests: hoist 01's helpers into `fixtures/deployment_config.rs` (no parallel shapes); divisor checked with a wrong-divisor unit test; per-branch `check_key_ref` tests on message content (dry-reviewer B, test).
- `SLOT_STATE_UNAVAILABLE` cited by name from signaling.proto, no numeral (dry-reviewer C).
- `MH_POD_MEMORY_LIMIT_BYTES` comment: MH-A2's startup check logs the effective limit (operations).
- Reconciliation sentence beside "DO NOT flip them" states the current images hard-read every retiring key, so an optional ref never loosens a bound (security N1).
- `MH_MAX_SOURCE_STREAMS_PER_MEETING` ceiling cited by role, not number (lead / media-handler).

### Open questions for @team-lead (ruled above)
1. **Membership rule vs key list (story-file wording wins).** The prompt's header rule says a key belongs ONLY if "both MC and MH must agree on it (and both read it)", but four of the nine keys (`MEDIA_VIDEO_CODEC/WIDTH/HEIGHT`, `MEDIA_AUDIO_CODEC`) are read by MC only — the file would contradict its own header. The story file (§Deployment) says "membership is restricted to values both services must agree on" with no "both read it" clause. Proposal: use the story wording and state why codec + geometry belong — the profile is one encoding contract, and MH's derived cost is only valid for the encoding the profile describes. Please rule.
2. **Memory check owner.** The prompt attributes the R-21 memory startup check to task 8 (MH-A1); the story file's media-handler design puts it in MH-A2 (task 9), and @paired-media-handler agrees. Comments will say MH-A2 (story wins).
3. **`MH_POD_MEMORY_LIMIT_BYTES` env-test.** The prompt asks to assert the RUNNING container's value equals the limit in bytes; that value is not observable (distroless, no exec; today's image neither reads nor publishes it). Plan asserts the structure + declared limit (Kubernetes then resolves exactly that limit) and records the gap; MH could close it later with a published gauge.

---

## Pre-Work

None

---

## Implementation Summary

### Manifests
| Item | Before | After |
|------|--------|-------|
| Shared profile | — | New generator-only base `infra/services/media-codec-profile/` (9 keys, header: membership rule + test, MC+MH roll, interim cost model citing ADR-0036 addendum item 9) + Kind overlay dir listed in the root's service list |
| mc-0/mc-1 | 5 media signalling refs | +9 hard profile refs, +2 hard cap refs (`MC_MAX_{AUDIO,VIDEO}_RECEIVE_SLOTS`), 4 retiring refs `optional: true` |
| mh-0/mh-1 | story-2 refs | +5 hard profile refs, +7 hard MH keys, `MH_POD_MEMORY_LIMIT_BYTES` via `resourceFieldRef` (`mh-service`, `limits.memory`, divisor 1); `MH_STREAM_COST_*`, `MH_MAX_MUTED_SOURCES_PER_MEETING`, `MH_MAX_STREAMS` `optional: true`; reconciliation with "DO NOT flip them" written once at `MH_MEDIA_LATENCY_SAMPLE_RATIO` |
| mc config.env | total cap + MC_AUDIO_* live | per-kind caps (truncation to `SLOT_STATE_UNAVAILABLE`, lever + blast radius), RETIRED banners stating the current image still enforces them; stale "resources: not configMapGenerator / does not roll" sentences corrected (KEK, settle, block header) |
| mh config.env | story-2 costs/muted live | 7 new keys, RETIRED banners, `MH_MAX_SOURCE_STREAMS_PER_MEETING` (real relationship, sizing guidance, whole-registration reject), budget comment = checkable property only (+ memory coupling), every live mention of retiring keys re-pointed, `MH_MAX_CONNECTIONS` "MH checks this at boot (MH-A2)", "1 Gi" restatements pointed at `resources.limits.memory` |
| Kind egress patch | — | one-line note (costs derived from the profile once MH-A1 lands; INF-C re-sizes); value untouched |

### dt-guard
- `common/services.rs:infra_service_bases()` — the ONE base enumeration (containment-gated; symlink escape = error). `kustomize.rs:SERVICE_BASES` deleted; R-15/R-16 targets derived.
- `common/kustomize_generators.rs:is_generator_only()` + `shared_generator_index()` — consumed by env-config AND R-21; ambiguous names never resolved.
- `env_config.rs` — cross-base resolution (local-first, then shared), `configmap_name_ambiguous_across_bases`, shared orphan pass over every canonical + non-canonical workload, suppression propagated (soundness stated), OK token counts shared maps. `analyze` split into helpers.
- `kustomize_content_addressing.rs` (R-21) — `RenderContext`: non-root targets defer exact-name bare refs to unambiguous shared generators; the root never defers; deferrals fail unless the root rendered in the same run (`kustomize.rs:unverified_deferral_hits`). Unreferenced positive control always runs; in non-root targets only, a SHARED generator's ConfigMap (hash-stripped name in the shared index) is exempt — it is held to the control at the root (revised at Gate 3 from a "pod template present" proxy, per code-reviewer). **Rule changes, scoped by name, not blanket exemptions**: both have tests that fail against the old rule, plus a negative (pod-template-free render with a non-shared ConfigMap still fails).
- `kustomize_resource_field_refs.rs` (new; policy content operations) + `pod_spec.rs:pod_resource_field_refs()` — env refs and downwardAPI volume items, limits/requests, containers + initContainers; runs on rendered bases + environment root; SOUNDNESS note + `no_tool_run_is_red_so_build_dependent_rules_cannot_pass_silently`; positive controls: the OK token carries the environment root's checked-ref count (`...-2-resource-field-refs`) and `real_mh_memory_limit_refs_are_seen_and_guarded` renders the real mh base (refs seen on both, clean; limit stripped -> exactly 2 findings).

### env-tests
- `fixtures/deployment_config.rs` (new): hoisted from 01 (pod/ConfigMap fetch, value shapes, wiring check) + `check_key_ref`, `check_resource_field_ref`, `quantity_to_bytes`; 13 unit tests.
- `01_mh_deployment_config.rs`: `(key, base, shape)` table, `RETIRING_KEYS` (optional + still present), memory-limit structure, relationship assertions (retry >= min interval, aggregation <= retry, sources >= egress, muted >= egress kept). CONFIG_GAUGES / Kind ceiling / published ceiling unchanged.
- `02_mc_deployment_config.rs` (new): mc-0/mc-1 wiring + retiring keys; one `media-codec-profile` generation across mc-0/mc-1/mh-0/mh-1.

### Docs
- `docs/TODO.md`: `MH_MAX_STREAMS` entry step 1 done; ONE step-2 entry for all eight keys (trigger incl. R-25 window + INF-C precondition, per-key site inventory); §Observability Debt entry for the `MH_POD_MEMORY_LIMIT_BYTES` gauge (media-handler, lands with MH-A2); local-run-blocks entry status.
- `docs/DEVELOPMENT.md`: MC block made complete (was non-functional) + story-3 keys; MH block extended; anchors name `profile.env`.
- `.env.example`: MC + profile + MH sections; dead `MC_BIND_ADDRESS` replaced with the name MC reads.
- `docs/runbooks/devloop-validation.md` §6.3: rows for `configmap-name-ambiguous-across-bases`, R-21 deferral, `kustomize-resource-field-ref-<n>`.

### Scratch-render verification (not committed; copy of `infra/` in /tmp)
Template hash = sha256 of each Deployment's rendered `spec.template` text (first 12 hex), from `kubectl kustomize infra/kubernetes/overlays/kind`.

| Render | mc-0 | mc-1 | mh-0 | mh-1 | media-codec-profile | mc-service-config |
|---|---|---|---|---|---|---|
| baseline | dc25bc8085b3 | 444148ea5fd4 | ed1fb60fca6a | 92c4605ea714 | -94ct5t99db | -56t988fg4t |
| `MEDIA_VIDEO_FRAME_RATE_HZ` 30->15 | e00fe93fbf48 | e1ed9e6ecdfe | 00deefcf39ce | ce1b0f5e8b2f | -2992d8k6f6 | -56t988fg4t |
| `MC_MAX_VIDEO_RECEIVE_SLOTS` 4->0 | 8b2c6b55471f | 954d8cd035e4 | ed1fb60fca6a | 92c4605ea714 | -94ct5t99db | -md42g2d5dg |

- Profile change: all four pod templates change (coordinated MC+MH roll). Cap change: mc-0/mc-1 only; mh-0/mh-1 unchanged.
- Root render: ONE `media-codec-profile-94ct5t99db` ConfigMap, referenced 28 times (9 x mc-0/mc-1 + 5 x mh-0/mh-1), no bare reference.
- Negative check (scratch): dropping `limits.memory` from mh-0 -> `dt-guard kustomize` FAIL `kustomize-resource-field-ref-2` (base + environment root).
- Real tree: `dt-guard kustomize` OK `kustomize-clean-kubeconform-skipped-24-consumed-configmaps-2-resource-field-refs`; `dt-guard env-config` OK `env-config-clean-4-services-6-workloads-9-configmaps`.

---

## Files Modified

```
 .env.example                                       |  52 +-
 crates/dt-guard/src/common/kustomize_generators.rs | 285 ++++++
 crates/dt-guard/src/common/pod_spec.rs             | 190 ++++
 crates/dt-guard/src/common/services.rs             |  85 ++
 crates/dt-guard/src/env_config.rs                  | 969 ++++++++++++++++-----
 crates/dt-guard/src/kustomize.rs                   | 366 +++++++-
 .../dt-guard/src/kustomize_content_addressing.rs   | 296 ++++++-
 .../dt-guard/src/kustomize_resource_field_refs.rs  | 349 ++++++++
 crates/dt-guard/src/lib.rs                         |   1 +
 crates/env-tests/src/fixtures/deployment_config.rs | 737 ++++++++++++++++
 crates/env-tests/src/fixtures/kube.rs              |  10 +-
 crates/env-tests/src/fixtures/mod.rs               |   1 +
 crates/env-tests/tests/01_mh_deployment_config.rs  | 616 ++++++-------
 crates/env-tests/tests/02_mc_deployment_config.rs  | 182 ++++
 docs/DEVELOPMENT.md                                | 108 ++-
 docs/TODO.md                                       |   8 +-
 .../2026-10-20-media-codec-profile-config/main.md  | 480 ++++++++++
 docs/runbooks/devloop-validation.md                |   5 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |   8 +-
 .../overlays/kind/services/kustomization.yaml      |   1 +
 .../media-codec-profile/kustomization.yaml         |  14 +
 .../mh-service/configmap-egress-budget-patch.yaml  |   8 +-
 infra/services/mc-service/config.env               | 242 +++--
 infra/services/mc-service/mc-0-deployment.yaml     | 112 ++-
 infra/services/mc-service/mc-1-deployment.yaml     | 112 ++-
 .../media-codec-profile/kustomization.yaml         |  38 +
 infra/services/media-codec-profile/profile.env     | 168 ++++
 infra/services/mh-service/config.env               | 447 +++++-----
 infra/services/mh-service/mh-0-deployment.yaml     | 162 +++-
 infra/services/mh-service/mh-1-deployment.yaml     | 162 +++-
 30 files changed, 5126 insertions(+), 1088 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| (see §Implementation Summary) | per-file detail recorded there by the implementer |

---

## Devloop Verification Steps

**Implementer `./scripts/layer-fast.sh` (2026-10-20)**: TOTAL_RESULT=OK — L1 OK, L2 OK, L3 OK (incl. `dt-guard env-config`, `dt-guard kustomize`, knowledge-index), L4 OK, L5 OK, L6 OK. First run failed L3 on `validate-knowledge-index` (INDEX.md 76/75 lines), fixed by merging two lines. `cargo clippy -p dt-guard -p env-tests --features env-tests/all --all-targets -D warnings` clean. Full `layer-all.sh` (incl. Layer 7) is the Lead's.

Gate 2 = `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree (run-all, headless): `TOTAL_RESULT=OK`, TOTAL_DURATION=1341s.

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 Compile | OK | 5s | |
| 2 Format | OK | 4s | |
| 3 Guards | OK | 134s | incl. `dt-guard env-config`, `dt-guard kustomize` (OK token carries resource-field-ref count), script self-tests |
| 4 Test | OK | 147s | cargo-nextest, doctests, nx test |
| 5 Lint | OK | 1s | clippy, buf lint, nx lint |
| 6 Audit | OK | 1s | cargo/pnpm audit SKIPPED-NO-DIFF (no-dep-changes); buf breaking passed |
| 7 Env-tests | OK | 1049s | deployed this task alone on current images; `env-tests-passed`, `browser-e2e-passed` (behaviour unchanged at cutover) |

Fast check before review (`layer-fast.sh`): TOTAL_RESULT=OK. Log: `/tmp/layerall1.log` (container view).

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | real-tree positive control for resourceFieldRef rule |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | real-tree control; tautological counter replaced |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | gauge names in comments; dropped implicit 1Gi restatement |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | R-21 positive control always runs; read_dir errors; local-shadow ambiguity |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | parse_docs/meta_str/manifest reader/kustomization reader/env-test fetch consolidated |
| Operations | RESOLVED-FIXED | 6 | 6 | 0 | step-2 precondition widened; requests.* doc; OK-token ref count |
| Paired Media Handler | RESOLVED-FIXED | 4 | 4 | 0 | comment accuracy |
| Paired Meeting Controller | RESOLVED-FIXED | 2 | 2 | 0 | audio-cap gauge caveat; keyframe rationale |

Semantic Guard: not spawned (no checks.md surface).

---

## Code Review Results

Per-reviewer verdicts, counts and key findings are in §Gate 3 — Verdicts above (all RESOLVED-FIXED, 30 found / 30 fixed / 0 deferred). DRY extraction opportunities appended to `docs/TODO.md`: none. Semantic Guard: not spawned.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

Forward-looking entries filed in `docs/TODO.md` as task scope (not deferrals): the scheduled retirement step-2 entry, and the §Observability Debt `MH_POD_MEMORY_LIMIT_BYTES` loaded-value gauge (owner media-handler, MH-A2).

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `0db5b8b4`
2. Review all changes: `git diff 0db5b8b4..HEAD`
3. Soft reset (preserves changes): `git reset --soft 0db5b8b4`
4. Hard reset (clean revert): `git reset --hard 0db5b8b4`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit — and reverting it alone is safe ONLY while none of MC-A / MH-A1 / MH-A2 / MH-B is deployed; once any is, these keys are required reads and a revert crash-loops that service. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Prompt vs story-file disagreements
**Problem**: Prompt's profile membership rule ("both read it") contradicted 4 MC-only keys; prompt named MH-A1 as memory-check owner where the story says MH-A2; runtime value of `MH_POD_MEMORY_LIMIT_BYTES` unobservable (distroless, current image doesn't read it).
**Resolution**: Lead ruled story-file wins on (1)/(2); (3) env-test asserts wiring + declared limit, gap filed under §Observability Debt. See §Lead rulings.

### Issue 2: R-21 content addressing rejected cross-base refs in per-service renders
**Problem**: Rendering mc/mh bases alone leaves the profile ref unhashed (8 R-21 failures).
**Resolution**: Rule changed with tests: per-service builds defer exact, unambiguous shared-generator names to the env root, valid only if the root rendered in the same run; positive control always runs, exempting only shared generators in non-root renders. No allowlist added.

### Issue 3: Story-level notes for MH-A1 (not INF-A defects)
**Problem**: Story ceiling 4096 for `MH_MAX_SOURCE_STREAMS_PER_MEETING` < 8192 egress ceiling; source ≥ egress rationale no longer holds since MC-A sends all publishers' source streams to every handler.
**Resolution**: Recorded in §Gate 1 for MH-A1; INF-A comments cite the ceiling by role and size the bound by publishers.

---

## Lessons Learned

1. A shared generator across bases needs every guard that resolves ConfigMap refs (env-config, R-21) to learn cross-base resolution through one index.
2. New walkers need a real-tree positive control, not only synthetic fixtures — four reviewers independently flagged it.

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
