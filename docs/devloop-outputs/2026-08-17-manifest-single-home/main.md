# Devloop Output: Manifest as the single home for per-task story state

**Date**: 2026-08-17
**Task**: Make the dt-story manifest the single home for per-task state across the story workflow — `/user-story` emits it, the runner records the devloop slug into it, `/close-story` reads status + slug from it, and the unread `branch` field is removed.
**Specialist**: operations (paired with protocol)
**Mode**: Agent Teams (v2) — full, headless (run-story task #4)
**Branch**: `feature/story-runner-hardening`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a51c352a084af943e47a832b0a90f632851fe651` |
| Branch | `feature/story-runner-hardening` |
| Story | `docs/user-stories/2026-08-11-story-runner-hardening.md` R-8, task 4 |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `complete` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-DEFERRED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `RESOLVED-DEFERRED` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `RESOLVED-DEFERRED` |
| Paired Protocol | `RESOLVED-FIXED` |

---

## Task Overview

### Objective

Close R-8: a story must be runnable without hand-written YAML, per-task status must have exactly one home, and the manifest must declare nothing nobody reads.

### Scope

- **Service(s)**: none — workflow scripts, the `dt-story` crate, and two skills
- **Schema**: manifest v1 schema change (`branch` removed, `slug` added). No DB schema.
- **Cross-cutting**: yes — `/user-story`, `/close-story`, `run-story.sh`, `crates/dt-story/`

### Debate Decision

NOT NEEDED — ADR-0035 §4 already decides this ("§Task Metadata … devloop-output slug … Yes — the only place"; "Delete §Devloop Tracking").

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. **`crates/dt-story/**` matches §6.4's `schema evolution`
criterion — which is path-independent, the enumerated GSA list being a snapshot
rather than the definition — and this task removes a required field, adds one,
and introduces a validation class. Those rows are therefore priority-high scope
items, and the requirement is satisfied by `--paired-with=protocol`, which makes
protocol a Gate 1 + Gate 3 reviewer on every `crates/dt-story/` row below.**
Worded this way at @security's request: do **not** cite this plan as precedent
that `crates/dt-story/` is exempt from the criterion. It is criterion-matched and
discharged by pairing, not out of scope.

| File | Classification | Owner |
|------|----------------|-------|
| `crates/dt-story/src/manifest.rs` | Mine (Domain-judgment) — schema: drop `branch`, add `Slug` newtype + `Task.slug`, fence-safe `to_block_body`, v1 rationale | operations, paired protocol |
| `crates/dt-story/src/markdown.rs` | Mine (Domain-judgment) — bail on ANY unterminated fence at EOF (fence-collision fix, V1) | operations, paired protocol |
| `crates/dt-story/src/engine.rs` | Mine (Domain-judgment) — `complete()` signature + `AlreadyComplete` slug persistence; `add_task()` gains `deps` (V22) | operations, paired protocol |
| `scripts/lang/rust/audit-remediation.md` | Mine (Mechanical) — one-line no-fenced-blocks comment + reason (V22) | operations |
| `scripts/lang/ts/audit-remediation.md` | **Not mine (Mechanical)** — same one-line comment; content-neutral | **client** (TS lane) |
| `crates/dt-story/src/main.rs` | Mine (Domain-judgment) — `--slug` flag, `TaskSummary.slug`, rule-doc rewrite | operations, paired protocol |
| `crates/dt-story/tests/cli.rs` | Mine (Domain-judgment) — 9 inline fixtures + new coverage | operations, reviewed by test |
| `crates/dt-story/tests/fixtures/runnable.md` | Mine (**Minor-judgment**) — delete one `branch:` line. *Upgraded from Mechanical at @semantic-guard's §6.2 challenge: the header declares this crate criterion-matched under §6.4, which disallows Mechanical inside a criterion-matched area. Accepted rather than narrowing the declaration.* @paired-protocol confirms these hunks at **Gate 3** via the `Approved-Cross-Boundary: protocol` trailer — their Gate 1 sign-off is explicitly **not** a hunk ACK | operations, paired protocol |
| `crates/dt-story/tests/fixtures/blocked.md` | Mine (**Minor-judgment**) — delete one `branch:` line. *Upgraded from Mechanical at @semantic-guard's §6.2 challenge: the header declares this crate criterion-matched under §6.4, which disallows Mechanical inside a criterion-matched area. Accepted rather than narrowing the declaration.* @paired-protocol confirms these hunks at **Gate 3** via the `Approved-Cross-Boundary: protocol` trailer — their Gate 1 sign-off is explicitly **not** a hunk ACK | operations, paired protocol |
| `crates/dt-story/tests/fixtures/all_complete.md` | Mine (**Minor-judgment**) — delete one `branch:` line. *Upgraded from Mechanical at @semantic-guard's §6.2 challenge: the header declares this crate criterion-matched under §6.4, which disallows Mechanical inside a criterion-matched area. Accepted rather than narrowing the declaration.* @paired-protocol confirms these hunks at **Gate 3** via the `Approved-Cross-Boundary: protocol` trailer — their Gate 1 sign-off is explicitly **not** a hunk ACK | operations, paired protocol |
| `crates/dt-story/tests/fixtures/malformed.md` | Mine (**Minor-judgment**) — delete one `branch:` line. *Upgraded from Mechanical at @semantic-guard's §6.2 challenge: the header declares this crate criterion-matched under §6.4, which disallows Mechanical inside a criterion-matched area. Accepted rather than narrowing the declaration.* @paired-protocol confirms these hunks at **Gate 3** via the `Approved-Cross-Boundary: protocol` trailer — their Gate 1 sign-off is explicitly **not** a hunk ACK | operations, paired protocol |
| `scripts/workflow/run-story.sh` | Mine (Minor-judgment) — commit-range slug derivation; NO-SLUG lane (zero vs many, loud, never a lane change); new narrow-class floor; four literal resume-class regexes collapsed to one `readonly`; `slug=`/`src=` on the COMPLETE log line | operations |
| `scripts/workflow/preflight-story.sh` | Mine (Minor-judgment) — widen the bare `story manifest invalid` message to name stale-binary vs invalid-manifest + the rebuild command (V10/C6) | operations |
| `scripts/guards/simple/validate-slug-class-sync.sh` (new) | Mine (Domain-judgment) — Rust `Slug` ↔ `/close-story` class parity guard | operations |
| `scripts/guards/validate-slug-class-sync.test.sh` (new) | Mine (Domain-judgment) — drift case; deliberately outside `guards/simple/` so `run-guards.sh`'s `find -name '*.sh'` does not run it as a guard | operations |
| `scripts/layer3.sh` | Mine (Mechanical) — explicit wiring line for the sync-guard self-test, mirroring `:44` | operations |
| `scripts/workflow/run-story.test.sh` | **Not mine (Minor-judgment)** — *upgraded at @test's challenge, accepted.* Deleting `branch: fixture-branch` is Mechanical, but the three new cases + stub knobs + bash slug floor author new expected-outcome judgments, which is not value-neutral. §Notes authorises the *venue*, not the value-neutrality. Owner is a reviewer here and confirms at Gate 1 + Gate 3; `Approved-Cross-Boundary: test` trailer with the §Notes authority as reason | **test** |
| `.claude/skills/user-story/SKILL.md` | Mine (Domain-judgment) — emit + validate the manifest block | operations |
| `.claude/skills/close-story/SKILL.md` | Mine (Domain-judgment) — read status + slug from the manifest | operations |
| `.claude/skills/devloop/SKILL.md` | Mine (Minor-judgment) — **deliberate departure from the task text**, see Q1 below | operations |
| `docs/user-stories/_template.md` | Mine (Domain-judgment) — drop Status column + §Devloop Tracking, add §Task Metadata | operations |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` | Mine (Minor-judgment) — this story's own file | operations |
| `docs/user-stories/2026-05-02-browser-client-join.md` | **Not mine (Mechanical)** — deletes the `branch:` line **and nothing else**. Forced by the schema change, value-neutral, no alternative, and guard-covered (`deny_unknown_fields` makes any missed instance fail loudly). Table work spun out to the owner, see V12 | **client** |
| `docs/decisions/adr-0035-story-runner.md` | Mine (Minor-judgment) — dated in-place correction to §4 | operations |
| `docs/TODO.md` (two `[ADR-0035 · Group 2]` entries) | Mine (Mechanical) — closure idiom | operations |
| `docs/TODO.md` (referent-durability entry, §"Contingent on surviving story task 4") | **Not mine (Minor-judgment)** — re-verify the three instances against the post-task-4 tree; the entry names its owner | **protocol** |
| `docs/TODO.md` (spin-out: browser-client-join table projection, V12) | Mine (Mechanical) — records dry-reviewer's recipe against owner `client` | operations |
| `docs/devloop-outputs/2026-08-17-manifest-single-home/main.md` | Mine (Mechanical) | operations |

---

## Planning

### Mechanism restatement (and the wider class it exposes)

Instance-language: *"retire the Devloop Tracking table and the `branch` field."*

Mechanism-language: **a durable planning artifact must have exactly one
*writable* home per fact; where a second home exists, either derive it from the
first or delete it — and a field with no reader is not a home, it is a claim.**

That restatement produces a **wider class than the task names**, with three
same-owner siblings the task's file list misses. All three are in scope:

1. **`docs/user-stories/2026-08-11-story-runner-hardening.md` §Implementation
   Plan carries `Status` *and* `Devloop Output` columns.** The task text only
   names §Devloop Tracking, but ADR-0035 §4 also says "The Ordered Task List
   keeps no status column and duplicates no manifest field." This file is the
   live drift instance of *that* sentence. Same for `_template.md`'s
   §Implementation Plan `Status` column (line ~99).
2. **`branch:` is in six files, not two.** The task says "Besides this file,
   only `docs/user-stories/2026-05-02-browser-client-join.md` carries it."
   Verified false for non-story files: `crates/dt-story/tests/fixtures/`
   `runnable.md:13`, `blocked.md:11`, `all_complete.md:8`, `malformed.md:8`
   each carry it, and `scripts/workflow/run-story.test.sh:538` *emits* it into
   every hermetic fixture story. Miss any one and Layer 1 reds.
3. **The three completed tasks' slugs already exist** in the Implementation
   Plan table's Devloop Output column and have nowhere durable to go once that
   column is deleted — they must be backfilled into the manifest, or deleting
   the column destroys them.

### Design

**(c) `branch` removal.** Delete `pub branch: String` from `Manifest`. Confirmed
zero readers: `grep -rn '\bbranch\b' crates/dt-story/src scripts/workflow/` hits
only the declaration itself plus unrelated prose. Six files lose the line
(above). `deny_unknown_fields` makes this a hard-fail-on-miss, which is the
desired loudness.

**Rebuild ordering verified.** `scripts/lang/rust/compile.sh:18` runs
`cargo build --release -p dt-story`; Layer 1 precedes Layer 3, where
`scripts/guards/simple/validate-story-manifest.sh` runs `validate` over every
manifest-bearing story file using `target/release/dt-story`. So schema change +
file updates in one commit is safe: the binary is rebuilt before it is asked to
parse the new files.

**(b) `slug`.** New `Task.slug: Option<String>`, declared **between `commit` and
`escalation`** (declaration order is serialization order) — it groups with the
other completion-time outcome fields. Status-independent and optional, so a
pending task carrying a slug still validates.

- `engine::complete(manifest, id, commit, slug)`.
  - `Pending` arm: set status, and set `slug` if supplied (mirrors `commit`).
  - `AlreadyComplete` arm: **persist a newly-supplied `slug`** (last-writer-wins)
    and return `AlreadyComplete { updated: bool }` so `main.rs` knows to save.
    A differing overwrite emits a loud stderr notice — visible, not silent.
- **The `commit`/`slug` asymmetry, justified rather than left unexplained.**
  `commit` is *not* overwritten because the runner deliberately declines to
  supply it at all (`run-story.sh:1196-1199`: the amend changes the sha, so any
  sha recordable there is stale by construction) — so any `commit` value present
  is a human annotation, and overwriting it with a value the runner itself calls
  unreliable is a regression. `slug` is the opposite: the runner is its
  *authoritative* producer, computed at the one moment the task has provably
  committed AND gated green. There is exactly one correct answer per completion,
  and refusing to overwrite would freeze a superseded attempt's slug — precisely
  the bug the task asks to fix. Different provenance, different rule.
- Runner: `"$DT_STORY" complete "$STORY_FILE" "$id" --slug "${continue_slug:-$(newest_devloop_output "$start_marker")}"` at `:1200`,
  before the `rm -f "$slug_file" "$start_marker"` at `:1205`. `--slug` is omitted
  when that expression is empty (no output dir ⇒ nothing to record; must not
  write an empty string). This *is* last-writer-wins meaning "the attempt that
  actually committed", because `:1200` is reached only after the no-commit check
  proved HEAD moved and the gates went green.

**(a) `/user-story` emits the manifest.** New Step 10 sub-section + a Step 10.5
validation gate:
- The story file gets a `## Task Metadata (dt-story manifest v1)` section
  carrying the fenced YAML block, mirroring the two existing manifest-bearing
  stories.
- **Prompt safety, explicit in the skill** (the (d) clause): prompts are emitted
  as YAML block scalars (`prompt: |`); `specialist` MUST match
  `^[a-z][a-z0-9-]*$` because `run-story.sh:917` floors it and it is the one
  manifest value still spliced onto a command line — pairing is recorded as
  *prose inside the prompt* ("Pair with protocol."), never as
  `test --paired-with=infrastructure` in the `specialist` field. That is this
  story's own established convention.
- **Step 10.5 (fail-closed):** run `target/release/dt-story validate <story>`
  and do not report the story as Ready until it exits 0. "Emit the manifest
  block it validates" is only true if the skill actually runs the validator.

**(b/d) `/close-story` reads the manifest.** Phase 1 becomes:
`target/release/dt-story list-tasks <story>` → `jq`. Completeness is
`all(.status == "completed")`. Slugs come from `.slug`. Per (d) there is **no
table-reading fallback**: a story file with no manifest block hard-fails Phase 1
with a named error, loudly. The existing `^[a-z0-9-]+$` slug regex still applies
at the input boundary (defence in depth — the manifest is model-written).

**The ADR-0035 §4 correction.** §4's closing line — "completeness is
`dt-story next` exit 3, never a table read" — would instruct the next reader to
build a mutation bug: `next` is not read-only (`main.rs` module doc lines 40-43;
`engine.rs:87-91` flips an escalated task back to pending and rewrites the file).
A `/close-story` completeness check running `next` on a story with an escalated
task would silently reopen it. Corrected **in place with a dated block**, this
repo's established idiom (see the two "*Premise corrected …*" blocks in the story
file), rather than deleted — otherwise the sentence stays citable.

**`TaskSummary.slug` (Q3).** Added. The four written rules are updated honestly
rather than quietly widened — see Q3 below. The exit-code/stdout contract prose
is **not** touched: `preflight-story.sh:104-111` asserts on it.

---

## Open Questions — my positions

**Q1 — `.claude/skills/devloop/SKILL.md:520-522`. Position: the sentence MUST
change. This is a deliberate, reasoned departure from "Do not change the devloop
skill."**

The task's stated reason for not touching the skill is that its Headless Mode
section already covers run-story. That rationale is **scoped to the run-story
integration** and does not reach this sentence, which is about the *table*. Two
concrete failures if it is left alone:

1. **It instructs models to recreate the artifact this task deletes.** A devloop
   told to "update the Devloop Tracking table" in a story file that no longer
   has one will either no-op silently or *add the table back*. Retiring a second
   home while leaving a standing instruction to write to it is not retirement.
2. **For an interactive (non-run-story) devloop it is the only completion
   record.** run-story calls `dt-story complete`; an interactive devloop does
   not. Delete the table with no replacement and an interactive story task
   completes with nothing recorded anywhere — `/close-story`'s new manifest
   completeness check then blocks forever with no documented gesture to unblock
   it. That is a strictly worse outcome than the drift being fixed.

**Smallest possible edit**, replacing the two-sentence instruction with a
mode-split one: interactive story devloops run
`target/release/dt-story complete <story-file> <id> --slug <this devloop's output slug>`
and include the story-file change in the Step 8 commit; **headless
(`HEADLESS RUN`) devloops do not** — the runner records completion itself after
its own gates pass. The headless half is not cosmetic: a devloop that
self-completes *before* the runner's gates run leaves `engine::escalate()`
unable to escalate a subsequent red gate (it bails on a non-pending task). That
hazard exists today (ADR-0035 §4 records the task #60 double-writer collision);
this wording shrinks it rather than widening it, while `complete`'s idempotency
stays as the safety net it was written to be.

**Q2 — scope of "retire the Devloop Tracking table". Position: template +
skills + this story's own file. Do NOT delete browser-client-join's table.**

- `_template.md`: delete §Devloop Tracking, delete the §Implementation Plan
  `Status` column, add §Task Metadata. Going forward, one home.
- `2026-08-11-story-runner-hardening.md` (this story): delete the
  §Implementation Plan `Status` and `Devloop Output` columns, backfilling the
  three completed slugs into the manifest first. Small (3 values), verifiable
  (`ls docs/devloop-outputs/` — all three confirmed present), and it is the
  *live* instance of the drift ADR-0035 §4 forbids.
- `2026-05-02-browser-client-join.md`: **keep the table, do not backfill ~60
  slugs.** The SSoT rule bites on artifacts that are still *written to*. This
  story's Status is Complete; there is no writer, so there is no drift to
  prevent — only a historical record to destroy. Hand-transcribing 60 slugs is
  exactly the operation that produced the drift ADR-0035 §4 cites (duplicate
  #18/#19/#20 rows, conflicting #20 status, found during backfill), performed in
  an unrelated task, unverifiable except by hand. The honest cost is that one
  file keeps two artifacts; ~~the mitigation is a one-line **frozen-record banner**~~ *(banner DROPPED — Lead Call A, V25; the mitigation is the `docs/TODO.md` spin-out to `client`, which names the residual)*
  above the table stating it is historical, superseded by the manifest, and not
  to be updated. Its `branch:` line still goes (schema forces it).
- **The four closed table-only stories: leave completely untouched.** They carry
  no manifest, so there is no second home *within the file* — nothing to retire.
  They carry no `branch:`. `/close-story` will never read them again per (d).
  Adding banners to them would be edits for their own sake. My position matches
  the Lead's reading; I am recording it explicitly rather than by silence.

**Q3 — does `slug` belong in `TaskSummary`? Position: yes.**

`/close-story` Phase 1 is a *model* reading a file, and the slug feeds a path
construction (`docs/devloop-outputs/{slug}/main.md`) — the exact injection
vector Phase 1's `^[a-z0-9-]+$` regex exists for. close-story's own
§Design Rationale states "structured-only extraction"; a `jq` read of a
mechanical projection is strictly safer than a model eyeballing YAML, and it is
what that rationale asks for. Widening a deliberately narrow contract is the
real cost, paid by updating the rules honestly:

- **Rule 1** currently reads "every field has a named consumer in
  `scripts/workflow/run-story.sh`". That is now false. Rewritten to: every field
  has a **named** consumer, and the consumer is named — `id`/`status`/`deps`
  for `run-story.sh`'s `--stop-after` and deps-aware refusal; `slug` for
  `/close-story` Phase 1 and Phase 4. The rule's force (add a field only
  together with its reader) is preserved; only the assumption that the sole
  consumer is the runner is dropped.
- **Rule 2** (never project an uncontrolled value) — *(revised by V4/V9; this is
  the current statement)* satisfied **by construction**, not by a producer-side
  filter: `Slug` is a newtype whose only constructor validates
  `^[a-z0-9]+(-[a-z0-9]+)*$` in the deserializer, so no unvalidated value can
  exist in a parsed `Task`. @paired-protocol correctly made this a *precondition*
  for projecting at all.
- **Rule 3** (referent durability) must say *why* `slug` passes where
  `escalation` and `commit` fail: `docs/devloop-outputs/<slug>/` is **git-tracked
  and committed in the same commit as the manifest write** (`run-story.sh:1201-1204`
  amends the devloop's own commit, which contains the output dir), so the
  referent and the reference travel together — unlike `escalation`'s
  container-scoped `$RUN_DIR` path or `commit`'s amend-invalidated sha. Stated
  with its honest residual: a later `git rm` of the output dir *can* invalidate
  it without the manifest being rewritten, so this is "travels with", not
  "cannot break". The remedy is the one the `docs/TODO.md` referent-durability
  entry already documents as the codebase's own positive instance —
  **revalidate at use** — which `/close-story` already does ("Missing devloop
  output dir → hard-fail with the path").
- **Rule 4** (`deps` is not `skip_serializing_if`) is unaffected; `slug` **is**
  `skip_serializing_if` on the *storage* type but is projected as
  `Option<&str>` → `null` on the wire, so consumers still get a total shape and
  `jq` never needs `// empty`.

The exit-code/stdout contract prose in the module doc is untouched.

---

## ADR-0035 §4 clause-by-clause

| §4 clause | How this diff satisfies it | Divergence? |
|---|---|---|
| Manifest holds "id, status, specialist, `env_tests`, deps, prompt, tag, **devloop-output slug** — Yes, the only place" | `Task.slug` added; runner writes it at `complete`; `/close-story` reads it | none |
| "**Delete §Devloop Tracking**" | Deleted from `_template.md`. **Diverges for `2026-05-02-browser-client-join.md`** — table left untouched (no banner; Lead Call A), with the projection spun out to owner `client`. See V12 + V25 | **yes, stated** |
| "The Ordered Task List keeps no status column and duplicates no manifest field" | `Status` column dropped from `_template.md` §Implementation Plan; `Status` **and** `Devloop Output` columns dropped from this story's §Implementation Plan after backfilling the three slugs into the manifest | none |
| "Consequence for `/close-story`: completeness is `dt-story next` exit 3" | **Not implemented — the clause is wrong.** `next` mutates (reopens escalated tasks). `/close-story` uses `list-tasks`. §4 corrected in place with a dated block | **yes, stated + ADR corrected** |
| "`complete` is idempotent … double-writer, not error" | Preserved. `AlreadyComplete` now also persists a newly-supplied `slug` (last-writer-wins), loudly on change | none |

## Schema-change safety (story §Deferred "Manifest contract")

Three properties, each checked:

1. **An existing story file still validates unedited** — false for `branch`, by
   design and by `deny_unknown_fields`. That is why the struct change and all
   six file edits are one commit, and why the carriers were found by grep
   (`grep -rln 'branch:' docs/user-stories crates/dt-story/tests scripts/workflow`)
   rather than trusted from the task text — which named 2 of 6.
2. **Required-ness stays status-aware** — `slug` is `Option<>` on the storage
   type and is enforced nowhere by the deserializer, consistent with
   `manifest.rs`'s module doc. No new `validate_manifest` violation is added:
   a completed task with no slug (every task completed before this change) must
   keep validating, or the Layer 3 guard reds on history.
3. **The runner never writes a manifest it would itself reject** — *(revised by
   V2/V9/V14; this is the current statement)* the only new write is a slug
   validated against the canonical narrow class `^[a-z0-9]+(-[a-z0-9]+)*$`, and
   `--slug` is omitted rather than passed empty. `newest_devloop_output` is **not**
   on the completion path. V14 makes this clause *mechanically enforced* rather
   than argued, via a round-trip assertion in `save()`.

## Conventions this diff holds itself to

- **No literal line-number anchors in new prose or comments.** New references
  anchor on function names (`newest_devloop_output`, `persist_resume_pointer`,
  `engine::complete`, `cmd_complete`, the `rm -f "$slug_file"` cleanup). Existing
  line-anchored text is not gratuitously rewritten.
- **One home for the manifest block's shape.** `_template.md` carries the
  *shape* (a real, minimal, `dt-story validate`-clean block). `/user-story`
  SKILL.md does **not** restate the schema — it points at the template and
  mandates the mechanical check (`dt-story validate`, fail-closed at Step 10.5).
  Two prose descriptions of one schema would be the drift this task exists to
  end. **Bonus, from @test:** `validate-story-manifest.sh` globs
  `docs/user-stories/*.md`, which includes `_template.md` — so putting the block
  in the template makes **Layer 3 validate the emitted shape on every run**, for
  free. That is the only mechanical backstop skills can have. Trap noted: the
  template's `{placeholder}` convention is YAML flow-mapping syntax, so
  `specialist: {specialist}` deserializes as a map, not a String — the block
  uses concrete example values, not braces.

---

## Revisions to the plan from Gate-1 reviewer input

Eight changes. Each is a reviewer's finding that I verified in-tree before adopting.

### V1 — HEADLINE: the fence-collision hole (@security). Verified, and worse than described.

`markdown.rs:49` closes the manifest fence on **any** line whose `trim() == "```"`.
A model-emitted prompt containing a fenced example therefore truncates the
manifest, and it is **silent**: the surviving prefix is a well-formed single
block, deps point backwards so no dangling-dep fires, and `validate_manifest`
returns **zero violations** on a manifest that has lost its tail. `run-story`
then reports ALL-TASKS-COMPLETE on a story missing tasks.

I traced the whole path and confirm security's analysis, and add the part that
changes the fix: **a per-field check in `validate_manifest` is unreachable for
the truncation case.** Truncation happens in `find_manifest_block`, *before*
deserialization — the parsed manifest never contains the offending string. So a
field-level validator cannot see it. Three-part fix instead:

1. **`markdown::find_manifest_block` bails on ANY unterminated fence at EOF**,
   not only an unterminated *manifest* fence (today's `if fence.is_yaml && …`).
   In the truncation scenario the manifest's real closing ``` opens a new fence
   that never closes, so EOF-with-open-fence fires and the file fails **loudly**
   instead of silently losing tasks. Non-heuristic — it is a genuine markdown
   well-formedness property — and ~3 lines.
2. **`Manifest::to_block_body` refuses to emit** a body containing a line whose
   trim is a bare fence, naming the fence-collision mechanism. This closes the
   *write* path completely: `add-task --prompt-file`, `complete`, `escalate` and
   `next`'s reopen can no longer corrupt a story file they merely round-trip.
   This is the check security asked for, moved to the one place it is reachable.
3. **`/user-story` emission rule**: prompts are YAML literal block scalars
   (`prompt: |`), never quoted flow scalars, and contain no fenced code blocks —
   inline single-backtick spans are fine.

Answering security's question directly: **no, this does not belong in the skill
only.** A prose rule is a model-followed convention, which is exactly the trade
`run-story.sh`'s prompt-interpolation comment deliberately refused. In scope
under clause (d) — "must not reintroduce the hazard by emitting unescaped prompt
text" — this is that hazard in the manifest medium.

### V2 — slug provenance: derive from the task's own commit, not from mtime (@test, @observability)

Both reviewers were right that my first draft was wrong, for two independent
reasons I verified:

- **@observability:** `persist_resume_pointer()` returns early when HEAD has
  moved — i.e. exactly when the devloop committed. So on the normal success path
  `$slug_file` was **never written** and the `rm -f` removes nothing. Reading
  `$slug_file` at completion would record a slug only for resumed tasks.
- **@test:** `newest_devloop_output "$start_marker"` is an **mtime race**.
  `start_marker` is touched once per *task*, not per attempt, so a retried task
  has several dirs newer than it and "newest" is not "the attempt that
  committed". Worse, `run-story.test.sh:54-65` records the measured hazard: on
  equal `%T@`, GNU `sort -rn` falls back to a whole-line compare and picks the
  lexically largest name **deterministically** — so a test would pass stably
  forever on whichever branch the alphabet chose.

**Adopting @test's alternative.** The task's own commit range contains the
devloop's `main.md` by construction, so:

```
git diff --name-only "$head_before" HEAD -- docs/devloop-outputs/
```
→ unique second path segment. Exactly one ⇒ that is the slug, and it is "the
attempt that actually committed" **by construction rather than by proxy**. No
mtime compare enters the durable record at all. `$continue_slug` is the fallback
when the git derivation yields nothing (it is authoritative for resumptions).
`newest_devloop_output` is **not** used at completion — it keeps its existing
resume-selection job, where filter-and-WARN is the right severity.

### V3 — empty/unsafe slug must be loud, and must NOT red a green task (@observability)

At the completion site the task's work is committed and the gates are green.
Flipping the manifest is strictly better than leaving it pending and
re-implementing a committed task, so this takes **no** lane. But it is never
silent: `slogerr "STORY_RUN: NO-SLUG task=N …"` naming the task id, the candidate
count, and the operator remedy (identify the dir under `docs/devloop-outputs/`
and hand-add `slug:`). Symmetric with how `newest_devloop_output` already treats
a lost resume pointer — **answered, not raised, but never silent**. This is
deliberately *not* the `|| true`-shrug pattern @semantic-guard is watching for:
the value is absent and the record says so, loudly, with a named remedy.

### V4 — `Slug` newtype with the floor in the deserializer (@paired-protocol). Adopted.

`Slug(String)`, `#[serde(try_from = "String", into = "String")]`, class
`^[a-z0-9]+(-[a-z0-9]+)*$` — the **intersection** of the consumers, not the
union. This puts one definition where `next`, `validate`, `list-tasks`,
`preflight-story.sh`, the Layer-3 guard and `/close-story` all inherit it, rather
than the three regexes we would otherwise have. It excludes `.` (so `..` cannot
reach a path component — it matches the runner's current
`^[0-9A-Za-z._-]+$` today) and excludes a leading `-` (so the value cannot
present as a flag). Protocol checked all 221 entries under
`docs/devloop-outputs/`; every real devloop dir conforms.

Stated rather than stumbled into: a deserializer-level reject makes a
hand-corrupted slug fail the **whole file** (`next` → exit 2 malformed) rather
than yielding a per-task `validate_manifest` violation. That matches in-house
style — illegal `Status` values and unknown fields already fail deserialization
by design.

**The bash-side consequence, which protocol flagged as not theirs.** If
`complete --slug X` rejects X, the runner would die under `set -e` at the
completion site *after* the gates are green — the exact outcome V3 forbids. So
`run-story.sh` applies the same class as a floor before passing `--slug`, and
warn-and-omits on violation. That is a second definition of one value, so it is
handled the way this repo already handles the identical bash↔Rust situation
(`docs/TODO.md:131`, `_gate2_binding.sh` ↔ `cross_boundary_scope.rs`): the Rust
newtype is declared canonical, the bash regex is commented as a mirror of it,
both sides are pinned by tests, and a parity entry is added to `docs/TODO.md`.
I considered removing the bash regex by calling `complete --slug` and retrying
bare on failure; rejected because `complete`'s contract is 0-or-2 with no
slug-specific code, so the retry would also swallow "task not found".

### V5 — `AlreadyComplete` arm, and the asymmetry (@paired-protocol, @test)

`engine::complete` writes `slug` on both arms; `cmd_complete` `save()`s on
`AlreadyComplete` **only when a slug was supplied and differs** — preserving
today's no-op when it is not. `CompleteOutcome::AlreadyComplete { updated: bool }`
carries that. The `"already completed — no change"` stderr line becomes a lie on
the slug path and is reworded. The opposite rules (`commit` first-writer-wins
with a warn; `slug` last-writer-wins) sit at adjacent declarations with the
justification written at the declaration site, and **one test per direction**.

### V6 — `v1` marker stays; the reason is recorded (@paired-protocol)

`MARKER` is stamped unconditionally on every rewrite and nothing parses a version
out of it, so a bump could never carry migration semantics — a v1 file would
silently become v2-marked on the first `complete`. The reason a required-field
removal does not need a bump is that there is no version skew to support: one
producer, all consumers in-tree, and `preflight-story.sh` requires a
`target/release/dt-story` built from the same tree. Recorded in `manifest.rs`'s
module doc, because "v1" currently reads as a compatibility promise the code
cannot keep.

### V7 — amend the §Deferred safety clause this change knowingly breaks (@paired-protocol)

The story's §Deferred "Manifest contract" clause one — "an existing story file
still validates unedited" — is **deliberately broken** by removing `branch`.
Fixing the files is necessary but not sufficient: left as written, the next
schema change cites a property the tree no longer satisfies. Amended in the same
commit to record that it was knowingly broken once, that the discharge was
same-commit file updates, and that the same-commit rule is the standing remedy.

### V8 — Q2 RECONSIDERED, then confirmed, on measured evidence

I seriously considered @semantic-guard's and the Lead's challenge and tested
whether a mechanical backfill of browser-client-join could let me delete its
table (which is what ADR-0035 §4 literally says). **It cannot.** Measured:

| Fact | Value |
|---|---|
| Manifest tasks | **65** |
| §Devloop Tracking rows with a `/devloop` invocation | **60** |
| Rows with an **empty** Devloop Output cell | **1** (row 20) |
| Table status tally | 59 `Completed`, 1 `Pending`, 1 `Not implemented` |
| Manifest status tally | **65 `completed`** |

So the table is neither complete (5+ manifest tasks have no row at all — the
runner-appended `audit-remediation-ts` task among them), nor internally
consistent with the manifest it would be merged into (@semantic-guard measured
the same disagreement independently). A "mechanical" backfill would require
**inventing** slugs for tasks that never had one and silently picking a winner
between two disagreeing status tallies — hand transcription under ambiguity,
which is precisely the operation ADR-0035 §4 cites as having produced the
original drift.

**Confirmed position:** keep browser-client-join's table as a frozen historical
record ~~with a banner~~ *(banner DROPPED — Lead Call A, V25)*; delete its `branch:` line only. This is a **stated
divergence from ADR-0035 §4's "Delete §Devloop Tracking"**, recorded in the
clause-by-clause table above, with the evidence above as its justification.

**The honest residual, which @observability's Phase-4 rule makes concrete.**
`/close-story` will hard-fail on a `status: completed` task with no `slug`
(naming the task id) rather than silently shortening the PR body. That makes
`/close-story browser-client-join` permanently fail. It is `Status: Complete`,
its close already happened, and re-closing is not a supported operation — but
that is a real, named residual, not an accident. The error message names the
task ids and points a human at the file's historical table. **That is a
diagnostic pointer, not a table-reading fallback** — clause (d) forbids the
skill *reading* the table, and it does not.

### V11 — @security round 2: fix 1 is layout-dependent, so `/user-story` must verify its own emission

Security re-walked V1's fix 1 and found the hole I had missed: "bail on any
unterminated fence at EOF" only fires when the fence lines remaining *after* the
truncation point leave one open. Measured on the corpus, both manifest-bearing
files have exactly **one** fence line after the marker (the manifest's own
closing fence), so the bail fires today — but that is **a coincidence of layout,
not a property of the fix**. Add one fenced block after the manifest and the
count goes even and the bail stops firing. That layout is not exotic; it is the
expected shape of `/user-story` output (`2026-04-12-mh-quic-connection.md`
carries 28 fence lines). And `to_block_body` never runs on the hand-authored
emission path, so fix 2 does not cover the primary new producer at all.

**Adopted — `/user-story` Step 10.5 becomes two blocking checks:**
1. `dt-story validate <story>` — mandatory, story is not reported Ready until 0.
2. `dt-story list-tasks <story>` → assert the returned **id set equals the
   planned id set** (and each task's `deps` match the planned graph). Truncation
   surfaces as missing trailing ids and fails loudly, **independent of fence
   parity anywhere in the file**. A numeric comparison, not a model-followed
   convention.

This is R-1's own principle applied to the new producer: *setting the redirect is
not the same as being redirected* → **emitting the manifest is not the same as
having emitted it.**

**Considered and declined:** routing `/user-story`'s emission through `dt-story`
write verbs so fix 2 covers it by construction. `engine::add_task` hardcodes
`deps: Vec::new()` with no `--deps` flag (cannot express a dependency graph) and
mandates `--tag` (an idempotency key for programmatic remediation appends, not a
planning concept). Scope growth into the crate for a guarantee check 2 already
provides mechanically.

**Known behaviour change, deliberate and tested:** fix 1 makes a 4-backtick
fence a hard failure of the whole file (it satisfies `strip_prefix("```")` so it
opens a fence, but can never satisfy `trimmed == "```"` so it never closes).
Zero occurrences in story files today. A named test asserts the message is
actionable — it must name the file and say nested/4-backtick fences are
unsupported — so the first person to write a nested fenced example gets a
diagnosis, not a puzzle.

**Sync-guard header** states that it covers the **two manifest-slug classes**
(Rust `Slug` ↔ `/close-story` read regex) and that `run-story.sh`'s
`^[0-9A-Za-z._-]+$` floors are a **deliberately separate, wider class** guarding
command-line interpolation, not manifest membership — because unifying the three
"helpfully" would silently widen the manifest class.

### V14 — final consolidated round (@observability P1-P7, @operations, @security, @semantic-guard)

**Round-trip assertion in `save()` — adopted, and it supersedes the fence patchwork as the primary read-side net (@operations).** Operations independently traced Shape A and reached @paired-protocol's conclusion, then proposed a strictly better control: after `write_atomic`, re-`load()` and assert the task id list round-trips; bail loudly if not. `save()` is the single chokepoint for `cmd_next`, `cmd_complete`, `cmd_escalate` and `cmd_add_task`, so this makes **any** corruption mechanism loud at write time — including ones nobody has thought of — and it surfaces the failure at `dt-story complete` time, attributable and with the tree in a known state, instead of at `next`-returns-`AllDone` time where it is indistinguishable from success. That last property is the decisive one: the silent-truncation signature is identical to ADR-0035 §3's "green over tests that never ran". Adopted alongside (i), (ii)-widened and the orphan detector; none is dropped, but the plan no longer claims the fence checks close the class.

**ADR-0035 §4 correction wording — @operations is right, and my draft would have created fresh drift.** I will **not** write "`dt-story next` is not read-only"; that contradicts `main.rs:241-242` ("only reopens rewrite the file — a plain `next` stays read-only"). The precise, checkable claim goes in instead: *`next` mutates on exactly one path — selecting an escalated task reopens it and saves. That path is reachable **precisely when the completeness check would fail**. A completeness probe built on `next` is therefore safe only when it passes and destructive when it fails, which is the inverse of what an audit gate must be. `list-tasks` is the read-only verb.* Also citing operations' third witness: `docs/TODO.md` already documents this mechanism, reproduced against a fixture — so the ADR correction can show the hazard was measured before this task rather than derived by us.

**P1 — the §Revisions provenance line was false as drafted, and it is the one line whose job is to be true.** My wording ("tasks 1-3 backfilled, only later tasks runner-written") reads task 4 as a runner write. It is not: the old script bytes call the two-arg `complete`. Corrected to state **all four are hand-written**, and that the first runner-written slug will appear in the next story that runs. @observability is right that main.md is not a substitute — main.md is the devloop's record; the story file is what a future reader opens, and a reader trusting the bad line would cite task 4 as evidence the runner's write path worked in production when it never executed.

**P2 — `/close-story` must discriminate history from a recording failure.** Adopted, and it also reconciles @observability's hard-fail with the Lead's adopt-@operations'-tolerate ruling. Discriminator is free and needs no schema: **does any completed task in this manifest carry a slug?**
- **None** → pre-slug-era frozen record. **Tolerate**, with a loud actionable message. This is the Lead's required behaviour, and it means a closed 65-task story cannot become uncloseable for lacking a field it predates.
- **Some, but not this one** → live recording defect. **Hard-fail** naming the task id, naming the runner's `NO-SLUG` line as the corresponding event, and naming `dt-story complete <file> <id> --slug <slug>` as the repair.
Without the split, the first real instance of a recording failure gets triaged as "just the old-story thing" — the alarm trains its own reader to ignore it. @operations reached the same finding via R-6 of this story ("when X is refused, the cause is distinguishable from the response alone") applied to my own refusal.

**P3 — NO-SLUG message: name the candidates, and split zero from many.** The runner holds the list, already floored to the slug class, so printing it is bounded and carries no uncontrolled text; sending an operator to `ls docs/devloop-outputs/` (221 entries) while holding the three-element answer is the message doing less work than it can. **Zero** and **>1** get distinct wording — zero means the commit range touched no output dir (the record was never written); >1 means the range is ambiguous. And stated explicitly because it was only implied: **>1 omits `--slug` rather than picking one.** "Pick the first" is the same deterministic-but-arbitrary selection V2 rejected `sort -rn` for, and being deterministic it would pass a test forever on whichever branch the tiebreak chose.

**P4 — adopted**: `slug=X src=commit-range` vs `src=resume-pointer` on the COMPLETE line. The two derivations have different standing, and since the stream is captured nowhere the answer has to be on the line or it does not exist.

**T-4 (@test) — adopted, and it is the sharpest composition catch.** V3 + a hard-fail compose into *a live story becoming permanently uncloseable* from one benign NO-SLUG — the harm class R-7 exists to prevent. P2's discriminator plus naming the runnable repair command in **both** messages fixes it: a diagnostic points, a command remedies.

**T-3 (@test) — `malformed.md` loses `branch:` for a stated reason**: it is malformed via `tasks: [`, and leaving `branch:` would make it malformed for a second, undeclared reason, so `next_malformed_exits_two_with_stderr_diagnosis` would pass without the condition it names being load-bearing.

**Test surface accepted in full**: T-1 (four fence cases incl. **(d) inline single-backtick spans still round-trip** — the over-rejection case that gets skipped), T-2 (Slug round-trip; invalid → whole-file `next` exit 2, pinning the blast radius as a decision; bash mirror pinned by a `run-story.test.sh` case asserting warn-and-omit rather than death under `set -e`), T-5 (`AlreadyComplete { updated: false }` asserted **byte-identical**, not exit 0 — "no change written" is only tested by a file comparison), T-6 (complete A then B, assert B — reds under first-writer-wins), T-7 (every new case asserts both halves: NO-SLUG token **and** status==completed **and** `ran.claude.devloop`; happy path asserts the slug **value**, not just that status flipped), T-8 (**execute** `validate-story-manifest.sh` and confirm `_template.md` is in the selected set — verifying by reasoning would undercut the whole point of the idea).

**Sync guard meets the full precedent** (@test, @observability): guard + self-test kept **outside** `guards/simple/` so `run-guards.sh`'s `find -name '*.sh'` does not run it as a guard + explicit wiring in `layer3.sh`, with a **drift case**. A sync guard without a drift case cannot fail and is indistinguishable from no guard — and the story's §Deferred already names "10 of 16 `scripts/**/*.test.sh` files have no invocation site" as a live gap.

**@security Q1 sub-questions.** (1) **Confirmed** — the `Slug` class is enforced at the clap boundary via `value_parser`, not only at deserialization; a bad `--slug` fails at parse time rather than getting committed and surfacing at close. This matters because Q1's mode-split creates a *new* producer: a model typing `--slug` on a command line, less constrained than the runner's derived value. (2) **My position: no**, `dt-story complete --slug` does **not** stat `docs/devloop-outputs/<slug>/main.md`. `dt-story` must not encode repo layout — `STORY_REPO_ROOT` fixtures relocate it, so the check would be wrong under test and would couple the schema engine to a directory convention. The claim is already revalidated at both *use* sites (the runner's resume check and `/close-story`'s missing-output-dir hard-fail), which is the revalidate-at-use discipline rule 3 admits `slug` under. Recorded rather than left unasked.

**Q1 residual (Lead condition 3): the second writer.** With the mode-split an interactive devloop writes the manifest alongside the runner. Safe because: they are **mutually exclusive by mode** (the skill instructs headless devloops *not* to write, precisely so the runner's post-gate write is the only one on that path); `complete` is idempotent by design for the case where they overlap anyway; and last-writer-wins on `slug` makes a double write converge rather than conflict. @security named the deeper reason and it is going in the plan: a headless devloop that self-completed *before* the gates ran would leave `engine::escalate()` unable to escalate a subsequent red gate, letting a devloop's own verdict override the pipeline's — which violates **ADR-0035 §3's pass/fail-authority separation** (authority is the runner's pipeline run, never the devloop's verdict). So the mode-split *upholds a named ADR property* rather than merely shrinking a hazard.

**P7 — stale prose in this file corrected in place** (§Schema-change safety item 3 and §Q3 Rule 2 both still cited `newest_devloop_output`'s wider class). Done. This file is what `/close-story` Phase 4 mines for the PR body, so a superseded statement here propagates.

### V20 — the detector goes in `find_manifest_block`, NOT `load()` (@security; verified)

**@security caught a placement bug that would have made V18's whole argument false.** I verified it directly rather than accepting it: `cmd_validate` does **not** call `load()` — it reads the file and calls `engine::validate_story(&text)` (`main.rs:418-426`), which calls `markdown::find_manifest_block` itself (`engine.rs:215-219`).

`validate` is the **only** verb that bypasses `load()`. Every other — `next`, `complete`, `escalate`, `add-task`, `list-tasks` — goes through it. And `validate` is precisely the verb used by the two gates whose entire job is catching a bad manifest before anything trusts it: `validate-story-manifest.sh:20` (Layer 3 / CI) and `preflight-story.sh:82` (the runner's pre-run gate).

So a detector in `load()` would be **bypassed by CI and by preflight**, firing only once the runner is already executing the story. V18's conclusion is right; its stated placement made the conclusion false — which is the load-bearing reason the residual was declared closed.

**Fix: the scan goes in `markdown::find_manifest_block`.** It is the common ancestor (`load()` calls it at `main.rs:179`; `engine::validate_story` calls it directly at `:216` as its *first* step), and it already holds both the full text and the span. **This is a SMALLER diff than V13's caller-side placement, not a refactor** (@operations): one function edited, zero call-site changes, and every verb inherits it.

Two consequences, both good:
- **Hard-error severity becomes structurally necessary rather than a preference.** `validate_story:216-218` converts an `Err` from `find_manifest_block` into a violation string, so a `bail!` surfaces with the right exit code automatically — while a WARN would be **silently swallowed**, because `validate_story` collects violations and has no WARN channel. The mechanism settles the question.
- **Tests: assert `dt-story validate` rejects a truncated fixture — and assert on `validate-story-manifest.sh` itself**, not only the verb, since the guard's greenness is what is actually being claimed.

**Shelf life of the false-positive measurements** (@security raised, @operations answered): they need no manual re-running — `validate-story-manifest.sh` re-runs them across every marker-bearing file on every CI invocation. **The guard *is* the standing measurement.** What it needs instead is an actionable failure message, which is V20b's constraint 3.

### V20b — `_template.md` auto-enrols into the Layer-3 guard; design for it (@security, @test T-8b)

The guard discovers by **content**, not name (`grep -l '# task-metadata (dt-story manifest '`), so the moment the template gains a marker-bearing block it is validated in CI like a real story. @security measured:
- placeholder form (`id: <n>`) → **hard FAIL**: `tasks[0].id: invalid type: string "<n>", expected u32`
- real minimal form (`id: 1, specialist: operations`) → passes once `branch` is removed

Three constraints on the template edit, none obvious while writing it:
1. **Real values, not placeholders** — the template must be a valid minimal manifest.
2. **Exactly one marker-bearing block** — showing both an empty and a filled example trips `found 2 manifest blocks`.
3. **No `- id:` lines outside the block — and this constraint goes IN `_template.md` as a comment beside the manifest block, with its reason, not only in main.md** (@operations). The template is a document whose entire purpose is to show examples, and it will be edited months from now by someone who has never heard of the orphan detector. They add an illustrative task table; Layer 3 goes **red repo-wide** — the guard validates every marker-bearing file, so it is not contained to the template — with an error that will not obviously mean "your example broke CI". Recording it only here leaves the trigger sitting in the file's own reason for existing. @test's T-8b: the template is *the file most likely in the whole tree* to carry an illustrative task-entry snippet in prose — that is what templates do — and such a snippet is an orphaned-`- id:` **by construction**, reddening Layer 3 on the template itself. Any prose example lives **inside** the single block, or is spelled so the predicate cannot match.

Both reviewers note the false-positive measurements covered the *current* tree, where `_template.md` has no manifest — and this task makes it the third manifest-bearing file. **T-8 execution extended:** after adding the block, run the guard **and** the detector against `_template.md` and confirm exactly one fence with no manifest-shaped lines outside it. Taken as a feature: a template that must pass `dt-story validate` in CI cannot silently drift from the schema it documents.

### V20c — three smaller adoptions

- **@dry-reviewer: `docs/TODO.md:386` is the *second* home of the stale `next` prescription**, and it sits five lines above its own refutation at `:391` (which documents that `next` rewrites the file, that this already broke the runner's clean-tree guard, and that it was reproduced end-to-end). Corrected in the same edit that closes the entry — classified **Mechanical** (superseded verb name → current one). §4:92 rides with §4:88's judgment classification. Recording their general pattern: `list-tasks` did not just add a capability, it **expired a documented prescription**; the cheap prevention is `grep -rn` the docs for the old mechanism when a change supersedes one. One command, found both homes.
- **@operations: one more test, pinning the interaction this run performs on itself.** Task 4's slug is hand-written, and the **pre-edit** runner will then call the old two-arg `complete <file> 4`, which must flip status and **leave the slug untouched**. That is not the path the new `AlreadyComplete` logic exercises. Test: *`complete` without `--slug` on a task that already carries one must preserve it.*
- **@operations: V13 item 3 gets an in-place dated supersession marker** pointing at V18, in the repo's `*(Corrected …)*` form — it is the *detailed* spec someone would implement from, it sits **after** its own correction in reading order, and it is therefore a citable-but-wrong sentence. This is the standard the story file states twice about itself and that I was held to on F1(a) and C7; it applies to my own V-series. V13's "zero false positives" result travels with the marker, since it was measured against the narrow form.
- **@test:** the four completion cases assert the `cause=`/`src=` **token**, not the sentence.

### V27 — the SKILL.md live-read exposure, and why it is narrower than first framed

**The exposure.** `docs/TODO.md:378` records that a fresh `claude -p` **live-reads `.claude/skills/devloop/SKILL.md` per task**, so a broken edit breaks the rest of an in-flight run. The story's §Notes inode-replacement safety argument covers `run-story.sh` and **does not transfer** — each task spawns a new process reading the new file. Raised by @semantic-guard.

**Task 4 is last, but that is not sufficient**, and the reason is specific: **ADR-0035 §7 has the runner auto-append an audit-remediation devloop at story close** if a language goes red. That devloop live-reads the edited skill. So the consumer at risk is not a human-invoked devloop — it is **one the runner appends, headless, after this task completes.**

**What I can and cannot do, stated without over-promising.** I **cannot** re-run the substrate probe mid-run; it lives in `preflight-story.sh`, which runs at runner start. What I do instead: keep the Q1 edit to a **two-sentence replacement inside one existing bullet**, no structural change, and re-read the file afterwards to confirm the section is intact.

**@semantic-guard's narrowing, which changes the risk class and is the reason this is not a gate.** A remediation devloop **obeying** the new instruction is *correct* — the runner completes it exactly as it completes any task, so the new text is the behaviour that consumer should have. **Therefore the live-read is a hazard only if the file is BROKEN, not if it is READ.** That reduces the risk from "a semantic change might mislead an in-flight consumer" to "do not ship a malformed file" — which the minimal-edit-plus-structural-re-read discharges directly, rather than merely mitigating.

**Two consequences for the wording, taken before writing it:**
1. **The mode-split must be unambiguous for the remediation path specifically.** If "headless devloops must not write the manifest" reads as scoped to the runner's *task loop*, a reader on the remediation path has to infer whether it applies. That is **the one consumer that will actually read the edited file mid-run**, so it is the one the text most needs to cover explicitly. The wording keys on `HEADLESS RUN` / `DEVLOOP_HEADLESS=1` — true for both paths — rather than on "task loop".
2. **Check the semantics land for that consumer, not just the syntax.** A structural re-read proves the file parses; it does not prove the instruction is right for a remediation devloop. It is — the runner's `complete` call covers it identically — but that is a thing to verify, not assume.

**HEADLINE (item 5) — emitted dep numbers are PREDICTIONS about ids `dt-story` will assign, and that is a silent-corruption path.** Verified: `add_task` assigns `new_id = max(existing) + 1` (`engine.rs:184`); the skill cannot choose ids. So `--deps 1,2` holds only while every append happens in order, from an empty skeleton, with none skipped. Break any of those and each subsequent dep **silently points at the wrong task — and it still validates**, because the ids all exist. **Id set intact, meaning wrong** — the identical shape @security used to overturn the id-based controls, arriving through a different door. The `Exists` short-circuit makes it *reachable* rather than theoretical: a re-run against a partially-populated file returns rc 4 **without appending**, so the max-id sequence a later call sees is not the one the emitter assumed.

**Remedy is free — the CLI contract already supports it.** `add-task` prints the assigned id on stdout on **both** arms, `Added` (rc 0) and `Exists` (rc 4). So **the skill reads ids back and maps its own task numbering onto assigned ids; it never predicts them.** With `--tag` mandatory and deterministic, the mapping is exactly tag → returned id, and it is resumable by construction.

**Stated explicitly because it does NOT follow from the tag decision.** "Deterministic tag ⇒ emission is idempotent and resumable" is true and is about *emission*. "Deps point at the intended tasks" is a **different property**, and the tag alone does not give it. Two arguments, neither of which finds the other's instance — the pattern the referent-durability TODO was written about. *(Recorded rather than left implicit: `--deps` could take **tags** and resolve internally, leaving nothing to predict. More surface; not taken.)*

**Item 1 — `--deps` shape.** Comma-separated single token (`--deps 1,2,3`), parsed by **our own function, not clap's `value_delimiter`**. A single token is trivially quotable and auditable in an emitted command line, where repeated flags force bash array-building and a variable-length command line — first-class concerns after R-2 defect 5 — and it mirrors the manifest's own YAML sequence. Explicit rejections, documented like the crate's other contracts: **`--deps ''` is a hard error** (a skill with an unset variable emits exactly that, and a silently dep-free task is the masked-failure class this story exists to kill; "no deps" is expressed by **omitting the flag**); empty element (`1,,2`) errors; non-numeric errors naming the element; **duplicates error rather than silently dedupe**, because a repeated dep is a caller bug and deduping hides it.

**Item 2 — validate deps at append time, in DELTA form.** My "one home" objection doesn't apply: calling `validate_manifest` from `add_task` **reuses** the single home rather than creating a second. What decides it is §Deferred clause three — *"the runner never writes a manifest it would itself reject"* — and `add-task` **is** the runner (`run-story.sh:882`). Appending a dangling dep writes a manifest `validate` immediately reds, breaking a clause this task is meant to strengthen. **Compute violations before and after; refuse only if the append INTRODUCED new ones.** Two reasons, and the second is the one I'd have missed: an absolute check turns `add-task` into a surprise gate on unrelated pre-existing state, and the audit-remediation path appends to a **live, mid-flight** story that must not red for something else in the file — and, decisively, **the delta form is what makes V24 compose**: with the zero-task predicate the skeleton legitimately carries a "no tasks" violation in `before`, which a delta ignores by construction while an absolute check would need a special case *in the gate that guards the skeleton*, which is exactly where a hole gets drilled later.

**Item 3 — params struct, for the TYPE reason not the arity reason.** Five args is aesthetics; the hazard is that `specialist: String`, `prompt: String`, `tag: String` are **three adjacent same-typed parameters** — transpose any two and it compiles cleanly, producing a task with the specialist recorded in the tag field. Silent corruption in the very struct this task makes authoritative. ~8 LoC, one caller. *(Alternative recorded: a `Tag` newtype kills transposition by type, symmetric with `Slug`, slightly more surface. Doing neither is the only wrong answer.)*

**Item 4 — v1-no-bump confirmed for `--deps`, and TWO corrections I owe.** `--deps` is additive, `deps` already exists on `Task` with `skip_serializing_if`, no serialized shape changes — **confirmed, nothing in protocol's reasoning moves.** But the **zero-task predicate** bears on this question and I had not listed it:
- **It is a SECOND knowing break of §Deferred clause one** ("an existing story file still validates unedited"), independent of `branch` — a manifest that validated yesterday (`tasks: []`) reds tomorrow. The amendment currently frames `branch` as *the* deliberate break. **There are two.** If the record names one, the next author finds the clause broken by something unexplained, which is precisely the failure the amendment exists to prevent. Still no marker bump — a validation tightening is not a schema change, and the marker cannot carry it.
- **`manifest.rs`'s module doc (`:3-7`) goes stale in the same commit that makes it authoritative.** It frames `validate_manifest` as enforcing *per-status* field requirements; the zero-task rule is **manifest-level and not status-aware — the first such check**. Updated in the same commit. This is **clause 4 of the standing review question, caught by a reviewer applying it to my own diff** — the fourth instance in this changeset.

**Item 6 — orphan detector: re-add the end anchor. `^\s*-\s+id:\s*\d+\s*$`.** Protocol kept @operations' leading-whitespace tolerance but restored the terminator, measured:

| line | unanchored | anchored |
|---|---|---|
| `- id: 2` / `  - id: 2` | match | match |
| `- id: 5 is the one that broke` | **match** | no |
| `- id: 3 (see above)` | **match** | no |

Tree-wide the forms are **identical today** (69 matches each, all inside manifest blocks, 0 outside), so the anchor costs nothing now and removes the prose exposure later — and this is a **hard error feeding two gates**, so a future story file merely *discussing* `- id: 3` in a sentence would red Layer 3. Truncation detection is unaffected: the orphaned `- id: N` line is bare in emitted YAML.

**Framing precision — latent, not active.** Both remediation docs have **zero fence lines today**, so `run-story.sh:882-885` is a **latent** instance. The plan says *latent-not-active* rather than letting a reader infer the tree is currently corrupting prompts — the difference between "fix now" and "fix now *and* audit history."

### V25 — Lead's Calls A and B, and the standing review question

**Call A — banner spun out with the dedup.** `browser-client-join.md` gets the `branch:` deletion and **nothing else**. Zero unsatisfiable-ownership rows in the table. The spin-out entry names the residual explicitly (the file holds a second home for the slug mapping with **nothing in-file marking it superseded**) and carries @dry-reviewer's projection recipe plus @semantic-guard's 51-of-65 measurement, so `client` inherits the evidence rather than re-deriving it.

**Call B — approved, and V23 below spends LESS than was approved.** Bound 1 (`--deps` and optional `--tag`, nothing else) is met more tightly: **`--tag` stays mandatory**, so the growth is `--deps` alone. Bound 3's question is answered in V23 rather than deferred. Bound 2 (@paired-protocol weighs in *before* building) is outstanding and is a gate on implementation, not a courtesy. Bound 4 (@test gets the cases) is folded into the test set.

**Ruling 3 reversal recorded as shared**, per the Lead: the void premise (Step 10.5 gives id-set integrity; R-8 requires prompt integrity) was mine to state and theirs to endorse over a standing @security warning that the skill route was "strictly better". The plan's history shows the reversal rather than a quiet substitution.

**STANDING REVIEW QUESTION** (Lead's instruction — promoted out of this devloop's narrative):

*(Rewritten at @code-reviewer's Gate-1 critique. My two-clause version covered four of six and missed the sharpest finding in this devloop. Their growth rule replaces mine — "one clause per failure mode" is how checklists die: six incidents, one clause each, and in a year it is a litany nobody runs, which is itself a control that stopped being reached.)*

**Four clauses, capped at four.** Each is phrased as a **question with a mechanically checkable answer**, on the model of the repo's own referent-durability rule (*"can this referent stop resolving while the artifact stays byte-identical?"*) — what makes that rule work is not its coverage but that someone can apply it in ten seconds and know whether they passed. **A fifth incident should sharpen an existing clause before it earns a new one.**

> 1. **What exactly does this control compare** — and is that the property that breaks? *(Findings #5, #6: Step 10.5 compares ids; the loss is inside a prompt. The skeleton's ids are valid; the broken property is its assertion of completeness.)*
> 2. **Which call paths reach it?** *(Findings #1-#4: `cmd_validate` bypasses `load()`; the regex was anchored where the input isn't.)*
> 3. **Does it run before or after the transformation that destroys the evidence it needs?** *(@code-reviewer's addition, and V22 proves it: a per-field check in `validate_manifest` is not merely on the wrong path, it is **structurally unreachable** — `find_manifest_block` discards the tail before deserialization, so the parsed manifest never contains the offending bytes. The path does reach the control; **the evidence does not survive to it.** Different mode, different fix — move the control upstream of the transformation, which is what (ii) does. Neither clause 1 nor 2 finds it.)*
> 4. **Does the prose explaining this control still describe it?** *(Three instances in this diff alone — `TaskSummary` Rule 1, Rule 2, and `cli.rs`'s empty-manifest doc comment. In every case the control is fine and the **sentence** silently stopped being true, with nothing failing.)*

**Clause 4 has a mechanical remedy the other three lack, and it converts a review question into a construction rule: phrase rules as claims a test can assert, so decoupling reds instead of rotting.**

**Placement — @code-reviewer is right and I was wrong.** I said this would go in main.md "as a durable artifact, not just this devloop's narrative." But **main.md *is* this devloop's narrative** — that is what the file is. A standing question living only here is read once, at Gate 3, by people who already know it. Recorded here now; **a `docs/TODO.md` entry promotes it to `.claude/skills/devloop/review-protocol.md` or `docs/specialist-knowledge/code-reviewer/INDEX.md`, owner `code-reviewer`**, who has said they will pick it up. Both are shared surfaces outside this changeset, and expanding scope into them at this point in the story is exactly what the entry exists to avoid. Otherwise this diff ends by producing a rule about controls that aren't reached, in a location nothing reaches.

Six findings in this review were each **correct in mechanism and wrong in placement or scope**, and every one was found by *executing* the path rather than reading its description:

| # | control | correct in | wrong in | found by |
|---|---|---|---|---|
| 1 | fence EOF bail | mechanism | scope — 1 of 3 shapes | @paired-protocol, then @security's fixtures |
| 2 | orphan regex | mechanism | anchoring — column 0 vs indented | @operations |
| 3 | orphan placement | mechanism | path — `cmd_validate` bypasses `load()` | @security |
| 4 | whole-file scan claim | mechanism | scope — spec is outside-span | **@semantic-guard auditing their own finding** |
| 5 | Step 10.5 id-set | mechanism | **property** — compares ids, loss is inside a prompt | @security |
| 6 | skeleton `tasks: []` | mechanism | **state** — valid manifest asserting completeness | **@security auditing their own proposal** |
| 7 | `:(glob)` verification fixture | mechanism | **comparand** — compared post-`cut` output, not the pathspec's own match set | **the implementer, self-caught** (near-miss) |
| 8 | Gate-2 verdict | mechanism | **proxy** — a private shell wrapper's own `echo $?` read as the pipeline's verdict (its real token is `TOTAL_RESULT=`) | **@team-lead, self-caught** |
| 9 | `.claude/skills/devloop/SKILL.md:379` | — | **prose** — asserts fail-fast; `layer-all.sh:107` deliberately does not | the implementer, contradicting the Lead acting on it |
| 10 | the FIX for #9 | — | **nonexistent referent** — told readers to consult `LAYER_ALL_RC`, which no in-tree artifact emits | @semantic-guard, by `grep` |
| 11 | @semantic-guard's own verification | mechanism | **silent truncation** — `grep -c` returning 0 exits non-zero, so their `&&` chain stopped before printing; they nearly confirmed prose they had not read, **and said so** | **@semantic-guard, self-caught** |
| 12 | the fix for #10 | — | **convention** — the Lead's suggested citation `scripts/layer-all.sh:103-114` used bare line numbers, which `validate-doc-citations-no-line-numbers` forbids and which contradicts this diff's own stated rule | **a guard, mechanically** |
| 13 | the commit's own `Approved-Cross-Boundary:` trailers | — | **format** — placed in their own paragraph, so `git interpret-trailers --parse` returned only `Co-Authored-By`; ADR-0024 §6.7 requires them parseable | **@team-lead, by running the parser** |
| 14 | editing `main.md` after its own commit | — | **sequencing** — a modified `main.md` marks the devloop active; the guard then requires every file in its classification table to be in the *working* diff, which post-commit they cannot be | the implementer, **by running the guard** |
| 15 | the option analysis for #14 | — | **referent** — priced (a) and (b) against a commit sha that `run-story.sh` amends as its very next action; both options weighed a durability that did not exist | **@team-lead** |

The question has **one clause per failure mode**, because #5 and #6 are misses in *what property was checked* and #1-#4 are misses in *which path reached it*; a single-clause version would find only one family.

**Instances 7-9 surfaced during Gate 2, in the REVIEW PROCESS rather than the artefact under review — and #9 changes clause 4's standing.** #7 and #8 are near-misses caught by their own authors. **#9 is not a near-miss: it produced a wrong decision by someone acting on it** — a Lead read a red Layer 2 as meaning layers 3-7 had not run, then attributed the fail-fast claim to ADR-0033 §4, which says only "sequentially" and contains no fail-fast language at all. Every other prose-decoupling instance in this diff was *"nothing failed, the sentence just stopped being true"*; this one **inverted a control that a fresh `claude -p` live-reads per task**. That is the difference between tidiness and a defect class, and it is the argument for promoting the review question out of this file. Filed at `docs/TODO.md` (question → owner `code-reviewer`; the `SKILL.md:379` fix → owner `operations`), and deliberately **not** fixed here: a second edit into that file would widen a live-read exposure accepted on the grounds that the first was two sentences inside one bullet.

**Five layers, three self-caught**: production control, test fixture, verification fixture, gate verdict, live-read prose.

**WHAT THE PANEL IS ACTUALLY FOR — and it is not "more eyes."** Every defect in this review was found by someone **executing a path rather than reading a description of it**, and several were found by people auditing conclusions they had already reached and published:

| who | audited | outcome |
|---|---|---|
| @observability | their own RESOLVED-FIXED | amended to HOLD on finding F-4 existed *because of* their own F5 — they asked for `cause=git-error`, verified it was emitted, and had not checked whether the artifact asserting the closed set kept up. Flagged the provenance rather than letting it read as someone else's finding, in the same round they pressed three other people on that class |
| @test | their own F-2 | withdrew the first version: it asserted `cause=unsafe-class` only, and would have gone green against all three symptoms |
| @semantic-guard | their own findings | recorded **two of five wrong** — orphan-scan scope inferred from a regex without reading the spec; Step 10.5 read as a subset test when it was set equality |
| @dry-reviewer | their own proposal | withdrew the column projection after their premise was falsified |
| @paired-protocol | their own sign-off | separated a Gate 1 confirmation from a hunk ACK so the ACK could not become retrospective |
| @team-lead | their own rulings | overruled themselves twice — the wrapper exit status, and the `SKILL.md:379` scope call |
| the implementer | their own verification fixture | first `:(glob)` fixture compared post-`cut` output, not the pathspec's match set, and returned a confident wrong answer |

**PRIMING DOES NOT HELP.** This is the claim the evidence supports, and it retires every softer version. Instance #10 was **introduced by the fix for instance #9** — written by the implementer who had just diagnosed the class, instructed by a Lead who had just been burned by it, and reviewed by the reviewer whose entire lens *is* this class. Three people at maximum attention on exactly this failure mode. **None of them caught it by reading. It fell out of a `grep`.**

Its provenance makes it sharper still: `LAYER_ALL_RC` was never a pipeline token. It was a variable name in the Lead's *private shell wrapper* (`./scripts/layer-all.sh > log; echo "LAYER_ALL_RC=$?"`), used in Gate-2 messages as though it were the pipeline's own, propagated in good faith into a live-read doc, and survived three review rounds. **The sentence written to prevent the defect instructed the reader to reproduce it.**

"Be careful", "review closely" and "more eyes" are each refuted by that single data point — and #11 refutes them again from the reviewer's side. @code-reviewer's independent observation is the same finding from the other end: **not one** prose-decoupling instance in this changeset was caught by someone reading for correctness. Every one came from executing a path, running a `grep`, or opening the artifact to see what it actually did.

**#13 is in the commit itself, and it is the cleanest instance in the table.** The two `Approved-Cross-Boundary:` trailers — which @test and @paired-protocol each spent a round wording precisely, and which @code-reviewer named as the one item they could not clear from the working tree — were written into the commit message in their own paragraph, separated from `Co-Authored-By:` by a blank line. `git interpret-trailers --parse` returned **only `Co-Authored-By`**. ADR-0024 §6.7 requires RFC-5322-style trailers parseable by that tool, so the audit breadcrumb was *present in the message and invisible to the tool that reads it*: **a control that appears connected and is not.** Caught by running the parser, not by reading the message. Amended; all three now parse (verified post-commit at `73fb511`).

It is also @code-reviewer's own point resolved by the mechanism they were arguing for. They were right that trailers cannot be checked from the working tree — and the check that settles it is **one command**. That is the `code-reviewer`-owned guard proposal in miniature: not "read the trailers carefully" but `git interpret-trailers --parse` in a hook.

**#14 and #15 arrived while recording #13, and #15 is the sharpest self-inflicted one in the table.**

**#14** is a sequencing trap, not a mistake: table correct, commit correct, guard correct. The combination is reachable *only* by touching `main.md` after its own commit — which is exactly what recording #13 required. Measured: clean tree `STATUS=OK`; with the edit, `drift-planned-untouched-21`, naming `crates/dt-story/src/manifest.rs` and 20 others that are in `73fb511` rather than the working tree. Found by running the guard, not by reading the diff. Filed as a live workflow trap (`docs/TODO.md`, owner `operations`) because its next victim will read it as their edit being wrong rather than as a constraint on when the edit may be made.

**#15 is mine.** I presented three options and priced two of them on "preserves / rewrites the sha `73fb511`". But `run-story.sh:1343-1345` does `git add "$STORY_FILE"` then `git commit --quiet --amend --no-edit` — **the runner amends this exact commit as its next action**, to fold in the manifest bump. The sha had minutes to live no matter what anyone chose. So (b)'s stated benefit preserved something already doomed and (a)'s stated cost was being paid regardless.

**I edited that amend call in this very diff**, wrote the plan section explaining that the manifest bump folds into the devloop's own commit, and still reasoned about the sha as a fixed referent one message later. That is **this story's own referent-durability rule** — *can this referent stop resolving while the artifact stays byte-identical?* — applied to a commit instead of a manifest, missed by the person who filed the rule's re-verification. Knowing a mechanism and *applying* it to the object in front of you are different acts, and only the second is a control.

**#12 closed the argument while it was being written.** Fixing #10 meant adding a citation; the Lead proposed `scripts/layer-all.sh:103-114`; I applied it — and `validate-doc-citations-no-line-numbers` reded, because bare line cites are forbidden repo-wide **and because this diff's own §Conventions commits to anchoring on greppable text rather than line numbers.** Neither of us caught it; an existing mechanical check did, in 2.5 seconds. Re-anchored on the quoted `set +e` comment, which `grep -cF` confirms is unique in the file.

That is the thesis demonstrated rather than argued: **the only things that caught #9, #10 and #12 were `grep`, `grep`, and a guard.** Attention caught none of the three, across a Lead, an implementer and a reviewer all primed on precisely this failure mode.

**The remedy is mechanical, not attentional**, and it is now a concrete proposal in the `code-reviewer`-owned `docs/TODO.md` entry rather than a lesson: *does every identifier this prose names in backticks actually appear in the artifact it describes?* Same shape as `validate-slug-class-sync.sh`, which this devloop already built for the slug class — and it would have caught #9 and #10 both.

**THE COUNT INCLUDES AN INSTANCE CREATED DURING THE ROUND THAT CLOSED THE OTHERS.** Stated plainly, because a retrospective claiming the class was eliminated would itself be an instance of the class:

- **F7** — the pre-partition comment stranded above the `complete` call by *my own* fix, describing a two-cause world that had become five, sitting where a reader lands.
- **F4** — the emitter's `*)` catch-all silently aliasing to `no-record-in-range`, so any unrecognised cause reported that specific factual claim. That is F1's failure mode **left latent inside F1's own fix**, and it stopped being hypothetical the same round, when a fifth cause (`git-error`) was added without an arm.

- **#10** — the `LAYER_ALL_RC` token, introduced by the fix for #9 and caught only by `grep`.

Fifteen instances, **seven** introduced during the round that closed the others (#7, #4, #10, #15 mine; #12 a Lead suggestion I applied without checking it against a convention I had myself written down; #13 in the commit that shipped the fixes for the other twelve; #14 reached only by the act of recording #13). The class was **not** eliminated; the controls that catch it were improved and the count is honest. A retrospective claiming otherwise would itself be an instance.

**THE ARGUMENT FOR THE PANEL — four reviewers audited their own conclusions and found them wanting.** This is the strongest evidence in the plan that the failure mode is **structural, not a matter of individual care**:

| reviewer | audited | found |
|---|---|---|
| @semantic-guard | their own orphan-detector finding | inferred the implementation from the regex without reading the spec — **ADR-0035 §4:88's own failure, committed by the reviewer who had diagnosed it twice in this review** |
| @security | the route-(a) design they had just argued for | its skeleton state is a valid manifest asserting completeness |
| @dry-reviewer | their own column-projection proposal | withdrew it after checking its premise |
| @operations | their own ruling that a residual was closed | the stated reason was false |

The transferable lesson is **not** "be careful" — that is not transferable. It is **"have someone run the path"**: all six defects were found by *executing*, none by reading a description. That is what the panel buys and why the coverage table records what each control does **not** cover.

**Process note — Lead judgment, recorded so it is visible rather than inferred.** Planning has run past this skill's nominal 3-revision-round limit. Headless, the mechanical equivalent would be a terminal escalation killing the story. The Lead declined, on the grounds that the limit exists to catch **non-convergence**, and this is converging: reviewers confirmed progressively, finding scope narrowed each round, and every round produced a **measured defect** rather than a re-litigation. Round 6 found two defects in the *fix* adopted in round 5 — which is convergence on the fix, not divergence of the plan.

### V24 — @security: the route-(a) skeleton is a valid manifest that asserts the story is COMPLETE

@security audited their own proposal and found the intermediate state is a new instance of the class this review has spent six rounds closing. Measured on the current binary:

```
skeleton:    story: test-skeleton / tasks: []
validate:    PASS (exit 0)
list-tasks:  []
next:        exit 3   <- ALL DONE
```

And `run-story.sh:872` on rc 3 → *"All tracked tasks done"* → `break` → story-close gate. **An interrupted `/user-story` leaves an artifact that does not look broken — it looks finished.** Step 10.5 catches it *if Step 10.5 runs*, and the entire premise of an interruption is that later steps do not.

**Fix: `validate_manifest` rejects a zero-task manifest.** One predicate. There is no valid state it excludes — at planning time the manifest holds the planned set, and after completion tasks remain in the list marked `completed` rather than being removed.

**Verified the composition claim myself, because the whole fix rests on it:** `cmd_add_task` goes `load()` → `find_manifest_block` → `Manifest::from_yaml` and **never calls `validate_manifest`** (`main.rs:334-352`). So the transient skeleton stays fully usable by the one tool whose job is to populate it, while never passing the gate that declares a story runnable. **Transient by construction, not by convention.**

It inherits both gates free: `preflight-story.sh:82` runs `validate`, so a skeleton left on disk makes the runner **refuse to start** rather than report a complete story; `validate-story-manifest.sh` makes a committed skeleton **CI red** — the durable protection that survives everyone forgetting this conversation.

**Collision check done, and it is clean:** the three existing `tasks: []` fixtures are `markdown.rs:110,135` (unit tests of `find_manifest_block`) and `cli.rs:736` (`list_tasks_empty_manifest_emits_empty_array_not_empty_stdout`). **None goes through `validate_manifest`**, so none breaks — and `cli.rs:736` in particular must **not** be "fixed", since its `[]`-not-empty-stdout guarantee is asserted on by `preflight-story.sh`. Noted so an implementer does not helpfully collapse the pair.

**But that test's DOC COMMENT is falsified by this change — a third live instance of clause 4** (@code-reviewer, verified at `cli.rs:729-731`). It reads:

> *"Nothing in `engine::validate_manifest` rejects an empty task list, so this is a reachable input, not a hypothetical one."*

V24 makes that sentence **false**, and **the test still passes**, because `list-tasks` never calls `validate_manifest`. Nothing reds; the prose just silently stops being true — exactly the mode of Rule 1's "every field has a named consumer in `run-story.sh`" and Rule 2's citation of `newest_devloop_output` after V2 removed it. The fixture also carries `branch: b`, so the file is being edited regardless. Rewritten to what will then be true — and it is a **sharper** statement of why the test exists than the original: *the input is reachable **for this verb**, because `list-tasks` does not validate, while `validate` now rejects it.* Phrased as a claim a reader can check against both code paths, per clause 4's construction rule.

**Test asserts BOTH halves**: `validate` **rejects** a zero-task manifest **and** `add-task` **accepts** it. That pair is the invariant; testing one half would let a later cleanup collapse them.

**RECOVERY GESTURE — INVERTED by V23, stated rather than carried forward.** @security originally asked the skill to document *"do not re-run emission; reset to skeleton and re-emit"*, because untagged `add-task` calls would append rather than reconcile. **With deterministic tags that instruction is now wrong**: re-running emission **converges**, so re-running IS the correct gesture and the warning would send an operator to do unnecessary manual surgery. Carrying it forward would be the same defect class this review has spent six rounds closing — a stale instruction that still reads as current. The two failures keep **separate** remedies:

| failure | remedy |
|---|---|
| emission interrupted **after** ≥1 `add-task` | **re-run emission** — converges by tag |
| interrupted **before any** `add-task` (bare skeleton on disk) | V24's zero-task predicate — `validate` rejects, preflight refuses to start, CI reds |
| plan **revised** via `--continue` | reset-to-skeleton + re-emit, **refusing if any task is non-`pending`** (V23) |

### V23 — @semantic-guard: keep `--tag` MANDATORY; `/user-story` passes a deterministic one

**Adopting the fix. Correcting one step of the reasoning, because I verified it and it does not hold.**

The mechanism is exactly as stated: `add_task` matches on `t.tag.as_deref() == Some(tag.as_str())`, so a `None` tag can never match and **every call appends**, with ids `1..N` then `N+1..2N` (`max + 1`). `validate` passes — every task is well-formed.

**But Step 10.5 does catch it.** I specified **set equality** ("the returned id set **equals** the planned id set"), and `{1..40} ≠ {1..20}`. @security independently reached the same reading ("catches it — extra ids, not missing ones — so it fails loudly"). So the "40 devloops, 20 duplicates" scenario is a loud failure at emission time, not a silent one. Recorded because a finding adopted on a premise I have verified false would be the exact failure this review keeps finding — and @semantic-guard set that standard themselves one message earlier.

**The conclusion survives the correction, on four independent grounds:**
1. **Idempotent-by-construction beats detect-after-the-fact.** A converging re-run needs no cleanup; a *detected* duplicate needs a human to delete 20 entries by hand.
2. **Resumable emission** — a partial Step 10 can simply be re-run. Step 10.5 does not provide this at any price.
3. **It shrinks the crate growth the Lead just approved, from `--deps` + optional `--tag` to `--deps` alone** — tighter than Call B's bound 1.
4. **`add-task`'s contract stays true.** `main.rs:104` ("idempotent on --tag") and `manifest.rs:55-59` keep describing reality, and `EXIT_TAG_EXISTS` stays reachable rather than becoming dead for the highest-volume caller.

**Tag form:** `story-{story-slug}-task-{n}` — both values known at emission time, no collision with `audit-remediation-{lang}`.

**This answers the Lead's bound 3 directly:** the audit-remediation idempotency guarantee is **untouched**, because `--tag` never becomes optional. `run-story.sh`'s rc-4 lane ("remediation already appended but the audit is still red" → exit 1 with its operator message) keeps working unchanged, and no new duplicate-append path is introduced. The answer to "can a re-run of `/user-story` or a `--continue` now append duplicates where it previously could not" is **no** — the opposite: it now converges where an untagged design would have duplicated.

**The emission loop must check the exit code of EVERY `add-task` call. This is not hygiene — it is what makes route (a)'s prompt-integrity guarantee real** (@security, verified in code).

Two facts from `cmd_add_task`:
- `save()` runs **only** on `Added`. On `Exists` the file is never written, so a *revised* prompt for an existing tag is **discarded, not merged**.
- `Exists` prints the id **on stdout** *and* exits **4**. So `id="$(dt-story add-task …)"` succeeds and returns a plausible id while the write it requested silently did not happen. That is the read-the-value-ignore-the-status shape this review keeps finding. `run-story.sh:881-887` gets it right — captures `add_id="$(…)"`, then reads `add_rc=$?` separately, under `set +e`. The emission loop must do the same.

**The bigger consequence, which is mine to state because it is V22's whole point:** `to_block_body`'s fence refusal surfaces as **rc 2**. That refusal *is* the by-construction prompt-integrity guarantee route (a) was adopted for. **A loop that ignores exit codes converts the guarantee back into nothing** — the task is silently skipped, and the failure resurfaces late and confusingly as a Step 10.5 id-set mismatch instead of at the call that hit it, naming the offending prompt. So exit-code checking is load-bearing, not defensive.

**Three arms, and rc 4's benign-ness is CONDITIONAL — stated because I had it unconditionally wrong.** Earlier I wrote that rc 4 is simply benign. It is not: two different situations produce it and `dt-story` cannot distinguish them, because it never compares prompts.

| rc | situation | handling |
|---|---|---|
| 0 | appended | record id |
| 4 | tag present, **same** prompt — resumed emission | benign, continue |
| 4 | tag present, **revised** prompt — `--continue` | **the revision was silently dropped** |
| 2 (or any other) | bad prompt file, **fence refusal**, write failure | **hard stop**, surface stderr |

**rc 4 is safe to treat as benign ONLY because V23's `--continue` control resets the block to the skeleton before re-emitting** — after a reset no tag exists, so the divergent row is unreachable by construction. **That coupling is load-bearing and is recorded at both sites**: drop the reset and rc 4 silently becomes a stale-prompt vector. This is the same shape as V18's "the regex form is load-bearing" — a later simplification that looks locally correct reopens the hole.

**NEW HAZARD deterministic tags create, named rather than discovered later.** `/user-story --continue` **revises** plans (Continue Mode step 7, "revise affected sections in place"). Re-running emission with the same tags hits `Exists` and **does not update the prompt** — so a revised task silently keeps its stale prompt. That is the silent-staleness class this whole task exists to close, and it does not exist under the untagged design.

**Control:** on `--continue`, the skill resets the manifest block to the skeleton and re-emits, so no stale entry can survive. **Fail-closed guard:** it refuses to reset if **any** task is not `pending` — i.e. the story has already started — and escalates instead, because resetting would destroy completed tasks' `status` and `slug`. This composes with V24 exactly as @security described: the reset's transient zero-task state is usable by `add-task` and rejected by `validate`, so an interrupted revision cannot leave an artifact claiming completeness.

### V22 — HEADLINE, LATE: every control aims at TASK loss; the defect's actual harm is PROMPT truncation

@security reproduced this end-to-end on the current binary. It is the most consequential finding of the review and it **reverses my decline of the producer-side route.**

**A machine producer of manifest prompts already ships**, which nobody (including me) had discussed: `run-story.sh`'s audit-remediation path splices a **markdown document** into the manifest —
`add-task --prompt-file <(tail -n +2 "scripts/lang/${red_lang}/audit-remediation.md")`. So `/user-story` is not the first; this one runs unattended whenever an audit goes red. Given that doc a fenced command example — the single most natural edit anyone would make to a remediation doc — the measured result was **7 lines in, 2 lines out**. `serde_norway` emits a literal block scalar with the fence indented four spaces; `find_manifest_block` trims before comparing, so indentation does not save it.

**The task still exists and still runs. It is just told something different.** That is R-2 defect 5's harm verbatim.

**Measured control coverage — all five green while the instruction is corrupt:**

| control | result |
|---|---|
| `dt-story validate` | **PASS** — does not catch |
| Step 10.5 id-set assertion | ids `[1,2]` intact — **does not catch** |
| V13/V20 orphan `- id:` detector | 0 orphans — **does not catch** |
| fix (i) EOF bail | silent — does not catch |
| `dt-story next` | exit 0 — **hands the truncated prompt to the devloop** |

**The reason is structural, and it indicts my whole control set: every one compares *ids*, and the loss is *inside a prompt*.** The id set is exactly the wrong invariant — it is intact **by construction** whenever the fence lands inside the last task's prompt rather than before a later task.

**`to_block_body`'s refusal covers the write paths, and for audit-remediation that is enough** — `add-task` fails, `add_rc` falls to `*)` → `exit 2`, operator lane, manifest untouched. Correct lane, fail-loud.

**It is NOT enough for `/user-story`, and my justification for declining the producer route is now void.** I declined route (a) on the grounds that Step 10.5 "already provides the guarantee mechanically". @security is right that this no longer holds: **Step 10.5 provides *id-set* integrity, not *prompt* integrity** — and prompt integrity is the property R-8 explicitly names ("must not reintroduce the hazard by emitting unescaped prompt text").

**ADOPTED — route (a): `/user-story` emits through `dt-story`.** The skill writes a skeleton (`story:` + `tasks: []`), then calls `add-task --specialist S --prompt-file F --deps … --env-tests` per task, then runs the Step 10.5 gates. This buys:
- `to_block_body`'s refusal as a **by-construction** guarantee for the primary new producer — closing task loss **and** prompt truncation in one mechanism;
- prompt bytes never on a command line (`--prompt-file`), the property task 1 established;
- the manifest's *shape* having exactly one home — **the Rust struct** — which is @dry-reviewer's item 6 that I previously declined;
- ADR-0035's deferred "the runner never writes a manifest it would itself reject" for free.

**Crate cost, stated honestly as scope growth:** `--deps` (parsed to `Vec<u32>`) and `--tag` made optional on `add-task`. My earlier objection — `deps: Vec::new()` hardcoded, `--tag` mandatory — was a real constraint and remains the *cost*; it is no longer a *reason*, because the benefit it was weighed against turned out not to exist. Paired with @paired-protocol, who is already reviewing this crate.

Rejected alternatives: **(b)** extending Step 10.5 to compare prompt *content* would require weakening `TaskSummary` rule 2's prohibition on projecting `prompt` — load-bearing for keeping prompts off command lines, and I will not trade it. **(c)** accept-and-document, refused on R-8's explicit wording.

**Two more files need `_template.md`'s treatment**, regardless: `scripts/lang/rust/audit-remediation.md` and `scripts/lang/ts/audit-remediation.md` are manifest-prompt **sources**. Both measured clean today (0 fence lines), but adding a code block to a remediation doc is an obviously reasonable edit that would arm an operator-lane failure reachable **only during an unattended run with a red audit**. Same one-line comment with its reason, and the same guard.

**Regression test asserts on the PROMPT CONTENT, not the id set** — from `/tmp/fencetest/rem.md` + `remdoc.md`.

### V21 — @semantic-guard's regex finding: scope-limited, but the SSoT half is adopted

@semantic-guard built a fixture whose prompt discusses task ids in prose (`- id: 7 was struck during planning`) and measured `^\s*-\s+id:\s*[0-9]+` → **3 matches** against `list-tasks`' **1 task**, hallucinating ids 7 and 9. Real shape, not contrived — task 53's prompt in `browser-client-join.md` does this, and **this task's own prompt does too**.

**Measured response: the false positive requires a *whole-file* scan, which is not the spec.** V13/V20 scan **only the text OUTSIDE the located span**. A prompt inside an intact manifest block is inside the span and is never scanned. I ran the outside-span predicate across all 8 story files:

```
zero outside-block matches, every file
```

So the demonstrated failure does not reach the specified detector. **This is the third instance of @operations' own three-for-three pattern** — correct in mechanism, wrong in scope — and it is worth naming that it applies to a *finding* as readily as to a fix.

**But the SSoT objection is right in principle and is adopted.** "An independent regex parser for the same data is a second reader that can disagree with the first" is exactly CLAUDE.md's rule, inside the mechanism commissioned to enforce it. Two things close it:

1. **Framing, which answers the objection directly: the detector is not a second parser of the manifest.** It never derives a competing id set. It answers a **boolean** — *does manifest-shaped content exist outside the single located block?* — which is the truncation signature and is a question `list-tasks` structurally cannot answer, because by then the tail is no longer in the manifest. So there is no second home for the id set and nothing to drift.
2. **@semantic-guard's reconcile option, taken as belt-and-braces:** where `list-tasks` succeeds, its id set is authoritative and the scan is confined to outside-span. The scan earns its keep precisely where `list-tasks` **cannot answer** — rc 2 / empty stdout on a manifest that does not parse — which is the case the detector exists for.

**Their two incidental confirmations, both load-bearing and both kept:** a manifest missing `branch:` makes `list-tasks` exit **2 with empty stdout**, exactly as the module doc says — independently corroborating both the rc-2 contract Phase 1 now depends on *and* the same-commit requirement. And that failure is **loud**, which is what keeps "cannot answer" distinguishable from "nothing to do".

### V20d — the `next` doc sweep has false positives; two homes, not four (@semantic-guard)

@semantic-guard ran the full sweep @dry-reviewer proposed and found **over-applying it would be worse than the original defect**. Four non-test homes; **two stale, two correct:**

**Fix:** `adr-0035-story-runner.md:92` and `docs/TODO.md:386` — both ask a read-only *"is it done?"* question.

**Do NOT touch:** `run-story.sh:906` — `next`'s escalated→pending reopen **is** the escalation-retry mechanism; swapping it to `list-tasks` would delete the lane ADR-0035 §4 calls *"rerunning the runner is the resume gesture."* And `adr-0035-story-runner.md:41` — the deferred `--keep-going` design's `next --skip-escalated` is work-selection and is *already* aware of the reopen semantics. Correctly reasoned, not stale.

**The discriminator, recorded because a grep cannot apply it:** `next` is right when **selecting a task to run**, wrong when **asking whether anything is outstanding**. Same verb, two questions; only the second is a read, and only the second is broken by the write side effect. If the retrospective records the sweep as this class's prevention, it must record the classification step too — otherwise the next person runs the grep and breaks the runner.

**`docs/TODO.md:386` carries two asks**, and I am doing only the first (read the manifest, not the table). The second — "verify Phase 4's PR synthesis scales to 60+-task stories" — is out of R-8's scope, so the entry **stays open with its verb corrected**, not half-ticked with a superseded prescription inside.

**`docs/TODO.md:391-400` is adjacent but NOT mine** — the escalated-story-unresumable entry's candidate fix turns on the runner's manifest-write ordering, owner recorded as "infrastructure, paired with protocol". Left open, owner unchanged; adding only a cross-reference.

**Banner status** — *(SUPERSEDED 2026-08-17 by the Lead's Call A: the banner is **DROPPED**. `browser-client-join.md` gets the `branch:` deletion and nothing else, and the classification row is deleted, leaving zero unsatisfiable-ownership rows. The paragraph below described the pending state and is retained only so the reversal is legible.)* ~~V12 dropped it on @dry-reviewer's prose-synchronisation objection, then the Lead's revised Ruling 2 put it back. Its *ownership* is what I escalated to @main (Minor-judgment, owner `client` absent), so it is **pending**, not removed.~~ Either way the tolerate-message will name **the file and what it contains**, never a banner or a heading — because the heading becomes `## Devloop Output Index` if `client` lands the spin-out, and a message whose whole job is sending a human to the right place must survive that rename. And per @observability: the spin-out entry states explicitly that `browser-client-join.md` is left holding a second home for the slug mapping **with nothing in-file marking it superseded**, so that is named as part of what `client` is being asked to resolve.

### V19 — the completion site emits a CLOSED SET of four outcomes (@observability, consolidating P3/P4/P8)

@test counted four distinguishable states where the plan had one token; @observability turned that into one design instead of three `if` arms — which matters, because three separately-motivated arms is how a taxonomy acquires a fifth state nobody named. Adopted as specified:

| Outcome | Reached when | Line |
|---|---|---|
| derived | ≥1 added `main.md` in range, unique dir | `COMPLETE task=N commit=… slug=X src=commit-range` |
| resumed | zero added, `$continue_slug` non-empty | `COMPLETE task=N commit=… slug=X src=resume-pointer` |
| absent | zero added **and** no resume pointer | `NO-SLUG task=N cause=no-record-in-range` + conjunction spelled out |
| ambiguous | >1 distinct dir | `NO-SLUG task=N cause=ambiguous candidates=3 <named>` + omit `--slug` |

Three checkable properties, not aesthetics:
1. **Closed** — every completion emits exactly one of the four, so *"none of the four appeared"* is itself a detectable state. Same argument the story's §Deferred makes about a run exiting through no lane: a per-outcome record covering only the outcomes you thought of structurally cannot represent the one you didn't. Nearly free here because the completion site is one place.
2. **`cause=` carries the discriminator, so message *prose* stops being the contract.** @test asserts on the conjunction wording for `absent`; a machine-readable token lets their test pin the **classification** while the prose stays free to improve. Otherwise every reword is a test edit and the tests get written against prose.
3. **`src=` is the whole of P4** and keeps the two *recorded* outcomes distinguishable after the fact.

`cause=no-record-in-range` must state the conjunction and must **not** say the devloop wrote no record — affirmatively wrong on the resumption path, where the record exists and is committed from an earlier run. That is a misdiagnosis, not a wording preference.

### V18 — the orphan detector's regex is load-bearing, and V13's form was wrong

@operations measured V13 item 3's specified predicate (`^- id: \d+\s*$`, anchored at column 0) against @security's independently-built fixture b, which indents its entries (`  - id: 2`) as YAML permits and hand-authors commonly write:

```
V13 narrow form   ^- id: [0-9]+[[:space:]]*$        -> 0 matches
widened form      ^[[:space:]]*-[[:space:]]+id:...  -> 2 matches (lines 10, 19)
```

**The detector added to catch silent truncation did not fire on the measured silent-truncation fixture.** V13's "detects `['- id: 2']`" held only because that synthetic used column-0 form. This matters more than a regex nit: the born-truncated case is precisely the **hand-authored** one, and hand-authored manifests are the ones most likely to indent under `tasks:` — `serde_norway` emits column-0 only because it is the *machine* writer.

**Adopted: `^\s*-\s+id:\s*\d+`.** Measured cost of widening is **zero** — identical match counts across all 12 marker-bearing and fixture files, and every match inside a manifest block (browser-client-join 65/65). V13's zero-false-positive result survives intact; the widened form is strictly more capable at identical cost.

**Recording that the regex form is load-bearing, and why** — a future reader tightening it back to `^- id:` for "precision" silently reopens this hole, and **the narrow form looks more correct**. That comment goes at the code, not only here.

**This closes the residual rather than shrinking it.** Because the detector sits after `find_manifest_block` — inside `load()` — it runs for **`validate`** too, so `validate-story-manifest.sh` catches a truncated manifest at **Layer 3, in CI**, not merely at runtime. The read path does not care *how* the file got truncated, so one control covers born-truncated, hand-edited-after-emission, and write-corrupted alike. That is the answer to @security's open residual, it is why the detector earns **hard-error** severity rather than WARN, and §Accepted Deferrals item 2 is accordingly **withdrawn**.

### V17 — final test set and the Phase-1 predicate

**Test set accepted in full: T-1(a-f), T-2..T-8, T-9(a-i).** The two that would otherwise have been missed:
- **T-1(b) even-fence** — a *properly closed* fenced example. Must **red against fix (i) alone** and green only once the orphan detector lands. Without it the fix ships covering the shape that almost never occurs.
- **T-1(f) detector over-rejection** — a story carrying a second, legitimate, non-manifest fenced block still parses. This story file has two (the `/devloop` invocation examples), so it is the live shape. T-1(e) guards the *fence fix* going too far; T-1(f) guards the *detector* going too far — different failure reasons, neither finds the other's instance.
- **T-9(g) resumption** — zero added candidates + `$continue_slug` set ⇒ slug **is** recorded from the fallback (harness already has `SEED_SLUG`). Must fail if the fallback is dropped, or the fallback is unverified code on a **routine** (16%) path.
- **T-9(h)** asserts the zero-diagnosis **conjunction** in the message text, because "the devloop wrote no record" is affirmatively wrong for (g).

@test also measured that the orphan detector cannot over-reject today: **zero `- id:` lines outside a manifest fence across every story file** (browser-client-join's 60+ all sit inside `:928-1527`). Measured, not inferred — to be re-run if the predicate changes.

With (g), NO-SLUG covers **four** distinguishable states — benign-modification (recorded), resumption (recorded, via fallback), zero (loud, conjunction named), many (loud, count named). One token cannot cover four; that is the vacuous-case shape at the message layer.

**Phase 1's predicate — @semantic-guard's strength requirement, adopted.** The check is **positively `status == "completed"` for every task**, not "no `pending`". `Status` has three variants, so `jq 'map(select(.status=="pending")) | length == 0'` **passes a story with an escalated task** — strictly weaker than the table check it replaces (`SKILL.md:62`, "every row's Status MUST be `Completed`"), and exactly the silent-weakening shape. `next` exit 3 got this right by filtering `Pending | Escalated`; a hand-written read easily will not.

**And `list-tasks` rc 2 exits, never shrugs.** Per the module doc, rc 2 means "this binary cannot answer" with guaranteed empty stdout. A `$(… || true)` or an unchecked `jq` over empty input converts that into "nothing to do" ⇒ vacuous pass. `preflight-story.sh` already treats rc-0-with-empty-stdout as a contract violation; Phase 1 is at least as strict, on `run-story.sh`'s house style (`STOP-AFTER-UNVERIFIABLE` exits rather than continuing).

**ADR-0035 §4 therefore gets a correction *pass*, not a grandfathering clause** — @semantic-guard and @code-reviewer converged on this and it supersedes V15's single-sentence framing. Three things in one dated block:
- **:88's "every column is already in the manifest"** — false for `browser-client-join.md`; the containment **reverses** there (manifest = skeleton, table = superset; 51 of 65 bare stubs), so deletion becomes destruction.
- **:92's `next` exit-3 prescription** — unsafe now that a read-only verb exists, in the precise phrasing @operations supplied.
- **The discriminator a future reader applies** (@code-reviewer): *"is this artifact still written to, AND is the manifest a superset of it?"* — **both** must hold before deleting. That gives the next reader a **test** rather than a precedent, which is the difference between a correction and an anecdote.

**BLOCKING 3 ownership half, recorded as @code-reviewer asked:** I took §Notes' **venue** half ("later runner tasks extend its suite") and the **pairing** half is superseded by this task's own later, more specific "Pair with protocol for the crate change". Legitimate because §Notes' actual concern — runner code has no env-test backstop, so task 1's suite is its only independent verification — is satisfied in substance: `test` is a mandatory reviewer, now holds owner status on that file, and drove the mtime-race correction that killed V2's first form. Written out rather than invoked as half a sentence.

**V9(a) is superseded** — by the Lead's revised Ruling 2 (no in-place row repair here), then by the Lead's **Call A**, which is final: `branch:` deletion **only**, no banner, dedup **and** banner both spun out to `client`, recorded in `docs/TODO.md` + §Accepted Deferrals.

### V16 — @security's fixtures: the four controls do not overlap, and each must be recorded with its scope

@security stopped reasoning and ran `target/release/dt-story` against built fixtures. Two-task manifest, task 1's prompt carrying an injected fence:

| fixture | injected | `validate` | ids from `list-tasks` | fix (i) |
|---|---|---|---|---|
| a | nothing (control) | PASS | `[1,2]` ✓ | n/a |
| b | untagged ` ``` ` block | **PASS** | **`[1]` — task 2 lost** | **SILENT** |
| c | tagged ` ```bash ` block | **PASS** | **`[1]` — task 2 lost** | BAILS |
| d | tagged block + one ordinary code block later in the file | **PASS** | **`[1]` — task 2 lost** | **SILENT** |

So **fix (i) catches one of three shapes**, decided by whether the model tags its fence and how many fenced blocks appear later — both formatting accidents. The governing quantity is the parity of bare-fence lines *after* the truncation point, contributed by the prompt's tail **and the rest of the document**. My V13 rescoping was still too generous; main.md no longer describes fix (i) as a detector for this class. It is kept as genuine markdown well-formedness at ~3 lines.

Security also measured that **`save()`'s round-trip does not cover a file born truncated** — it detects corruption *introduced by a write*, not *inherited by a read*. On the already-truncated fixture: `complete` exits 0, ids round-trip `[1]`→`[1]`, and `next` returns exit 3 AllDone. Correct.

**The four controls, with the scope each one actually has — recorded so a later reader cannot drop one as redundant:**

| Control | Covers | Does NOT cover |
|---|---|---|
| `to_block_body` refuse-to-emit (widened) | creation via any `dt-story` write, by construction — **primary** | a file `/user-story` hand-authors; this never runs there |
| `save()` round-trip (@operations) | corruption *introduced* by any `dt-story` write, mechanism-independent | corruption *inherited* — a file born truncated |
| Orphan detector, read path (@paired-protocol) | a truncation that leaves the lost tail physically in the file as prose — **fires on `next`/`validate`/`list-tasks`** | a manifest whose tail was never written at all |
| **Step 10.5 id-set assertion (@security)** | a manifest **born** truncated, compared against the *planned* id set — the only control with external ground truth | files not produced by `/user-story` |

**THE VERIFICATION LAYER HAD THE SAME DEFECT AS THE PLAN — found by an assertion a reviewer required.**

The `claude` stub in `run-story.test.sh` committed only `work.txt`, never its own output directory. The runner derives a task's slug from `git diff --diff-filter=A` over `docs/devloop-outputs/*/main.md`, so **every** slug case resolved to zero candidates: M1 (derived), M2 (benign multi-dir) and M4 (ambiguous) were all silently exercising M3's no-record-in-range path. The suite would have contained four cases, run one path, and reported four passes.

This is the standing question's **clause 2 — "which call paths reach it?" — applied to a test rather than to production code.** A test is a control, and it is subject to the same failure: its *precondition* was never established, so it exercised a different path than the one it names. The distinction the clause forces is exactly the one that was missing: **a green test proves a path ran, not that *the* path ran.**

It was caught only because @test's T-7 required the happy path to assert the slug **value** and its presence in the manifest, rather than that status flipped. The natural assertion — "ALL TASKS COMPLETE", which four neighbouring cases in this suite use — would have passed against the broken fixture. The fix (stage `docs/devloop-outputs` in the stub, with a comment saying why) is the artifact; **the finding is that assertion strength, not case count, was what separated four real cases from four copies of one.**

Filed against the `docs/TODO.md` review-question entry as its strongest evidence: six instances in planning were failures of *controls*, and this is the same shape in the *verification* layer, which is the layer everything else is trusted to.

**SEVENTH INSTANCE — mine, in a verification fixture, and a near-miss.** While Gate 2 held the tree I verified the runner's slug derivation in throwaway `/tmp` repos, since it is the one part of this diff that cannot be exercised against a real devloop. Both reviewer rationales hold, now measured rather than reasoned:

| check | result |
|---|---|
| `--diff-filter=A`, commit that ADDS `2026-08-17-new/main.md` and MODIFIES a prior dir | yields exactly `2026-08-17-new` |
| same, filter dropped (the unshipped variant) | yields both ⇒ false `cause=ambiguous`, reproducing @test's T-9 |
| bare `docs/devloop-outputs/*/main.md` vs `alpha/nested/main.md` + `beta/main.md` | matches **both** ⇒ manufactured `cause=ambiguous` |
| `:(glob)` same input | matches `beta` only |

**The near-miss.** My *first* fixture placed the nested `main.md` under a directory that also had a top-level one, so `cut -d/ -f3 | sort -u` folded both to one slug and bare-glob and `:(glob)` printed **identically**. Stopping there would have put *"the downstream `cut` mitigates it, so @observability's rationale does not hold"* into this file — and left a comment in `run-story.sh` justifying a live control by a rationale I had just wrongly disproved. It took a second fixture isolating the nested case to get the real answer. **That is clause 1 — *what exactly does this control compare?* — landing on the person applying it**: the first fixture compared post-`cut` output rather than the pathspec's own match set. The collapse that hides it is now recorded in the `run-story.sh` comment itself, so the next person re-testing does not repeat it.

**The question to ask of this table — three-for-three (@operations).** Fix (i) was real but caught 1 of 3 shapes. The orphan detector's regex was real but anchored where the fixture wasn't. Its placement was real but on the one path the gates don't take. **Every one was correct in mechanism and wrong in placement or scope, and every one was found by executing the path rather than reading the description of it.** So the question to ask of any row below is not *"does this check work"* but **"which call paths actually reach it."**

**Why Step 10.5 survives, stated because @security is right that three controls make it look droppable:** it is the only one standing in front of **the primary new producer this task creates**, and it is the only one that compares the manifest against something outside the file. `to_block_body` never runs on that path and `save()` provably does not cover it.

**Correcting security's residual, in their favour:** they listed "a read-side scan for orphaned `^\s*-\s+id:\s*\d+` lines" as an *option*. It is already adopted (V13, @paired-protocol, measured zero false positives across all six manifest-bearing files) — and their own fixture b reports "orphaned `id: 2` still physically in the file: 1 occurrence", so the detector fires there. That shrinks the residual to: **a manifest whose tail was truncated by hand-editing in a way that also removes the orphaned YAML.** Named in §Accepted Deferrals rather than left as the gap between controls that each look complete.

Fixtures b, c and d become three distinct regression tests — any fix that only closes c is the current state.

### V15 — @test T-9 and @code-reviewer's seven

**T-9 — `>1` candidate is a recurring shape, not an edge case. Adopted.** @test measured the last 200 commits: three touch more than one dir under `docs/devloop-outputs/`, two identical in form — a devloop that improves the output *template* modifies `_template/main.md` in the same commit that adds its own output (`055cbdc`, `909d0b4`). Under V2 as drafted both yield 2 candidates ⇒ NO-SLUG ⇒ via the close gate, an uncloseable story, for an entirely benign reason, at ~1% of commits **concentrated in exactly the workflow-tooling stories this runner drives**. Fix is precise, not heuristic:

```
git diff --name-only --diff-filter=A "$head_before" HEAD -- 'docs/devloop-outputs/*/main.md'
```

"The devloop dir *created* in this commit range" is a tighter statement of "the attempt that actually committed" than "any devloop path touched in this range", so this strengthens V2's by-construction property rather than patching it. **Composition order stated, because a test cannot otherwise distinguish them:** `--diff-filter=A` + `*/main.md` is the **primary**; the `Slug` class floor is **defence in depth applied after**. @test is right that I must not let correctness rest on `_template` happening to violate the slug class — that would not generalise, since a devloop modifying a *real* prior output dir (class-valid) still produces 2 candidates. The `run-story.test.sh` case is built as the measured shape (stub creates its own dir **and modifies a second pre-existing class-valid one**, asserting the slug **is** recorded), so it reds against V2-as-drafted and greens after — plus a separate genuine-ambiguity case (two dirs *created*) asserting NO-SLUG **with the count**, since zero and many both produce "no slug" and a message without the count makes the cases indistinguishable.

**P4 upgraded to a finding, accepted — the fallback is a routine path, not an exception arm.** @observability measured 199 commits: `added=0, dirs=1` is **16%**. That row exposes a run-story shape the plan was treating as a corner — a `--continue` resumption **across invocations**, where the previous run's devloop already committed its `main.md`, so the new run's `head_before` already contains it and `--diff-filter=A` yields **zero by construction**, making `$continue_slug` the sole supplier. That is exactly the multi-attempt case (task 2, attempts=4) that "last-writer-wins" exists to serve. So the two derivations are two *normal* paths with materially different standing, not primary-and-emergency, and `slug=X` alone cannot say which produced the value. `src=commit-range` / `src=resume-pointer` is required, not optional.

**P8 — the zero-candidate wording must not misdiagnose the resumption case.** "This task's devloop wrote no output record" is **false** in the shape above: the record exists, is committed, and is in the tree from an earlier run. Telling an operator the devloop wrote nothing sends them hunting a file that is present. Zero must state the conjunction that actually held: **no `main.md` added in this task's commit range, AND no resume pointer** — because by the time the message fires the fallback has already been tried and also came back empty. That phrasing also points at the right remedy (`--slug` against a dir the operator can see) rather than at a hunt.

**P9 — pathspec pinned to one segment.** Git matches pathspecs with fnmatch **without** `FNM_PATHNAME`, so `*` crosses `/`. Measured inert today (214 `main.md` at one level, 0 at two), but a future nested `main.md` would match alongside its parent's and manufacture a 2-candidate NO-SLUG — the same false-positive shape T-9 just removed, re-entering through the pathspec. Using `:(glob)docs/devloop-outputs/*/main.md`.

**SUBSTANTIVE 4 — Rule 2's justification corrected** (already fixed in §Q3 above): it now cites the `Slug` newtype's deserializer floor plus the bash completion-path floor, not `newest_devloop_output`, which V2 removed from that path. Same defect I was warned about for Rule 1, one rule over.

**SUBSTANTIVE 5 + @dry-reviewer's final ask — §4 gets *both* clauses corrected in one dated block.** Not just the `next` clause. The "Delete §Devloop Tracking" clause is scoped to artifacts still written to, and a grandfathering sentence names the one permanent exception **at the rule**, not only in a devloop output — because a reader checking §4 against the tree finds an unexplained violation and will not look here for the explanation. Adopting dry-reviewer's wording with @semantic-guard's framing and the measured tally inline: *the drift §4 cites cannot be repaired by deletion in this file, because here the table is the **superset** and the manifest is the **skeleton** — 65 tasks, of which only 14 carry `prompt`/`specialist`/`env_tests` and 8 carry `deps`, so 51 are bare `id`+`status` stubs.* This also means dry-reviewer files **no** `docs/TODO.md` entry for the exception — one home for the explanation.

**SUBSTANTIVE 6 — the contradiction is resolved, and the answer is "legal everywhere".** @code-reviewer is right that §Schema-change safety item 2 and V8 disagreed. Deciding it once, and I lean where they lean: **absent-slug is legal everywhere.** Every task completed before this commit has no slug, so absent-slug is the *historical norm*, not corruption, and a hard-fail would make the norm an error. `validate` does not reject it. P2's discriminator is what makes `/close-story` consistent with that: **no** completed task carries a slug ⇒ pre-slug-era story ⇒ **degrade, naming the omitted devloops**; some carry one and this one does not ⇒ live recording defect ⇒ hard-fail naming the id and the repair command. Both sections now say the same thing.

**SUBSTANTIVE 7 — converted to a fact at implementation time.** After the `find_manifest_block` change I run `target/release/dt-story validate` over all six manifest-bearing story files **and** `_template.md`, and paste the output here. `load()` is on the path of every verb, so the widened EOF bail's blast radius is whole-file; one command beats an assumption.

**BLOCKING 3 — the §Notes pairing sentence, surfaced rather than resolved by me.** @code-reviewer is right that I cannot take that sentence's authority for the test file and decline it for the script: it says *both* "later runner tasks extend its suite" **and** "runner tasks are paired `--paired-with=test`". This task is paired with **protocol**, per the task text, for the schema change. My position: the sentence governs both, and its substance is discharged — `test` is an active Gate-1 and Gate-3 reviewer here with hunk-ACK on their own file, which is functionally the pairing — while the formal `--paired-with` slot went to protocol because the schema contract is the larger risk surface. But that is a Lead call about the task's own configuration, not mine to assert, so it is flagged to @main.

### V12 — browser-client-join: `branch:` only; the table work is spun out to its owner

Two reviewers converged on this file from opposite directions and @test raised an
ownership blocker that decides it.

- **@dry-reviewer** countered my in-place repair with a better idea: project the
  table down to `| # | Devloop Output |`, deleting the four columns that overlap
  the manifest (invocation/specialist/deps/status — §4's own enumeration). It
  preserves the slug mapping byte-for-byte, resolves both contradictions **by
  deletion rather than winner-picking**, and needs no banner (they correctly
  called my banner prose-synchronisation, the very mechanism they objected to
  elsewhere). They also measured a contradiction I had missed: **row 53 says
  `Not implemented` while the manifest says `completed`** — so my 3-row repair
  would have left the same defect class in place, one row instead of two.
- **@test (C-2)** independently ruled the row-repair not Mechanical (picking a
  winner between two disagreeing records is the definition of not-value-neutral)
  and named the blocker: **ADR-0024 §6.3 requires the owner be a reviewer on this
  devloop to confirm a Minor-judgment edit, and `client` is not on this devloop.**

That blocker applies to dry-reviewer's projection too — deleting four columns
from another specialist's closed story file is not value-neutral either, however
deterministic the `awk` is. So I cannot land either version here without the
owner, and I will not classify my way around that.

**Decision: `browser-client-join.md` gets the `branch:` deletion and nothing
else.** That edit is genuinely Mechanical — forced by the schema change, no
alternative, no judgment. **No banner** (dry-reviewer is right that it is prose
synchronisation, and I was applying two standards). The table work is spun out to
`client` in `docs/TODO.md` **carrying dry-reviewer's projection recipe verbatim**
— the target shape, the four columns to drop, the two contradictions it resolves,
the heading rename to `## Devloop Output Index`, and the measured row counts — so
it is a decision already made and owned, not a rediscovery. Per CLAUDE.md this is
a legitimate deferral: it is blocked on owner availability under a written
protocol rule, which I say plainly rather than calling it task-sized for effort
reasons — dry-reviewer is right that the projection is cheap.

Divergence from §4 shrinks accordingly and is recorded as such: for every story
going forward the new `slug` field means §4 holds with **no** divergence; the
only open item is one legacy closed file, owned and scheduled.

### V13 — @paired-protocol ran the parser: fix (i) misses the likelier shape

Protocol ported `find_manifest_block` line-for-line and executed it against both
truncation shapes rather than reasoning about it. My V1 claim was **overstated**
and is rescoped:

- **Shape B** (prompt contains a *language-tagged* example, ```` ```bash ````):
  the tagged line is swallowed, the example's bare close ends the manifest early,
  the real close dangles at EOF → **fix (i) fires.** ✅
- **Shape A** (prompt contains a *properly closed bare* fenced example): fence
  lines pair up exactly, nothing dangles. Measured: `blocks found: 1`,
  `dangling fence: no`, **fix (i) bails: False**, parsed ids `['1']` — task 2
  silently gone. ❌ **And Shape A is the more likely shape**, because a model
  writing an example normally closes it.

Adopted changes:
1. **Keep (i), unguarded** — protocol measured all 8 story files + 4 fixtures:
   **zero would bail**, so it is not too blunt. But the plan's claim is rescoped
   from "closes the truncation class" to "catches the language-tagged subcase."
2. **(ii) widened**: `to_block_body` rejects **any** line whose trim *starts with*
   ```` ``` ````, not only a bare one. Shape B shows a tagged fence is equally
   fatal, and independently CommonMark cannot nest a 3-backtick fence inside a
   3-backtick block, so a tagged fence also corrupts how the file renders.
3. **NEW — orphan detector on the read path** (protocol's proposal, measured):
   scan the text *outside* the located span for an orphaned task-entry line. In a
   truncation the lost tail sits outside the block and is full of them. Shape A
   synthetic: **catches exactly what (i) misses.** ~6 lines, and a **hard error**,
   not a WARN — a manifest that lost its tail must not reach `next`.

   *(Corrected 2026-08-17, twice — see V18 and V20.* The predicate specified here
   was `^- id: \d+\s*$`, anchored at column 0; **V18** widened it to
   `^\s*-\s+id:\s*\d+` after @operations measured it produced **0 matches** on the
   real fixture, because hand-authors indent under `tasks:`. The "zero false
   positives across 6 files" result quoted here was measured against the **narrow**
   form; V18 and @test re-measured it for the widened one, and it holds. The
   placement stated here — "after `find_manifest_block` succeeds", i.e. in
   `load()` — was also wrong: **V20** moves the scan **into**
   `markdown::find_manifest_block`, because `cmd_validate` bypasses `load()`
   entirely, so the `load()` placement would be invisible to CI and preflight.
   Corrected in place rather than rewritten, because this is the *detailed* spec
   an implementer would work from and it sits after its own corrections in reading
   order.*)
4. **Framing corrected.** "A per-field validator is unreachable" is true of the
   *read* path and wrong as a general statement — the check is perfectly
   reachable on **the value before it is written**, which is what (ii) is. Stated
   that way so a later reader cannot conclude no validator is possible and
   quietly drop (ii).

**Residual limit, stated rather than implied.** Even with (i)+(ii)+(iii)+the
detector, the read path cannot fully disambiguate, because the ambiguity is in
the *format*: a 3-backtick block cannot legally contain a 3-backtick fence. So
(ii) imposes a real expressiveness limit — **manifest prompts can never contain
fenced code examples** — and that limit goes in the schema doc comment next to
the `Slug` and write-rule commentary, not only in an error message. An
unexplained rejection is where people start hand-editing around the tool.
**Escape hatch recorded, not taken:** opening the manifest with a 4-backtick
fence (`````` ````yaml ``````) would let CommonMark contain 3-backtick fences and
genuinely close the class. Cost: the fence lines sit outside the span
`replace_manifest_block` rewrites, so it is a hand edit of 7 files plus the
fence-length rule in the parser; the Layer-3 guard greps the marker, not the
fence, so discovery is unaffected. Not done here — recorded so the cheap route
reads as a decision with a known escape hatch rather than the only option anyone
saw.

### V9 — @dry-reviewer: repair the ADR's cited defect in place, and use a sync guard not a TODO entry

**(a) The duplicate 18/19/20 rows.** dry-reviewer is right that shipping a
de-duplication task while ADR-0035 §4's own cited evidence ("duplicate rows for
tasks 18/19/20 and a conflicting status for 20") is *still in the tree* is a bad
outcome. But deleting the table destroys the 59-row slug mapping
(@semantic-guard's item 1). These two reviewers want opposite things, so I am
taking the resolution neither asked for but both are satisfied by: **repair the
defect in place.** Delete the three duplicate rows, keeping the `Completed` one
for 20 (which agrees with the manifest, the authority). That removes the live
inconsistency — which is the actual harm — at a fraction of the risk of a
59-row hand backfill, and preserves the mapping. ~~Plus the frozen-record banner,
so no future drift is possible: nothing writes to it and readers are told the
manifest is authoritative.~~ *(V9(a) is superseded twice over — see the note at
the end of V17. Final status, Lead **Call A**: `branch:` deletion only; no row
repair, **no banner**; both spun out to `client`.)* The four pre-manifest
table-only stories get the one-line note dry-reviewer would accept, in
`_template.md`'s vicinity rather than as four file edits.

**(b) Slug classes — finding accepted, TODO parity entry withdrawn.** dry-reviewer
correctly shot down my V4 fallback: the `_gate2_binding.sh` TODO precedent is
justified by "the pre-commit hook is coreutils-only and must not depend on a
built binary", and that rationale **does not transfer** — `/close-story` and the
runner both already hard-depend on `dt-story`. Revised:
- The Rust `Slug` newtype class is **canonical**.
- `/close-story` **keeps** its read-time regex (@security, @semantic-guard,
  @paired-protocol all insisted, and they are right — the manifest is a
  hand-editable markdown block, so it is an input boundary regardless of who
  wrote it). But its class becomes **identical** to the Rust one. That answers
  dry-reviewer's actual finding, which is that the two classes *disagree*
  (`docs/devloop-outputs/_template` is a real dir the producer accepts and the
  consumer rejects) — not that redundancy exists at a trust boundary.
- `run-story.sh`'s four literal copies of `^[0-9A-Za-z._-]+$` collapse to one
  `readonly`. That class keeps its existing, *different* job (flag-injection
  floor on an ephemeral resume value); the new completion-path floor uses the
  narrow class.
- The narrow class gets a **sync guard**, not a TODO entry —
  `scripts/guards/simple/validate-subdomain-regex-sync.sh` is the named
  precedent and the bar.

**(c) The `add-task` route (dry-reviewer item 6) — rejected, with the constraint
they asked me to name.** `engine::add_task` hardcodes `deps: Vec::new()` and has
no `--deps` flag, so it **cannot express a dependency graph** — which is the
whole point of a planned story. It also mandates `--tag`, an idempotency key for
programmatic remediation appends, not a planning concept. Making it work means
adding `--deps` and optional `--tag` to the crate: real scope growth for a
benefit the fail-closed `dt-story validate` gate already delivers. So
`/user-story` emits YAML directly, `_template.md` is the single shape example
(guarded at Layer 3 — dry-reviewer's option (a)), and the skill points at it
rather than restating it.

### V10 — @operations: the runner executing THIS task runs the pre-edit script

Measured by operations and consistent with the story's own §Notes: the task loop
is one compound command, already parsed, and Edit replaces by inode. So the
**old two-arg `dt-story complete "$STORY_FILE" "$id"`** is what actually runs at
the completion site, against the **new** binary. Design consequences, all already
required by the plan but now load-bearing rather than incidental:

- **`--slug` MUST be optional and `complete` must exit 0 without it.** A required
  flag (or a non-`Option` `Task.slug`) would kill the run under `set -e` with
  task 4's work committed and the manifest unflipped — a self-inflicted instance
  of the exact operator-shaped red R-4 of this same story exists to prevent.
- **Task 4's own `slug` is hand-written** (`slug: 2026-08-17-manifest-single-home`)
  because the old runner will not write it. Stated explicitly here so it does not
  read as a runner write. Last-writer-wins makes it safe if the runner ever does.
- **Tasks 1-3's slugs backfilled** from the retiring column, with an
  @observability-requested dated line under `## Revisions` recording that 1-3 are
  a hand-backfill and only later tasks are runner-written.
- **C5 — the recovery message must name a fix that works.** Today
  `dt-story complete <file> <id>` on an already-completed task writes nothing, so
  the recovery text at the pipeline-precondition lane cannot repair a missing
  slug. My V5 design (`AlreadyComplete` persists a supplied slug) *is*
  operations' option (a), so `complete --slug` becomes a working repair gesture —
  and the NO-SLUG message names it. Silently completing without a slug is
  precisely what V3 forbids.
- **`branch:` is in 7 locations, not 6.** Operations found two I had missed:
  `crates/dt-story/src/markdown.rs:110,135` (unit tests inside the file I am
  already editing). Full inventory now: `manifest.rs` (struct), `markdown.rs`
  (2 unit tests), `tests/cli.rs` (9 literals), `tests/fixtures/` (4 files),
  `run-story.test.sh:538`, and the 2 story files.
- **`preflight-story.sh:82`'s bare `story manifest invalid`** is widened to name
  both causes (stale binary vs invalid manifest) plus the rebuild command,
  mirroring what `run-story.sh:314` already does. After this lands, a stale
  binary meeting a branch-less manifest is the *most likely* cause of that
  message. One line.
- **C7 correction, owed back to operations** *(phrasing corrected per their
  follow-up — the flat "`next` is not read-only" contradicts `main.rs:241-242`
  and would create the exact ADR↔code drift this task exists to end)*: `next`
  mutates on **exactly one path** — selecting an escalated task reopens it and
  saves. That path is reachable **precisely when the completeness check would
  fail**. A completeness probe built on `next` is therefore safe only when it
  passes and destructive when it fails, which is the inverse of what an audit
  gate must be. `list-tasks` is the read-only verb. §4 corrected in place with a
  dated block.

---

## Implementation Summary

Landed as planned across V1-V27. Verification run at implementation time
(@code-reviewer's S7, @test's T-8/T-8b), pasted as facts rather than claims:

```
dt-story validate, every marker-bearing file:
  docs/user-stories/2026-08-11-story-runner-hardening.md   OK
  docs/user-stories/_template.md                           OK
  docs/user-stories/2026-05-02-browser-client-join.md      OK
_template.md in validate-story-manifest.sh's selected set: yes (1)
'- id:' lines outside a manifest block, tree-wide:         none
cargo test -p dt-story:                                    46 passed, 0 failed
cargo clippy -p dt-story --all-targets:                    clean
scripts/workflow/run-story.test.sh:                        168 passed, 0 failed
scripts/guards/validate-slug-class-sync.test.sh:           6 passed (incl. drift case)
scripts/guards/run-guards.sh:                              37 passed, 0 failed
```

**Schema.** `Manifest.branch` removed; `Task.slug: Option<Slug>` added between
`commit` and `escalation`, with the opposite write-rules documented at
adjacent declarations. `Slug` is a newtype validating `SLUG_PATTERN`
(`^[a-z0-9]+(-[a-z0-9]+)*$`) in the **deserializer**, so `next`, `validate`,
`list-tasks`, preflight, the Layer-3 guard and `/close-story` all inherit one
floor. `branch` was removed from **7** locations, not the 2 the task text
named.

**Fence-collision controls, four, non-overlapping.** `to_block_body` refuses
to emit any line starting with a fence (closes the write path);
`find_manifest_block` bails on any unterminated fence **and** scans for
orphaned `- id:` lines outside the block (read path — placed at the common
ancestor because `cmd_validate` bypasses `load()`, so a caller-side check
would be invisible to CI and preflight); `save()` re-reads and asserts the id
list round-trips; `/user-story` Step 10.5 asserts id-set equality against the
plan. Measured: the orphan scan is what catches the **even-parity** shape (a
properly closed fence — what a model actually writes), which the EOF check
provably cannot see.

**Emission.** `/user-story` writes a skeleton then calls `add-task` per task,
so `to_block_body`'s refusal covers the primary new producer by construction.
`add-task` gained `--deps` (single comma-separated token, own parser, empty
value is a hard error); `--tag` stayed **mandatory** with the skill passing
`story-{slug}-task-{n}`, which makes emission idempotent and resumable — and
kept the crate change smaller than what was approved.

**Runner.** Slug derived from `git diff --diff-filter=A "$head_before" HEAD --
:(glob)docs/devloop-outputs/*/main.md` — the task's own commit range, so "the
attempt that actually committed" holds by construction rather than by mtime
proxy. Closed set of four outcomes, each with a machine-readable token:
`slug=X src=commit-range|resume-pointer`, or `NO-SLUG cause=no-record-in-range
|ambiguous|unsafe-class` naming the repair command. Never a lane change — the
work is committed and the gates are green at that point.

**Deliberate departure from the task text**: `.claude/skills/devloop/SKILL.md`
changed (Q1). Two-sentence in-bullet replacement, mode-split on
`DEVLOOP_HEADLESS`; post-edit structural check per the Lead's Call C —
headings 44 → 44, Headless Mode and Steps 8/9 present, diff +11/-3.

---

## Files Modified

**Crate (5)** — `manifest.rs` (schema, `Slug`, fence-safe emit, v1 rationale),
`markdown.rs` (EOF bail, orphan scan, 2 fixtures), `engine.rs`
(`complete` slug, `NewTask`, delta dep validation, zero-task predicate),
`main.rs` (`--slug`, `--deps`, `TaskSummary.slug`, rule rewrite, rc-4 notice,
round-trip `save`), `tests/cli.rs` (+20 tests).

**Fixtures (4)** — `tests/fixtures/{runnable,blocked,all_complete,malformed}.md`.

**Scripts (6)** — `run-story.sh`, `run-story.test.sh` (+5 cases, 2 stub knobs),
`preflight-story.sh`, `layer3.sh`, `guards/simple/validate-slug-class-sync.sh`
(new), `guards/validate-slug-class-sync.test.sh` (new).

**Skills (3)** — `user-story` (Steps 10.4/10.5), `close-story` (manifest read),
`devloop` (Q1 departure).

**Docs (6)** — `_template.md`, `2026-08-11-story-runner-hardening.md`,
`2026-05-02-browser-client-join.md`, `adr-0035-story-runner.md`, `TODO.md`,
`scripts/lang/{rust,ts}/audit-remediation.md`.

---

## Devloop Verification Steps

_(populated at Gate 2)_

---

## Code Review Results

| Reviewer | Plan Status | Verdict | Findings | Fixed | Deferred |
|----------|-------------|---------|----------|-------|----------|
| Security | confirmed | **RESOLVED-FIXED** | 1 | 1 | 0 | Third unguarded copy of the canonical slug class, with a comment falsely claiming guard coverage. Verified fix by own synthetic drift, not the selftest. |
| Test | confirmed | **RESOLVED-DEFERRED** | 6 | 6 | 0 | C-1 upgrade (Mechanical→Minor-judgment) accepted; C-2 spun out to `client`. F-1..F-4 fixed. Spin-out forces DEFERRED. |
| Observability | confirmed | **RESOLVED-FIXED** | 7 | 7 | 0 | F1-F7. Amended own verdict on finding F-4 existed because of their own F5. |
| Code Quality | confirmed | **RESOLVED-DEFERRED** | 3 | 3 | 0 | Stale `cli.rs` prose; false guarantee in `run-story.sh`; guard header count. DEFERRED on the `client` spin-out they carry. |
| DRY | confirmed | **RESOLVED-DEFERRED** | 2 | 1 | 1 | Homes count pinned to the diff (14→3, unguarded 6→0) exposed the third class copy. Spin-out to `client` accepted. |
| Operations | confirmed | **RESOLVED-FIXED** | 11 | 10 | 0 | C1-C6 + F1-F4 + the `SKILL.md` fail-fast correction. C7 withdrawn by them after verifying the implementer's rebuttal. |
| Semantic Guard | confirmed | **RESOLVED-DEFERRED** | 4 | 4 | 0 | Non-disjoint NO-SLUG set; `SKILL.md:379` (escalated, upheld); `LAYER_ALL_RC`; `--continue` recovery gesture. DEFERRED on the `client` spin-out. |
| Protocol (paired) | confirmed | **RESOLVED-FIXED** | 2 | 2 | 0 | §Deferred clause unamended; `save()` compared ids not content. Trailer granted after judging the actual hunks. |

---

## Accepted Deferrals

- **`browser-client-join.md` §Devloop Tracking projection + duplicate rows 18/19/20** → `docs/TODO.md` §"[ADR-0035 · Group 2 · spun out of story task 4, owner `client`]". Blocked on ownership, not effort: the edit adjudicates between two disagreeing records, `client` owns the file and is not a reviewer here, so ADR-0024 §6.3 cannot be satisfied at any tier. Forces @code-reviewer's verdict to RESOLVED-DEFERRED rather than RESOLVED-FIXED — expected, and the protocol working as designed.
- **Promote the "which call paths reach this control" review question to a shared surface** → `docs/TODO.md` §"[Review protocol · owner `code-reviewer`]". Left out of `review-protocol.md` deliberately: scope growth into the review protocol should be a decision, not a tail-end addition to this task.

*(A third entry — hand-editing a story file to truncate its manifest after emission — was WITHDRAWN at V20 rather than deferred: the orphan scan lives in `markdown::find_manifest_block`, which `validate` reaches, so Layer 3 and preflight both catch it.)*

---

## Commit Trailers (Cross-Boundary, ADR-0024 §6.3)

Two non-Mine rows need owner co-sign. Recorded here verbatim because a trailer
is a durable breadcrumb — the next person reading `git log` for precedent takes
its wording as the rule.

**`scripts/workflow/run-story.test.sh`** — supplied verbatim by owner `test`:

```
Approved-Cross-Boundary: test run-story.test.sh Minor-judgment — owner co-sign at Gate 1 and Gate 3 per ADR-0024 §6.3; story §Notes fixes the venue (later runner tasks extend task 1's suite), not the classification
```

**Why this replaced my draft, recorded because the correction is the point.**
Mine read *"§Notes authorises later runner tasks to extend task 1's suite, which
is the venue authority…"* — which **cites §Notes as the authority for the
approval**, inverting the ownership model. §Notes settles **where** the work
goes; it says nothing about whether a non-owner authoring test semantics there
is value-neutral, and it *cannot* — that is precisely what made the row
Minor-judgment rather than Mechanical (@test's C-1 upgrade). The authority is
ADR-0024 §6.3: the owner specialist is a reviewer on this devloop and confirms
the hunks at Gate 1 and Gate 3.

Left as drafted, someone reading `git log` would reasonably conclude **a story-file
clause can authorise a cross-boundary edit. It cannot — only an owner can.**
@test's wording keeps §Notes in the record for the venue (genuinely load-bearing)
while naming the actual authority, states `Minor-judgment` explicitly so the
upgrade is visible in the commit rather than only here, and stays single-clause
to match the closest structural precedent in the repo
(`Approved-Cross-Boundary: global-controller paired-owner co-sign at Gate 1 and
Gate 3 for gc-alerts.yaml + gc-overview.json`).

**`crates/dt-story/**` (schema rows + the four test fixtures)** — owner
`protocol`, granted at Gate 3. Exact string to be taken verbatim from
@paired-protocol's own message rather than paraphrased here, for the same reason
the above was corrected.

---

## Rollback Procedure

`git revert` the single commit — but **atomic only while this commit is HEAD**
(@operations F1; the earlier three-line version of this section was untrue in
both halves and is corrected here).

The schema change and all `branch:` carriers land together, so an immediate
revert is clean. **Measured against the shipped diff: 10 files, 19 lines** —
`manifest.rs` 1, `markdown.rs` 2, `tests/cli.rs` 9, the four fixtures 1 each,
`run-story.test.sh` 1, `browser-client-join.md` 1, `story-runner-hardening.md` 1.
(An earlier draft said "seven", which was a count of *bullets* in a review note
with the four fixtures collapsed into one — not of files. The sweep procedure
below is keyed on "every file carrying the `# task-metadata` marker" rather
than on a count, so only this descriptive sentence was affected.) It stops being clean the moment any data has been
written under the new schema, because `deny_unknown_fields` cuts **both**
directions and the plan above only reasoned about the forward one:

- Revert restores `pub branch: String` — **required**. Every story file
  `/user-story` emits after this commit lacks `branch:`, so `Manifest::from_yaml`
  fails on a missing field.
- Revert removes `Task.slug`. Every manifest the runner has since written a
  `slug:` into fails `deny_unknown_fields`.

Either direction fails `dt-story validate`, and `validate-story-manifest.sh`
validates **every** marker-bearing file in the tree — so an un-swept revert reds
**Layer 3 repo-wide**, including stories unrelated to this change. That blast
radius is the thing this section exists to make visible.

**After any `/user-story` emission or any runner `complete`, the revert requires a
data sweep:** add `branch: <any>` to every file carrying the `# task-metadata`
marker, and delete `^\s*slug:` lines from every manifest. Verify with
`scripts/guards/simple/validate-story-manifest.sh`.

`deny_unknown_fields` is deliberately **not** touched — it is load-bearing
(`tests/cli.rs` pins the `owner: bob` rejection) and this brittleness is inherent
to it. Note this is also the strongest argument for the `--paired-with=protocol`
pairing: a schema contract with no version negotiation means "roll back the code"
is never just code.

---

## Issues Encountered & Resolutions

_(populated as they arise)_
