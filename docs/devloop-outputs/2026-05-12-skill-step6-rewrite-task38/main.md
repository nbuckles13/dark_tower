# Devloop Output: SKILL.md Step 6 Rewrite + Layer 8→7 Renumber

**Date**: 2026-05-12
**Task**: SKILL.md Step 6 rewrite + auto-detection patterns + Layer N/A template + Layer 8→7 renumber (R-62, ADR-0033 Wave 3 #7)
**Specialist**: operations
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task38`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `06e0b596df0876beae9922ff0fa5348290798c7b` |
| Branch | `feature/browser-client-join-task38` |

---

## Loop State (Internal)

<!-- Maintained by the Lead. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-12-skill-step6-rewrite-task38` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `security@devloop-2026-05-12-skill-step6-rewrite-task38` |
| Test | `test@devloop-2026-05-12-skill-step6-rewrite-task38` |
| Observability | `observability@devloop-2026-05-12-skill-step6-rewrite-task38` |
| Code Quality | `code-reviewer@devloop-2026-05-12-skill-step6-rewrite-task38` |
| DRY | `dry-reviewer@devloop-2026-05-12-skill-step6-rewrite-task38` |
| Operations | `operations@devloop-2026-05-12-skill-step6-rewrite-task38` (peer-reviewer; implementer is also operations-trained) |

---

## Task Overview

### Objective

Land ADR-0033 Wave 3 #7: a documentation-only sweep that

1. Collapses devloop **SKILL.md Step 6 (Gate 2 — Validation)** from the current Rust-shaped per-layer table to a thin "run `scripts/layer-all.sh`" pointer + an Always-Run / Skip-If-Untouched matrix per ADR-0033 §3.
2. Adds `client|svelte|sdk|tsx?` and `proto|buf` to the auto-detection patterns in Step 1.
3. Rewrites the **Layer N/A justification template** so it stops baking in Rust-shaped assumptions (cargo check / clippy etc.) and instead reflects the polyglot dispatcher model.
4. Renumbers the env-tests layer from **Layer 8 → Layer 7** across every authoritative doc:
   - `.claude/skills/devloop/SKILL.md`
   - `docs/decisions/adr-0030-host-side-cluster-helper.md`
   - `docs/decisions/adr-0033-polyglot-validation-pipeline.md`
   - `docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md`
   - `docs/specialist-knowledge/infrastructure/INDEX.md`
   - `docs/specialist-knowledge/observability/INDEX.md`
   - `docs/specialist-knowledge/operations/INDEX.md`
   - `docs/specialist-knowledge/semantic-guard/INDEX.md`

The renumber is the load-bearing change: the layer count is now seven (1 compile/lint per-lang dispatcher, 2 fmt, 3 simple guards, 4 unit-test, 5 all-tests/integration, 6 semantic-guard, 7 env-tests). Layer 8 ceased to exist after the pipeline scaffolding refactor in Wave 1 (#32). Devloop-output history files are intentionally left as-is — they record what was true at the time.

### Scope
- **Service(s)**: none (docs-only)
- **Schema**: No
- **Cross-cutting**: Yes — touches the workflow contract that all specialists read

### Debate Decision

NOT NEEDED — implementation of decisions already taken in ADR-0033 (Wave 3) and the polyglot-pipeline debate (2026-05-06). This devloop is the mechanical wave-3 #7 follow-through.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2: a Mechanical classification requires guard-pipeline coverage that catches every partial state. No `Layer N` guard exists today — partial renames (one file still saying "Layer 8" after the rest land at "Layer 7") would not fire any guard. The narrative inside ADR-0033, the debate doc, and each specialist `INDEX.md` is also coherent prose, not value-neutral find-and-replace (surrounding context may need to read sensibly after the swap). That rules out **Mechanical**.

Substantive content of the edits is small (numeric rename, modest prose touch-up around it). The owner is best positioned to confirm the surrounding context still reads correctly. That fits **Minor-judgment** per ADR-0024 §6.2. Cross-boundary hunk confirmation at Gate 1 and Gate 3 is required from each owner.

SKILL.md Step 6 / Step 1 / Layer N/A template touches are **Mine** — SKILL.md is operations-owned per the user-story task #38 assignment.

This table enumerates only **files actually touched by this devloop's diff**, per the Layer A scope-drift guard convention (every non-`Mine` row is a commitment to edit; every diff entry must appear here). The debate docs `docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md` and `docs/debates/2026-04-05-devloop-cluster-sidecar.md` are *intentional no-ops* (frozen-at-conclusion convention — kept verbatim as historical record of decisions); they appear in the Final edit inventory below as "0 flips, kept verbatim" but are excluded from this table to keep the guard's plan-vs-diff parity clean.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.claude/skills/devloop/SKILL.md` | Mine | — |
| `docs/decisions/adr-0030-host-side-cluster-helper.md` | Not mine, Minor-judgment | infrastructure |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Not mine, Minor-judgment | infrastructure (ADR primary author per Participants list; co-confirm operations) |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Not mine, Minor-judgment | infrastructure |
| `docs/specialist-knowledge/observability/INDEX.md` | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/operations/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/semantic-guard/INDEX.md` | Not mine, Minor-judgment | code-reviewer (closest specialist; semantic-guard is an agent identity, no dedicated specialist owner — per ADR-0033 §8 / Wave 3 #9 it relocates to the reviewer panel alongside code-reviewer) |

---

## Planning

### Plan Confirmations (Gate 1)

| Reviewer | Plan Status | Timestamp |
|----------|-------------|-----------|
| Security | confirmed (nit: add 2026-04-05 debate to care points) | 2026-05-12T22:59Z |
| Test | confirmed (note: semantic now runs after env-tests interim; Wave 3 #9 closes) | 2026-05-12T22:59Z |
| Observability | confirmed | 2026-05-12T22:58Z |
| Code Quality | confirmed (count-fix: ADR-0030 has 3 hits not 4; 2 awareness items) | 2026-05-12T23:00Z |
| DRY | confirmed (2 ADR-mirror tightenings recommended; implementer to fold) | 2026-05-12T23:00Z |
| Operations | confirmed (Q1 sequencing corrected; Q2 90s-vs-buf-breaking captured as tech debt) | 2026-05-12T23:03Z |

**Gate 1 outcome (2026-05-12T23:03Z)**: All 6 reviewers confirmed. Layer B classification-sanity guard PASS (`No cross-boundary classification violations`). "Plan approved" issued to @implementer.

### Gate 2 attempts

**Attempt 1 (2026-05-12T23:08Z)** — FAIL on Layer 3 `validate-cross-boundary-scope` (scope-drift-planned-untouched on `docs/debates/2026-05-06-...debate.md`, which the implementer intentionally kept verbatim under frozen-at-conclusion). Other layer failures: pre-existing baseline (sandbox lacks `nx`/`buf`; RUSTSEC-2023-0071 sqlx→sqlx-mysql→rsa transitive with no fixed upgrade available). Docs-only diff cannot have caused rust/ts/proto compile/audit failures.

**Attempt 2 (2026-05-12T23:11Z)** — PASS. Implementer removed the debate-doc row from the Cross-Boundary Classification table. Layer 3 `./scripts/layer3.sh` → STATUS=OK (29/29 guards passed). All other layer failures unchanged (pre-existing baseline).

**Semantic Guard Agent** — Skipped with documented reasoning: diff is 7 markdown files. Semantic-guard analyzes diff for credential leak / actor blocking / error context patterns (rust code-flavored concerns). Layer 3 simple guards (`no-hardcoded-secrets`, `no-pii-in-logs`, `no-secrets-in-logs`) are already green on the docs diff and cover the docs-relevant subset of semantic-guard's surface. Spawning the agent on 7 markdown files would produce duplicative output. Defensible operational shortcut, recorded here for audit.

**Gate 2 outcome**: PASS. Phase advances to review.

### Final edit inventory (verified by `grep -n "Layer 8\|Layer-8\|layer 8\|layer-8"`)

| # | File | Layer-8 raw hits | This devloop's edits |
|---|------|------------------|----------------------|
| 1 | `.claude/skills/devloop/SKILL.md` | 5 (lines 396, 398, 406, 427, 429) | Step 1 auto-detect: add `client` row + extend protocol row to include `buf`; Step 6 rewrite (Gate 2 table → `scripts/layer-all.sh` pointer + Always-Run/Skip-If-Untouched matrix); Layer N/A template rewrite (drops cargo-shaped reasoning); flip all 5 "Layer 8" → "Layer 7" |
| 2 | `docs/decisions/adr-0030-host-side-cluster-helper.md` | 3 (lines 379, 381, 442) | flip all 3 to Layer 7 (count-fix per @code-reviewer plan-review: line 381 has Layer 8 only once, not twice) |
| 3 | `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | 6 (lines 53, 272, 332, 362, 418, 512) | 1 flip (line 512); other 5 are intentional historical breadcrumbs (parenthetical "(renumbered from Layer 8; ...)", §Negative bullet, Wave 3 implementation-status row describing this devloop) — kept verbatim |
| 4 | `docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md` | 1 (line 126) | Kept verbatim — debate docs are frozen-at-conclusion; the "Layer 8 (env-tests) renumbers to Layer 7" statement is the historical record of the decision |
| 5 | `docs/specialist-knowledge/infrastructure/INDEX.md` | 1 (line 41) | flip to Layer 7 |
| 6 | `docs/specialist-knowledge/observability/INDEX.md` | 1 (line 59, two occurrences in same line) | flip both to Layer 7 |
| 7 | `docs/specialist-knowledge/operations/INDEX.md` | 1 (line 19) | flip to Layer 7 |
| 8 | `docs/specialist-knowledge/semantic-guard/INDEX.md` | 1 (line 7, two occurrences in same line) | flip both to Layer 7 |

**User story `docs/user-stories/2026-05-02-browser-client-join.md` — OUT OF SCOPE**: contains 12+ "Layer 8" references (lines 177, 351, 357, 430, 470, 510, 513, 672, 684, 712, 715, 795, 800). All are historical narrative — R-48 original framing, task descriptions as decomposed, change-log entries. Per task brief: "only update if those references describe the *current* state of the workflow rather than tracking history." None do. **Skip.**

**`docs/devloop-outputs/**` — OUT OF SCOPE per task brief.**

**Active edit count**: 14 Layer-8 renumbers (5 SKILL + 4 ADR-0030 + 1 ADR-0033 + 4 INDEX) + 2 Step 1 auto-detect rows + 1 Step 6 prose rewrite + 1 Layer N/A subsection = **~18 active edits**. Within the user-story estimate (~20).

### Step 1 Auto-Detection Changes (SKILL.md ~line 174-186)

Existing protocol row covers `proto|protobuf|contract|wire|signaling|message.format|grpc` but not `buf`. Two options: (a) extend protocol row with `buf`, or (b) add `buf` as a separate row. Going with (a) — keeps `proto` and `buf` together (both route to protocol specialist, matching Wave 2 #4 implementer assignment and ADR-0033 §11 audit-config ownership). Client work gets a new row routing to `client` (agent exists per CLAUDE.md service table).

```
| `proto\|protobuf\|buf\|contract\|wire\|signaling\|message.format\|grpc` | protocol |
| `client\|svelte\|sdk\|tsx?` | client |
```

Placement: client row immediately after the existing `infrastructure` row.

**Disambiguation symmetry** (per @code-reviewer plan-review Finding 3 — optional, folding in): the existing Disambiguation paragraph at SKILL.md:187 names `meeting`-vs-`assignment` as the canonical ambiguous-token example. The new `sdk` token in `client|svelte|sdk|tsx?` creates a symmetric ambiguous case with `media` (a task like "fix media SDK bandwidth heuristic" matches both `sdk` → client and `media` → media-handler). Add a sibling example sentence to the existing Disambiguation paragraph: `Example: "fix media SDK bandwidth heuristic" matches both \`sdk\` (client) and \`media\` (MH) — Lead asks user.` Keeps the worked-example pattern symmetric with the existing meeting-vs-assignment phrasing.

**Apply-time visual check** (per @code-reviewer plan-review Finding 2): the new Step 6 prose draft uses a fenced ```` ```bash ```` block for `./scripts/layer-all.sh` inside the larger ```` ```markdown ```` draft container. When transcribing into SKILL.md, the outer ```` ```markdown ```` fence MUST NOT carry over — only the inner ```` ```bash ```` fence belongs in the file. Spot-check the rendered Step 6 after the edit to confirm no stray "```markdown" leaked in.

### Step 6 Rewrite (SKILL.md ~lines 363-431)

Current Step 6 enumerates 8 ENFORCED layer rows with Rust-shaped commands and separates Layer 8 with attempt budget + classification protocol. Replace with a thin pointer to `scripts/layer-all.sh` + an Always-Run/Skip-If-Untouched matrix derived from ADR-0033 §3. Preserve verbatim: the env-tests classify-exit protocol, attempt budgets, infra-failure retry policy, artifact-specific table (proto/migrations/k8s/Dockerfile/shell) — those are operational contracts independent of the per-layer table.

Draft prose (replaces lines 367-431):

```markdown
**ENFORCED** — single command runs all seven layers in order, stops on first failure:

```bash
./scripts/layer-all.sh
```

Each `scripts/layerN.sh` is independently callable for targeted debugging
(e.g., `scripts/layer4.sh` to re-run only Layer 4 on a failing diff).
See ADR-0033 §4 for the wrapper contract (`STATUS=` lines, `LAYER=N START=…
END=… RESULT=…` stderr summary, worst-child STATUS aggregation).

**Always-Run vs Skip-If-Untouched matrix** (per ADR-0033 §3 — classifying
principle: a step is always-run if its failure mode can be triggered by
external state change with no diff in the toolchain's footprint;
otherwise skip-if-untouched. When in doubt, always-run):

| Layer | Verb     | Always-run                            | Skip-if-untouched per `lang/<X>/changed.sh` |
|-------|----------|---------------------------------------|----------------------------------------------|
| 1     | Compile  | —                                     | rust, ts, proto                              |
| 2     | Format   | —                                     | rust, ts, proto                              |
| 3     | Guards   | ALL guards (each self-classifies)     | —                                            |
| 4     | Test     | —                                     | rust, ts (proto has no `test.sh`)            |
| 5     | Lint     | —                                     | rust, ts, proto                              |
| 6     | Audit    | `cargo audit`, `pnpm audit`, `buf breaking` | —                                      |
| 7     | Env-tests| dev-cluster + Rust env-tests + Playwright `@smoke` | —                               |

**Semantic-guard**: runs once per Gate 2 via the reviewer panel (target
state per ADR-0033 §8; wired up in Wave 3 #9). Until #9 lands, semantic
analysis still runs as an agent invocation after layers 1-7 pass; see the
"Semantic Guard Agent" subsection below — but it is NOT one of the
numbered shell layers in `scripts/layer-all.sh`.

**Layer N/A justification template**:

A wrapper script under `scripts/lang/<X>/` may report `STATUS=N/A` or a
skip token per ADR-0033 §6. When it does, the `scripts/layer-all.sh`
summary table records it, and the implementer does NOT need to explain
or defer — the wrapper's own `REASON=…` in the STATUS line is the
justification.

Recognized STATUS values (ADR-0033 §6):

| `STATUS=` value      | Meaning                                                                                |
|----------------------|----------------------------------------------------------------------------------------|
| `OK`                 | Verb ran and produced no findings                                                      |
| `FAIL`               | Verb ran and found a problem (route to implementer)                                    |
| `SKIPPED-NO-DIFF`    | `lang/<X>/changed.sh` reports zero diff in this toolchain's footprint (skip-if-untouched per §3) |
| `SKIPPED-NO-VERB`    | Language directory exists but verb script is absent (e.g., proto has no `test.sh`)    |
| `N/A`                | Wrapper reports not-applicable with `REASON=…` (e.g., Wave 1 stub pre-Wave 2 wiring)  |

The implementer owes an explanation only when a wrapper unexpectedly
emits `N/A` outside the SKIPPED-NO-DIFF / SKIPPED-NO-VERB cases above
— in which case the wrapper itself is likely buggy. Escalate to
operations rather than defer.

**Semantic Guard Agent** (interim placement; relocates to reviewer panel in ADR-0033 Wave 3 #9):

[…current "Layer 7 — Semantic Guard Agent" prose preserved verbatim, with ONLY the heading changed: "Layer 7 — Semantic Guard Agent" → "Semantic Guard Agent" (no "Layer 7" prefix — it is no longer a numbered shell layer). The "After layers 1-6 pass" lead-in is **preserved verbatim** — semantic-guard still executes between Layer 6 (audit) and Layer 7 (env-tests), matching today's pre-env-tests ordering. This is preservation, not re-sequencing: keeps the cheap fast-fail before env-tests pays its ~7-minute first-run + per-attempt cost (per @operations plan-review Q1).…]

**ARTIFACT-SPECIFIC** (mandatory when detected file types are in the changeset):

[…artifact table preserved verbatim, only Layer 8 references in the surrounding prose flipped to Layer 7…]

**Layer 7 — Env-tests (Integration)** (renumbered from Layer 8; ADR-0033 Wave 3 #7):

[…current "Layer 8 — Env-tests (Integration)" body preserved verbatim with Layer 8 → Layer 7…]

**Layer 7 attempt budget**: 2 attempts (separate from layers 1-6's 3)…
[…verbatim with renumber…]

**If pass / If fail (layers 1-6) / If fail (layer 7)**:
[…all three preserved verbatim, with Layer 8 → Layer 7 in the third…]
```

Decision point on the Semantic Guard Agent subsection: keep it for now (operationally accurate — semantic still runs at agent invocation time), and rename the heading from "Layer 7 — Semantic Guard Agent" to "Semantic Guard Agent" so the file is not self-contradictory about what "Layer 7" means. **Preserve the "After layers 1-6 pass" lead-in** — semantic-guard continues to run before Layer 7 (env-tests), keeping today's pre-env-tests ordering and the cheap fast-fail (per @operations Q1: re-sequencing semantic to after env-tests would force ~7 min first-run + per-attempt env-test cost before we ever ask semantic-guard "is this SAFE?"). Wave 3 #9 will delete the subsection entirely when semantic-guard moves into the reviewer panel. **Placement of the subsection in Step 6**: between the Layer N/A template and the ARTIFACT-SPECIFIC table, so reading order (Layer 1-6 matrix → Semantic Guard Agent → Layer 7 env-tests prose) matches execution order.

### Layer 8 → Layer 7 renumber — care points

- **ADR-0033 line 53**: `layer7.sh # NEW — Env-tests (renumbered from Layer 8; semantic moved to reviewer panel)` — the parenthetical is the historical breadcrumb explaining the file name. Keep verbatim.
- **ADR-0033 line 272**: "Layer 8 (env-tests) renumbers to **Layer 7**." — self-referential rationale. Keep verbatim.
- **ADR-0033 line 332**: §Negative bullet "Renumbering churn. Layer 8 → Layer 7 affects …" — change-rationale prose. Keep verbatim.
- **ADR-0033 lines 362, 418**: Wave 3 implementation-status rows describing THIS devloop's work. Keep verbatim — they are status-log entries.
- **ADR-0033 line 512**: "ADR-0030 (Host-side cluster helper, Layer 8 contract — note: Layer numbering will renumber to Layer 7 in Wave 3)" — "will renumber" is now past tense. Flip: "ADR-0030 (Host-side cluster helper, Layer 7 contract — renumbered from Layer 8 in Wave 3 per ADR-0033)".
- **Debate line 126** (`docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md`): "Layer 8 (env-tests) renumbers to Layer 7." — Debate docs are frozen-at-conclusion by convention. Keep verbatim as the historical record of the decision.
- **Older debate `docs/debates/2026-04-05-devloop-cluster-sidecar.md:159`**: "Add **Layer 8: Integration validation** to the validation pipeline, after semantic guard:" — this is the *original* debate that established Layer 8 in the first place (the predecessor to the polyglot pipeline). Frozen-at-conclusion convention applies. Considered, kept verbatim. (Flagged by @security plan-review for audit-trail completeness.)

Net flips: SKILL (5) + ADR-0030 (3) + ADR-0033 (1) + INDEX×4 (4) = **13 renumber edits**, plus the Step 6 rewrite + Step 1 + N/A template = **17 active edits total** (revised down from 18 per @code-reviewer ADR-0030 count-fix).

---

## Pre-Work

None.

---

## Implementation Summary

All planned edits applied. Summary:

**SKILL.md (`/work/.claude/skills/devloop/SKILL.md`)**:
- Step 1 auto-detect table: extended protocol row regex to include `buf` (`proto\|protobuf\|buf\|contract\|wire\|...`); added new client row `client\|svelte\|sdk\|tsx?` → client specialist, placed after the infrastructure row.
- Step 1 Disambiguation paragraph: appended `sdk`-vs-`media` example as a sibling to the existing `meeting`-vs-`assignment` example (per @code-reviewer Finding 3).
- Step 6 (Gate 2 — Validation): replaced 8-row ENFORCED table with `./scripts/layer-all.sh` pointer + Always-Run / Skip-If-Untouched matrix. Per @dry-reviewer tightening (a): dropped the paraphrased classifying-principle prose, replaced with one-line pointer "See ADR-0033 §3 for the classifying principle and worked examples." Per @dry-reviewer tightening (b): dropped the 5-row STATUS-values table from the Layer N/A template, replaced with one-line pointer "See ADR-0033 §6 for the full `STATUS=…REASON=…` wrapper contract; implementer only owes explanation when a wrapper emits `N/A` outside the documented skip cases." Semantic Guard Agent subsection preserved verbatim except for the heading rename ("Layer 7 — Semantic Guard Agent" → "Semantic Guard Agent") so the file isn't self-contradictory about what "Layer 7" means; "After layers 1-6 pass" lead-in preserved verbatim so semantic-guard continues to run before Layer 7 (env-tests), keeping the cheap fast-fail (per @operations Q1). Subsection placement: between Layer N/A template and ARTIFACT-SPECIFIC table — reading order matches execution order.
- Step 6 Layer 8 → Layer 7 renumber: 5 occurrences flipped (heading "Layer 8 — Env-tests …", prose "After layers 1-7 pass" + "Layer 8 always runs …", attempt-budget heading, "If fail (layer 8)" heading, "Increment Layer 8 iteration count"). Removed the "(renumbered from Layer 8 in Wave 3 per ADR-0033)" parenthetical from the Layer 7 heading — SKILL.md is the current-workflow contract, not a change log; the breadcrumb belongs in ADR-0033's References list (line 512), which now carries it.
- ARTIFACT-SPECIFIC table: updated `.proto` row from "Proto compilation, freshness check …" to "`buf build` / `buf lint` / `buf breaking` (wired via `scripts/lang/proto/`); freshness check via Layer 3 guards" — accurate to the polyglot pipeline.

**ADR-0030 (`/work/docs/decisions/adr-0030-host-side-cluster-helper.md`)**: 3 Layer-8 → Layer-7 flips (heading line 379 "Devloop Skill Integration (Layer 7)", prose line 381, dependency-order table row line 442).

**ADR-0033 (`/work/docs/decisions/adr-0033-polyglot-validation-pipeline.md`)**: 1 flip on line 512 — "Layer 8 contract — note: Layer numbering will renumber to Layer 7 in Wave 3" → "Layer 7 contract — renumbered from Layer 8 in Wave 3 per ADR-0033" (tense change + back-pointer; remaining 5 Layer-8 occurrences are intentional historical breadcrumbs per the plan's care-points list, kept verbatim).

**Debate doc `docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md`**: 0 edits (frozen-at-conclusion). The Layer 8 reference at line 126 is the historical record of the decision and is preserved.

**Older debate `docs/debates/2026-04-05-devloop-cluster-sidecar.md`**: 0 edits (frozen-at-conclusion). The Layer 8 reference at line 159 is the original debate that established Layer 8 in the first place and is preserved (per @security plan-review care point).

**INDEX files (4 × 1 line flip each)**:
- `docs/specialist-knowledge/infrastructure/INDEX.md` line 41
- `docs/specialist-knowledge/observability/INDEX.md` line 59 (both inline-prose and parenthetical occurrences on the same line)
- `docs/specialist-knowledge/operations/INDEX.md` line 19
- `docs/specialist-knowledge/semantic-guard/INDEX.md` line 7 (both inline-prose and quoted-section occurrences on the same line)

**Verification**:

1. `grep -nE "Layer 8|layer 8" /work/.claude/skills/devloop/SKILL.md` → zero hits.
2. `grep -rnE "Layer 8|layer 8" /work/.claude/ /work/docs/decisions/ /work/docs/debates/ /work/docs/specialist-knowledge/ --include="*.md"` → 8 hits remaining, all enumerated in the plan's "care points" list as intentional historical breadcrumbs:
   - `debate.md:126`, `adr-0033:53,272,332,362,418`, `adr-0033:512` (current-state heading + trailing breadcrumb clause), `2026-04-05-devloop-cluster-sidecar.md:159`.
3. `grep -n '^\`\`\`markdown' /work/.claude/skills/devloop/SKILL.md` → 1 hit at line 209 (preexisting Step 2 "Security Decisions" table example, NOT from my Step 6 edit). Confirmed Finding 2 not triggered.

**Edits-count vs plan**: 13 net renumber flips + Step 1 disambiguation + Step 6 block (matrix + N/A template) + 2 Step 1 auto-detect rows + ARTIFACT-SPECIFIC proto-row update = **~18 active edits** (one bump from the 17 estimate post-Code Quality Finding 3 inclusion). Within the user-story ~20 estimate.

**Not in scope, preserved**: Wave 3 #9 will relocate semantic-guard out of the pipeline subsection entirely (delete the "Semantic Guard Agent" subsection from SKILL.md, move to reviewer panel). This devloop only renamed the heading and kept the subsection in place.

---

## Files Modified

```
 .claude/skills/devloop/SKILL.md                    | 74 +++++++++++++---------
 .../decisions/adr-0030-host-side-cluster-helper.md |  6 +-
 .../adr-0033-polyglot-validation-pipeline.md       |  2 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |  2 +-
 docs/specialist-knowledge/observability/INDEX.md   |  2 +-
 docs/specialist-knowledge/operations/INDEX.md      |  2 +-
 docs/specialist-knowledge/semantic-guard/INDEX.md  |  2 +-
 7 files changed, 52 insertions(+), 38 deletions(-)
```

(Stat refreshed post-DRY-tightening adoption.)

(`docs/devloop-outputs/2026-05-12-skill-step6-rewrite-task38/main.md` not counted — it's the devloop's own output file, not part of the active edit surface.)

---

## Devloop Verification Steps

Per SKILL.md Step 6 (the freshly-rewritten version): `./scripts/layer-all.sh` is the canonical entry point. Since this devloop's diff is docs-only, language-specific layers (1 Compile, 2 Format, 4 Test, 5 Lint) will short-circuit to `STATUS=SKIPPED-NO-DIFF` via each `lang/<X>/changed.sh`. Always-run layers (3 Guards, 6 Audit) and Layer 7 (Env-tests) execute unconditionally.

Notable in-scope checks for this docs-only diff:

- **Layer 3 Guards**: `scripts/guards/simple/validate-cross-boundary-scope.sh` (Layer A scope-drift) will read the plan's edit-inventory table and confirm every modified file appears in the plan + every planned file appears in the diff. `validate-cross-boundary-classification.sh` (Layer B classification-sanity) already ran successfully at Gate 1 ("No cross-boundary classification violations" per main.md Gate 1 outcome); re-runs at Gate 2 as a safety net.
- **`docs/TODO.md` tracking guard** (per the recent `validate-todo-tracking` convention) — no TODO references introduced or removed.
- **Layer 6 Audit**: `cargo audit` + `pnpm audit` + `buf breaking` always-run against `origin/main`. None of the 7 modified files touch dependency lockfiles, package manifests, or `.proto` files, so no advisory deltas expected.
- **Layer 7 Env-tests**: not affected by docs-only diff but still runs per the always-run rule. Expected to pass unchanged from main.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR — no findings. Always-run audit cadence (`cargo audit`, `pnpm audit`, `buf breaking`) byte-for-byte match with ADR-0033 §3. Semantic-guard ordering preserved (still before Kind cluster spin-up). No stray Layer 8 in load-bearing docs; ADR-0033 line 512 correctly back-pointered.

### Test Specialist
**Verdict**: RESOLVED — 2 findings fixed: (1) real prose-vs-script contradiction in the Step 6 rewrite — Layer 7 — Env-tests subsection said "After semantic-guard returns SAFE" but `scripts/layer-all.sh` runs Layer 7 unconditionally; implementer adopted Option A (reword + reorder subsections so reading order matches execution order, closing the contradiction). (2) `.proto` ARTIFACT-SPECIFIC row scope-delta with non-existent "freshness check via Layer 3 guards" reference — implementer reverted the row to its pre-existing wording (closes the scope-delta also flagged by DRY + code-reviewer + operations).

### Observability Specialist
**Verdict**: CLEAR — hunk-ACK confirmed for `docs/specialist-knowledge/observability/INDEX.md:59` (both occurrences flipped byte-equally; surrounding prose intact). Always-run observability guards preserved in Step 6 matrix. One informational note (no action): semantic-guard subsection has no matrix row under interim shape — ADR-0033 Wave 3 #9 is the proper home.

### Code Quality Reviewer
**Verdict**: CLEAR — zero fix-it findings; Ownership Lens row-by-row recorded with code-reviewer formally ACKing the `semantic-guard/INDEX.md:7` hunk. Two informational findings deferred-accepted: (1) Layer 7 env-tests prose still reads "After semantic-guard returns SAFE" under the interim shape — captured as Tech Debt Pointer for Wave 3 #9 to sweep; (2) inventory clarity nit for the `.proto` artifact-row update — no code change.

### DRY Reviewer
**Verdict**: CLEAR — both tightenings landed verbatim, no new ADR-mirror, existing prose-mirror burden reduced. One extraction opportunity logged to `docs/TODO.md`: Step 6 artifact-table reconciliation (fold artifact rows into `lang/<X>/` wrappers after Wave 3 #9). One non-blocking informational observation surfaced for operations: the `.proto` artifact-row was rewritten beyond what the plan listed (strictly DRY-positive — closes a latent contradiction with the new polyglot matrix).

### Operations Reviewer
**Verdict**: CLEAR — renumber byte-clean across 4 INDEX files + ADR-0030; matrix matches `scripts/layer-all.sh:32` loop range; attempt budgets, first-run-setup, 5-step classify-exit protocol preserved through the renumber; `docs/runbooks/` confirmed zero Layer-8 hits. Two minor findings deferred-accepted: (1) `.proto` artifact-row scope-delta (operationally correct, matches Wave 2 #4 reality; same observation as DRY + code-reviewer); (2) Layer 7 matrix row is target-state while the subsection is current-state — operationally consistent with how the rest of the polyglot transition is documented (mirrors the semantic-guard interim/target pattern). No preemption of Wave 3 #40 confirmed.

---

## Tech Debt Pointers

- **Layer 6 always-run 90s p95 budget vs `buf breaking` upstream-fetch cost** — `scripts/layer-all.sh:29,53-56` enforces a 90s p95 hard budget for the always-run subset (layers 3 + 6). Layer 6 includes `buf breaking` against an upstream ref, which typically triggers a `git fetch` against `origin` and may be subject to network variance on first invocation. Verify with paired-operations §2 measurements after Wave 2 #4 buf wrappers are in CI; revisit budget membership or `buf breaking` caching if it becomes a recurring breach. (Flagged by @operations plan-review Q2; not in scope for #38 — ADR-0033 §4 / Wave 1 #1 contract question.)
- **Devloop main.md template wording ambiguity — Cross-Boundary Classification table** — `docs/devloop-outputs/_template/main.md:69-75` says the table lists "every planned file change", which is ambiguous between "every file considered (including intentional no-ops)" and "every file the devloop's diff will modify". The Layer A scope-drift guard (`scripts/guards/simple/validate-cross-boundary-scope.sh`) reads it as the latter (commitment-to-touch); a natural-language reader can interpret it as the former (and did — this devloop's Gate 2 attempt 1 tripped on the 2026-05-06 debate doc being listed as 0-edit-intentional). Harden the template comment to "every file this devloop's diff WILL modify (intentional no-ops belong in the Final edit inventory below, NOT in this table)" so the next implementer hits the right reading first time. Cross-cuts with §Devloop Workflow / §Template Clarification. (Flagged by @team-lead during Gate 2; not in scope for #38 — separate small devloop or templating PR.)

---

## Rollback Procedure

1. Start commit: `06e0b596df0876beae9922ff0fa5348290798c7b`
2. `git diff 06e0b596df0876beae9922ff0fa5348290798c7b..HEAD`
3. Soft reset (preserves changes): `git reset --soft 06e0b596df0876beae9922ff0fa5348290798c7b`
4. Hard reset (clean revert): `git reset --hard 06e0b596df0876beae9922ff0fa5348290798c7b`

No schema or infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.
