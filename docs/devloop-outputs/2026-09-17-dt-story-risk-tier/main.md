# Devloop Output: Risk-tier the dt-story manifest (ADR-0037 D2)

**Date**: 2026-09-17
**Task**: Add a `tier` field to the dt-story Task manifest that gates the Gate-1 planning round (ADR-0037 D2)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/devloop-improvements-and-story-2-taskb`
**Duration**: ~1h10m (setup 18:52 → commit ~20:10 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b8e527470294b90d60e7c70ef8d250f96540721c` |
| Branch | `feature/devloop-improvements-and-story-2-taskb` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (Gate 1 PASSED; Gate 2 PASSED; Gate 3 PASSED 2026-09-17 — all 6 verdicts CLEAR/RESOLVED-FIXED) |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned (ADR-0037 D4 — no check surface)` |

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (4 conditions → constraints 15/16/17/18) |
| Test | confirmed (5 conditions → constraint 10 + cli.rs A/B) |
| Observability | confirmed (O1/O1b/O2/O3 → constraints 11/12/13/14) |
| Code Quality | confirmed |
| DRY | confirmed (P1 reversed→keep floor; P2/P3/P4 → constraints 20/21/22) |
| Operations | confirmed (MUST-1/2/3 → constraints 11/23/24; SHOULDs → 25) |

**Gate 1 result: PASSED.** All 6 confirmed; classification-sanity guard `STATUS=OK` (all rows Mine, no GSA). Floor settled to `^(full|light)$` (constraint 19). `null`→exit-2 (security over ops' full+warn). Semantic-guard not spawned (ADR-0037 D4). "Plan approved" issued to @implementer.

**Non-blocking items carried to implementation / Gate 3:**
- (DRY) Constraint 23 should REFERENCE §Lightweight Mode's "Not eligible" exclusion list (`.claude/skills/devloop/SKILL.md` ~198-205) rather than re-copy it — same-file collapse prevents drift (an exclusion added there would otherwise leave the tier backstop skipping Gate 1 on that surface). Keep the GSA clause spelled out.
- (Ops → Gate 3) Constraint 24's SKILL-side refusal must REFUSE an unrecognized `--tier`, not coerce to full — verify wording at Gate 3, resist any later softening to "default to full".
- **RESOLVED (security ruling, supersedes the "untested by design" plan-of-record):** constraint 18's run-story `null`→exit-2 shell branch IS tested — a fail-loud path must not be untested code. Test: `DT_STORY` fake emitting a TIER-LESS `next` payload (real stale-binary reproduction, not bogus-enum), assert exit 2, distinct `INVALID-TIER` token (mirror `INVALID-SPECIALIST`), `assert_no_marker`. Both null-defense layers (shell floor + constraint-24 SKILL refusal) are now covered.

### Gate 2 result: PASSED (2026-09-17)
Full pipeline via `scripts/layer-all.sh`, then affected-layer re-run (1/2/3/4/5) after the Gate-3 fix batch. All layers OK on substance; zero failures pipeline-wide incl. Layer-7 env-tests + browser-E2E (10/10) on the first full run. `TOTAL_RESULT=N/A` is the benign aggregation artifact of two documented-N/A layers (Layer-4 proto `not-applicable-to-this-lang` test-gap; Layer-6 `no-dep-changes`), verified against each layer's substance — NOT a failure or unevaluated code. **Environment note (not the diff):** buf/proto codegen failed on the first run because `/home/dev/.cache` is root-owned (uid mismatch); worked around with `BUF_CACHE_DIR` redirected to scratchpad (operator lane, did not consume an attempt). Container-provisioning follow-up for operations/infrastructure — it will fail every proto/TS gate in this container. One diff-caused Layer-3 failure was found and fixed (a bare line-number doc citation `manifest.rs:158-176` in guarded SKILL prose).

### Gate 3 result: PASSED (2026-09-17)
| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 |
| Test | CLEAR | 0 | 0 | 0 |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 |
| Code Quality | CLEAR | 0 | 0 | 0 |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 |
| Operations | RESOLVED-FIXED | 5 | 5 | 0 |

Zero deferrals, zero escalations — no RESOLVED-DEFERRED, so §Accepted Deferrals is empty. Notable Gate-3 catches (all fixed in-diff): security — SKILL wording implied a Gate-1 GSA guard runs on the light path (it's skipped); operations F-1 — the `INVALID-TIER` runbook message rendered "A 'null' of 'null'…" (broken interpolation); DRY-2 — an unused `Tier::Display` that was a third unguarded copy of the tokens, plus a vacuous literal-vs-literal "pin" test, both DELETED. **ADR-0037 D4 data point:** operations F-2 (comment-vs-code drift) is a semantic-guard-class catch, found here by the standing quartet at zero extra seat cost — evidence D4's "skip semantic-guard, quartet backstops" bet held on this task.

**Observability's Gate-1 must-fixes (in-scope; implementer to fold into plan before "Plan approved"):**
- **O1 (must — legibility)**: record the tier in `docs/devloop-outputs/<slug>/main.md` Loop State, plus an explicit `Gate 1 — SKIPPED (tier=light, ADR-0037 D2)` marker, so a skipped Gate-1 is distinguishable from an abandoned/interrupted one. `docs/devloop-outputs/_template/main.md` Loop State has no Mode/Tier row today — add it to the template too.
- **O1b (must — correctness)**: a light task interrupted AFTER the Gate-1 skip must not silently resume as full. `/devloop` Continue Mode + the headless resume lane re-derive mode from flags that carry no tier (SKILL.md ~704 says treat Loop State as authoritative), so without O1 the resumed devloop reruns the plan round it was meant to skip. Recording the tier + Gate-1-SKIPPED in Loop State is what makes constraint 6's "tier need not be re-passed on the resume lane" TRUE — the resumed devloop must read tier from Loop State. Handle explicitly in the SKILL edit.
- **O2 (must — cheap)**: add `tier=` to the `STORY_RUN: START` log line (run-story.sh:2044), alongside `specialist=`.
- **O3 (should / deferrable-with-TODO if task-sized)**: add `tier` to the cost-ledger entry in `report_task_cost` (the host-persistent D5 record) so the D2 trial has a durable per-task tier that doesn't depend on joining against a post-run-mutable manifest.
- **O4 (confirm-only)**: keep constraint 7's "light implementer plans inline **into main.md**" explicit — that Planning + files table is the Layer-A scope-drift baseline the classification-sanity net keys off.

**Test's Gate-1 conditions (carry to implementation):**
1. `list_tasks_projects_exactly_four_keys_in_manifest_order` (cli.rs:613) asserts EXACTLY `[deps, id, slug, status]` — adding `tier`/`tier_reason` to the projection MUST update this test + its name, or the suite reds at Gate-3.
2. Backward-compat must be a real check: run `dt-story validate` against the actual story-1 manifest (`docs/user-stories/2026-08-27-hear-yourself-through-handler.md`) and record it as a Gate-2 verification step.
3. "Full omits `tier:`" and light-rejection tests must inspect serialized bytes / assert the specific violation over an OTHERWISE-VALID manifest — not parse-back-only, not non-zero over an already-broken manifest.

---

## Task Overview

### Objective
Add a `tier` field (`full` | `light`, default `full`) plus optional `tier_reason` to the
dt-story Task manifest so the story runner can tell each `/devloop` whether to run the full
Gate-1 planning panel (`full`) or skip it and plan inline (`light`). Governing design:
`docs/decisions/adr-0037-devloop-cost-tuning.md` §D2.

### Scope
- **Service(s)**: `crates/dt-story/` (schema + CLI + tests); `scripts/workflow/run-story.sh` (read/gate); `.claude/skills/{devloop,user-story}/SKILL.md` (gate + emit prose).
- **Schema**: dt-story *manifest* schema (NOT a DB schema/migration). Backward-compat mandatory.
- **Cross-cutting**: touches the story-runner + two SKILLs, all infrastructure-owned.

### Debate Decision
NOT NEEDED — governing design already exists (ADR-0037 §D2, a trial ADR being exercised on this story).

---

## Cross-Boundary Classification

Per ADR-0037 D1, only Domain-judgment cross-boundary edits and Guarded Shared Areas earn a
classification row. **Every file this task touches is infrastructure-owned (dt-story crate,
run-story.sh, the two SKILLs, the story template).** No GSA path (`proto/**`,
`crates/common/src/{jwt,secret,…}`, `db/migrations/**`, etc.) is touched. Therefore: no
classification rows, no owner-confirmation, no Ownership Lens verdicts. Implementer still lists
every planned file below for the Layer-A scope-drift guard.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-story/src/manifest.rs` | Mine | — |
| `crates/dt-story/src/engine.rs` | Mine | — |
| `crates/dt-story/src/main.rs` | Mine | — |
| `crates/dt-story/tests/cli.rs` | Mine | — |
| `scripts/workflow/run-story.sh` | Mine | — |
| `.claude/skills/devloop/SKILL.md` | Mine | — |
| `.claude/skills/user-story/SKILL.md` | Mine | — |
| `scripts/workflow/run-story.test.sh` | Mine | — |
| `docs/devloop-outputs/_template/main.md` (Observability O1: Tier row in Loop State) | Mine | — |

**Decided NOT touched** (noted, not in the diff): `docs/user-stories/_template.md` — its skeleton stays tier-less, which is valid and round-trips (no `tier` ⇒ full); the authoritative emission rule lives in `/user-story` Step 10.4, not the illustrative snippet. `scripts/guards/simple/validate-story-manifest.sh` — it only shells `dt-story validate`, which inherits the new Light⇒reason rule for free; backward-compat (a tier-less story-1 validating clean) needs no guard change.

(Implementer: adjust this list to the actual diff before Gate 1 — it is the scope-drift baseline.)

---

## Planning

### Implementer approach + ADR/task details interpreted

Approach maps 1:1 onto the 10 authoritative constraints below. Three points required
interpretation (raised to reviewers, none contradict the ADR/task):

1. **`clap::ValueEnum` derived on the lib type `manifest::Tier`, not a `main.rs` mirror.**
   clap is already a crate-wide dependency (used by the binary today). Deriving `ValueEnum`
   on the single `Tier` type gives `--tier <full|light>` help/validation without a second
   enum that could drift from the storage type. Trade-off: the domain type gains a CLI-framework
   derive. Chosen over a mirror enum to preserve single-source-of-truth for the tier values.
   (`Slug` is parsed via `FromStr` at the clap boundary; `Tier` uses `ValueEnum` because it is a
   closed set, which is what the plan asked for.)
2. **The run-story `^(full|light)$` floor is doing TWO jobs: interpolation-safety AND
   presence/staleness detection — and its failure mode IS reachable (corrected).** An earlier draft
   called it "structurally unreachable"; that is true only for a *malformed* tier in a real manifest
   (serde fails the whole file → `next` exits 2 first). It is FALSE for a *missing* one: a
   `dt-story` binary predating the `tier` field emits a `next` payload with no `tier` key, so
   `jq -r .tier` yields the literal string `null`, and this floor is what stops `--tier=null`
   reaching the `/devloop` line. `null` MUST exit 2, never coerce to full (fail-loud, not mask — a
   `null` at the JSON layer can ONLY mean a stale binary, since a current dt-story always serializes
   a concrete `tier` on the non-`Option` `RunnableTask` field). Negative coverage splits by
   reachability (constraint 18, revised): a bogus ENUM value is serde-closed → covered at the serde
   layer by cli.rs (constraint 10 A), NOT a shell test; the NULL/missing-tier path IS reachable via
   real version skew → covered by a run-story.test.sh case driving a stale-binary-simulating
   `DT_STORY` fake (tier-less `next` JSON) that asserts exit 2 + a distinct `INVALID-TIER` token.
3. **`_template.md` and `validate-story-manifest.sh` are NOT touched** (see Cross-Boundary table
   note). Template skeleton stays tier-less = full and round-trips; the guard only shells
   `dt-story validate`, which inherits the Light⇒reason rule for free.

The validation-rule substitution (Light ⇒ `tier_reason` non-empty, vs the ADR's
"downgrade needs a reason") is carried as the task states — a deliberate, manifest-checkable
simplification, not ADR drift.

### Observability Gate-1 must-fixes — ACCEPTED, folded into the plan

All five accepted (O3 too, not deferred). These extend constraints 6–7 and add one file
(`docs/devloop-outputs/_template/main.md`) to the diff:

- **O1** — /devloop SKILL records `Mode`/`Tier` in main.md Loop State and, when tier=light,
  emits an explicit `Gate 1 — SKIPPED (tier=light, ADR-0037 D2)` marker (distinguishable from
  abandoned/interrupted). Add the `Tier` row to `docs/devloop-outputs/_template/main.md` Loop State.
- **O1b** — /devloop Continue Mode + headless resume lane read tier from Loop State, so a light
  task interrupted after the Gate-1 skip does NOT resume as full and rerun the round. This is what
  makes constraint 6's "tier not re-passed on the resume lane" correct; stated explicitly in the SKILL.
- **O2** — `tier=` added to the `STORY_RUN: START` log line (run-story.sh:2044), beside `specialist=`.
- **O3** — `tier` added to the `report_task_cost` cost-ledger entry (the D5 host-persistent record),
  so the D2 trial has a durable per-task tier not dependent on a post-run-mutable manifest join.
- **O4** — confirmed: "light implementer plans inline INTO main.md" stays explicit (the Planning +
  files table is the Layer-A scope-drift baseline).

### Test Gate-1 conditions — ACCEPTED (carried to implementation)

1. Update + rename `list_tasks_projects_exactly_four_keys_in_manifest_order` (cli.rs:613) — the
   closed-world key set goes 4→6 (`deps,id,slug,status,tier,tier_reason`).
2. Backward-compat is a real Gate-2 step: `dt-story validate docs/user-stories/2026-08-27-hear-yourself-through-handler.md` exit 0.
3. "Full omits `tier:`" and light-rejection tests inspect serialized bytes / assert the specific
   violation over an OTHERWISE-VALID manifest (not parse-back-only, not non-zero over a broken one).

Updated file list (supersedes the Cross-Boundary table's for scope-drift): the 8 files there PLUS
`docs/devloop-outputs/_template/main.md` (O1). `_template/main.md` is an infrastructure-owned devloop
output template — no new cross-boundary surface.

### Design constraints carried from the ADR + task (authoritative)

1. **Schema (`manifest.rs`)**: `Tier` enum `{ full, light }`, serde `rename_all = "lowercase"`,
   `#[serde(default)]` → `Full`. Field `tier: Tier` on `Task` with `#[serde(default)]` and
   `skip_serializing_if` = "is full" so existing manifests (no `tier`) round-trip **byte-identical**
   and a Full task never writes `tier: full`. Add `tier_reason: Option<String>`
   (`skip_serializing_if = "Option::is_none"`). Field order = serialization order; append at the
   end so existing fields' order is undisturbed.
2. **Validation rule** (in `validate_manifest`, applies to every task regardless of status):
   `tier == Light ⇒ tier_reason present and non-empty (after trim)`. Fail-loud. This is the
   guardable invariant "any planning skip carries a stated reason". **Note (ADR traceability):**
   the ADR D2 wording is "author may only *downgrade* with a recorded reason"; the task
   deliberately substitutes the simpler, manifest-checkable rule above (a downgrade-needs-reason
   rule needs deliverable-type the manifest doesn't hold). This is a *stated* task decision, not an
   invention — reviewers should confirm the substitution is sound, not treat it as ADR drift.
3. **No new regex dependency** for any new pattern check — match `SLUG_PATTERN`'s hand-rolled
   style. (The tier *value* is validated by serde enum deserialization = fail-loud exit 2; the
   reason check is a `.trim().is_empty()`, no pattern needed.)
4. **CLI (`main.rs`)**: `add-task` gains `--tier <full|light>` (clap `ValueEnum`, default full) and
   `--tier-reason <text>` (`Option<String>`), threaded through `NewTask`. `list-tasks` `TaskSummary`
   gains `tier` (always-present key) and `tier_reason` (`null` when absent) — document the named
   consumer (run-story gating + humans) per the struct's rule 1. `validate` inherits the rule for
   free. A `--tier light` with no reason is refused at `add-task` time via the existing delta-
   validation (before/after `validate_manifest`) — verify this holds.
5. **`next` / RunnableTask**: add `tier` to `RunnableTask` + `runnable_payload` so `next`'s JSON
   carries it — this is the single per-task manifest read run-story already does. (Additive to the
   documented exit-code contract; the field is always concrete since Task::tier defaults to Full.)
6. **run-story.sh (read/gate)**: read `tier="$(jq -r .tier <<<"$task_json")"` **ADJACENT to the
   `specialist` read** (obs `set -u` note) — bind it once there so every downstream call site,
   including `report_task_cost` on the `--finish` / `--revalidate` / escalate lanes that reach it
   early in the same loop iteration, sees a bound variable rather than an unbound-variable abort.
   Floor it with `^(full|light)$` and fail-loud (exit 2, distinct `INVALID-TIER` slogerr mirroring
   `INVALID-SPECIALIST`) on anything else incl. `null`/empty — constraint 19, SETTLED. Read-time,
   before `build_devloop_prompt`, so both the headless and interactive lanes inherit it from one
   site (security). Thread it through `build_devloop_prompt` (fresh lane only) as a `--tier=<tier>`
   token on the `/devloop` invocation. Resume/`--continue` lane needs no tier **because constraint 11
   records `Tier` in main.md Loop State and the resumed lane reads it there (constraint 14) — without
   11 this lane silently re-tiers to full (O1b).** Preserve every existing invariant.
7. **/devloop SKILL (gate)**: Step 1 parse `--tier` (default full); Arguments section documents it
   (set by run-story from the manifest; manual/standalone defaults to full = safe). Gate 1 (Step 5)
   gains a tier gate: **`tier == light` → skip the plan-panel round entirely (implementer plans
   inline into main.md and proceeds to implementation; Gate-3 unchanged, full reviewer panel
   unchanged). `tier == full` (or absent/manual) → run the plan panel as today.** Make explicit
   that tier=light is **distinct from `--light`**: `--light` cuts the whole panel to 3 and has an
   exclusion list; tier=light keeps the full Gate-3 panel and only skips the Gate-1 plan round. The
   Gate-1 classification-sanity guard still runs as a cheap net where a plan/classification table
   exists. **MUST-3 parse-side clause (constraint 24):** absent `--tier` ⇒ full, but a
   PRESENT-AND-UNRECOGNIZED value STOPS and says so — never coerced to full (this is the hand-typed
   `/devloop` path; run-story's floor handles the runner-injected path — both needed).
8. **/user-story SKILL (emit)**: Step 10.4 — the lead sets each task's `--tier` from the mechanical
   ADR-D2 rule: **full** when the task delivers `proto`/touches a wire contract or public API,
   introduces/moves an architectural seam, spans >1 service/specialist, or is author-flagged
   ambiguous; **light** otherwise, with a one-line `--tier-reason`. Carry the rule exactly.
9. **Backward-compat (mandatory)**: no `tier` ⇒ full; story-1's existing manifest
   (`docs/user-stories/2026-08-27-hear-yourself-through-handler.md`) still validates unchanged and
   is treated as full. Verify with `dt-story validate` on that file after the change.
10. **Tests (`tests/cli.rs`)**: round-trip a manifest with tiers — assert on SERIALIZED bytes (via
    `markdown::manifest_yaml`), not parse-back-only: a Full/tier-less task does NOT emit `tier:`
    (reuse `assert_non_manifest_bytes_preserved` for byte-identical backward-compat), a Light task
    round-trips WITH `tier: light` + its reason. Light-requires-reason rejection must be
    NON-VACUOUS — the manifest is otherwise fully valid (pending task WITH specialist AND prompt) so
    the ONLY violation is the reason; assert the SPECIFIC error substring, cover empty AND
    whitespace-only; the add-task refusal asserts `manifest.tasks.len()` unchanged + reason on
    stderr (mirror `add_task_refuses_a_dangling_dep`). no-tier→full default (`task.tier ==
    Tier::Full`). list-tasks surfaces tier(+reason) via an updated MIXED_MANIFEST (one task
    light-with-reason). **Serde-layer negative coverage (the bogus-ENUM half; the null half is a
    shell test, constraint 18):** (A) `tier: bogus` in an OTHERWISE-VALID real manifest fails the
    WHOLE file to parse (exit 2), mirroring `invalid_slug_fails_the_whole_manifest_to_parse`
    (cli.rs:884) — proves the serde enum closes the input class the shell floor guards; (B) assert
    `next`'s JSON emits `.tier` as the exact lowercase token (`"full"` for a full/tier-less task,
    `"light"` for a light one) — the wire-contract pin run-story's `jq -r .tier` reads, which is what
    makes `null` a stale-binary-ONLY case. Both (A) and (B) are REQUIRED (upgraded from @test's
    earlier "good-to-have"). **UPDATE + rename** (not supplement)
    `list_tasks_projects_exactly_four_keys_in_manifest_order` (cli.rs:613) — closed-world key set
    4→6 (`deps,id,slug,status,tier,tier_reason`), banned-keys loop kept.

### Constraints 11–25 — accepted reviewer additions (authoritative, Gate-3 checklist)

11. **(Obs O1 / Ops MUST-1) Record the tier + reason in main.md** — the `**Mode**:` header carries
    the tier (`full (Gate-1 present)` vs `light-tier (Gate-1 SKIPPED — <tier_reason>)`); /devloop
    SKILL Step 2 adds a `Tier` row to Loop State (`full` / `light — <tier_reason>`); Step 5 under
    tier=light REPLACES the confirmations table with a single `### Gate 1 — SKIPPED (tier=light;
    reason: <tier_reason>)` line — never all-`pending` rows (a skipped gate must be distinguishable
    from an abandoned/interrupted one; this is the ADR's own "reports clean over something it never
    checked" defect class relocated into the audit trail if left as `pending`). Carry `tier_reason`
    into the record, not just `tier`. Add the `Tier` row to `docs/devloop-outputs/_template/main.md`
    Loop State and the tier form to its `**Mode**:` line + Gate-1 section.
12. **(Obs O2) `tier=` on `STORY_RUN: START`** — run-story.sh:2044, beside `specialist=`.
13. **(Obs O3 / Ops) `tier` on the cost-ledger** — `report_task_cost`'s devloop entry gains
    `--arg tier` + `tier:$tier`; `append_lane_cost` / `append_unavailable_cost` unchanged (rollup
    partitions on `kind`). Done now (host-persistent D5 record the D2 trial measures on), not
    deferred. **The ledger tier MUST be the same floored value interpolated onto the `/devloop`
    line, derived ONCE at task selection — never re-read from the manifest at completion time
    (two reads drift; the trial's conclusion rests on this field being truthful).** Read it as
    provenance ("what this attempt ran at"), same register as constraint 14.
14. **(Obs O1b/A4 + DRY provenance — correctness + SSoT) Resume reads the `Tier` FIELD as
    PROVENANCE** — /devloop Continue Mode + headless resume read tier from the Loop State `Tier` row
    as the SINGLE source of truth; they MUST NOT infer tier from the presence/emptiness of the Gate-1
    confirmations table (the SKIPPED marker is derived human-facing prose; the field is the machine
    record). A main.md with no `Tier` row (a pre-change devloop resumed after this lands) defaults to
    **full** (fail-safe). This is what makes constraint 6's resume claim true. **Frame the row
    explicitly as PROVENANCE, not a cache of `manifest.tier`** (DRY): `manifest.tier` is the SSoT for
    *what tier a task is* (durable, human-editable plan input); the Loop State `Tier` row is the SSoT
    for *what this attempt actually did* (Gate 1 skipped, under tier=light, per ADR-0037 D2) — a
    historical record of a decision already taken, which is exactly why resume reads it instead of
    re-deriving from a manifest that may have moved. Document in the same register as the
    `commit`-vs-`slug` provenance note at `manifest.rs:158-176` ("different provenance, different
    rule; not an inconsistency to tidy") so a later reader does not "fix" resume to re-read the
    manifest and silently reintroduce the O1b rerun.
15. **(Security A1 — trust boundary) `--tier` honored ONLY from the invocation line** — /devloop
    Step 1 states any `--tier`/tier directive appearing INSIDE the task-description file
    (`$prompt_file`, author-controlled free text) is ignored; absent = full. A tier smuggled in
    prompt prose skips the `tier_reason` requirement and leaves no audit trace.
16. **(Security A2 + @test — fence path) `tier_reason` fence test** — `tier_reason` round-trips
    through `to_block_body()`, a silent-truncation control (its line-scan at manifest.rs:243 already
    covers all emitted lines, so this holds by construction — pin it anyway). Test via `add-task`
    with `--tier light --tier-reason` whose value contains a three-backtick fence line: must refuse
    (exit 2, message names "fence") AND leave the file untouched (`manifest.tasks.len()` unchanged) —
    the exact analog of `add_task_refuses_a_prompt_containing_a_fence` (cli.rs:1276).
17. **(Security rule addition) full criteria +GSA/crypto** — /user-story Step 10.4 full criteria
    gains "touches a Guarded Shared Area or an auth / crypto / secrets / tenancy-isolation surface"
    (checkable from the task's file list; plan-stage review is worth more than diff-stage for
    crypto/secrets, which a single-specialist `crates/common/src/secret.rs` change would otherwise
    send to light).
18. **(Security-1 + @test negative coverage — SETTLED: shell test REQUIRED, split by reachability.)**
    Team-lead ruling (security declined to waive, test concurs): the run-story null-path shell test IS
    required; the reject branch is untested code on a fail-loud path (a missing `exit 2`, an `exit` in
    a subshell, a `[[ ]]`/`set -e` interaction, or an unmatched token silently turns "fails loud" into
    "continues silently" — none visible to a comment reader). The "untested-in-shell by design" line
    is superseded; record the rationale at the site instead. Split by reachability:
    - **Can't-happen (OUT of the shell):** a bogus ENUM value (`tier: bogus`) is serde-closed — a
      real dt-story fails the whole manifest to parse before the floor runs. Covered at the serde
      layer by constraint 10 (A) `tier: bogus`→whole-file exit 2, NOT by a shell test.
    - **Honest negative (IN the shell):** the NULL/MISSING-tier path is REACHABLE via real version
      skew — a stale `$DT_STORY` predating the field emits tier-less `next` JSON, `jq -r .tier` =
      `"null"`, and the `^(full|light)$` floor rejects it. run-story.test.sh adds a case driving a
      `DT_STORY` fake that emits tier-LESS `next` JSON (stale-binary simulation — NOT a bogus enum),
      asserting exit 2, a DISTINCT `INVALID-TIER` token (harness rule at :764 needs the specific
      token), and `assert_no_marker` that no devloop ran. Both @security and @test endorse this.
    - **Implementation requirement that flows from testability:** the floor must `slogerr` a distinct
      `INVALID-TIER` message mirroring `INVALID-SPECIALIST` (already in constraint 6/19).
    - Positive `--tier=full`/`--tier=light` threading still asserted (exercises the PASS branch).
    - The wire-contract pin constraint 10 (B) (`next` JSON `.tier` exact lowercase token) is what
      makes the null path a stale-binary-ONLY case.
19. **(DRY-P1 — SETTLED: keep `^(full|light)$`, exit 2 on any non-match incl. `null`/empty.)** The
    single membership floor satisfies security-1 (null→exit-2, never coerce), ops MUST-3
    (unrecognized→loud refuse), and DRY (no sync-guard: drift fails loud both ways). Do NOT decompose
    into null-check + token-class; do NOT use `^[a-z]+$` (it accepts `null`). Comment block, matching
    the `commit`-vs-`slug` provenance idiom (`manifest.rs:158-176`):
    (a) deliberate second copy of `manifest::Tier`'s value set;
    (b) **CORRECTED — this floor does two jobs: set membership AND presence/staleness detection, and
        its failure IS reachable.** Not "structurally unreachable": a `dt-story` predating the field
        emits no `tier`, `jq -r .tier` prints the string `null`, and this floor is what stops
        `--tier=null` reaching the `/devloop` line. `null` at the JSON layer can ONLY mean a stale
        binary (a current dt-story always serializes a concrete `tier` on the non-`Option`
        `RunnableTask`), so it is an environment fault → exit 2, never a "use default". (A plain
        token class cannot substitute *because* of this second job.)
    (c) **adding a tier to `manifest::Tier` requires widening this literal too**;
    (d) a sync guard was considered and declined — drift fails loud in BOTH directions here (Rust
        widens → floor rejects the new value at run time; Rust narrows → the value is unreachable),
        unlike slug-class whose drift was silent. Record the security(exit-2)-vs-ops(full+warn)
        disagreement and its resolution to exit-2.
    Failure message names `manifest::Tier` + the rebuild (`cargo build --release -p dt-guard -p dt-story`).
    **Floor-helper extraction (team-lead optional / security proposal): DECISION — keep tier's floor
    INLINE, mirroring the specialist floor, with the comment above.** The specialist floor
    (run-story.sh:1743) carries a substantial security-sensitive rationale comment; extracting a
    shared `floor_token <name> <pattern> <value> <id>` helper risks disturbing it for a 2nd caller,
    which is scope-creep into a security-sensitive guard for marginal DRY gain. One-line note left at
    the tier floor that a `floor_token` helper extraction is available if a THIRD such floor ever
    appears (the point at which the abstraction pays).
20. **(DRY-P2) Single `Tier` type** — derive `clap::ValueEnum` on `manifest::Tier`; no parallel CLI
    enum, no hand-rolled match. (= interpretation (a).)
21. **(DRY-P3) One authoritative rule** — /user-story Step 10.4 carries the full/light rule verbatim
    from ADR-0037 §D2 and cites §D2 as authoritative-on-disagreement; /devloop consumes the tier
    token only and does NOT restate the criteria.
22. **(DRY-P4) Vocabulary disambiguation from panel `Mode`** — label the Loop State row `Tier`,
    visibly distinct from `Mode` (`full`/`light` already means panel mode in these files); Step 10.4
    states it sets `--tier`, never `--light`; /devloop states tier=light skips only the Gate-1 plan
    round and keeps the full Gate-3 panel, whereas `--light` cuts the panel to 3 with an exclusion
    list; state precedence for a hand-typed `--light --tier=full` — `--light` already skips Gate 1,
    so tier is a no-op under it.
23. **(Ops MUST-2 — light escalates to full on an exclusion/GSA surface)** — /devloop SKILL states
    that a light-tier implementer whose INLINE plan reaches a `--light` exclusion surface OR a GSA
    path runs the Gate-1 panel round anyway and records the escalation in main.md's `Tier` row.
    **REFERENCE the exclusion list, do NOT re-copy it (DRY, team-lead):** point at §Lightweight
    Mode's canonical "Not eligible" list (`.claude/skills/devloop/SKILL.md` ~198-205) — "reaches a
    `--light` exclusion surface (§Lightweight Mode → Not eligible)" — a free same-file pointer that
    cannot drift; copying the list would let a new exclusion added there silently keep skipping Gate
    1 here. Hard clause (KEPT spelled out — GSA is NOT in the exclusion list, does independent work):
    ADR-0024 §6.4 requires GSA owner-confirmation AT Gate 1 and Gate 3 — a skipped Gate 1 cannot
    satisfy that, so a GSA path in the plan forces full, full stop. Read-side backstop to constraint
    17's emit-side tiering (a surface that slips through decomposition is caught when inline planning
    reaches it). Mirror the existing `--light` "any reviewer can request upgrade" + "when in doubt,
    full" register in wording.
24. **(Ops MUST-3 — unrecognized `--tier` on the human path refuses, not coerces)** — /devloop
    Step 1 + §Arguments: `--tier` ABSENT ⇒ full (safe); a PRESENT but unrecognized value
    (`--tier=ligth`) STOPS and says so, never silently becomes full. Silent coercion teaches the
    operator the flag worked when it didn't. (Distinct from the run-story floor, constraint 19 —
    this is the manual/standalone `/devloop` path.)
25. **(Ops SHOULDs — bundled)** — (i) comment at the `--continue` / `RESTART_FROM_TREE` branch of
    `build_devloop_prompt` that this lane DELIBERATELY carries no tier (the resumed session
    re-derives gate shape from main.md Loop State per constraint 14), so a future editor does not
    "fix" the apparent omission by threading tier onto the resume line; (ii) add
    `--tier`/`--tier-reason` to /user-story Step 10.4's documented "silently discarded on `add-task`
    rc-4" list (SKILL.md ~376) — `engine::add_task` returns `Exists` before writing any field, so a
    re-emit over an already-emitted (story-2) manifest discards them; the only route for tiers onto
    already-pending tasks is a hand-edit of the manifest block, which is why light-requires-reason
    lives in `validate_manifest` (constraint 2/validate) and not only at the `add-task` boundary;
    (iii) one sentence in the Rollback section that a PARTIAL revert (revert the crate, keep a tiered
    manifest) reds Layer 3 repo-wide because `deny_unknown_fields` fails the file — revert both or
    neither. Also (noted, no code change): the audit-remediation `add-task` call stays tier-less =
    full (safe direction); do not "optimize" it later.

---

## Implementation Summary

Added a Gate-1 planning-round `tier` (`full`|`light`, default full) + optional `tier_reason` to the
dt-story Task manifest, gating the /devloop Gate-1 plan round per ADR-0037 §D2. All 25 constraints
implemented; backward-compat verified (story-1's 26 tier-less tasks validate clean, all treated as
full, byte-identical round-trip — no `tier:` introduced).

- **manifest.rs**: `Tier {Full, Light}` enum (serde `rename_all="lowercase"`, `Default=Full`, `Copy`,
  `Display`, `clap::ValueEnum` — the single SSoT for the value set); `Task.tier` (`skip_serializing_if
  = "Tier::is_full"`) + `Task.tier_reason` (`skip_if None`), appended at END of the field list.
- **engine.rs**: status-independent validation rule `tier==Light ⇒ non-empty (trimmed) tier_reason`;
  `RunnableTask.tier` + `runnable_payload` (so `next` JSON carries the token); `NewTask.{tier,
  tier_reason}` threaded through `add_task` (the existing before/after delta-validate refuses
  `--tier light` with no reason).
- **main.rs**: `add-task --tier <full|light>` (ValueEnum, default full) + `--tier-reason`;
  `TaskSummary.{tier, tier_reason}` (tier always-present, tier_reason null-when-absent).
- **run-story.sh**: read `tier` adjacent to `specialist` (bound under `set -u` at all
  `report_task_cost` call sites); `^(full|light)$` floor, fail-loud `INVALID-TIER` exit 2 on any
  non-match incl. `null` (with the constraint-19 a–d comment); `--tier=<tier>` threaded onto the
  fresh `/devloop` line (resume lane deliberately none); `tier=` on `STORY_RUN: START` (O2) and on
  the `report_task_cost` devloop ledger entry (O3, derived once at selection).
- **run-story.test.sh**: i4b (`--tier=full` threading), i8 (`--tier=light` threading), i9 (stale-binary
  `DT_STORY` fake emitting tier-less JSON → exit 2 + `INVALID-TIER` + `assert_no_marker`).
- **/devloop SKILL**: §Arguments + Step 1 (`--tier` invocation-line-only, present-but-unrecognized
  refuses); Step 2 (Loop State `Tier` row + Mode line, provenance framing); Step 5 tier gate
  (light skips plan round; escalate-to-full on `--light` exclusion surface or GSA path — GSA forces
  full); Continue Mode reads the `Tier` FIELD as SSoT (never infers from the table; no row ⇒ full).
- **/user-story SKILL** Step 10.4: mechanical full/light rule carried verbatim from ADR-0037 §D2 with
  §D2 authority-on-disagreement, the GSA/crypto/secrets/tenancy full-criteria extension (flagged as a
  security extension beyond the ADR), `--tier` (never `--light`) note, and the rc-4 discard list.
- **docs/devloop-outputs/_template/main.md**: `Tier` row + Mode-line form + LEAD REMINDER note.

## Files Modified

- `crates/dt-story/src/manifest.rs`
- `crates/dt-story/src/engine.rs`
- `crates/dt-story/src/main.rs`
- `crates/dt-story/tests/cli.rs`
- `scripts/workflow/run-story.sh`
- `scripts/workflow/run-story.test.sh`
- `.claude/skills/devloop/SKILL.md`
- `.claude/skills/user-story/SKILL.md`
- `docs/devloop-outputs/_template/main.md`

(Exactly the scope-drift baseline in the Cross-Boundary table — no drift. `docs/user-stories/_template.md`
and `scripts/guards/simple/validate-story-manifest.sh` NOT touched, as decided.)

## Devloop Verification Steps

Self-verified before signalling ready (Gate-2 authority is the Lead's pipeline run):
- `cargo fmt -p dt-story -- --check` clean (fmt auto-applied locally per ADR-0037 D7).
- `cargo clippy --release -p dt-story --all-targets` clean.
- `cargo test -p dt-story`: 58 passed, 0 failed.
- `bash scripts/workflow/run-story.test.sh`: 440 passed, 0 failed.
- `bash scripts/guards/simple/validate-story-manifest.sh`: rc 0 (validates story-1 clean).
- `bash scripts/guards/simple/validate-slug-class-sync.sh` + `validate-run-dir-path-sync.sh`: rc 0.
- Backward-compat: `dt-story validate docs/user-stories/2026-08-27-hear-yourself-through-handler.md`
  rc 0; all 26 tasks `tier=full`, `tier_reason=null`; no `tier:` line introduced on round-trip.
- Floor non-vacuity: `^(full|light)$` rejects `null`/empty/`full `/`bogus`/`Full`, accepts only
  `full`/`light`.

## Code Review Results

Gate 3 PASSED — all six reviewers CLEAR or RESOLVED-FIXED, zero deferrals, zero escalations. (Semantic Guard not spawned — ADR-0037 D4, no check surface.)

- **Security — RESOLVED-FIXED** (1 finding, fixed): the /devloop Step-5 light-path wording implied the Gate-1 classification-sanity guard runs on the light path, but it's skipped under tier=light; corrected to name constraint 23's escalation as the load-bearing pre-implementation GSA control. Verified the tier value cannot smuggle onto the /devloop line; manifest trust-boundary invariants (deserializer floor under `deny_unknown_fields`, `tier_reason` fence refusal) hold.
- **Test — CLEAR**: all 5 conditions verified non-vacuously with positive controls (bogus-tier whole-file parse fail; `next` lowercase `.tier` token; i9 stale-binary→INVALID-TIER shell test with `assert_no_marker`; closed-world key test updated 4→6; serialized-byte "Full omits tier:"; light-rejection over an otherwise-valid manifest; real `dt-story validate` on story-1). 58/58 cli tests.
- **Observability — RESOLVED-FIXED** (2, fixed): F1 the `## Recovery` headless-resume paragraph now names the Step-5 tier gate; F2 the ledger `tier` field pinned by source-presence (o21b, inspection-only precedent). Light-tier skip is durably traceable in all three artifacts (main.md, cost-ledger, run log).
- **Code Quality — CLEAR**: `Tier` mirrors `Status`; backward-compat `skip_serializing_if` mechanism verified precise; ADR-0002/0035/0037 §D2 compliant (constraint-2 substitution treated as a flagged, sound deviation).
- **DRY — RESOLVED-FIXED** (2, fixed): DRY-1 widening obligation enumerated in the `Tier` doc comment (all 4 downstream sites); DRY-2 unused `Tier::Display` + its vacuous literal-vs-literal pin test both DELETED. The `{full,light}` spread across 5 sites is a recorded non-collapse (fail-loud at every site).
- **Operations — RESOLVED-FIXED** (5, fixed): F-1 `INVALID-TIER` runbook message interpolation bug; F-2 `tier_reason` comment-vs-code drift; M-3/4/5 (Gate-2 diff-scan pickup, ledger resume-lane comment, template Mode placeholder). Gate-behavior posture confirmed: every unknown resolves toward more review; every malformed input fails loud at two layers.

## Accepted Deferrals

- (none surfaced in this devloop — every finding was fixed in-diff; no RESOLVED-DEFERRED verdict)

### Forward-looking notes (not deferrals — no cost shift in this diff)
- **ADR-0037 D2 ratification input (observability + operations):** judging D2 keep/revert/revise at story-2 close needs light-tier loops' **Gate-3 finding counts** alongside their cost — the ADR claims "loses the early catch, not the catch," and a heavier Gate-3 on light loops is the only way to test that. Both inputs (per-task `tier` in the cost-ledger + main.md `Tier` row) are now durable.
- **ADR-0037 D4 data point:** operations F-2 (comment-vs-code drift) is a semantic-guard-class catch made by the standing quartet at zero extra seat cost — evidence the D4 skip-with-backstop bet held here.
- **Environment (operations/infrastructure):** `/home/dev/.cache` is root-owned in this container, so buf/proto codegen can't write its cache (`BUF_CACHE_DIR` workaround used for Gate 2). Provisioning follow-up — it fails every proto/TS gate here.

## Rollback Procedure
Start commit `b8e527470294b90d60e7c70ef8d250f96540721c`. `git reset --hard` is sufficient — no
schema/migration or infra apply in this task. **Revert both or neither (ops):** a PARTIAL revert
that reverts the dt-story crate but keeps a tiered manifest reds Layer 3 repo-wide, because
`Task`'s `#[serde(deny_unknown_fields)]` fails a manifest carrying `tier:`/`tier_reason:` the
reverted binary can no longer parse (surfaced by `validate-story-manifest.sh`). The crate change and
any tiered manifest revert together.
