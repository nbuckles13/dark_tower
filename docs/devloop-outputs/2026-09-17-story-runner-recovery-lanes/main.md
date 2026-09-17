# Devloop Output: Harden the story runner — ledger persistence + --finish / --interactive recovery lanes (ADR-0037 D5+D6)

**Date**: 2026-09-17
**Task**: Persist the cost ledger outside the container (D5); add a model-free `--finish` lane and an attached `--interactive` retry lane (D6/f, D6/g)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2), full — Gate 1 (changes runner control flow)
**Branch**: `feature/devloop-improvements-and-story-2`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6936c81f5ac537b527d377f9561cb3764b410c3d` |
| Branch | `feature/devloop-improvements-and-story-2` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Commit | `1f81c64` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned (ADR-0037 D4 — shell/skill diff, no checks.md surface plausibly touched; safety surface of the attached spawn is owned by @security)` |

---

## Task Overview

### Objective
Three changes to the story runner, all traceable to ADR-0037 §D5/§D6 and the detailed shapes in `docs/TODO.md` §"Devloop & Team Structure Review" items (f)/(g)/(e):

1. **D5 — ledger persistence**: bind-mount a host-persistent, git-external dir (`${HOME}/.cache/devloop/story-runs/`) into the devloop container, default `RUN_DIR` in `run-story.sh` to that mounted path so the cost ledger + per-task logs survive container destroy; keep it overridable via `DEVLOOP_TMP` for `DEVLOOP_TEST` runs; preserve the existing `RUN_DIR` character floor; fail loudly if the path is unwritable (never silently fall back to `/tmp`).
2. **D6/f — commit-intent checkpoint + `--finish` lane**: devloop SKILL writes a machine-readable commit-intent (message, trailers, task slug, exact expected file list) to a known path under `RUN_DIR` at Gate-3 close; `run-story.sh --finish` re-runs the gate with **no model turn**, and if green stages exactly the intent's files (staged-index check — reject on any staged/changed-set mismatch), commits with the recorded message/trailers, `dt-story complete`s the slug; any mismatch refuses and falls back to resume.
3. **D6/g — `--interactive` retry lane**: runner spawns `claude` attached to the operator's TTY for the escalated task (same prompt/`--continue`, `DEVLOOP_HEADLESS` unset for that spawn only, no stream-json redirect, no task timeout, Stop-hook relaxed), waits for the human to exit, then falls through to the **unchanged** gate / commit-detection / complete / escalate machinery. Cost ledger records `kind=interactive`. Consider auto-suggesting the lane after the second escalation of one task.

Carry the ADR/brief details exactly — do not invent new mechanism. If a detail isn't traceable to the ADR/brief, raise it rather than adding it.

### Scope
- **Service(s)**: none — devloop/story-runner tooling (`scripts/workflow/`, `infra/devloop/`, `.claude/skills/devloop/`)
- **Schema**: No
- **Cross-cutting**: runner control flow + a safety surface (attached-spawn Stop-hook/timeout relaxation)

### Debate Decision
NOT NEEDED — governing design already exists (ADR-0037 D5/D6, Proposed/trial). This is implementation of an approved-for-trial ADR.

---

## Cross-Boundary Classification

<!-- Populated by the implementer at planning. Per ADR-0037 D1, classification
     rows + Owner routing apply ONLY to Domain-judgment cross-boundary edits and
     GSA paths. The implementer owns devloop.sh + run-story.sh; the security-
     sensitive Stop-hook/timeout relaxation is reviewed by @security on the panel.
     Full file list still listed for the scope-drift guard. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/workflow/run-story.sh` | Mine | — (infrastructure owns the runner) |
| `infra/devloop/devloop.sh` | Mine | — (infrastructure owns devloop.sh) |
| `.claude/skills/devloop/SKILL.md` | Mine | — (infrastructure owns the devloop SKILL) |
| `scripts/workflow/run-story.test.sh` | Mine | — (runner test-infra; @test reviews the coverage) |
| `scripts/guards/simple/validate-run-dir-path-sync.sh` | Mine | — (NEW guard machinery; binds the ledger-base literal pair (this diff's new encoding) + folds in the pre-existing marker pair, infrastructure) |
| `docs/TODO.md` | Mine | — (VERIFY obs's 3 constraints in ops's existing retention entry; no new entry) |
| `docs/devloop-outputs/2026-09-17-story-runner-recovery-lanes/main.md` | Mine | — (this loop's output doc) |

`scripts/workflow/preflight-story.sh` is **NOT** touched: its `RUN_BASE` (:163, the substrate-probe
marker) is per-CONTAINER and stays on `DEVLOOP_TMP`/`/tmp/devloop` per ops O-1 — only `RUN_DIR`'s default
moves. No GSA paths, no cross-domain core-logic edits. The `--interactive` Stop-hook/timeout relaxation is
a safety surface reviewed by @security (code is mine). Per ADR-0037 D1 all rows are Mine, so this table
exists only for the scope-drift guard (plan-vs-diff), not owner routing.

---

## Planning

**Scope partition is authoritative** — see §Scope Partition (Lead ruling) at the end. In-scope items are
built now; the retention/prune engine is a `docs/TODO.md` follow-up (ops already filed it — I only verify
obs's three constraints are folded into that ONE entry). This section reflects the full converged review;
per-item traceability tags (Oxx/Sxx/Cxx) name the reviewer constraint each line satisfies.

### D5 — Ledger persistence: two-key decomposition + directly-testable base resolution

**Root cause (security S-1 / ops O-2).** `RUN_DIR` already lands host-side when the helper is up
(devloop.sh:523 mounts `HELPER_RUNTIME_DIR` at container `/tmp/devloop`); it dies because teardown
`rm -rf "$HELPER_RUNTIME_DIR"` (:213) deletes it and the no-cluster path has no mount. The win is a path
outside the teardown path.

**Two keys, two lifetimes (Lead P4 / DRY P4 / ops / obs — AUTHORITATIVE):**
- **MARKER + substrate-probe = container-scoped, ephemeral. UNCHANGED, zero edits.** `RUN_BASE="${DEVLOOP_TMP:-/tmp/devloop}/story-runner"`
  (run-story.sh:516), `preflight-story.sh:163`, and `devloop.sh:715` `INFLIGHT_MARKER` are ALL untouched.
  **The original open-point-(a) INFLIGHT_MARKER move is DROPPED** (security/DRY/Lead): the marker never
  moves, so skip-CLI-update-during-run keeps working with no change and there is nothing to keep in
  agreement across two mounts. (The stale first-draft bullets that moved the marker and edited preflight are
  gone from this section — confirming to reviewers who cited the old line numbers.)
- **RUN_DIR / cost-ledger / durable evidence = STORY-scoped, host-persistent, FLAT.** Only `RUN_DIR`'s base
  moves.

**Base resolution — extracted, directly testable (Lead F-1 / DRY P2 / test / security):**
`resolve_run_base <kind>` (kind ∈ `marker` | `run-dir`) is the ONE home of the composition. It takes a kind
argument *precisely because there are two bases that must NOT collapse* (DRY note-1 — a single-value helper
would re-unify what the decomposition split; the arg + comment prevent the "why does this take an arg"
simplification):
```
resolve_run_base marker  -> "${DEVLOOP_TMP:-/tmp/devloop}/story-runner"            # container-scoped
resolve_run_base run-dir -> "${DEVLOOP_TMP:-${DEVLOOP_STORY_RUN_BASE:-${HOME}/.cache/devloop/story-runs}}/story-runner"
RUN_DIR="$(resolve_run_base run-dir)/<story>"
```
The `story-runner/` segment is KEPT for its own reason (DRY — not "because RUN_DIR_OF hardcodes it"; once
RUN_DIR_OF derives from the startup emission it requires nothing): it namespaces story-runner artifacts
away from the other `DEVLOOP_TMP` consumers (`layer-N.log`, `gate2-verdict`, `changed-files.*`).
- **Precedence is a SECURITY INVARIANT, `DEVLOOP_TMP` MUST dominate (security / DRY (a)):** the harness sets
  `DEVLOOP_TMP` to a mktemp dir to isolate; `devloop.sh` sets `DEVLOOP_STORY_RUN_BASE` on every container so
  it is present for `run-story.test.sh` too. If the carrier outranked `DEVLOOP_TMP`, every hermetic case
  would write fixture evidence into the live persistent ledger (H2 through the front door), and the missing
  CI presence-clause would become a live gap. Comment at the site names BOTH properties; a test asserts
  `DEVLOOP_TMP` wins when both are set.
- **`DEVLOOP_STORY_RUN_BASE` is a NON-SEAM production carrier (security ruled, Lead accepted the reversal):**
  a seam refuses when present without the sentinel; the production launcher sets this on every run
  (`DEVLOOP_TEST` unset), so enrolling it would refuse every production run — that is exactly what
  distinguishes a carrier from a seam. It gets NO sentinel/CI clause; the seam count stays two. One line in
  the `:51-88` seam header records this taxonomy (the launcher-sets-it → can't-be-a-seam argument) so it is
  not re-litigated. Its four obligations: char-floor on the COMPOSED RUN_DIR; the H2 sibling assertion; a
  value-shape check (absolute, no `..`); and the startup slog below. A leaked/inherited carrier under
  `DEVLOOP_TEST=1` is inert because `DEVLOOP_TMP` dominates AND the harness `env -i` scrub (test.sh:652)
  removes it — two layers, so no extra refuse-under-test is needed (noted to DRY/security). **Tail
  discrepancy (security flagged, Lead's call):** I use `${HOME}/.cache/devloop/story-runs` as the final
  fallback (security's recommendation — keeps persistence when the carrier is absent but a mount exists),
  NOT the Lead's `/tmp/devloop`. Flagging for a one-word Lead confirm/override; everything downstream
  assumes the HOME tail.

**devloop.sh (ops O-4 / S-4 / S-6 / DRY P3):** one shell var owns the container path, used as BOTH the mount
target AND the carrier:
```
CONTAINER_LEDGER_BASE="/home/dev/.cache/devloop/story-runs"   # single source of the container-side path
STORY_RUNS_HOST="${HOME}/.cache/devloop/story-runs"           # host-side, FLAT (shared across slugs)
mkdir -p -m 0700 "$STORY_RUNS_HOST"                           # before podman run (no root-owned auto-create)
: > "$STORY_RUNS_HOST/.ledger-mount"                          # O-3 positive-control marker, host-written
EXTRA_PODMAN_ARGS+=(-v "${STORY_RUNS_HOST}:${CONTAINER_LEDGER_BASE}:z")            # :z SHARED (flat, many containers) — NOT :Z; reason at site (DRY)
EXTRA_PODMAN_ARGS+=(-e "DEVLOOP_STORY_RUN_BASE=${CONTAINER_LEDGER_BASE}")          # carrier = the actual mount target (single source; DRY P3)
```
Mount added UNCONDITIONALLY (not gated on kind). `:z` not `:Z` — the flat dir is shared across concurrent
containers, so a private relabel would break the second container; a one-line reason sits at the mount since
its two `:Z` neighbours (:523/:607, both per-slug private) make `:z` read like a typo (DRY note-2).

**Flat + STORY-keyed (Lead rule 2):** host `~/.cache/devloop/story-runs` shared across containers, evidence
under `story-runner/<story>`. A story resumed in ANY container writes the SAME `cost-ledger.jsonl` → close
rollup (:1718) complete, `latest_escalation_record`/retry lanes find the prior attempt across generations
(the failure D6/f recovers from). Matches the ADR literal, no divergence.

**Concurrent-same-story hazard + `.owner` guard (ops / Lead rule 5):** flat means two containers running the
SAME story share one evidence dir, which the per-container marker cannot see. Write `$RUN_DIR/.owner`
(slug + pid + UTC start) at runner start; refuse (distinct token) if a present `.owner` names a DIFFERENT
LIVE slug — the positive form of `__seam_assert_run_dir_isolated`'s `:221-224` in-use check, including its
clause that a stale owner from a killed run is removable deliberately rather than blocking recovery forever.
obs's owner-field-on-every-entry + multi-owner-rollup-abstain is accepted as **defense-in-depth** — I take
the `.owner` guard (primary) now and RECORD the entry-field as a residual gap per obs's explicit "take
`.owner` and I'll accept the residual."

**Writability + persistence positive-control (Lead item 8 / O-3 — IN):**
- Bare `mkdir -p "$RUN_DIR"` → loud create-and-writable check; failure → `STORY_RUN: RUN-DIR-UNWRITABLE`
  + exit 2, message NAMES the refused path + `infra/devloop/devloop.sh --recreate <slug>` recovery (ops
  O-4 / security operator-visibility). NEVER /tmp fallback.
- **Positive control:** when the production ledger base is in use (`DEVLOOP_TMP` unset), assert the
  host-written `.ledger-mount` marker is present under the base; absent → `STORY_RUN: LEDGER-NOT-PERSISTENT`
  + exit 2 (distinct token/repair). Catches "writable but not the mount" (host-hatch, failed helper,
  container created before this change via devloop.sh's already-running short-circuit) — the §Assertion-
  Vacuity #5 silent-non-persistence D5 exists to kill. (Token is manual-testable only, since it requires
  `DEVLOOP_TMP` unset which the seam forbids — noted in the test plan.)
- **Char floor kept byte-for-byte** (`^[A-Za-z0-9._/-]+$`, security S-3 do-not-widen), applied to the
  COMPOSED RUN_DIR after resolution; refusal text gains a `$HOME`/carrier clause (ops O-5).

**Startup RUN_DIR emission — ONE line, three consumers (test #1 / security / DRY, converged):** immediately
AFTER the writability check succeeds (never before mkdir; genuinely ABSENT on the `UNSAFE-RUN-DIR` /
`RUN-DIR-UNWRITABLE` refusal lanes, since there is no usable dir to point at), on EVERY lane that
establishes a usable run dir (fresh/resume/retry/finish/interactive), unconditional/unsuppressible:
`STORY_RUN: <resolved RUN_DIR> source=DEVLOOP_TMP|DEVLOOP_STORY_RUN_BASE|default`. Serves (a) the harness
read-back (RUN_DIR_OF derives from this grep, replacing the :688 restatement — kills the SSoT drift trap),
(b) operator visibility (evidence location, currently underivable), (c) the `source=` discriminator security
+ test want. **Vocabulary is `STORY_RUN:`, NEVER `STATUS=`/`REASON=`** — a `STATUS=` line would cast a
spurious vote in Layer-3's verdict via `tee_collect_statuses` (test.sh header ~:81-91) AND break the
read-back; a site comment pins this so a cosmetic edit can't silently break it.

**Seam H2 sibling assertion (Lead RULING — ADD it, not a comment; DRY traced it against all four checks).**
`__seam_assert_run_dir_isolated`'s docstring names two hazards; the decomposition removes coverage of the
second in TWO ways, both silent:
- **H1 (EXIT-trap marker deletion)** lives on `RUN_BASE`/`DEVLOOP_TMP` — `:210`/the `/tmp/devloop` literal
  stays CORRECT, untouched.
- **H2 (fixture evidence interleaving)** now concerns the RUN_DIR base. `DEVLOOP_TMP` dominating precedence
  only closes "the default is never *silently* used"; it does NOT close H2, because a
  `DEVLOOP_TEST=1 DEVLOOP_TMP=<ledger base>` run has `DEVLOOP_TMP` present and winning and STILL lands
  `RUN_DIR` identical to a live run's — `:211` only refuses `-ef /tmp/devloop` (post-decomposition the
  MARKER base, not the ledger base). DRY confirmed all four checks (`:192`/`:211`/`:215`/`:221`) pass in
  that scenario. And `:221`'s in-use check covered the run dir only *incidentally*, while marker and run
  dir shared a base ("is a marker here" doubled as "is evidence here"); the decomposition splits the
  question and `:221` silently keeps answering only the marker half — it cannot announce this, since from
  its own view nothing changed. A control covering two concerns by co-location loses one silently when the
  co-location ends.
- **Fix — DEFINITIVE (Lead's final H2 ruling; operations' OR'd form. Security confirms literal-only is safe;
  the OR'd refinement is the Lead's ruling, priced at one extra `-ef` + one refusal message. The "derive the
  target" instruction is RETRACTED.)** LEFT derives, RIGHT is an OR of TWO LITERALS. Add a sibling clause
  beside `:211`:
  - LEFT operand DERIVES — `resolve_run_base run-dir` (the resolved run-dir base UNDER TEST; it *should*
    follow the environment, it's the run's own base). This is the SAME left side as `:211`. The two clauses
    are **symmetric in FORM** (both a test-resolved LEFT `-ef` a non-relaxable literal RIGHT, same reason)
    but **NOT symmetric in COVERAGE**: `/tmp/devloop` is the marker base on EVERY lane incl. the host hatch,
    whereas `/home/dev/.cache/...` is the ledger base ONLY in-container — and that coverage gap is exactly
    WHY clause (b) below exists. An unqualified "they're symmetric" is the sentence that would hide the
    host-lane gap, so state both (stated at the SEAM-ASSERTION site in run-story.sh, not only the guard).
  - RIGHT: refuse if the LEFT base `-ef` **EITHER** (a) `/home/dev/.cache/devloop/story-runs` — the container
    production base, an env-IMMUNE floor; **OR** (b) `${HOME}/.cache/devloop/story-runs` — adds coverage of
    the `STORY_RUNNER_ALLOW_HOST` host-hatch lane where the real base is the operator's `$HOME` (security's
    live H2 gap: without (b), a host-lane `DEVLOOP_TMP` at the operator's own ledger compares against a
    literal that doesn't exist → `-ef` false → passes → H2 open). Each clause gets its OWN refusal text
    naming which matched (a parameterised "is a ledger base" without saying which sends the operator to the
    wrong path — same discipline as `FINISH-FILE-MISMATCH`/`FINISH-CONTENT-MISMATCH`).
  - **Decision rule pinned at the site (so this can't be "simplified" back to a bare derive):** the
    comparison FLOOR must be a value the test process's environment cannot influence; a derived clause may
    only be OR'd ON TOP, never be the sole target. OR-ing a redirectable clause (b) with a non-redirectable
    one (a) means (b) can only ADD coverage, never subtract — the literal (a) is a floor nothing in the env
    can lower. Neither pure form works alone: literal-only misses the host lane; derived-only is
    env-relaxable (`resolve_run_base` reads `DEVLOOP_STORY_RUN_BASE`/`HOME`, so a nonexistent target makes
    `-ef` compare a missing operand → false → passes → the `:200-209` hazard, one base over).
  - `-ef` is device+inode identity → **exact directory match, NOT prefix containment** (security checked; a
    carrier/`DEVLOOP_TMP` at a broad parent does not sweep fixtures beneath it) — pin this at the site so
    nobody loosens it to prefix matching on a wrong assumption.
  - Own re-made `-ef` fresh-host argument (absent literal ⇒ no live run sharing it, missing-operand pass is
    correct; a live run necessarily creates the dir, so the check is present exactly when it can matter —
    stronger than `:196-199`). DISTINCT refusal token(s) (NOT `SEAM-RUN-DIR-NOT-REDIRECTED`). Split the
    `:189`/`:212` messages so each names its own hazard; fix the `:171`/`:222` comments (RUN_DIR no longer
    under RUN_BASE — falsified; no live control lost).

**Sync guard — `validate-run-dir-path-sync.sh`, IN-SCOPE (Lead REVERSED E; new facts: the sibling literal
is a NEW encoding this diff introduces, so `:779` requires it carry a binding check — in-scope by the same
standard as `.ledger-mount`/`.owner`, not the pre-existing-untouched case E was about).** New file, STATIC
extract-and-compare of SOURCE TEXT ONLY (NO runtime resolution — a guard whose verdict depends on its runtime
env has the same defect it guards; DRY/security's load-bearing point), `[ -f ] || fail` per source as its own
positive control, env-overridable source paths for self-test. Follows the in-tree precedents
(`validate-slug-class-sync.sh`, `validate-subdomain-regex-sync.sh`, `validate-gsa-sync.sh`) — a new file on an
established pattern, not new machinery. Covers BOTH: (i) the **ledger-base literal pair** (run-story's default
tail + the H2 sibling literal ↔ `devloop.sh`'s `CONTAINER_LEDGER_BASE`) — the `:779` binding for the sibling
literal; AND (ii) the **pre-existing `INFLIGHT_MARKER` pair** (`devloop.sh:715` ↔ `run-story.sh:517`, folded in
at marginal cost now the guard exists — closes the open-point-(a) near-miss). SCOPE comment carries DRY's
THREE statements, separated: (1) the marker pair agrees; (2) the ledger-literal pair agrees; (3) marker base
and ledger base are DELIBERATELY NOT in agreement with each other (two-key decomposition — so a reader seeing
(1)+(2) doesn't "fix" the one that differs and re-collapse the keys). Plus the TWO host-lane limits, named not
assumed-total: `STORY_RUNNER_ALLOW_HOST` ($HOME ≠ /home/dev); and a runtime-redirected carrier (fundamental —
a test process cannot know another process's redirected path). Keep security's in-diff decomposition-site
comment ALONGSIDE the guard (guard = enforcement after the fact; comment = stops the author *proposing* the
marker move, where open-point (a) actually went — different moments, neither redundant). Both `:779` legs:
literal floor for non-relaxability + static guard for non-staleness.

**cleanup safety (ops O-2/O-15):** `cleanup()` must NOT delete `story-runs` (now FLAT/shared — deleting on
one slug's `--destroy` nukes other slugs' stories); it removes `$HELPER_RUNTIME_DIR` (:213) and
`$HOME/.cache/devloop/devloop-${TASK_SLUG}` (:216), neither of which is `story-runs` — asserted, with a
do-not-widen comment at both `rm -rf` sites. `--destroy` prints one line naming the retained `story-runs`
path (ops Q1). **`DEVLOOP_TMP` stays put** (verdict + layer logs + probe marker derive from it) — comment
pins that it must not be unified with the ledger base.

### D6/f — commit-intent checkpoint + `--finish` lane (full envelope)

**Write side (SKILL, at Gate-3 close — AFTER the owner co-sign, BEFORE Step 8 commit; ordering stated at
the write site, S-18/S-20).** Runner passes `DEVLOOP_COMMIT_INTENT_FILE=$RUN_DIR/task-<id>.commit-intent.json`
into the headless spawn (env precedent: `DEVLOOP_START_HEAD`/`DEVLOOP_STOP_COUNT_FILE`). Open-point (b)
resolved: that env var is the path; documented in SKILL + here. At Gate-3 close the devloop stages its
changeset, captures the SAME extraction `--finish` will compare against (Lead item A — like-for-like), and
writes the intent, then proceeds to Step 8. Schema stated ONCE in SKILL beside the escalation contract;
run-story references it via `ANCHOR (DRY)` comment (DRY C6); `--finish` fails closed on any unreadable field.
```json
{ "story": "docs/user-stories/<file>.md",          // S-10
  "task_id": <id>,                                 // S-10
  "head": "<DEVLOOP_START_HEAD the intent was written against>",   // S-10
  "slug": "YYYY-MM-DD-task-slug",                  // SLUG_CLASS_CANONICAL at read (S-11)
  "message": "<complete RENDERED commit message: subject+body+trailers>",  // DRY C6 — rendered, no hard-coded Co-Authored-By
  "raw": "<git diff --cached --raw -z --no-abbrev, base64/quoted>" }        // S-18-revised: path+status+blob+mode, native add/delete
```

**Read side (`run-story.sh --finish`).** Its OWN branch, placed BEFORE the fresh-start clean-tree check
(code-reviewer — `--finish` EXPECTS a dirty tree), with its own `RETRY_APPLIED` one-shot; does NOT run the
`NO-COMMIT-EVIDENCE`/`NO-COMMIT-REFUSED` checks (inverse precondition — O-6). Fail-closed throughout; every
refusal a distinct STORY_RUN token; after any post-staging refusal `git reset` the index so a failed finish
doesn't wedge the next run (ops NEW-1).
1. `esc_rec` present or `NO-ESCALATED-TASK`; intent present or `FINISH-NO-INTENT`; parse ok or
   `FINISH-INTENT-MALFORMED` (test's distinct token — must not collapse to an empty file-list).
2. **Bindings (S-10):** `story`==story file ∧ `task_id`==id ∧ `head`==current HEAD baseline, else
   `FINISH-STALE-INTENT`. **No expiry check** — bindings are the control; state "no expiry, and why" (S-21).
3. **Path validation at PARSE time (S-7, Lead — security measured the hole):** reject (not sanitise) any
   `files`/raw path that is absolute, contains `..`, starts with `:` (pathspec magic `:/`,`:(glob)`,`:!`,`:^`
   each stage the WHOLE repo and pass a naive check), is `.`, empty, or violates the char floor. git's
   rc-128 symlink-escape refusal is handled as a refusal, not a `set -e` abort.
4. **Trailer vocabulary (S-9):** validate each trailer line shape AND `Approved-Cross-Boundary:` against the
   known specialist vocabulary — a well-formed trailer naming an uninvolved specialist is refused (it's the
   ADR-0024 §6.7 owner co-sign record, a governance control).
5. **Verdict via layer-all.sh, NOT run_gate (ops NEW-1, BLOCKER — Lead endorsed, OVERRIDES DRY C3's
   run_gate reuse):** `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (mirroring story-close :1705). run_gate
   runs layers individually and emits NO Gate-2 verdict; the pre-commit hook (`_gate2_binding.sh:647-690`)
   requires a verdict at `${DEVLOOP_TMP}/gate2-verdict` (produced ONLY by layer-all.sh's EXIT trap :72) once
   a devloop-shaped changeset (main.md at Phase=complete) is staged, and blocks fail-closed without one — so
   run_gate would run the full gate and then be rejected at commit. `DEVLOOP_TMP` is unchanged by D5, so the
   verdict path is intact. This is still shared authority (story-close uses layer-all.sh), not a fork. NOT
   `--no-verify` (masked-failure class; undercuts the replay-a-reviewed-decision claim).
6. Red → re-escalate `finish-pipeline-red-layer<N>` (exit 1).
   **Post-stage unstaged-remainder check (ops, `--revalidate` parity — BUILD PROACTIVELY per Lead):** layer-all
   ran over the WORKING TREE (local mode = committed+staged+unstaged+untracked), so a file dirty-but-UNSTAGED
   outside the intent set passes the pre-stage index-clean check AND the staged-set equality, yet its content
   is in the green but not the commit — a green not reproducible from the commit (the exact
   `REVALIDATE-DIRTY-TREE` reasoning at :1326). AFTER staging, reuse the C7 tree-attestation helper
   (`git status --porcelain -- . :(exclude)$STORY_FILE`, minus the staged intent set) — any remaining
   change → own `--finish` token naming the offending paths, refuse BEFORE the commit. NEW-1 only backstops
   this with a wrong-place/wrong-message Gate-2 signature failure, so this refuses in the right place.
7. Green → **S-8 pre-stage:** assert index clean rel to HEAD; stage exactly the intent's files
   (`git add -- "$f"`, `--` mandatory); **single extraction (S-18-revised, folds S-8+S-18):**
   `git diff --cached --raw -z --no-abbrev`, compare {path,status,post-image blob,mode} to the intent's
   captured raw as SETS, exact equality — "set matched but content didn't" is unrepresentable, add/delete
   native, catches mode flips, avoids the `--no-filters` CRLF trap and the `hash-object` exit-128-on-delete
   trap (ops O-16). Path-set mismatch → `FINISH-FILE-MISMATCH`; content/blob mismatch → distinct
   `FINISH-CONTENT-MISMATCH` (ops O-16 — different repair). Both `git reset` + resume with an HONEST
   two-branch recovery message (revert the post-Gate-3 edit + re-`--finish` (cheap) OR resume for re-review
   (expensive) — never imply resume is free).
8. `git commit -F <message-file>` (S-9 — never argv/eval); reuse `complete_task`'s slug-derivation +
   `dt-story complete` + ledger + suppression-note + amend via a new `finish` `cost_mode` arm (DRY C3, minus
   the gate which NEW-1 moved to layer-all). **KEEP the amend — do NOT fold the manifest into the `-F` commit**
   (ops verified by reading the hook): `gate2_staged_complete_slug` reads index-vs-HEAD, and on the amend
   only `$STORY_FILE` is staged (HEAD already carries `main.md` from the `-F` commit), which
   `gate2_is_devloop_mainmd` rejects → trigger conjunct 1 false → hook no-ops, no verdict needed. Folding the
   manifest INTO the `-F` commit would instead satisfy both conjuncts and make a structurally-exempt step
   verdict-dependent. The amend exemption is already exercised on every normal + `--revalidate` completion,
   so this is not new behaviour.
9. **Accept-path receipt (S-20/obs, bounded):** one `slog` — `intent=<file> task=<id> head=<sha> paths=<n>
   pathset_ok=true digests_ok=true` (counts+verdicts, not per-path enumeration; the intent file is the
   durable enumeration). Composable with `--stop-after`.

### D6/g — `--interactive` retry lane

Flag family, `RETRY_APPLIED` one-shot, `NO-ESCALATED-TASK` refusal. **TTY gate up front (O-9/S-13):**
`[ -t 0 ] && [ -t 1 ]` (+ `-t 2`), else `INTERACTIVE-NO-TTY` + exit 2 — with the timeout dropped and hook
inert a no-TTY spawn hangs forever holding the container/cluster/marker. **Never** auto-selected by
`canary_classify` or any recovery lane (S-13) — explicit operator flag only; auto-suggest SUGGESTS, never
selects.

**Structure (code-reviewer, Lead — fail-CLOSED):** branch at ENTRY to the session-limit inner loop, not
per-lane guards inside it:
```
if [ "$INTERACTIVE_SPAWN" = 1 ]; then
  env -u DEVLOOP_HEADLESS claude "<prompt>" --model "$STORY_MODEL"   # attached; NO -p, NO timeout, NO stream-json redirect, NO --dangerously-skip-permissions
  claude_rc=$?
else
  <existing inner while-loop, VERBATIM>
fi
```
Interactive then structurally never enters the canary/session-limit machinery (fail-closed vs R-2-defect-4
fail-open). `env -u DEVLOOP_HEADLESS` (O-11/S-12 — `-u`, not mere omission, so an ambient export can't
re-arm the hook; NO settings.json edit). No `DEVLOOP_START_HEAD` either → the Stop hook self-gates inert
(devloop-stop-hook.sh:14). Drop `--dangerously-skip-permissions` (S-15 — a human can answer prompts; the
flag's "no human present" premise is false here). Prompt built via the shared prompt-builder helper (DRY C5)
WITHOUT the `HEADLESS RUN` prefix (Lead asks the human), `--continue=<slug>` if a resume pointer exists else
fresh `--specialist`.

**Post-spawn (O-10/S-14): HEAD movement + gate rc are the authority.** Of the shared tail, only the
`claude_rc != 0 → escalate devloop-session-error` at :1614 is semantically load-bearing to guard (a
deliberate human exit must not auto-escalate); the `${canary_class:-}` guard at :1608 and the `124` timeout
at :1599 can't fire without the wrapper/canary anyway. The `.devloop-escalation.json` check, no-commit
check, layer-all/gate, and complete stay **unchanged** (S-14 stated explicitly). A human who commits →
gate+complete; who exits dirty → no-commit escalate. The interactive no-commit escalation records
`claude_rc` (security — a session that never started reads as "no commit" but the rc explains why).

**Ledger (S-17/O-12/O2/O2a/obs, Lead):** `kind=interactive`, `usd:null` (NEVER 0 — jq `add` folds
0/null/absent all to $0), `measured:false`, `cost_source:"attached-session-no-stream-json"`, plus
`wall_clock_seconds` (obs pressed — it's `date +%s` either side, the existing :1031/:1695 pattern, no session
log needed, and it's the only magnitude that survives; ACCEPTED) and `started_at`/`ended_at`. **O2a:** both
`complete_task` and `escalate()` take the lane's `cost_mode` so `report_task_cost` never re-greps the STALE
prior `task-N.devloop.log` and attributes it to this attempt. Auto-suggest: `escalate()` counts
`task-N.runner-escalation.*.json` and appends a "consider --interactive" SUGGESTION line on the 2nd+
escalation (O-13 — suggestion, never auto-switch).

### Cost-ledger invariant (Lead — ONE statement; O1/O2/O4/O5/O6)

State ONCE, in plan + code, over what window and with what supersession (obs's single-owner clause folded in
— the bound is the WRITER, not the story, since flat story-keying lets two containers produce
mutually-non-superset accumulations): *a ledger entry with no `kind` (or `kind=devloop`) is a cumulative
superset of this task's attempts within this story **for a single live owner**; two concurrent runs over one
story produce accumulations that are supersets of each other in neither direction, which is what `.owner`
exists to prevent; an entry with any other `kind` is a per-lane singleton that never supersedes a devloop
entry; unmeasured lanes carry cost fields as `null`+`measured:false`, never 0.* Consequences:
- **Rollup fix (:1718):** last devloop entry per task PLUS lane singletons counted separately — a
  zero/placeholder/short lane entry can no longer erase a task's real cost (obs reproduced $6.90→$2.10 today
  for `kind:"revalidate-gate-only"`; `finish`/`interactive` would triple it). Emit `unmeasured_tasks=N`.
- **kind on EVERY entry** (incl. `kind=devloop`) + one uniform `COST` line shape (O5) so the invariant is
  mechanically checkable.
- **Window (O4 accounting half — IN-SCOPE, Lead rule 1; NOT "resolved by construction"):** persistence makes
  `report_task_cost` re-grep a `task-N.devloop.log` that now survives across generations, so a same-story
  re-run accumulates across generations regardless of key — the invariant, not the flat key, is what makes
  this honest. The window is stated explicitly ("all attempts for this story, all generations") on the COST
  and STORY-COST lines so the number and its label always agree; `report_task_cost` does not silently
  double-count under a story-level label. (The log-GROWTH half of O4 is the deferred retention TODO.)
- **Concurrent-owner abstain (obs, defense-in-depth — RECORDED GAP):** the `.owner` guard (see §D5) is the
  primary control; obs's owner-field-on-every-entry + rollup-abstain-on-multiple-owners is accepted as
  optional and RECORDED as a residual gap per obs's "take `.owner` and I'll accept the residual."
- **Fail-loud (O6):** `STORY_RUN: COST-UNAVAILABLE task=<id> reason=<…>` on a jq/append failure (telemetry
  only, no control flow).
- **Measurement (Lead item B, IN):** a read-only size line — run-dir size + bytes-per-attempt of
  `task-N.devloop.log` — on the COST line / beside the story-close rollup. No threshold, no env, no
  deletion; D5's stated purpose is measurability and this can only be captured on a live run (story 2 now).

### Flag discipline + reuse (R-5; DRY C4/C5/C7; code-reviewer)

Booleans `--finish`/`--interactive`: argv case + duplicate-flag guards + usage strings + `UNKNOWN-ARGUMENT`
line. Mutual exclusion: keep the specific `--revalidate`+`--restart` → `REVALIDATE-WITH-RESTART` check FIRST
(its tailored guidance; N1 kept), then ONE general `(REVALIDATE+RESTART+FINISH+INTERACTIVE) > 1` →
`MULTIPLE-RETRY-FLAGS` naming which were set (not six pairwise ifs). Both new flags join `RETRY_APPLIED`;
`--revalidate`/`--restart` control flow untouched. Reuse: shared prompt-builder helper for headless +
interactive (C5, keeps the R-2-defect-5 out-of-band discipline in one home; lanes differ only in claude
flags); extract a tree-attestation helper for the `git status --porcelain -- . :(exclude)$STORY_FILE`
pattern now that `--finish` is the 3rd caller (C7, beside git_head/tree_dirty/git_error_lane).

### Tests (run-story.test.sh) + manual test plan

New section (O), mirroring (N)'s REUSE_FIXTURE pattern:
- flag discipline: `--finish`/`--interactive` duplicate tokens; `MULTIPLE-RETRY-FLAGS` for combos (N1
  `REVALIDATE-WITH-RESTART` intact); `--finish` + `--stop-after` composition (test).
- `--finish`: NO-ESCALATED-TASK; FINISH-NO-INTENT; FINISH-INTENT-MALFORMED (unparseable JSON, distinct
  token); FINISH-STALE-INTENT (HEAD moved); green→layer-all verdict→commit+complete (slug recovered, ledger
  kind=finish); FINISH-FILE-MISMATCH — set equality as THREE cases (obs/Lead, vacuity class): staged path the
  intent omits FAILS, intent path not staged FAILS, and the clean case as the positive control (a
  one-directional/containment check reports clean while testing the wrong side; the accept-path receipt's
  `pathset_ok=true` can't distinguish a correct compare from an inverted one); FINISH-CONTENT-MISMATCH (blob);
  post-stage unstaged-remainder refusal (own token, `--revalidate` parity — a dirty-unstaged file outside the
  intent set); index-reset-on-refusal; gate-red→`finish-pipeline-red`; RETRY_APPLIED one-shot (2-task
  fixture). Positive control (test-C): assert the claude stub was NOT invoked with `-p`
  during --finish PAIRED with the layer-all/gate marker PRESENT (no vacuous absence).
- SECURITY/edge fixtures (landed as O24/O25/O26 — the doc now matches the suite): O24 staged-entry
  discrimination (FAKE_DEVLOOP_STAGE reproduces the producer's post-crash STAGED index so the reset-and-
  rederive fix is actually EXERCISED, not vacuously green — security F-3; **discrimination VERIFIED by a
  one-time hand mutation 2026-09-17**: reinstating the removed `git diff --cached --quiet → FINISH-INDEX-DIRTY`
  precondition reds O24 alone, confirming it reproduces the staged state; O24 also normalises the MKOUT doc's
  future mtime to avoid a git racy-timestamp edge unique to carrying a staged future-mtime file across the
  REUSE); O25 delete-path (`raw` carries a
  `D` status; per-file `git add` stages the removal with no set-e abort and commits — the design note's
  native-delete claim); O26 CRLF (a CRLF file commits cleanly — the `--raw` STAGED-blob comparison passes
  capture and re-stage through the SAME filter, so the original `--no-filters` hash-object CRLF trap does
  NOT apply here; this REPLACES the planned "plain hash-object == staged blob" fixture, which was obviated
  when S-18-revised moved from per-path hash-object to the single `git diff --cached --raw` extraction); a
  `files:[":/"]` parse-time-rejection case (S-7, O12).
- `--interactive`: NO-ESCALATED-TASK; INTERACTIVE-NO-TTY; spawns attached (argv has NO `-p`, NO
  `--output-format stream-json`, NO `--dangerously-skip-permissions`; `devloop.session.headless` marker
  ABSENT) with the mode=interactive stub marker PRESENT (test-D positive control); falls through to
  no-commit escalate vs gate+complete on `FAKE_INTERACTIVE_COMMIT`; ledger kind=interactive `usd:null`.
- `--finish` NO model turn (test-C positive control): assert the claude stub was NOT invoked with `-p`
  during `--finish` PAIRED with the layer-all/gate marker PRESENT (absence-only would pass vacuously if
  `--finish` no-op'd). FINISH-NO-SLUG: intent `files` omits the output-doc `docs/devloop-outputs/*/main.md`
  → distinct fail-closed refusal token (separate from FINISH-INTENT-MALFORMED — a valid-JSON-but-no-output-doc
  intent is a different failure than unparseable).
- cost rollup: a lane entry does NOT erase a task's real devloop cost in the fixed rollup (obs's
  $6.90→$2.10 reproduction as the fixture).
- **`resolve_run_base <kind>` unit test (Lead F-1 / test / security — the SOLE mechanical assertion of D5's
  default, since the seam makes the full-run else-branch unreachable):** three precedence branches as
  SEPARATE cases with distinguishable tokens — (1) `DEVLOOP_TMP` set → wins (both others ignored);
  (2) `DEVLOOP_TMP` unset + `DEVLOOP_STORY_RUN_BASE` set → wins over the HOME default; (3) both unset,
  `HOME` PINNED to a fixture → exact `$HOME/.cache/devloop/story-runs/story-runner`. Assert the EXACT
  composed string (never `[ -n … ]`, mech #1); pin HOME explicitly (never ambient, mech #5). Also assert
  `resolve_run_base marker` stays `${DEVLOOP_TMP:-/tmp/devloop}/story-runner` (the two bases don't collapse).
- **RUN_DIR startup emission read-back (test #1):** harness greps the single `STORY_RUN: <path> source=…`
  line, RUN_DIR_OF DERIVES from it (replacing the :688 restatement — kills the drift trap); startup
  precondition REFUSES (distinct token) if the line is absent EXCEPT on the two fail-loud refusal lanes
  (UNSAFE-RUN-DIR / RUN-DIR-UNWRITABLE, which assert their own token and are exempt). "read-back empty" is a
  separate assertion/token from "no record written". Assert RUN-DIR-UNWRITABLE names the refused path +
  recovery command.
- **H2 sibling seam assertion (test) — TWO clauses (OR'd), separate cases + tokens:** clause (a)
  `DEVLOOP_TEST=1 DEVLOOP_TMP=/home/dev/.cache/devloop/story-runs` (the container literal) refused;
  clause (b) `DEVLOOP_TMP=$FIXHOME/.cache/devloop/story-runs` (the HOME-derived host-lane path, with HOME
  pinned to FIXHOME) refused. Each with its OWN token (NOT `SEAM-RUN-DIR-NOT-REDIRECTED`; pin the token,
  distinct per clause so a broken clause can't pass against the other's message). Positive control pointed AT
  the exit path: establish the run reached THIS check (earlier seam checks passed / dir otherwise usable),
  else an earlier seam refusal reads green against the wrong clause. Hermetic — LEFT = `resolve_run_base
  run-dir` (`DEVLOOP_TMP`-based here); point `DEVLOOP_TMP` at each literal in turn; the fixture never touches
  the operator's real `~/.cache`. Right operands are LITERALS, not derived (§D5; derive form retracted).
- **`validate-run-dir-path-sync.sh` self-test:** the guard reds when the ledger-literal encodings
  (`CONTAINER_LEDGER_BASE` ↔ run-story's default tail + H2 literal) OR the marker pair (`devloop.sh:715` ↔
  run-story `INFLIGHT`) are made to disagree via its env-overridable source paths; greens when they agree.
  (Static source-text extraction — no runtime resolution.)
- **`:652` `env -i` comment (test/security/DRY, comment-only):** one block naming (a) `HOME=$FIXHOME`'s role
  (a trimmed `HOME=` lets a case write the operator's real `~/.claude/settings.json` via
  preflight-story.sh:140); (b) the `env -i` scrub as a SECOND independent layer keeping the container's
  ambient production carriers (`DEVLOOP_STORY_RUN_BASE`, an exported `DEVLOOP_TMP`) out of the runner-under-test
  — framed as belt, not sole barrier (mandatory-`DEVLOOP_TMP` already forecloses live exposure); (c) revisit
  trigger — if `DEVLOOP_TMP` ever stops being mandatory under `DEVLOOP_TEST=1` (:188-191), the
  carrier-inertness argument collapses and the waived sentinel-gated refusal becomes necessary again.
- **`.owner` guard:** a second live owner on the same story dir is refused; a stale owner is removable.
- D5 item-A reachability: (1) under the seam `DEVLOOP_TMP` wins → RUN_DIR = `$DT/story-runner/<story>`
  (existing d1 reaffirmed; isolation cases green by construction since the seam block is untouched);
  (3) unwritable base → `RUN-DIR-UNWRITABLE`, never /tmp (chmod a `DEVLOOP_TMP` dir, hermetic). The
  production `$HOME/.cache` default branch is unreachable under `DEVLOOP_TEST=1` in a FULL run, which is
  exactly why `resolve_run_base` is extracted and unit-tested directly (above) — the default is ASSERTED,
  not narrated. DRY measured that an unset HOME + unset vars trips `set -u` (fails loud, not a silent
  expansion), so NO `env -i … HOME=` comment at the default site (that hazard doesn't exist); the real
  `HOME=` note belongs at preflight-story.sh:140 and is out of scope.
Stub: `mode=interactive` arm (no -p, no stream-json) recording its own marker + `FAKE_INTERACTIVE_COMMIT`.
**Manual-only** (no mechanical path): D5 container destroy/re-create persistence (state it survives resume
of the same STORY — flat/story-keyed, so cross-slug resume finds it too, not merely same-slug), and the
`LEDGER-NOT-PERSISTENT` token (requires `DEVLOOP_TMP` unset, which the seam forbids under `DEVLOOP_TEST=1`).

### Scope Partition (Lead ruling — authoritative)

**IN (build now):** D5 two-key decomposition (RUN_DIR default→flat story-keyed persistent mount; marker +
probe + `:210` seam literal + INFLIGHT_MARKER + preflight UNCHANGED; `resolve_run_base <kind>` extracted +
unit-tested; `DEVLOOP_STORY_RUN_BASE` non-seam carrier = the actual mount target, `DEVLOOP_TMP`-dominant
precedence pinned by comment+test; H2 sibling seam assertion (LEFT derives / RIGHT = OR of two literals,
container floor + host-lane); validate-run-dir-path-sync.sh (static, binds the new ledger-base literal
pair + folds in the pre-existing marker pair — E REVERSED, in-diff); `.owner` concurrent-story guard; startup
RUN_DIR emission with `source=`; char-floor kept; loud
RUN-DIR-UNWRITABLE + LEDGER-NOT-PERSISTENT positive control, never /tmp; 0700 + do-not-promote note +
do-not-widen cleanup comments + `--destroy` names the retained path); full `--finish` envelope (Gate-3-close
write-ordering, story+task+HEAD binding, parse-time pathspec-magic rejection, trailer-vocab validation,
layer-all verdict per NEW-1, single-extraction staged-set+content check, `-F` commit, index-reset-on-refusal,
distinct mismatch tokens with honest recovery, S-20 receipt); cost-ledger invariant (rollup fix + kind
discriminator + null-not-zero unmeasured + wall_clock + unmeasured_tasks=N + COST-UNAVAILABLE + size
measurement); TTY gate; hook relaxation via `env -u DEVLOOP_HEADLESS`; reuse (layer-all/complete_task/
prompt-helper/tree-attestation-helper/one exclusive-flag-group).

**OUT → `docs/TODO.md` §Story Workflow Follow-ups (ops already filed the entry — I only VERIFY obs's three
constraints are folded into that ONE entry, no second entry):** the active retention/prune engine
(`STORY_RUN_RETENTION_DAYS`, tiered age-based prune-at-start, `retention.log` tombstone, Tier-A/B two-lifetime
split, prune-only-manifest-complete predicate, dangling-evidence-pointer hygiene / inline-fields-not-raw-tails,
per-attempt sizing → "size the run dir after story-2 task 1, revise the default on data"). Rationale (Lead):
a prune subsystem is new mechanism untraceable to ADR-0037 D5 ("a RUN_DIR default change plus loud-fail-if-
unwritable, fallout none material"), unevidenced until story 2. In-scope mitigations (0700 + do-not-promote,
scoped to the secrets-bearing log class per S-19 narrowing) ship now.

---

## Manual Test Plan

(Ledger persistence D5 can only be proven on a real container run — it belongs here, not in unit tests.)

1. **Ledger persistence (D5).** After a story-runner run, note a task's ledger entry (`jq` its task id), then destroy the container (`devloop.sh --destroy <slug>`) and re-create it and resume the SAME STORY (flat/story-keyed, so cross-slug resume finds it too — not merely same-slug). Assert **that specific pre-destroy task entry is still present** in `~/.cache/devloop/story-runs/story-runner/<story>/cost-ledger.jsonl` — NOT merely that the file exists (a fresh empty-but-present file passes "exists" and is the D5 failure dressed as a pass — ops).
2. **`LEDGER-NOT-PERSISTENT` positive control (manual — the token needs `DEVLOOP_TMP` unset, which the seam forbids under `DEVLOOP_TEST=1`).** Start a run in a container created BEFORE this change (no `.ledger-mount` marker / no mount) and confirm it refuses loudly with `LEDGER-NOT-PERSISTENT` rather than silently writing to a container-local dir.
3. A `run-story.sh --finish` invocation commits a reviewed-but-uncommitted task with no model turn (layer-all verdict, no `claude` spawn), from a recorded commit-intent, and refuses (distinct token, honest two-branch recovery) on any path-set/content/HEAD/malformed mismatch.
4. A `run-story.sh --interactive` invocation drops the operator into an attached `claude` (no `-p`, no timeout, hook inert) for the escalated task; after they exit, the runner completes or escalates through its unchanged machinery; the ledger records `kind=interactive` with `usd:null`+`wall_clock_seconds`. **The attached-spawn argv contract is MANUAL-ONLY by construction** (an attached spawn requires a controlling TTY, which the hermetic `env -i` harness lacks — the TTY gate refuses first, which is itself unit-tested as `INTERACTIVE-NO-TTY`). During the manual run, confirm the spawned `claude` argv carries NO `-p`, NO `--output-format stream-json`, NO `--dangerously-skip-permissions`, and that `DEVLOOP_HEADLESS` is unset for it (the stub's `mode=interactive` arm records exactly these negatives for the day a pty-harness exists).
5. **`--finish` Gate-2 hook interaction (real repo only).** In a real repo with the pre-commit hook active, confirm `--finish`'s `git commit -F` is accepted because `layer-all.sh` produced the verdict, and that `complete_task`'s subsequent manifest amend does NOT re-trip the hook (index-vs-HEAD; only `$STORY_FILE` staged → `gate2_is_devloop_mainmd` rejects → no-op). The hermetic suite sets `core.hooksPath=/dev/null`, so this path is not exercised there.

---

## Implementation Summary

Built exactly to the approved plan; all 344 hermetic assertions in `run-story.test.sh` pass (290 pre-existing + 54 new, zero regressions), the new guard passes, and both shell scripts pass `bash -n`.

**D5 — ledger persistence (two-key decomposition).**
- `infra/devloop/devloop.sh`: `STORY_RUNS_HOST` (flat host dir) + `CONTAINER_LEDGER_BASE` (single source of the in-container path); unconditional `-v …:z` mount + `-e DEVLOOP_STORY_RUN_BASE=` + host-written `.ledger-mount` marker, `mkdir -p -m 0700` before `podman run`; do-not-widen comments at both `cleanup()` `rm -rf` sites + `--destroy` names the retained path.
- `scripts/workflow/run-story.sh`: `resolve_run_base <kind>` (marker vs run-dir, `DEVLOOP_TMP`-dominant precedence pinned in-comment); `RUN_DIR` default → `resolve_run_base run-dir`; `RUN_BASE`/`INFLIGHT` unchanged on the marker base; `LEDGER-NOT-PERSISTENT` positive control + loud `RUN-DIR-UNWRITABLE` (never /tmp); char floor kept, `$HOME` clause added; single unconditional `STORY_RUN: RUN-DIR <path> source=…` startup emission; H2 sibling seam assertion (LEFT `$resolved`, RIGHT = OR of two literals) + `:189` message split; `.owner` concurrent-same-story guard.

**D6/f — `--finish`.** `finish_lane()`: intent parse + `FINISH-NO-INTENT`/`-INTENT-MALFORMED`/`-STALE-INTENT` (story+task+HEAD bindings, no expiry) + `FINISH-BAD-TRAILER` (Approved-Cross-Boundary vocab) + `FINISH-UNSAFE-PATH` (parse-time pathspec-magic/absolute/`..` rejection) + `FINISH-NO-SLUG` (output-doc required); `layer-all.sh` verdict (NEW-1, not `run_gate`); S-8 pre-stage clean index; per-file stage; set-equality path check (`FINISH-FILE-MISMATCH`) + staged-raw content check (`FINISH-CONTENT-MISMATCH`) + post-stage remainder (`FINISH-DIRTY-REMAINDER`); `git commit -F` + reuse `complete_task … finish`; `FINISH-VERIFIED` receipt; index-reset on every post-stage refusal. SKILL Step 8 write-side + schema (ordering: after co-sign, before commit).

**D6/g — `--interactive`.** Branch at ENTRY to the session-limit loop (fail-closed); `INTERACTIVE-NO-TTY` gate; `env -u DEVLOOP_HEADLESS` + no `-p`/timeout/redirect/`--dangerously-skip-permissions`; shared `build_devloop_prompt` helper (no `HEADLESS RUN` prefix); the three `claude_rc`-driven lanes skipped for interactive (only HEAD movement + gate rc decide), non-zero exit recorded as evidence; `escalate()` auto-suggest on 2nd escalation; per-iteration `INTERACTIVE_SPAWN` reset.

**Ledger invariant.** Stated once; `report_task_cost` gains `kind:"devloop"`/`measured:true` + `COST-UNAVAILABLE` loud lines; `append_lane_cost` (compact JSONL) for gate-only/finish (true-zero) and interactive (`usd:null`+`measured:false`+`wall_clock_seconds`); `escalate()`+`complete_task` route the lane cost_mode (O2a); rollup rewritten to last-devloop-entry-per-task + lane-singletons + `unmeasured_tasks=N` + window label; read-only `RUN-DIR-SIZE` measurement line.

**Guards / reuse.** New `scripts/guards/simple/validate-run-dir-path-sync.sh` (static extract-and-compare; binds the ledger-base literal pair + folds in the marker pair; 3-statement SCOPE + host-lane limits; auto-discovered by `run-guards.sh`). `working_tree_status` helper (C7) + `build_devloop_prompt` (C5) + one exclusive-flag-group (C4).

## Files Modified

- `scripts/workflow/run-story.sh` — D5 base resolution + writability/persistence + H2 sibling + `.owner`; D6 flags/mutual-exclusion + `finish_lane` + interactive spawn; ledger invariant; `working_tree_status`/`build_devloop_prompt` helpers.
- `infra/devloop/devloop.sh` — D5 ledger mount + carrier + `.ledger-mount` + 0700 + cleanup comments/`--destroy` line.
- `.claude/skills/devloop/SKILL.md` — D6/f commit-intent write-side (Step 8) + schema.
- `scripts/guards/simple/validate-run-dir-path-sync.sh` — NEW static drift guard.
- `scripts/workflow/run-story.test.sh` — `mode=interactive` stub arm; section (O): `resolve_run_base` unit, startup-emission read-back pin, `RUN-DIR-UNWRITABLE`, flag discipline, `--finish` refusals + green + mismatches, `--interactive` refusals + TTY gate.
- `docs/devloop-outputs/2026-09-17-story-runner-recovery-lanes/main.md` — this doc.
- `docs/TODO.md` — NOT edited (verified obs's three retention constraints are present in ops's existing §Story Workflow Follow-ups entry; per-plan, no new entry).

---

## Gate-3 Findings Resolution

All ~25 Gate-3 findings applied as one batch; suite now **422 pass, 0 fail** (was 344), guard passes (+ a self-test proving it goes RED on drift), syntax clean, zero stale refs.

- **Correctness**: security F-1 (`git reset -q` re-derive from the intent; `FINISH-INDEX-DIRTY` lane removed — the SKILL stages before writing the intent, so a crash leaves a staged index the old pre-stage assertion would have refused); ops F1 (`T` added to the remainder filter); code-reviewer F1 (`LEDGER-NOT-PERSISTENT` gated `&& STORY_RUNNER_ALLOW_HOST != 1` so the host lane isn't falsely refused).
- **test F1 — via a real PTY, NOT a seam** (security reversed the seam approval to the strictly-better option, Lead ratified): interactive fall-through cases run under `script -qec` (fail-loud if absent), so the TTY gate is exercised PASSING; O10 kept as the no-PTY positive control (`INTERACTIVE-NO-TTY` still fires). Covers the argv contract (no `-p`/stream-json/`--dangerously-skip-permissions`, `DEVLOOP_HEADLESS` unset), the `INTERACTIVE_SPAWN` per-iteration reset (2-task story, O23), and the `claude_rc` guards (O10d). **Seam count stays two.** A latent bug this surfaced: `--interactive` now commits the `dt-story next` manifest reopen (mirroring `--restart`) so the fresh-start clean-tree check passes.
- **DRY**: F1 (C7 extraction completed — `working_tree_status` is now the one home, `--restart`/`--revalidate` converted via a message param); F2 (`run_full_gate` extracted, both callers); F3 (seam message names both hazards); F4 (complete_task header synced); F5 (guard renamed `validate-run-dir-path-sync.sh`, `fail()`/vars too).
- **Telemetry (obs)**: F1 (recoverable `COST-UNAVAILABLE` now appends an `unavailable`/`measured:false` entry → rollup counts it); F2 (rollup fails loud — `STORY-COST-UNAVAILABLE`, never silent). security F-2 (newline-safety comment, not `-z`).
- **Coverage**: O7a/O7b (missing + extra side); O11-O15 + O15b the fail-closed refusal lanes (NO-SLUG/UNSAFE-PATH/BAD-TRAILER/CONTENT-MISMATCH incl. security's `files[]⊊raw` headline/DIRTY-REMAINDER); O16 committed-message+files+zero-cost; O17 `--finish`+`--stop-after`; O18 gate-red re-escalate; O19 unwritable names path; O20 unavailable+unmeasured; O21 rollup fail-loud; O22 guard red-on-drift self-test.

## Code Review Results

**Gate 3: PASS** — all six spawned reviewers returned a verdict; **no ESCALATED**. Second review round brought the suite to **435 pass / 0 fail** (staged-entry O24, delete-path O25, CRLF O26, plus the F6 nits, on top of the first round's 422). Semantic Guard was **not spawned** (ADR-0037 D4 — shell/skill diff, no `checks.md` surface plausibly touched); its verdict row is omitted per the skill, not left blank.

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-DEFERRED | 4 (F-1, F-2, F-3, + planning-site comment) | 4 | 1 residual (Approved-Cross-Boundary: trailer check-leg — pre-existing) |
| Test | RESOLVED-FIXED | 11 (+2 paired) | 11 | 0 |
| Observability | RESOLVED-DEFERRED | 3 | 3 | 1 (owner-field detection gap) |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 |
| DRY | RESOLVED-FIXED | 6 | 6 | 0 |
| Operations | RESOLVED-DEFERRED | 6 | 6 | 1 (retention engine — Lead Gate-1 scope ruling) |

Each RESOLVED-DEFERRED is driven **solely by an accepted deferral, not an unfixed finding** — every raised finding was fixed in this PR. The three deferrals are recorded under §Accepted Deferrals below.

**Highest-value catches** (all found by executing/reproducing, not reading — the ADR-0037 review-value thesis):
- **operations NEW-1** (Gate 1): `--finish` would have run a full ~15-min gate then been rejected at commit, because only `layer-all.sh` emits the Gate-2 verdict the pre-commit hook requires — failing hardest in the exact dead-container scenario the lane exists to rescue. Fixed by routing `--finish` through `layer-all.sh`.
- **security F-1**: `--finish` refused its own headline scenario (the producer stages before writing the intent, so a terminal crash leaves a staged index the pre-stage precondition rejected). Fixed by mixed `git reset -q` + re-derive from `files[]`.
- **security F-3**: F-1's fix was *unexercised* — all 18 `--finish` cases entered clean, so every one passed against pre-fix code. Fixed with `FAKE_DEVLOOP_STAGE`/O24, **mutation-verified** to discriminate the fix.
- **code-reviewer F1 / ops F1**: false-positives of fail-closed checks (host-lane `LEDGER-NOT-PERSISTENT`; `T`-typechange in the remainder filter).
- The two-key decomposition (marker vs ledger), the cost-rollup zero-erasure (reproduced $2.10 vs true $6.90), and the H2 seam sibling all corrected at planning.

Per-reviewer verdict text is recorded in each reviewer's Gate-3 message; full findings resolution is in §Gate-3 Findings Resolution above.

---

## Accepted Deferrals

Each bullet is a cost shift left in the tree; the body lives in `docs/TODO.md`, the pointer here surfaces it at the devloop level.

- `docs/TODO.md` §Story Workflow Follow-ups (run-story v1 lessons) — **run-dir retention/prune engine** (deferred by the Lead Gate-1 scope ruling; ADR-0037 D5 is "a RUN_DIR default change + loud-fail, fallout none material", a prune subsystem is untraceable new mechanism with an unevidenced default). Interim: `0700` + do-not-promote + a manual `rm -rf` once the ledger is read out. Owner: operations (policy) + infrastructure (machinery).
- `docs/TODO.md` §Story Workflow Follow-ups — **cost-ledger owner-field detection gap** (observability): a ledger interleaved by two live owners is silently wrong rather than detectably wrong; accepted because `.owner` refuses the second owner at start. Conditional re-open trigger recorded (if `.owner` is weakened/removed or its stale-marker clearing path widened).
- `docs/TODO.md` §Story Workflow Follow-ups — **`Approved-Cross-Boundary:` trailer check-leg** (security): the trailer is a second encoding of a review event with no mechanical check that the co-sign happened (S-9 only checks the specialist exists). Pre-existing (D6/f doesn't introduce it); bounded by S-18 exact-set-equality + S-10 HEAD pin; conditional acceptance — re-opens if either binding weakens (incl. relaxing set-equality to a subset check).

(The retention TODO entry is already filed by operations; the owner-field and trailer entries are written into `docs/TODO.md` in this same commit.)
