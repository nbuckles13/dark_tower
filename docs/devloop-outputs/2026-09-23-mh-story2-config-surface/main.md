# Devloop Output: MH story-2 configuration surface (egress budget, stream costs, §8 policy keys, MH_MAX_STREAMS retirement)

**Date**: 2026-09-23
**Task**: Land media-handler story-2 config keys in infra/services/mh-service (R-19, R-22, R-23, R-24), Kind overlay budget patch, MH_MAX_STREAMS retire-pending-removal, env-test for deployed-value parity + Kind ceiling
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired-with=test) <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/hear-each-other`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `378a1374d64a9172bd7df5c37d156cfee9e8aa6e` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (infrastructure, opus)` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `paired-test (--paired-with=test)` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Media Handler (conditional domain owner) | `spawned` |

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
| `infra/services/mh-service/configmap.yaml` | Mine | — |
| `infra/services/mh-service/mh-0-deployment.yaml` | Mine | — |
| `infra/services/mh-service/mh-1-deployment.yaml` | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` (new) | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/kustomization.yaml` | Mine | — |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Minor-judgment | test (paired) |
| `crates/env-tests/src/lib.rs` (lift `repo_root()` to the crate's single home, beside `NAMESPACE`) | Not mine, Minor-judgment | test (paired) |
| `crates/env-tests/tests/33_alert_rules_loaded.rs` (use the lifted `repo_root()`; delete its private copy) | Not mine, Minor-judgment | test (paired) |
| `docs/runbooks/devloop-validation.md` (§6.3 failure-token row for the new `env-config-configmap-key-name-mismatch` token) | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (navigation pointers) | Mine | — |
| `infra/kind/scripts/setup.sh` (restart a service's pods when `apply -k` changed one of its ConfigMaps in place; all four services) | Mine | — |
| `scripts/setup.test.sh` (hermetic decision-table tests for the restart helper) | Mine | — |
| `.env.example` | Mine | — |
| `docs/DEVELOPMENT.md` (MH local-run block) | Not mine, Minor-judgment | operations |
| `docs/DATABASE_SCHEMA.md` (one-line note, §8 media_handlers) | Not mine, Minor-judgment | database |
| `docs/runbooks/mh-deployment.md` (stale `MH_MAX_STREAMS` contrast row only) | Not mine, Minor-judgment | operations |
| `crates/dt-guard/src/env_config.rs` (new key==name rule) | Mine | — |
| `docs/observability/dashboard-conventions.md` (flip `MH_EGRESS_BUDGET_BPS` forward-reference bullet to live) | Not mine, Minor-judgment | observability |
| `docs/TODO.md` (close §8-keys-no-ConfigMap entry; file scheduled `MH_MAX_STREAMS` removal) | Mine | — |

---

## Planning

**Mechanism restatement.** "A config value's number has exactly one home, every consumer derives from it, and every relationship between two deployed numbers is asserted by a check rather than by prose." Instances in this task: (1) budget/cost/ceiling relationship — Kind value asserted by env-test, base value carries no arithmetic; (2) §8 keys — ConfigMap becomes the home at values equal to code defaults (task 8 deletes the defaults); (3) `MH_MAX_STREAMS` — four sites with three disagreeing values collapse to one retired ConfigMap key + scheduled removal; (4) deployed-vs-running parity — env-test compares ConfigMap to the pod's own startup report. Wider-class instance found: `docs/runbooks/mh-deployment.md`'s `MH_MAX_STREAMS` contrast row ("its correction is a later story") becomes false with this devloop — fixed in-tree (one row). The runbook's required-keys table gaining rows for the new keys is task 18's (operations) scope and is NOT yet true (keys are not required until task 8) — left to task 18, which is explicitly assigned the MH config-failure triage + retirement record.

**Files / changes.**
1. `configmap.yaml` (base): add 10 keys in two commented blocks.
   - Egress chain (R-19/R-23): `MH_EGRESS_BUDGET_BPS "10000000"` (placeholder; comment: not sized, force-a-choice placeholder costed at the video rate, not a capacity claim, same spirit as `basis="unmeasured"`; announces itself via MH's startup "configure me" nudge; operator sets it to expected peak; effective ceiling is in the startup log; margin stated QUALITATIVELY with no numerals: the placeholder still boots, sits above MH's refuse-to-boot floor, and the remedy for the nudge is a real budget, never a lower floor; NO formula, no ceiling/floor literal), `MH_STREAM_COST_AUDIO_BPS "90000"`, `MH_STREAM_COST_VIDEO_BPS "2500000"`, `MH_EGRESS_REJECTION_RATIO_THRESHOLD` (one value read by admission and the exhaustion alert via the threshold gauge; comment carries @observability's C1 windowed-ratio, C2 all-rejected-publishes-1.0, C3 chosen-not-derived / revisit when the MH SLO ratifies in story 8), `MH_MAX_REGISTERED_MEETINGS`, `MH_MAX_MUTED_SOURCES_PER_MEETING` (§8-guard family, pre-allocation bound).
   - §8 policy block (R-22): `MH_MAX_EGRESS_STREAMS_PER_MEETING "512"`, `MH_MAX_CANDIDATE_SOURCES_PER_EGRESS "16"`, `MH_MAX_TOTAL_EGRESS_EDGES "65536"`, `MH_POLICY_APPLY_TIMEOUT_MS "1000"` — resource-exhaustion guards, never capacity, not advertised, not placement; `MH_MAX_TOTAL_EGRESS_EDGES` cites `config.rs::DEFAULT_MAX_TOTAL_EGRESS_EDGES` + Scenario 13, and says it becomes an ordinary resource guard only once R-20 teardown reclaims ended meetings (conditional).
   - `MH_MAX_STREAMS`: comment rewritten to RETIRED-PENDING-REMOVAL (no code reads it after task 8; kept because `rollout undo` restores the configMapKeyRef, not the ConfigMap → `CreateContainerConfigError`); value unchanged. `MH_MAX_CONNECTIONS` comment's "that is MH_MAX_STREAMS" pointer updated to the derived stream ceiling.
2. `mh-0-deployment.yaml` / `mh-1-deployment.yaml`: 10 hard `configMapKeyRef`s each (per workload), not `optional:` (a missing key must be loud — the file's own rule). `MH_MAX_STREAMS` ref kept with retired marker.
3. NEW `overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`: `MH_EGRESS_BUDGET_BPS "100000000"`; comment carries the formula (ceiling = budget / max(cost_audio, cost_video), video cost even though audio-only), the requirement (N+1 all-hear-all = (N+1)×N edges; suite locked N=3 = 12, sized for N=5 = 30), the symptom of under-sizing ("some receivers missing some senders"), guard-coverage note (env-config covers base key; patch changes a value not a key), pointer to the env-test. No ceiling literal. + `patches:` entry in overlay kustomization.
4. `01_mh_deployment_config.rs` (design merged with @paired-test): reuse `fetch_pod()` (newest live pod); add `fetch_configmap(name)` with the same never-skip / `-o json` discipline. One shared const list of story-2 keys (+ retained `MH_MAX_STREAMS`), iterated for BOTH `mh-0` and `mh-1`:
   (a) WIRING — env entry exists, is `valueFrom.configMapKeyRef`, `name == mh-service-config`, `key == env name` (catches a crossed ref); resolve `data[key]` from the live ConfigMap, absent key = distinct panic. This is primary per-workload coverage in THIS task: rule 1 only engages once task 8 makes the reads required, and rule 3 is satisfied by either workload.
   (b) PARSE — every resolved value parses as its type (u64 > 0; threshold f64 in [0,1]) so the manifests are proven loadable under task 8's required-read image before it ships.
   (c) RUNNING-POD PARITY — for the four §8 keys, the running MH process's own `"Configuration loaded successfully"` startup event (kubectl logs, JSON, named fields only) reports the same value as the deployed ConfigMap. This is the "deployed value == what the running pod reports" assertion (the current image already reads and logs these four; it covers ConfigMap-changed-but-pod-not-restarted staleness directly for the four §8 keys; (c2) covers the rest). Missing startup line = distinct loud failure. Task 8 extends the KEY→field table with budget/costs/ceiling.
   (c2) STALENESS — for every key, the container's `state.running.startedAt` must be at or after the newest `metadata.managedFields[].time` of `mh-service-config` (paired-test's proposal). A stale pod gets its own branch naming `kubectl rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower`. `kubectl apply -k` of a value change does not restart pods, and the Kind patch comment says so too.
   (d) OVERLAY APPLIED — deployed `MH_EGRESS_BUDGET_BPS` == value parsed from the Kind patch file (serde_norway); deployed == base placeholder gets its own "overlay did not apply" branch.
   (e) CEILING — `test_kind_egress_stream_ceiling_meets_demo_requirement`: floor(budget / max(audio, video)) from DEPLOYED values ≥ `required_edges(DEMO_N=5)` where `required_edges(n) = (n+1)*n`; const assert suite N=3 ≤ demo N; cost > 0 checked before divide; failure says raise the Kind budget, never lower costs/N. ANCHOR comment: second encoding of task 8's derivation; becomes checked when task 8's env-test compares the ceiling gauge to this.
   The module doc names the emission site `crates/mh-service/src/main.rs` ("Configuration loaded successfully") as a test-load-bearing interface. The TODO §Observability Debt entry asks task 8 to mark that emission site as test-load-bearing. The (a)-proves-wiring / (c)-proves-reading sentences sit adjacent to each other.
   PII policy paragraph extended to cover the ConfigMap read and the log read (scalars only; never Value / env array / data map / raw stdout / raw log line).
4b. `crates/dt-guard/src/env_config.rs` (guard machinery, mine): new rule `configmap_key_name_mismatch` — a `configMapKeyRef.key` must equal its env var name. Verified zero existing violations across `infra/`. Static, all-services version of (a)'s crossed-ref check; + unit tests + precedence entry.
5. `.env.example`: remove `MH_MAX_STREAMS=10000`; add the story-2 MH keys at base values with a pointer to the ConfigMap as SoT (ANCHOR (DRY)).
6. `docs/DEVELOPMENT.md` MH block: remove `MH_MAX_STREAMS`; add the new keys; ANCHOR (DRY) comment naming ConfigMap + `config.rs::from_vars` (option (a) of TODO §DEVELOPMENT.md drift entry, whose trigger this edit fires).
7. `docs/DATABASE_SCHEMA.md`: one-line note — migration `20260124000001_mh_registry.sql` `max_streams DEFAULT 1000` is intentionally retained and unreachable (GC register always binds the MH-derived ceiling); no migration (ADR-0036 arc).
8. `docs/runbooks/mh-deployment.md`: correct the stale `MH_MAX_STREAMS` contrast row to "retired-pending-removal".
9. `docs/TODO.md`: mark the "four §8 keys have no ConfigMap entry" entry resolved (manifests half; task 8 flips reads); add a SCHEDULED removal: delete `MH_MAX_STREAMS` ConfigMap key + both refs, trigger = first deploy after the task-8 image is proven (rolled out, no rollback needed); owner infrastructure; lists all sites.

**Values not named by the task (proposed; domain ruling requested from @media-handler / @observability / @security):**
- `MH_EGRESS_REJECTION_RATIO_THRESHOLD` `0.05` — warning-grade: 1 in 20 admissions refused sustained 10m.
- `MH_MAX_REGISTERED_MEETINGS` `1024` — resource-exhaustion guard, never capacity: far above this deployment's single-digit concurrent meetings; bounds the empty-egress-set surface the edge bound is blind to. Defence-in-depth against a buggy/compromised MC inside the operator trust boundary.
- `MH_MAX_MUTED_SOURCES_PER_MEETING` `512` — pre-allocation bound; reasoning: a muted source must be a sender in that meeting, and every sender is also a subscriber holding at least one egress stream (N ≥ 1), so a legitimate snapshot can never name more senders than `MH_MAX_EGRESS_STREAMS_PER_MEETING` admits. Sized against the per-meeting sender population, not the 16-bit sender-id space.

**Comment conventions (from @observability / @security / @media-handler input).** Every new block ends `# Enforced by:` / `# Where an operator sees it:`; not-yet-live metrics/alerts named as landing with the MH code task / alerts task; runbook content cited by TITLE (Scenario 13 "RegisterMeeting Timeout — Clients Kicked" by title + number since it exists today). Base budget block is QUALITATIVE only: no `4`, no `2`, no division, no `30` — the margin is stated as "the placeholder still boots, sits above MH's refuse-to-boot floor (a code constant owned by the MH code task, cited by name once it exists), and the fix for the startup nudge is a real budget, never a lower floor". Threshold number lives only in the ConfigMap. `MH_MAX_STREAMS` retired marker states the "live on the wire / GC places against it" claim expires with the next MH image. Additional edits: `docs/observability/dashboard-conventions.md` flips the `MH_EGRESS_BUDGET_BPS` live-instance bullet (the `{basis="unmeasured"}` gauge forward-reference stays); `docs/TODO.md` §Observability Debt gets the expiry line: env-test's ceiling formula flips to reading `mh_media_egress_stream_ceiling` when the MH code task lands.

**Operations input folded (OPS-A..H).** OPS-A: 4(c) is the real "running pod reports" check for the four §8 keys (the current image logs them). For the budget, costs, threshold and caps, the test header says plainly that it proves deployed-artifact vs requirement, NOT that the process observed the value (the pod may predate a ConfigMap edit). The strengthening is filed in TODO §Observability Debt, together with the formula expiry: task 8's env-test scrapes `mh_media_egress_stream_ceiling`. OPS-B: `Enforced by:` / `Where an operator sees it:` on all ten keys. `MH_MAX_TOTAL_EGRESS_EDGES` says that today the only signal is `mh_media_policy_applies_total{outcome="apply_failed"}`, which is terminal, and that the edges/limit gauges land later. The `MH_MAX_CONNECTIONS` "far above peak" sentence is NOT carried over. OPS-C: the base budget comment names the join-outage consequence (a starved ceiling means GC placement has no candidate, so join fails with ServiceUnavailable, by the same mechanism as scaling MH to zero). OPS-D: the startup-log pointer is phrased as the MH code task's obligation, not as a fact about today's image. OPS-E: hard refs on both workloads, no `optional:`, no `envFrom`. OPS-F: the TODO scheduled removal is itself two deploys (A drops the refs, prove it, B drops the key), with a trigger. OPS-G: `mh-deployment.md` gets the "Three keys begin MH_MAX_" count corrected and the `MH_MAX_STREAMS` row relabelled retired-pending-removal. The ten full rows go to task 18. OPS-H: `dt-guard kustomize` already builds `overlays/kind/services/<svc>` (kustomize.rs:306-316), so a malformed patch fails the build. The strategic-merge rename hazard is caught at runtime by 4(d) and is the class tracked in TODO "`dt-guard env-config` does not cover `infra/kubernetes/overlays/**`" (task-sized, different resolution model). That entry's list of overlay patches gets the MH patch added so the entry stays accurate.

**Rulings and further input folded.**
- @media-handler approved the three values: threshold 0.05, registered meetings 1024, muted sources 512. The muted comment says "bounded by", never "must equal".
- @security:
  - The muted comment names its precondition (N >= 1 for every participant; ZERO_REQUESTED or send-only participants void it).
  - Egress and muted cross-reference each other both ways.
  - The muted bound is described as a loose never-falsely-reject bound. The egress check does not subsume it, because a hostile snapshot can have zero egress streams.
  - The rule doc carries the redis `REDISCLI_AUTH` secretKeyRef counter-example and the rule is a hard FAIL.
  - The log extraction reads named JSON fields only.
- @dry-reviewer:
  - `MH_MAX_TOTAL_EGRESS_EDGES` cites its full heading text rather than a bare number.
  - The audio-cost comment says it is derived from ADR-0036 §4 "Cost, stated plainly" (~90 kbps wire cost, not a codec bitrate). It names the constants it is not, and records the under-cost against the 236 B nominal and the MC 48 kbps ceiling (non-operative while video dominates max()).
  - All four §8 keys get an ANCHOR (DRY).
  - The threshold comment forbids a PromQL literal.
  - No SUITE_N constant.
  - The full MH block in DEVELOPMENT.md is corrected, and `.env.example`'s `MH_BIND_ADDRESS` becomes `MH_WEBTRANSPORT_BIND_ADDRESS`.
  - The removal TODO enumerates all five sites.
  - Noted for task 16 / observability: the `infra/docker/prometheus/rules/mh-alerts.yaml` ~475 sentence "no such budget exists in this build" becomes false when the task-8 image ships.
- @code-reviewer: the new rule gets positive plus negative plus per-instance self-tests and a scratch real-manifest run. Freezing key==name repo-wide is deliberate. serde_norway is already an env-tests dep.
- @operations OPS-A1/A2: covered by 4(a) and by distinct hard-fail branches.

**Late Gate-1 resolutions (@dry-reviewer A/B/c/d, @observability, @media-handler).**
- **A — successor home for the unreclaimable-floor prose.** Shape (ii): task 8's flip to required deletes `DEFAULT_MAX_TOTAL_EGRESS_EDGES`, but `MAX_TOTAL_EGRESS_EDGES_CEILING` survives (a required read still needs its hard ceiling for `parse_bounded`). The ~40 lines of ratcheting-floor prose, the retraction of the "never growth" framing, and the raising-is-not-the-remedy paragraph MOVE there. My ConfigMap anchor names the successor symbol, not just its own expiry. Recorded in §Notes for task 8 so @media-handler does not delete four consts without knowing three documents depend on one. The other three §8 consts carry little prose; their anchors name the surviving sibling the same way — but the four successor symbols are NOT uniformly named (`MAX_EGRESS_STREAMS_PER_MEETING_CEILING`, `MAX_CANDIDATE_SOURCES_PER_EGRESS_CEILING`, `MAX_TOTAL_EGRESS_EDGES_CEILING`, and `MAX_POLICY_APPLY_TIMEOUT_MS` with NO `_CEILING` suffix, `config.rs:161`). Each anchor is written from the symbol that exists, never from a `DEFAULT_X` → `X_CEILING` pattern; a mechanical rewrite would dangle exactly one of the four and read as uniform to a reviewer. The rename to `MAX_POLICY_APPLY_TIMEOUT_MS_CEILING` is deliberately NOT taken here: it is inside `crates/mh-service/` (@media-handler's domain), it is unnecessary for the citations to resolve, and task 8 is editing those same lines — renaming under an in-flight task buys uniformity at the cost of a conflict. @media-handler ACCEPTED the rename for task 8, which would have made my `MH_POLICY_APPLY_TIMEOUT_MS` anchor a site their rename must find — an unguarded manual-sync pair. RESOLVED by @dry-reviewer's better shape instead of tracked: all four anchors cite by **role plus location** ("the hard-ceiling constant for `MH_POLICY_APPLY_TIMEOUT_MS` in `crates/mh-service/src/config.rs`") rather than by bare symbol. This is the same move this ConfigMap's own keepalive CORRECTED block prescribes for runbook scenarios — refer by TITLE, which survives renumbering. It is unambiguous today (exactly one such constant per key), survives task 8's rename under either outcome, and needs NO cross-task coordination. Applied to all four, not just the renamed one: the argument does not depend on a rename being scheduled, and three of the four are being edited by task 8 anyway. The env var name a reader greps is the stable half; the symbol is the volatile half, so the citation keys on the stable one.
- **B — `docs/DEVELOPMENT.md:479` status line re-derived per service** in the same change: MH corrected 2026-09-23 with its full required set; MC still non-functional as written; the single "19 of 23" count is replaced by a per-service statement so the next block edit cannot silently falsify it.
- **(c) `MH_STREAM_COST_AUDIO_BPS` is provably inert today** — the ceiling divides by `max(cost_audio, cost_video)` and video dominates by ~28x, and admission counts streams rather than byte-accounting per kind. The comment says so, so an operator who tunes it and sees nothing change does not conclude the key is broken.
- **(d) Non-collapse recorded at BOTH sites**: the dt-guard rule checks declared manifests at layer 3; env-test (a) checks live cluster state at layer 7 and earns its keep on a cluster that does not match the manifests (hand `kubectl apply -f`, partial rollout). The rule doc adds that `key == env name` forecloses a deliberate name-differs pattern and was zero-violation at introduction, so a future exception is a debate rather than a mystery red.
- **@media-handler ruling on 90000: keep.** The comment cites ADR-0036 §4 as the source of the ~90 kbps nominal (no restated arithmetic), does NOT call it the wire cost, and records the residual coupling by naming `MC_AUDIO_MAX_BITRATE_BPS = 48000` (~110 kbps wire, so 90000 under-costs at MC's max), operative only when per-kind costing lands, reconcilable only as an ADR-0036 §4 decision. A one-line `docs/TODO.md` pointer tracks that reconciliation.
- **@observability**: the TODO §Observability Debt bullet also asks task 8 to REWRITE the justification comment above the four §8 log fields in `main.rs` (its "logged because all four are optional-with-default" rationale expires with the flip) rather than leave an expired justification. The test-load-bearing marker belongs at the `main.rs` emission site, not at `config.rs:160-178`.

**Handed-over `docs/TODO.md` entries (@observability, routed by @dry-reviewer).** Two entries land under §Observability Debt beside the formula-expiry line, text supplied verbatim; neither is task-2 work beyond pasting, and both were verified before acceptance:
- Entry 1 — scheduled edit of `infra/docker/prometheus/rules/mh-alerts.yaml`'s `MHMediaEgressQueueOverflowRate` description, trigger = the MH code task. Its "no such budget exists in this build" clause is TRUE today, which is why this task must not edit that file. The first clause (this alert IS the §1 queue bound) is permanently true; only the trailing clause expires, and it is REPLACED WITH A POINTER rather than deleted, because once a budget exists the disambiguation matters more. Verified: `docs/observability/alerts.md:831` paraphrases Condition/Impact and does not restate the sentence, so there is no lockstep second site.
- Entry 2 — requirement on the alerting task that `MHMediaEgressBudgetExhausted` compare gauge-to-gauge against the published threshold gauge, never a PromQL literal. The load-bearing part is that `validate-metric-coverage.sh` FAILS OPEN once the gauge lands on a dashboard panel, so no guard distinguishes "read by the alert" from "on a board, alert uses a literal".

**Validation.** `dt-guard env-config` both directions (rule 1 per workload; rule 3 orphan check satisfied because every key is referenced from both workloads), `kustomize build` of the Kind overlay, `./scripts/layer-fast.sh`.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired-test) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Media Handler (domain owner) | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead ruling: new `dt-guard` `configmap_key_name_mismatch` rule is IN scope (static layer of the env-test's wiring invariant; zero existing violations). Database owner not on panel — the DATABASE_SCHEMA.md edit is a one-line doc note whose content is dictated verbatim by the task (Minor-judgment, review-only).

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Configuration surface (`infra/services/mh-service/`)
| Item | Before | After |
|------|--------|-------|
| Egress-budget chain keys | absent | `MH_EGRESS_BUDGET_BPS` `10000000` (unsized placeholder), `MH_STREAM_COST_AUDIO_BPS` `90000`, `MH_STREAM_COST_VIDEO_BPS` `2500000`, `MH_EGRESS_REJECTION_RATIO_THRESHOLD` `0.05`, `MH_MAX_REGISTERED_MEETINGS` `1024`, `MH_MAX_MUTED_SOURCES_PER_MEETING` `512` |
| §8 policy keys | code defaults only, no manifest | `MH_MAX_EGRESS_STREAMS_PER_MEETING` `512`, `MH_MAX_CANDIDATE_SOURCES_PER_EGRESS` `16`, `MH_MAX_TOTAL_EGRESS_EDGES` `65536`, `MH_POLICY_APPLY_TIMEOUT_MS` `1000` — byte-equal to the code defaults |
| Refs | — | all 10 as hard `configMapKeyRef` (no `optional:`) on BOTH `mh-0` and `mh-1`; the two deployments still differ only in instance name |
| `MH_MAX_STREAMS` | "known-wrong, deferred" | RETIRED — PENDING REMOVAL; key and both refs kept for rollback safety; scheduled removal filed |
| Kind budget | inherits base | new `configmap-egress-budget-patch.yaml` → `100000000`; formula + N=3/N=5 requirement + env-test pointer, no ceiling literal |

### Guard (`crates/dt-guard/src/env_config.rs`)
New check 4, `configmap_key_name_mismatch` (FAIL token `env-config-configmap-key-name-mismatch-<n>-of-<m>-findings`): a `configMapKeyRef.key` must equal its env var name. Frozen repo-wide (zero violations at introduction), scoped to ConfigMaps only (the redis `REDISCLI_AUTH` secretKeyRef counter-example is in the rule doc). The crossed ref is still recorded as a reference, so its key is not misreported as an orphan. Three self-tests (positive control, negative control, per-instance). Failure-token row added to `docs/runbooks/devloop-validation.md` §6.3.

### Env-test (`crates/env-tests/tests/01_mh_deployment_config.rs`)
Four new tests over one shared key list and both instances: wiring + parse; staleness; startup-log parity for the four §8 keys; Kind ceiling vs `required_edges(5)`. `repo_root()` lifted into `crates/env-tests/src/lib.rs` (beside `NAMESPACE`) rather than copied from `33_alert_rules_loaded.rs`.

### Docs
`.env.example` + `docs/DEVELOPMENT.md` MH block corrected to MH's full required set (the DEVELOPMENT.md status line re-derived per service); `docs/DATABASE_SCHEMA.md` note; `docs/runbooks/mh-deployment.md` count claim and `MH_MAX_STREAMS` row; `docs/observability/dashboard-conventions.md` live-instance bullet and its header count; `docs/TODO.md` (see Accepted Deferrals).

---

## Files Modified

```
 .env.example                                       |  19 +-
 crates/dt-guard/src/env_config.rs                  | 235 ++++++-
 crates/env-tests/src/lib.rs                        |  15 +
 crates/env-tests/tests/01_mh_deployment_config.rs  | 696 ++++++++++++++++++++-
 crates/env-tests/tests/33_alert_rules_loaded.rs    |  10 +-
 docs/DATABASE_SCHEMA.md                            |   2 +
 docs/DEVELOPMENT.md                                |  63 +-
 docs/TODO.md                                       |  19 +-
 docs/observability/dashboard-conventions.md        |  12 +-
 docs/runbooks/devloop-validation.md                |   1 +
 docs/runbooks/mh-deployment.md                     |  12 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |   4 +-
 .../mh-service/configmap-egress-budget-patch.yaml  |  57 ++   (new)
 .../kind/services/mh-service/kustomization.yaml    |   8 +
 infra/services/mh-service/configmap.yaml           | 350 ++++++++++-
 infra/services/mh-service/mh-0-deployment.yaml     |  76 +++
 infra/services/mh-service/mh-1-deployment.yaml     |  76 +++
 17 files changed, 1593 insertions(+), 62 deletions(-)
```

---

### Lead Gate-2 record

- Attempt 1 (not counted): L3 `validate-cross-boundary-scope` FAIL + L7 `PRECONDITION_FAILURE cluster-rebuild-failed`. Cause: implementer edited `setup.sh` / plan table while the pipeline (and host-side `setup.sh`) was running. Operator-lane; tree frozen, re-run.
- Attempt 2 (frozen tree, `git diff` hash unchanged across run): `LAYER=1..3,5,7 OK`, `LAYER=4,6 N/A` (aggregate only; cargo-test/clippy/audit lanes OK), `TOTAL_RESULT=N/A`, exit 0. L7: `env-tests-passed`, `browser-e2e-passed`; `01_mh_deployment_config` 6/6 passed incl. `test_kind_egress_stream_ceiling_meets_demo_requirement`, `test_story2_keys_wired_into_every_mh_instance`, `test_policy_bounds_reported_by_running_mh_match_configmap`, `test_mh_pods_started_after_configmap_last_changed`. Duration 1386s.

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

### Gate 3 — Verdicts (Lead)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 | S-1..S-3; ruled MH_MAX_REGISTERED_MEETINGS=8192 w/ media-handler |
| Test (paired) | CLEAR | 0 | 0 | 0 | paired fixes landed pre-gate |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | O-1, O-2 |
| Code Quality | CLEAR | 0 | 0 | 0 | 1 withdrawn (stale read) |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | DRY-3 = TODO extraction record |
| Operations | CLEAR | 0 | 0 | 0 | |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |
| Media Handler (domain owner) | RESOLVED-FIXED | 1 | 1 | 0 | 1024→8192; "until R-20" residual fixed pre-final-gate |

Final Gate 2 (attempt 4, frozen tree, hash verified unchanged across run): L1-3,5,7 OK; L4,6 N/A aggregate; `TOTAL_RESULT=N/A` exit 0; env-tests + browser e2e passed; `01_mh_deployment_config` 6/6. Attempt 3 was invalidated by a concurrent implementer `layer-fast` sharing `/tmp/devloop` (operator lane, not counted).

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

**S-1 — half-applied hard-ceiling invariant.** The §8 block states that each of its four keys has a hard-ceiling constant in `config.rs` (verified present), but the two NEW resource guards — `MH_MAX_MUTED_SOURCES_PER_MEETING` and `MH_MAX_REGISTERED_MEETINGS` — recorded no such requirement. The missing half is the sharper one: the muted bound sits in front of an allocation, so without a ceiling a fat-fingered "raise the limit" during an incident is not a loosened guard but NO guard. FIXED: both keys now carry the requirement using this file's existing REQUIREMENT-ON-THE-MH-CODE-TASK device, with the muted one stating why the ceiling is what keeps it a pre-allocation guard. Ceiling values left to the MH code task, as the existing four were. Security's secondary note (no upper bound on `MH_EGRESS_BUDGET_BPS`) was explicitly not a finding and is left as a recorded decision: it is a genuine capacity knob, backstopped by the §8 bounds and `MH_MAX_CONNECTIONS`.

**S-2 — the direction correction left a contradiction.** After DRY-1 corrected the muted/egress direction in the egress block, the muted key's OWN block still read "BOUNDED BY that key … the two may legitimately diverge" — the opposite direction, sitting on the key an editor is most likely to be reading. FIXED: that block now states "THE DIRECTION IS LOAD-BEARING: this value must be AT LEAST `MH_MAX_EGRESS_STREAMS_PER_MEETING`", names the consequence, and cites the env-test. Security also contributed the 512/512 rationale — security wants the muted bound as small as possible (it sizes an allocation against a caller-controlled count), correctness wants it at least the egress bound, and 512 is the tightest value satisfying both, so the equality is an answer rather than a coincidence and a change to either key must move both.

**S-3 — a retracted premise in the instruction position.** My "REVISIT WHEN R-20 LANDS … both owners' rulings converge on `MH_MAX_TOTAL_EGRESS_EDGES / 2`" paragraph encoded a premise both owners had since retracted, in the one paragraph a reader would act on. Verified before fixing (`docs/TODO.md` §Media Path Obligations, the `EndMeeting`-has-no-fence entry): `EndMeeting` carries no generation, so a late `RegisterMeeting` re-creates the meeting and reclamation rests on an MC-side quiesce MH does not enforce — from the principal this key bounds. FIXED: the paragraph now says R-20 landing does NOT make the bound concurrent, 8192 stands, and the honest revisit trigger is "when the fence and resurrection questions are closed". See §Issue 8 for the recurring mechanism.

### Test Specialist
**Verdict**: CLEAR (as sent by @paired-test to the Lead: 0 Gate-3 findings; the two items below were paired-collaboration fixes during implementation, not Gate-3 findings)
**Findings**: 0 at Gate 3

Bits-vs-bytes agreement was a numeric claim about deployed values held only by a comment → the test now computes the ceiling in BOTH conversion orders and holds the requirement against the smaller. The staleness check's precondition held only on the devloop path → `setup.sh` now restarts a service when `apply -k` changed one of its ConfigMaps in place, for all four services. Both detailed in §Issues 7.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

**O-1 — the retracted framing arrived by a new route.** `MH_MAX_REGISTERED_MEETINGS` was described as bounding meetings held "at once" and sized against "concurrent-meeting load". Verified independently before fixing: `SessionState.registered_meetings` is insert-only (`insert` at `session/mod.rs:525`; the `.remove(` at :532 is on `pending_connections`, not this map), and `insert` overwrites on re-registration — so the bound is consumed CUMULATIVELY by every distinct meeting ever registered, rising with uptime, exactly the property filed against its sibling. "Sized well above concurrent load" is the framing `docs/TODO.md` retracted as false-in-every-clause, reintroduced in different words on a key with the same defect. FIXED: the concurrency language is gone, the R-20 conditional now matches the sibling's wording, and "Where an operator sees it" states the gauge counts registrations held rather than meetings in progress and does not fall until teardown — the direction that otherwise hides exhaustion. Observability's consequence 3 (1024 is a tighter cumulative bound than the edge bound, so it exhausts first) is routed to @media-handler and @security as new information, not closed by the comment fix.

**O-2 — "SINGLE HOME" falsified by this diff.** The egress block claimed to be the single home for every number in it while the same diff added anchored mirrors in `.env.example` and `docs/DEVELOPMENT.md`. FIXED to claim AUTHORITY rather than uniqueness, naming both mirrors and stating this file wins. Observability later amended the severity (true-but-brittle rather than false, since an anchored mirror is the repo's convention, not a competing home) and asked that the clause not read as an apology for the mirrors — it does not: it names where the numbers appear, which wins, and that neither mirror is a consumer.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 1 raised and WITHDRAWN by the reviewer, 0 outstanding

Reported 6 files missing from the Cross-Boundary table. Verified mechanically: all 19 changed files have rows, and `comm` against `git status` shows no changed file absent and no row without a change. The rows had been added earlier in the round as `validate-cross-boundary-scope` caught them (§Issue 6), which is also why that guard passes. The reviewer re-read the table and withdrew the finding as a stale read — caused by my editing `main.md` mid-review rather than by a gap. No code-level findings: Rust idioms, ADR-0002 scoping, the vacuity controls in both the guard unit tests and `setup.test.sh`, and the TODO citations all verified clean.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- **DRY-1 — the cross-reference named the SAFE direction and was silent on the harmful one.** The muted bound's "can never falsely reject a legitimate snapshot" claim requires `muted >= egress`. My pointer said *lowering* the egress bound means revisiting the muted one — but lowering is harmless; RAISING egress above muted is what breaks it, silently rejecting legitimate registrations so server mute stops working for large meetings. FIXED both halves: the direction is corrected, and — per this task's own mechanism statement, which I had applied to the ceiling but not to this — the relationship is now ASSERTED in `assert_story2_keys_wired` rather than narrated. Mutation-checked against the live cluster: raising the deployed egress bound to 1024 fires the assertion naming both values; restored byte-identically.
- **DRY-2 — this diff falsified two clauses of the TODO entry it half-closes.** `docs/TODO.md:295` still said site 4 "carries no `ANCHOR (DRY)` comment", which this diff makes false, and its fix-list step (a) is now done for MH. FIXED: the clause is dated and scoped per service, and (a) is marked done-for-MH / open-for-MC-GC.

**Extraction opportunities** (appended to `docs/TODO.md`):
- `docs/TODO.md` §Cross-Service Duplication (DRY), entry :295 — extended with DRY-3: site 4 now mirrors ~30 VALUES and not only names; the entry's proposed check (b) is presence-only by construction and would go green on a doc listing every key at wrong values; and the deliberate `MH_EGRESS_BUDGET_BPS` exception (docs mirror the Kind overlay, not the base placeholder) that a naive value-equality guard would red on immediately and must not "fix".

### Media-Handler (conditional domain-owner reviewer)
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

**MH-1 — `MH_MAX_REGISTERED_MEETINGS` was sized on an invalidated model.** Its Gate-1 approval of 1024 rested on "well above concurrent-meeting load", and the bound is not concurrent (see O-1). Raised to a value decision rather than a comment fix, and ruled jointly with @security. Resolution: **1024 → 8192**, landed with all three mirrors in sync (`configmap.yaml`, `.env.example`, `docs/DEVELOPMENT.md`; the env-test reads the deployed value and hardcodes nothing).

The reasoning, because the number alone does not carry it: the binding axis is not memory but APPLY COST. `RoutingTable::install` is the only apply path and calls `RoutingSnapshot::with_policy`, which clones the whole meetings map per apply — verified in `crates/mh-service/src/routing/mod.rs` — so each retained entry is O(1) to create and then taxes every subsequent registration, becoming O(M²) steady-state once §8's periodic re-assert lands. Both owners' conditions resolve to 8192 unconditionally, because the bound stays cumulative even after R-20: `EndMeeting` has no fence against a late `RegisterMeeting`, so teardown-then-resurrect regrows the map without tripping the edge bound. The accepted consequence, stated in the comment: at 8192 this bound is deliberately tighter in meeting-equivalents than the edge bound and preempts it for small meetings — a capped apply-cost tax preferred over an unbounded quadratic one with no saturation signal.

### Operations Reviewer
**Verdict**: CLEAR (as sent by @operations to the Lead)
**Findings**: 0 at Gate 3 (OPS-A1/A2 and the four Issue-7 constraints resolved pre-gate)

Its OPS-F removal ordering was improved by operations itself into the `optional: true` retirement that needs no guard change (§Issue 4), and the four Issue-7 constraints are recorded at the sites (§Verification notes).

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found, 0 fixed, 0 deferred

No findings. Spot-checked the load-bearing comment claims (code defaults, `gc_client` sends, migration default, the retracted-framing docstring, `setup.sh` restart behaviour, the env-test's live-value derivation) as truthful, and confirmed the retired `MH_MAX_STREAMS` comments read as retired rather than live.

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

---

## Issues Encountered & Resolutions

### Issue 1: Staleness check would have failed on every run (`managedFields` stripped)
**Problem**: Verified read-only against the live Kind cluster before relying on it. `kubectl get -o json` strips `metadata.managedFields` unless asked, so the ConfigMap's last-change time came back as an empty list and the staleness check would have failed on every run — loudly, but wrongly.
**Resolution**: `fetch_configmap` passes `--show-managed-fields`, with a comment marking it load-bearing, and the empty-list failure message names the flag. Re-verified live: ConfigMap last changed `00:07:17Z`, pod started `00:11:15Z`, check passes in the right direction.

### Issue 2: Two citations true today but false after the MH code task
**Problem**: The `MH_MAX_TOTAL_EGRESS_EDGES` comment said the reasoning is "stated once, at the hard-ceiling constant" — but today it lives on `DEFAULT_MAX_TOTAL_EGRESS_EDGES`, which task 8 deletes. `docs/DATABASE_SCHEMA.md`'s note said GC binds "the stream ceiling MH derives", which is only true once task 8 ships.
**Resolution**: Both rewritten so they are true in BOTH states — the comment names the current symbol (as the task requires) and the move; the schema note says GC binds whatever `max_streams` MH advertises. A forwarding note was added to the open reclamation entry in `docs/TODO.md`, the one other live citation of that const.

### Issue 3: "Where an operator sees it: nowhere directly" was false for two §8 bounds
**Problem**: Checked the claim against `grpc/mh_service.rs` instead of assuming. An over-limit `egress_streams` or `candidate_sources` IS counted, as `mh_media_policy_applies_total{outcome="rejected_invalid"}`, with a WARN whose `reason` names the bound.
**Resolution**: Both blocks now name that signal, and note that the counter alone does not say which bound tripped. Also confirmed that the total-edge cap surfaces as `apply_failed` (`EdgeCapExceeded`), as that block states.

### Issue 4: The scheduled two-deploy removal conflicts with the guard
**Problem**: The removal operations specified — deploy (A) drops the refs, deploy (B) drops the key — leaves the key as a ConfigMap entry with no referencing workload between A and B. That is exactly what `dt-guard env-config` rule 3 fails as `orphan-key`, so the tree (A) must ship cannot pass Layer 3 as the guard stands. Both rules are correct; they were never designed against each other.
**Resolution**: @operations found a third resolution that needs neither a guard change nor a red window, and it is now the PREFERRED ordering in the `docs/TODO.md` entry. The guard never reads `optional` (verified: its only occurrence is an unrelated test fixture), so: (1) flip both refs to `optional: true` — guard green, no runtime change; (2) later, delete key and refs together — guard green, and `rollout undo` lands on revision (1), whose OPTIONAL refs start the container against a ConfigMap without the key. The rule was restated precisely — *never delete a ConfigMap key while a HARD ref to it is reachable by `rollout undo`* — because the coarse "never delete key and refs together" is what hid this. My two options are kept as fallbacks for a future retirement of a key that is still READ, where an optional ref would change behaviour. Every statement of the old ordering (both deployment comments, the `mh-deployment.md` row) was updated.

### Issue 5: `test-rigidity` rule 5b on a log-line classifier
**Problem**: Layer 3 flagged the `Ok(v) =>` arm in the startup-log parser as an assertion-free match arm. It was a line classifier rather than an outcome acceptance, and the pass condition is fully asserted afterwards — a lexical false positive.
**Resolution**: Did NOT weaken the guard's matcher. Re-expressed the classification with `filter_map`, and documented in place why tolerating an individual non-JSON line cannot produce a vacuous pass (passing requires exactly one parsed event whose four fields equal the deployed values; an all-non-JSON stream has its own branch). Re-verified live that the parser still reaches and passes every log branch.

### Issue 6: Three edited files missing from the Cross-Boundary table
**Problem**: `validate-cross-boundary-scope` caught `crates/env-tests/src/lib.rs`, `33_alert_rules_loaded.rs` (from lifting `repo_root()`) and `docs/runbooks/devloop-validation.md` (the new token row).
**Resolution**: Rows added — the two env-tests files as Minor-judgment with owner test (paired), the runbook row as Mine.

### Issue 7: Two things held only in prose (raised by @paired-test)
**Problem**: (a) The ceiling comment asserted the bits-order and bytes-order ceilings agree "while every deployed value is a multiple of 8" — a numeric claim about deployed values held by a comment, the pattern this task exists to remove. (b) The staleness check's precondition (pods restart after a ConfigMap change) held only on the devloop path: `setup.sh` restarted MC and MH only inside the `DT_HOST_GATEWAY_IP` arm, and AC and GC never.
**Resolution**: (a) The test computes the ceiling in BOTH orders and holds the requirement against the smaller, so it is true whichever order the MH code task uses and imposes no multiple-of-8 rule on operators; a byte-cost of zero gets its own failure. The same prose claim was removed from the TODO entry. (b) Rather than move the restart out of the `if` — which would churn every pod on every fresh bring-up and every idempotent re-run — `apply_reports_configmap_changed` restarts a service only when `apply -k` reported one of its ConfigMaps as `configured` (changed in place); `created` and `unchanged` need no restart. The kubectl output shape was verified with a server-side dry run. Applied to ALL FOUR services, not just the MC/MH siblings, because the mechanism is identical for AC and GC and costs nothing when nothing changed. MC and MH fold their devloop-path restart into one decision, so that path still restarts exactly once. Seven hermetic tests pin the decision table (`scripts/setup.test.sh`: 99 pass). **The first mutation check was vacuous** — the `sed` never matched, so both "mutants" were the original file; caught by the zero match count and redone with an exact-string replacement that asserts it applied: always-false fails the 2 must-restart cases, always-true fails the 5 must-not-restart cases; restored byte-identically.

**@operations' four Issue-7 constraints, all recorded at the sites** (verified, not asserted): (1) the script NARROWS the staleness window and does not close it — a hand `apply -k`, which `mh-deployment.md` tells operators to run, bypasses it; the env-test staleness branch is the fail-closed backstop for every path the script does not own. (2) The detector is FAIL-OPEN, acceptable only because of (1); both the helper comment and the test header now say each is the other's justification, so neither can be deleted citing the other. (3) Scope verified by inspection: only `deploy_{ac,gc,mc,mh}_service` call the helper — `postgres`, `redis`, observability and the collector do not, deliberately (state and connection pools, and a restart can race migration or seeding). (4) A "do not port this to a production deploy path unchanged" note is at the helper: outside Kind an MH restart drops every live WebTransport session that pod carries, so a ConfigMap value edit would silently become a media outage.

**Residuals, stated rather than assumed (@paired-test)**: (1) `apply_reports_configmap_changed` matches kubectl's own `configmap/<name> configured` wording; if kubectl rewords it, the helper silently stops restarting. MH is backstopped by env-test (d), which reds; AC, GC and MC have no staleness check, so for them it would fail silent. (2) A run that dies between the apply and the restart leaves stale pods, and the next run sees `unchanged`. Neither is a regression — there was no restart at all on the host path before — and both are also written into the helper's comment in `setup.sh`. (3) Nothing in the pipeline shellchecks `setup.sh`, and shellcheck is not installed here; the change passes `bash -n` plus hermetic tests shown to fail when the helper is broken in either direction.

### Issue 8: A retracted premise reached an authoritative comment twice (@security's process note)
**Problem**: Two of security's Gate-1 framings were later retracted by their author, and both had already been written into `infra/services/mh-service/configmap.yaml` as authoritative operator-facing text. S-2 was the muted/egress direction ("bounded by … may legitimately diverge", when the requirement is `muted >= egress`). S-3 was worse: my "REVISIT WHEN R-20 LANDS … both owners' rulings converge on `MH_MAX_TOTAL_EGRESS_EDGES / 2`" paragraph encoded the retracted premise in the INSTRUCTION position, where a reader would act on it — raising the value to 32768 the day R-20 merged, citing a convergence that no longer existed. The block simultaneously stated the correct fact (reclamation is partial) and the retracted one, with the retracted one being the actionable half.
**Resolution**: Both fixed in the file. The mechanism is worth stating, because it is not a one-off: **a retraction that lands only in the review thread is not a retraction.** The comment is the artifact that outlives the conversation, so a premise withdrawn in discussion has to be chased into every comment it reached. This file already carries three dated CORRECTED blocks for the same reason; these two would have been the fourth and fifth. Practical consequence for this devloop: when a reviewer retracts a premise, grep the diff for it rather than assuming the conversation closed it.

### Verification notes (scratch runs, recorded as agreed with @paired-test)
- **Guard positive control on a REAL manifest**: temporarily crossed `mh-1-deployment.yaml`'s `MH_STREAM_COST_VIDEO_BPS` onto key `MH_STREAM_COST_AUDIO_BPS` → `STATUS=FAIL REASON=env-config-configmap-key-name-mismatch-1-of-1-findings`, naming workload mh-1 and both names; reverted byte-identically (`cmp`) → `STATUS=OK REASON=env-config-clean-4-services-6-workloads`.
- **Env-test non-vacuity against the UN-deployed cluster**: the three tests that need the new keys each fail on the correct "input absent" branch, naming the key, with no log line or data map in any message. `test_policy_bounds_reported...` got past every log branch on the real MH image (one event per pod, all four fields present as integers) before failing on the absent key — proving the log contract holds today.
- **Pass path checked offline against the rendered overlay** (deploying is Gate 2's): ConfigMap carries `environment=kind`; ceiling 40 ≥ 30; budget and both costs are multiples of 8 (bits/bytes agreement holds); all 11 refs on both instances are hard, `key == name`, none optional.

---

### Scheduled later-task obligations (not deferrals)
Filed in `docs/TODO.md` §Media Path Obligations (MH_MAX_STREAMS two-deploy removal; audio cost vs MC ceiling), §Observability Debt (Kind ceiling formula expiry; `MHMediaEgressQueueOverflowRate` clause expiry; egress alert gauge-to-gauge), §Cross-Service Duplication (DRY) (DRY-3 value-mirror record). Every reviewer reported 0 deferred findings.

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
