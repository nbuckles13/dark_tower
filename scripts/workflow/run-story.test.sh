#!/usr/bin/env bash
# run-story.test.sh — hermetic self-test for scripts/workflow/run-story.sh
# (story-runner-hardening task #1, ADR-0035 §12; story R-1..R-5).
#
# The runner drives unattended `claude -p --dangerously-skip-permissions`
# sessions and reaches `git reset --hard` + `git clean -fdq` from a string
# classification decision, with zero tests before this file. A naive suite would
# be DESTRUCTIVE: the runner computes REPO_ROOT from BASH_SOURCE and otherwise
# operates entirely on the real repo. So every case runs the REAL runner (at its
# real path, so its BASH_SOURCE-derived "real root" is genuine) against a
# THROWAWAY fixture repo, via the DEVLOOP_TEST-gated STORY_REPO_ROOT / DT_STORY
# seams.
#
# COVERED
#   Seams + containment (R-1): sentinel inertness (absent/0/false/yes),
#     fail-loud override-without-sentinel, CI presence-rejection, both halves of
#     the containment predicate (own-toplevel AND differs-from-real), and the
#     DEVLOOP_TMP run-dir isolation constraint.
#   --stop-after (R-5): non-numeric, absent id, unreachable id, `01`, extra
#     argv, valid stop (close gate NOT run), terminal never-fired check.
#   The five R-2 defects: canary prose classification; canary stderr merged into
#     the JSON it parses; operator-vs-implementer gate lanes; transient git
#     errors; task-prompt splicing into /devloop "%s".
#   R-3: canary classification against 429 / null / absent / non-429-numeric /
#     non-JSON / empty output, plus the healthy path.
#   R-4: rc 1 escalates (manifest touched); any other non-zero routes to the
#     infra lane (manifest untouched).
#   ADR-0035 §6: the canary's --allowedTools "" + --dangerously-skip-permissions.
#   devloop-stop-hook.sh's git-rev-parse fail-open (ADR-0035 Follow-on Group 1
#     item 1) — same M2 mechanism as two of the runner sites, kept beside them so
#     the class is visible in one place rather than split across two files.
#
# NOT COVERED, deliberately
#   The REAL preflight-story.sh. It is reached CWD-relative, so under an active
#   seam it resolves inside the fixture and is stubbed. Its container-boundary
#   gate (ADR-0035 §5), its 420s substrate probe and its settings.json
#   registration are therefore NOT exercised here. That is the right call for a
#   hermetic suite, and it is contained: the seam only redirects when
#   DEVLOOP_TEST is exactly "1", GITHUB_ACTIONS is unset, and the root passes
#   both containment halves.
#   Whether the real CLI honours `--allowedTools ""` as "no tools" rather than
#   "unset => allow all". A stub can only pin argv; asserting the restriction
#   itself is substrate-probe work (ADR-0035 §8). Parse-safety WAS measured
#   out-of-band against claude 2.1.229 (rc 0, empty stderr, well-formed JSON).
#
# A COMMENT IS NOT A CASE. Every `# En:` heading below must be followed by
# assertions carrying that label. A prose heading with no `assert_*` behind it
# reads as coverage in review and provides none — the documentation-layer form
# of the h3 class (a case whose expected outcome is also the outcome of the
# mechanism never running). One survived Gate 2 here and hid a live defect
# (OPS-4). Check with:
#   grep -oE '^# [A-Z][0-9]+b?:' this-file | ... vs the emitted assert_* labels
#
# FIXTURE INVARIANT for docs/devloop-outputs/ (do not weaken to a count)
#   The runner selects a resume target with `find … | sort -rn | head -n 1`, and
#   that value decides resume-vs-fresh. Two hazards live on that pipeline with
#   OPPOSITE thresholds: SIGPIPE needs many entries; WRONG SELECTION needs
#   exactly two. With equal %T@, GNU `sort -rn` falls back to a whole-line
#   compare under -r and picks the LEXICALLY LARGEST NAME — deterministically,
#   not racily. So a two-dir case exercises whichever branch the alphabet chose
#   and passes stably forever; rerunning cannot detect it.
#   So: at every point the runner reads that pipeline, AT MOST ONE dir is newer
#   than $start_marker — or the case sets mtimes explicitly (touch -d) and
#   asserts WHICH slug was selected. The stub creates at most one
#   (FAKE_DEVLOOP_MKOUT), which satisfies the first form.
#
# LAYER-3 LANE FOR COULD-NOT-VERIFY (verified end-to-end before adopting).
#
# Without this, every exit-2 path below reaches run_and_emit, which appends
# STATUS=FAIL, which lang/_common.sh ranks 5 -> Layer 3 exits 1 -> the story
# runner's gate reads rc 1 as IMPLEMENTER and escalates a task against a clean
# diff. A concurrent reviewer edit would blame the task. That is R-4's exact
# class, produced by the suite built to prevent it.
#
# Printing our own STATUS line to STDOUT fixes it with existing vocabulary and
# no _common.sh change: tee_collect_statuses collects EVERY STATUS= line in the
# stream (not just run_and_emit's), and __status_rank puts PRECONDITION_FAILURE
# at 6 above FAIL at 5, so worst-wins carries ours. Measured:
#   collected: PRECONDITION_FAILURE FAIL  ->  worst=PRECONDITION_FAILURE, exit 2
#
# THREE couplings this acquires, named because none of them is asserted anywhere
# and all three would break it silently:
#   1. ANY line this file prints starting with `STATUS=` is now a VOTE in Layer
#      3's verdict. Safe today (run-story.sh emits none by design; assert_status
#      prints the needle, not the haystack) — but both are now load-bearing.
#      Do not print a bare STATUS= line from a fixture, stub or assertion.
#   2. BOTH status lines reach the log. A human reading a red Layer 3 sees ours
#      AND `FAIL REASON=run-story-selftest-failed`; OURS WINS, by rank 6 > 5.
#   3. Correctness rests on run_and_emit staying ADDITIVE and on that rank
#      ordering holding. If either changes, this lane silently reverts to FAIL
#      and starts escalating tasks again. This line is deliberate, not debris.
#
# NOT emitted on CONTAINMENT-FAILURE: a real breach means this suite escaped its
# fixture, which IS an implementer-class defect and belongs on the FAIL lane.
__emit_precondition_status() {
  printf 'STATUS=PRECONDITION_FAILURE REASON=%s\n' "$1"
}

# CONTAINMENT VERDICT VOCABULARY (closed set — add a row here before inventing a
# fifth spelling; same discipline as record_infra_incident's lane comment).
# These are what a human greps in a failed layer-3 log, and this is the one
# control whose output nobody can afford to misread. ONE TOKEN PER CAUSE, not
# per site: two positions that fail for the same reason share a token, and the
# message body says which position.
#   NOT-QUIESCENT           CAUSE: concurrency. The tree was ALREADY moving
#                           before any case ran -> refuse up front; re-run when
#                           quiescent.
#   CONTAINMENT-UNREADABLE  CAUSE: git health. A capture could not be read at
#                           all (either pre-flight or post-run) -> investigate
#                           .git / permissions / disk, then re-run.
#   CONTAINMENT-UNVERIFIED  CAUSE: concurrency, post-run. The tree changed and
#                           is STILL MOVING, so this run cannot attribute the
#                           change -> claims NEITHER breach nor safety; re-run
#                           on a quiescent tree.
#   CONTAINMENT-FAILURE     CAUSE: this suite. The tree changed and the
#                           post-state is STABLE, so a single actor explains it
#                           -> real breach; do NOT weaken the check.
#
# HERMETICITY
#   env -i; `claude`, `sleep` and `date` are PATH-injected stubs that RECORD
#   every invocation; `git` is REAL (containment must be tested against the tool
#   that actually resolves upward); `dt-story` is the REAL binary via the
#   DT_STORY seam. Fixture HOME, and GIT_CONFIG_GLOBAL/SYSTEM=/dev/null, so the
#   suite behaves identically on an operator host carrying commit.gpgsign or
#   core.hooksPath. No case sets STORY_RUNNER_ALLOW_HOST=1.
#
# Wired into scripts/layer3.sh (there is no *.test.sh auto-runner), so it runs
# every devloop + CI. Consumes scripts/lang/_test_helpers.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_REPO_ROOT="$(cd "${__here}/../.." && pwd -P)"
# shellcheck source=../lang/_test_helpers.sh
source "${__here}/../lang/_test_helpers.sh"

RUN_STORY="${__here}/run-story.sh"
STOP_HOOK="${__here}/devloop-stop-hook.sh"
REAL_DT_STORY="${REAL_REPO_ROOT}/target/release/dt-story"

# The runner exits 1 and 2 on most cases here; set -e must NOT abort the
# harness on a captured non-zero. report_results provides the final exit code.
set +e

# --- Preconditions (loud, never a skip) ---------------------------------------
# An unrun test is untested; a test that skips itself when its fixture is missing
# is worse, because it reports green. Layer 1 builds dt-story, but layer3.sh is
# routinely run standalone.
if [ ! -x "$REAL_DT_STORY" ]; then
  printf 'run-story.test.sh: PRECONDITION — dt-story not built but the runner suite needs it (cargo build --release -p dt-guard -p dt-story)\n' >&2
  exit 2
fi
if ! "$REAL_DT_STORY" list-tasks --help >/dev/null 2>&1; then
  printf 'run-story.test.sh: PRECONDITION — target/release/dt-story is STALE (no `list-tasks` verb; --stop-after validation depends on it). Rebuild: cargo build --release -p dt-guard -p dt-story\n' >&2
  exit 2
fi

# =============================================================================
# CONTAINMENT PROOF (R-1) — configuration verified is not outcome verified.
# =============================================================================
# capture_real_tree_state <real-root> -> "HEAD|porcelain-sha|diff-sha" on stdout.
#
# Three captures, not two. `git status --porcelain` prints ` M path` whether or
# not content changed, and this suite runs inside a devloop whose tree is dirty
# by construction, so "already modified" is the normal state — the `git diff
# HEAD` hash is what actually detects a reverted/altered tracked file. Untracked
# CONTENT is deliberately uncovered: the only two mutators in play are `git reset
# --hard` and `git clean -fdq`, and neither rewrites untracked content; `clean`
# REMOVES entries, which porcelain's presence/absence lines already register.
#
# Every rc is checked EXPLICITLY. This runs under `set +e`, where neither a bare
# assignment nor `local x=$(...)` propagates a failure — so an empty "before"
# compared against an empty "after" compares EQUAL and reports containment
# verified having verified nothing. That is R-2 defect 4 (a conditional that
# fails OPEN on a git error) reproduced inside the harness built to catch it. Do
# not simplify these checks away.
#
# The `| sha256sum` needs its own rc check rather than the pipeline's: a failing
# `git diff` yields sha256sum-of-empty-input, e3b0c442..., a stable
# plausible-looking hash that compares equal every time — worse than an empty
# string, and the same collapse this story's Deferred §Gates records for
# gate2_signature.
capture_real_tree_state() {
  local root="$1" head porcelain porcelain_sha diff_raw diff_sha
  head="$(git -C "$root" rev-parse --verify HEAD 2>/dev/null)" || return 1
  porcelain="$(git -C "$root" status --porcelain 2>/dev/null)" || return 1
  porcelain_sha="$(printf '%s' "$porcelain" | sha256sum)" || return 1
  diff_raw="$(git -C "$root" diff HEAD 2>/dev/null)" || return 1
  diff_sha="$(printf '%s' "$diff_raw" | sha256sum 2>/dev/null)" || return 1
  printf '%s|%s|%s' "$head" "$porcelain_sha" "$diff_sha"
}

# QUIESCENCE PRE-FLIGHT. Sample twice before any case runs and refuse up front
# if the tree is ALREADY moving. Refusing at second zero beats discovering it
# after a full run, and it is the same "assert the preconditions you depend on"
# rule this change applies to the guard binaries. Concurrent writers are normal
# during an Agent-Teams devloop (measured: reviewers filing docs/TODO.md entries
# and a co-implementer landing crate fixes while this suite ran), and the
# post-hoc proof CANNOT attribute a delta once they are active.
#
# No sleep between samples: each capture runs `git status` + `git diff HEAD`
# over the whole repo, so the two are separated by real work, and this suite
# must not add wall-clock to an always-run layer.
REAL_STATE_BEFORE="$(capture_real_tree_state "$REAL_REPO_ROOT")"
REAL_STATE_BEFORE_2="$(capture_real_tree_state "$REAL_REPO_ROOT")"
if [ -n "$REAL_STATE_BEFORE" ] && [ -z "$REAL_STATE_BEFORE_2" ]; then
  # Second sample could not be READ. Not quiescence, and not a concurrency
  # observation — absence of evidence. The `-n` guard below would otherwise let
  # this fall through as "quiescent", which is could-not-verify read as
  # verified. Takes the capture-failed token because the CAUSE is git health,
  # not concurrency: the operator investigates .git / permissions / disk, not
  # "wait for the tree to settle".
  printf 'run-story.test.sh: CONTAINMENT-UNREADABLE — could not read the tree state at %s for the quiescence pre-flight (the second sample failed; git error).\n' "$REAL_REPO_ROOT" >&2
  printf '  Refusing to run: quiescence is UNVERIFIED, not established. Investigate git health at %s.\n' "$REAL_REPO_ROOT" >&2
  __emit_precondition_status containment-not-quiescent
  exit 2
fi
if [ -n "$REAL_STATE_BEFORE" ] && [ -n "$REAL_STATE_BEFORE_2" ] \
   && [ "$REAL_STATE_BEFORE" != "$REAL_STATE_BEFORE_2" ]; then
  printf 'run-story.test.sh: PRECONDITION — NOT-QUIESCENT: %s changed between two back-to-back samples, so another process is writing to it right now.\n' "$REAL_REPO_ROOT" >&2
  printf '  sample 1: %s\n  sample 2: %s\n' "$REAL_STATE_BEFORE" "$REAL_STATE_BEFORE_2" >&2
  printf '  This is NOT a code failure and NOT a containment failure. It is normal mid-devloop\n' >&2
  printf '  (a reviewer or co-implementer editing the tree). The containment proof cannot attribute\n' >&2
  printf '  a post-run delta while that is happening, so this suite refuses to run rather than\n' >&2
  printf '  produce an unattributable verdict. Re-run once the tree is quiescent:\n' >&2
  printf '    git -C %s diff HEAD | sha256sum   # twice; identical means quiescent\n' "$REAL_REPO_ROOT" >&2
  __emit_precondition_status containment-could-not-verify
  exit 2
fi
if [ -z "$REAL_STATE_BEFORE" ]; then
  # Fatal at the BEFORE site: a containment proof that cannot establish its
  # baseline must not run any case, or every case runs unverified against the
  # operator's clone.
  printf 'run-story.test.sh: PRECONDITION — could not capture the real tree state at %s; refusing to run any case (the containment proof would be vacuous).\n' "$REAL_REPO_ROOT" >&2
  exit 2
fi

WORK="$(mktemp -d)"

# Suite-side containment pre-flight. Deliberately NOT sourced from run-story.sh:
# a suite that borrows its safety from the seam it exists to verify takes the
# operator's clone down on the exact run meant to catch it. Hard exit, never an
# accumulating assert_* (those return 0 unconditionally, so an assertion-only
# failure would let the harness proceed into `git reset --hard`/`git clean -fdq`).
# Concrete hazard this catches: a TMPDIR pointing inside /work makes mktemp -d
# land inside the real clone.
case "${WORK}/" in
  "${REAL_REPO_ROOT}"/*)
    printf 'run-story.test.sh: PRECONDITION — mktemp -d landed inside the real repo (%s). TMPDIR is misconfigured; refusing to run.\n' "$WORK" >&2
    exit 2
    ;;
esac

# ONE composed EXIT trap. Bash traps REPLACE rather than accumulate, so a second
# `trap ... EXIT` line would silently disable whichever of cleanup/proof came
# first (precedent: audit-suppressions-check.test.sh's combined handler).
# Verification runs BEFORE cleanup — the fixture's state is part of the
# diagnosis — and the handler sets its OWN exit code, because report_results has
# already chosen 0/1 by the time a trap fires, so appending to FAILURES here
# would never be printed and the suite would still exit green. Tree-was-mutated
# outranks all-green: those are precisely the conditions under which the summary
# is least trustworthy.
__on_exit() {
  local rc=$? after
  after="$(capture_real_tree_state "$REAL_REPO_ROOT")"
  if [ -z "$after" ]; then
    printf '\nrun-story.test.sh: CONTAINMENT-UNREADABLE — could not re-read the real tree state at %s after the run. Treat as a possible containment failure.\n' "$REAL_REPO_ROOT" >&2
    rm -rf "$WORK"
    exit 2
  fi
  if [ "$after" != "$REAL_STATE_BEFORE" ]; then
    # THREE STATES, not two: held / violated / COULD NOT BE VERIFIED.
    #
    # Discriminator: a suite that mutated the tree leaves a STABLE post-state —
    # its writes are done, it is exiting. A concurrent writer leaves a MOVING
    # one. So re-sample; if it moved again, no single actor explains both deltas
    # and this run cannot attribute the change.
    #
    # Reporting the third state as CONTAINMENT-FAILURE is not a cosmetic error:
    # under layer 3 that verdict reaches run_and_emit, collapses to STATUS=FAIL,
    # arrives at the runner's gate as rc 1, and escalates a task against a clean
    # diff — R-4's exact class, produced by the suite built to prevent it. (The
    # rc-1 collapse itself is @paired-infrastructure's filed follow-up; this
    # stops feeding it a false signal.)
    #
    # Two known limits, both stated rather than papered over:
    #   * a writer that stops before the re-sample looks stable and is reported
    #     as a containment failure — over-reporting, the safe direction;
    #   * a genuine breach WHILE a writer is active is reported as
    #     unattributable — under-reporting, which is why the quiescence
    #     pre-flight above exists to make that combination rare.
    local after_2
    after_2="$(capture_real_tree_state "$REAL_REPO_ROOT")"
    # FOUR states, not three. capture_real_tree_state returns EMPTY on any git
    # failure, so an empty after_2 means the second sample was never taken —
    # falling through to CONTAINMENT-FAILURE would assert "stable across two
    # samples" on a sample that does not exist, and name the suite as the actor.
    # That is could-not-verify reported as a definite verdict: the defect this
    # handler exists to fix, one level down inside its own implementation.
    #
    # It takes the capture-failed token, NOT the concurrency one. The
    # enumeration is one token per CAUSE, not per site: a read failure is a
    # git-health condition ("investigate .git / disk"), while UNVERIFIED means
    # the tree is still moving ("wait for quiescence"). Routing a read failure
    # to the concurrency token would assert a cause that was never observed —
    # the same defect again, in the fix for it.
    if [ -z "$after_2" ]; then
      printf '\nrun-story.test.sh: CONTAINMENT-UNREADABLE — %s changed during this run, and the RE-SAMPLE could not be read (git failed on the second capture).\n' "$REAL_REPO_ROOT" >&2
      printf '  before: %s\n  after:  %s\n  after+1: <unreadable>\n' "$REAL_STATE_BEFORE" "$after" >&2
      printf '  With no second sample there is no evidence either way, so this claims NEITHER a breach\n' >&2
      printf '  NOR that containment held, and it does NOT claim a concurrent writer. Investigate git\n' >&2
      printf '  health at %s, then re-run. The fixture is preserved: %s\n' "$REAL_REPO_ROOT" "$WORK" >&2
      __emit_precondition_status containment-unreadable
      exit 2
    fi
    if [ "$after_2" != "$after" ]; then
      printf '\nrun-story.test.sh: CONTAINMENT-UNVERIFIED — %s changed during this run AND is STILL CHANGING.\n' "$REAL_REPO_ROOT" >&2
      printf '  before:   %s\n  after:    %s\n  after+1:  %s\n' "$REAL_STATE_BEFORE" "$after" "$after_2" >&2
      printf '  A still-moving tree means another process is writing to it, so this run CANNOT attribute\n' >&2
      printf '  the change and does NOT claim the suite escaped its fixture. It also does NOT claim\n' >&2
      printf '  containment held — that is unverified, which is why this is a loud failure and not a pass.\n' >&2
      printf '  Most likely a reviewer or co-implementer editing the tree mid-devloop. Confirm with:\n' >&2
      printf '    git -C %s status --porcelain && git -C %s diff --stat\n' "$REAL_REPO_ROOT" "$REAL_REPO_ROOT" >&2
      printf '  then re-run on a quiescent tree. If it reproduces on a quiescent tree, treat it as a\n' >&2
      printf '  REAL containment failure. The fixture is preserved: %s\n' "$WORK" >&2
      __emit_precondition_status containment-could-not-verify
      exit 2
    fi
    printf '\nrun-story.test.sh: CONTAINMENT-FAILURE — the REAL repository at %s changed during this run.\n' "$REAL_REPO_ROOT" >&2
    printf '  before: %s\n  after:  %s\n' "$REAL_STATE_BEFORE" "$after" >&2
    printf '  (fields: HEAD|sha256(status --porcelain)|sha256(diff HEAD))\n' >&2
    printf '  The fixture is preserved for diagnosis: %s\n' "$WORK" >&2
    # Reached only when the post-state is STABLE, i.e. a single actor explains
    # the delta and this suite is the one that was running. The comparison is
    # deliberately NOT narrowed to make this quiet — the three captures stay,
    # because narrowing is what would make the proof worthless.
    printf '  The post-run state is STABLE across two samples, so no concurrent writer explains this:\n' >&2
    printf '  the suite is the actor. Treat as a real containment failure. Do NOT weaken this check.\n' >&2
    printf '  Confirm with: git -C %s status --porcelain && git -C %s diff --stat\n' "$REAL_REPO_ROOT" "$REAL_REPO_ROOT" >&2
    exit 2
  fi
  rm -rf "$WORK"
  exit "$rc"
}
trap __on_exit EXIT

# =============================================================================
# Stubs — every one RECORDS its invocation.
# =============================================================================
# A case that asserts only an exit code cannot distinguish "the path I meant to
# test ran" from "another path produced the same status", and a rejection case
# passes trivially when the harness is broken. Markers are the distinguishing
# evidence, and "no real claude ran" is PROVEN by a recorded stub invocation
# rather than inferred from env -i.
STUB_BIN="${WORK}/bin"
FIXHOME="${WORK}/home"
mkdir -p "$STUB_BIN" "$FIXHOME"

REAL_DATE="$(command -v date)"
REAL_SLEEP="$(command -v sleep)"
[ -n "$REAL_DATE" ] || { printf 'run-story.test.sh: PRECONDITION — no real `date` on PATH\n' >&2; exit 2; }

# --- claude stub --------------------------------------------------------------
# Three roles, discriminated exactly as run-story.sh invokes them:
#   --version                     -> substrate version probe
#   --output-format json + haiku  -> canary probe
#   --output-format stream-json   -> the devloop session
cat > "${STUB_BIN}/claude" <<'STUB'
#!/usr/bin/env bash
M="${DEVLOOP_TEST_MARKERS:?claude stub needs DEVLOOP_TEST_MARKERS}"
# Record the FULL argv, %q-quoted so an empty-string argument is visible as ''.
{ printf '%q ' "$@"; printf '\n'; } >> "${M}/claude.argv"

mode=version
for a in "$@"; do
  case "$a" in
    stream-json) mode=devloop ;;
    json)        [ "$mode" = devloop ] || mode=canary ;;
  esac
done
case " $* " in *" --version "*) mode=version ;; esac

case "$mode" in
  version)
    : >> "${M}/ran.claude.version"
    printf '%s\n' "${FAKE_CLI_VERSION:-2.1.229 (Claude Code)}"
    exit 0 ;;
  canary)
    n=$(( $(cat "${M}/canary.count" 2>/dev/null || echo 0) + 1 ))
    printf '%s' "$n" > "${M}/canary.count"
    : >> "${M}/ran.claude.canary"
    [ -n "${FAKE_CANARY_STDERR:-}" ] && printf '%s\n' "$FAKE_CANARY_STDERR" >&2
    [ -n "${FAKE_CANARY_JSON:-}" ] && printf '%s\n' "$FAKE_CANARY_JSON"
    exit "${FAKE_CANARY_RC:-0}" ;;
  devloop)
    n=$(( $(cat "${M}/devloop.count" 2>/dev/null || echo 0) + 1 ))
    printf '%s' "$n" > "${M}/devloop.count"
    : >> "${M}/ran.claude.devloop"
    # Per-attempt rc list, e.g. FAKE_DEVLOOP_RCS="1 0" => fail then succeed.
    rc=0
    if [ -n "${FAKE_DEVLOOP_RCS:-}" ]; then
      i=0
      for r in ${FAKE_DEVLOOP_RCS}; do
        i=$((i+1)); [ "$i" -eq "$n" ] && rc="$r" && break
        rc="$r"
      done
    fi
    [ "${FAKE_DEVLOOP_MKOUT:-0}" = "1" ] && {
      MKOUT_NAME="${FAKE_DEVLOOP_MKOUT_NAME:-2026-08-13-fixture-devloop}"
      mkdir -p "docs/devloop-outputs/${MKOUT_NAME}"
      printf '# fixture devloop\n' > "docs/devloop-outputs/${MKOUT_NAME}/main.md"
      # Force the mtime strictly forward. The runner selects with
      # `find -newer "$start_marker"`, and the stub runs milliseconds after the
      # runner touches that marker — so on a coarse-granularity filesystem the
      # two timestamps can be EQUAL and `-newer` (strictly greater) excludes the
      # dir. Measured before this line: a 1-in-8 flake in K1. Timestamps that a
      # case's outcome depends on are SET, never inherited from execution order.
      touch -d 'now + 1 minute' "docs/devloop-outputs/${MKOUT_NAME}"; }
    [ -n "${FAKE_DEVLOOP_ESCALATION:-}" ] && printf '%s\n' "$FAKE_DEVLOOP_ESCALATION" > .devloop-escalation.json
    if [ "${FAKE_DEVLOOP_COMMIT:-1}" = "1" ]; then
      printf 'work %s\n' "$n" >> work.txt
      git add work.txt >/dev/null 2>&1
      git commit --quiet -m "fixture task work ${n}" >/dev/null 2>&1
    fi
    # Simulate a TRANSIENT git failure at the point the runner reads HEAD back.
    [ "${FAKE_BREAK_GIT_HEAD:-0}" = "1" ] && printf 'ref: refs/heads/nonexistent-branch\n' > .git/HEAD
    exit "$rc" ;;
esac
STUB
chmod +x "${STUB_BIN}/claude"

# --- sleep stub ---------------------------------------------------------------
# Records the requested duration and returns immediately: the session-limit lane
# must be exercised without a 1-hour wall clock.
cat > "${STUB_BIN}/sleep" <<'STUB'
#!/usr/bin/env bash
M="${DEVLOOP_TEST_MARKERS:?sleep stub needs DEVLOOP_TEST_MARKERS}"
printf '%s\n' "${1:-}" >> "${M}/sleep.args"
exit 0
STUB
chmod +x "${STUB_BIN}/sleep"

# --- jq stub ------------------------------------------------------------------
# Fails ONLY on record_infra_incident's write (identified by `--argjson task`,
# which no other jq call in the runner uses), so the INCIDENT-RECORD-UNWRITABLE
# branch can be driven without disturbing manifest parsing, cost telemetry or
# canary classification. Inert unless FAKE_JQ_FAIL_INCIDENT=1.
REAL_JQ="$(command -v jq)"
[ -n "$REAL_JQ" ] || { printf 'run-story.test.sh: PRECONDITION — no real `jq` on PATH\n' >&2; exit 2; }
sed -e "s|__REAL_JQ__|${REAL_JQ}|" > "${STUB_BIN}/jq" <<'STUB'
#!/usr/bin/env bash
M="${DEVLOOP_TEST_MARKERS:-}"
if [ "${FAKE_JQ_FAIL_INCIDENT:-0}" = "1" ]; then
  case " $* " in
    *" --argjson task "*)
      [ -n "$M" ] && : >> "${M}/ran.jq.incident-fail"
      echo "jq: simulated failure" >&2
      exit 5 ;;
  esac
fi
exec __REAL_JQ__ "$@"
STUB
chmod +x "${STUB_BIN}/jq"

# --- date stub ----------------------------------------------------------------
# DELEGATES to the real date for every format, pinning only "now". The
# session-limit sleep math (the +300 margin, the reset<=now => +86400 rollover,
# the 93600 cap) is then pinned against the REAL arithmetic rather than a
# reimplementation. A bare `-d <time-of-day>` is anchored to the pinned date so
# `date -u -d 3:00pm +%s` is deterministic across days.
sed -e "s|__REAL_DATE__|${REAL_DATE}|" > "${STUB_BIN}/date" <<'STUB'
#!/usr/bin/env bash
M="${DEVLOOP_TEST_MARKERS:-}"
[ -n "$M" ] && printf '%s\n' "$*" >> "${M}/date.args"
NOW="${FAKE_NOW:-2026-08-13T12:00:00Z}"
dspec=""; fmt=""
while [ $# -gt 0 ]; do
  case "$1" in
    -u|--utc|--universal) ;;
    -d) dspec="${2:-}"; shift ;;
    --date=*) dspec="${1#--date=}" ;;
    -d?*) dspec="${1#-d}" ;;
    +*) fmt="$1" ;;
  esac
  shift
done
[ -n "$fmt" ] || fmt="+%a %b %e %H:%M:%S %Z %Y"
if [ -n "$dspec" ]; then
  case "$dspec" in
    @*|*[0-9]-[0-9]*) exec __REAL_DATE__ -u -d "$dspec" "$fmt" ;;
    *)                exec __REAL_DATE__ -u -d "${NOW%%T*} ${dspec}" "$fmt" ;;
  esac
fi
exec __REAL_DATE__ -u -d "$NOW" "$fmt"
STUB
chmod +x "${STUB_BIN}/date"
: "${REAL_SLEEP:=}"   # referenced for symmetry; the stub supersedes it on PATH

# =============================================================================
# Fixture template — built ONCE, cp -a'd per case (N git inits is the whole
# runtime budget; layer 3's per-layer warn is 20s and it already runs
# run-guards.sh plus seven self-tests).
# =============================================================================
TEMPLATE="${WORK}/template"
mkdir -p "$TEMPLATE"/{scripts/workflow,scripts/lang,docs/user-stories,docs/devloop-outputs}

# Stub preflight: the real one is not under test here (see NOT COVERED).
cat > "$TEMPLATE/scripts/workflow/preflight-story.sh" <<'STUB'
#!/usr/bin/env bash
: >> "${DEVLOOP_TEST_MARKERS}/ran.preflight"
exit "${FAKE_PREFLIGHT_RC:-0}"
STUB

# Stub layers: per-layer rc injection, each dropping a ran-marker.
for n in 1 2 3 4 5 6 7; do
  cat > "$TEMPLATE/scripts/layer${n}.sh" <<STUB
#!/usr/bin/env bash
: >> "\${DEVLOOP_TEST_MARKERS}/ran.layer${n}"
printf 'fixture layer${n} output\n'
exit "\${FAKE_LAYER${n}_RC:-0}"
STUB
done
cat > "$TEMPLATE/scripts/layer-all.sh" <<'STUB'
#!/usr/bin/env bash
: >> "${DEVLOOP_TEST_MARKERS}/ran.layer-all"
exit "${FAKE_LAYER_ALL_RC:-0}"
STUB
chmod +x "$TEMPLATE"/scripts/workflow/preflight-story.sh "$TEMPLATE"/scripts/layer*.sh

# mk_story <path> <tasks-yaml> — write a fixture story carrying a v1 manifest.
mk_story() {
  local path="$1" tasks="$2"
  {
    printf '# Fixture story\n\nHermetic fixture for run-story.test.sh.\n\n'
    printf '```yaml\n'
    printf '# task-metadata (dt-story manifest v1)\n'
    printf 'story: fixture\n'
    printf 'branch: fixture-branch\n'
    printf 'tasks:\n'
    printf '%s\n' "$tasks"
    printf '```\n'
  } > "$path"
}

DEFAULT_TASKS='- id: 1
  status: pending
  specialist: test
  env_tests: false
  prompt: fixture task one prompt'

mk_story "$TEMPLATE/docs/user-stories/2026-08-13-fixture.md" "$DEFAULT_TASKS"

git -c init.defaultBranch=main init -q "$TEMPLATE"
git -C "$TEMPLATE" config user.email "fixture@example.invalid"
git -C "$TEMPLATE" config user.name "Fixture Runner"
git -C "$TEMPLATE" config commit.gpgsign false
git -C "$TEMPLATE" config core.hooksPath /dev/null
git -C "$TEMPLATE" add -A >/dev/null
git -C "$TEMPLATE" commit --quiet -m "fixture baseline" >/dev/null

# =============================================================================
# Runner
# =============================================================================
# run_story [KEY=VAL ...] -- [runner-args ...]
# Sets FIX, DT, MARK, OUT, ERR, RC, and OUTPUT (stdout+stderr combined).
run_story() {
  local env_kv=() args=() seen_sep=0 a
  for a in "$@"; do
    if [ "$a" = "--" ] && [ "$seen_sep" -eq 0 ]; then seen_sep=1; continue; fi
    if [ "$seen_sep" -eq 1 ]; then args+=("$a"); else env_kv+=("$a"); fi
  done

  FIX="$(mktemp -d "${WORK}/fix.XXXXXX")"
  cp -a "${TEMPLATE}/." "${FIX}/"
  DT="$(mktemp -d "${WORK}/dt.XXXXXX")"
  MARK="$(mktemp -d "${WORK}/mark.XXXXXX")"
  # Optional fixture corruption, applied AFTER the copy and BEFORE the runner —
  # the only window for defects the runner hits before it invokes any stub.
  if [ -n "${FIXTURE_PRERUN:-}" ]; then "$FIXTURE_PRERUN" "$FIX"; fi
  # Optional pre-seeded resume pointer, for the branches that read $slug_file
  # on entry rather than writing it.
  if [ -n "${SEED_SLUG:-}" ]; then
    mkdir -p "$(RUN_DIR_OF "$DT")"
    printf '%s\n' "$SEED_SLUG" > "$(RUN_DIR_OF "$DT")/task-1.slug"
  fi
  OUT="${MARK}/stdout"; ERR="${MARK}/stderr"

  # STORY_REPO_ROOT / DT_STORY / DEVLOOP_TMP are defaults here; a case can
  # override any of them by passing the same KEY=VAL later in env_kv (later
  # assignments win in `env`), which is how the containment cases work.
  env -i \
    PATH="${STUB_BIN}:${PATH}" \
    HOME="$FIXHOME" \
    GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null \
    DEVLOOP_TEST_MARKERS="$MARK" \
    DEVLOOP_TEST=1 \
    STORY_REPO_ROOT="$FIX" \
    DT_STORY="$REAL_DT_STORY" \
    DEVLOOP_TMP="$DT" \
    STORY_SESSION_LIMIT_RETRIES=0 \
    "${env_kv[@]}" \
    bash "$RUN_STORY" "${args[@]}" >"$OUT" 2>"$ERR"
  RC=$?
  OUTPUT="$(cat "$OUT" "$ERR" 2>/dev/null)"
}

# Fixture mutators for the git-error branches. Real git, real corruption — the
# containment argument requires git stay real, so faults are injected into the
# fixture's own .git rather than by stubbing git.
#
# corrupt_git_index: garbage in .git/index makes `git diff --quiet` exit 128
# while `rev-parse HEAD` still succeeds — landing precisely on tree_dirty's rc-2
# branch, which is otherwise unreachable without stubbing git.
corrupt_git_index() { printf 'garbage-not-an-index' > "$1/.git/index"; }
# corrupt_git_head: a dangling symref makes `rev-parse --verify HEAD` fail while
# `rev-parse --show-toplevel` still resolves, so the seam's containment check
# passes and the failure lands on the runner's own pre-devloop HEAD read.
corrupt_git_head() { printf 'ref: refs/heads/nonexistent-branch\n' > "$1/.git/HEAD"; }

# manifest_status <fixture> <id> — the task's status per the REAL dt-story.
manifest_status() {
  "$REAL_DT_STORY" list-tasks "$1/docs/user-stories/2026-08-13-fixture.md" 2>/dev/null \
    | jq -r --argjson id "$2" '.[] | select(.id == $id) | .status' 2>/dev/null
}

RUN_DIR_OF() { printf '%s/story-runner/2026-08-13-fixture' "$1"; }

# =============================================================================
# (A) SEAM INERTNESS + FAIL-LOUD  (R-1)
# =============================================================================
# Each rejection case asserts its DISTINCT token, not just a non-zero exit —
# any broken fixture also exits non-zero — plus assert_no_marker proving the
# runner did no work.
for sentinel in "" "0" "false" "yes"; do
  label="a-sentinel-${sentinel:-absent}"
  run_story DEVLOOP_TEST="$sentinel" -- fixture
  assert_exit      "${label}-exit2" 2 "$RC"
  assert_status    "${label}-token" "SEAM-OVERRIDE-WITHOUT-TEST-SENTINEL" "$OUTPUT"
  assert_no_marker "${label}-no-preflight" "$MARK" 'ran.*'
done

# =============================================================================
# (B) CONTAINMENT — both halves  (R-1)
# =============================================================================
# B1: a root INSIDE the real clone. Git resolves upward, so --show-toplevel
# returns the real root: no containment at all. No filesystem writes needed —
# scripts/ already exists inside the real repo.
run_story STORY_REPO_ROOT="${REAL_REPO_ROOT}/scripts" -- fixture
assert_exit      "b1-inside-real-clone-exit2" 2 "$RC"
assert_status    "b1-inside-real-clone-token" "SEAM-ROOT-NOT-OWN-TOPLEVEL" "$OUTPUT"
assert_no_marker "b1-inside-real-clone-no-work" "$MARK" 'ran.*'

# B2: THE load-bearing half — a root set to the real repository is a no-op
# redirect that passes the own-toplevel test and would otherwise be accepted.
run_story STORY_REPO_ROOT="$REAL_REPO_ROOT" -- fixture
assert_exit      "b2-is-real-repo-exit2" 2 "$RC"
assert_status    "b2-is-real-repo-token" "SEAM-ROOT-IS-REAL-REPO" "$OUTPUT"
assert_no_marker "b2-is-real-repo-no-work" "$MARK" 'ran.*'

# B3: a root that is not a git repository at all.
nongit="$(mktemp -d "${WORK}/nongit.XXXXXX")"
run_story STORY_REPO_ROOT="$nongit" -- fixture
assert_exit   "b3-not-a-repo-exit2" 2 "$RC"
assert_status "b3-not-a-repo-token" "SEAM-ROOT-NOT-OWN-TOPLEVEL" "$OUTPUT"

# B4/B5: DEVLOOP_TMP must be redirected — an un-redirected run dir deletes a
# live run's .run-in-flight marker via the runner's own EXIT trap and
# interleaves fixture artifacts into that run's evidence directory.
run_story DEVLOOP_TMP="" -- fixture
assert_exit   "b4-run-dir-unset-exit2" 2 "$RC"
assert_status "b4-run-dir-unset-token" "SEAM-RUN-DIR-NOT-REDIRECTED" "$OUTPUT"

run_story DEVLOOP_TMP=/tmp/devloop -- fixture
assert_exit   "b5-run-dir-default-exit2" 2 "$RC"
assert_status "b5-run-dir-default-token" "SEAM-RUN-DIR-NOT-REDIRECTED" "$OUTPUT"

# B6: another runner already owns the run dir.
busy="$(mktemp -d "${WORK}/busy.XXXXXX")"; mkdir -p "${busy}/story-runner"
printf 'story=other pid=1 started=now\n' > "${busy}/story-runner/.run-in-flight"
run_story DEVLOOP_TMP="$busy" -- fixture
assert_exit   "b6-run-dir-in-use-exit2" 2 "$RC"
assert_status "b6-run-dir-in-use-token" "SEAM-RUN-DIR-IN-USE" "$OUTPUT"

# =============================================================================
# (C) CI PRESENCE-REJECTION  (bypass closure)
# =============================================================================
# Byte-for-byte the honoured configuration with GITHUB_ACTIONS as the SOLE
# delta, asserting the distinct token — a case that only checked "non-zero"
# would pass against any broken fixture.
run_story GITHUB_ACTIONS=1 -- fixture
assert_exit      "c1-ci-repo-root-exit2" 2 "$RC"
assert_status    "c1-ci-repo-root-token" "SEAM-SET-IN-CI" "$OUTPUT"
assert_no_marker "c1-ci-repo-root-no-work" "$MARK" 'ran.*'

# The seam is rejected on DT_STORY alone too (STORY_REPO_ROOT absent).
run_story GITHUB_ACTIONS=1 STORY_REPO_ROOT="" -- fixture
assert_exit   "c2-ci-dt-story-only-exit2" 2 "$RC"
assert_status "c2-ci-dt-story-only-token" "SEAM-SET-IN-CI" "$OUTPUT"

# =============================================================================
# (D) BASELINE — the harness CAN go green.
# =============================================================================
# A fixture where every case expects non-zero proves nothing.
run_story -- fixture
assert_exit   "d1-baseline-exit0" 0 "$RC"
assert_status "d1-baseline-complete" "ALL TASKS COMPLETE" "$OUTPUT"
assert_status "d1-baseline-seam-announced" "TEST SEAMS ACTIVE" "$OUTPUT"
assert_marker "d1-baseline-preflight-ran" "$MARK" 'ran.preflight'
assert_marker "d1-baseline-devloop-ran"   "$MARK" 'ran.claude.devloop'
assert_marker "d1-baseline-layers-ran"    "$MARK" 'ran.layer1'
assert_marker "d1-baseline-close-gate-ran" "$MARK" 'ran.layer-all'
if [ "$(manifest_status "$FIX" 1)" = "completed" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[d1-baseline-manifest-completed] task 1 status is '$(manifest_status "$FIX" 1)', expected completed"); fi

# =============================================================================
# (E) --stop-after  (R-5)
# =============================================================================
run_story -- fixture --stop-after=abc
assert_exit   "e1-non-numeric-exit2" 2 "$RC"

# E2: an id that names no task. Today validated only as a number, so the flag
# silently does nothing and the runner continues to the end.
run_story -- fixture --stop-after=99
assert_exit      "e2-absent-id-exit2" 2 "$RC"
assert_status    "e2-absent-id-token" "STOP-AFTER-UNKNOWN-TASK" "$OUTPUT"
assert_no_marker "e2-absent-id-no-devloop" "$MARK" 'ran.claude.devloop'

# E3: an id present in the manifest but never reachable (already completed).
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" '- id: 1
  status: pending
  specialist: test
  env_tests: false
  prompt: fixture task one prompt
- id: 2
  status: completed'
git -C "$TEMPLATE" commit --quiet -am "two-task fixture" >/dev/null
run_story -- fixture --stop-after=2
assert_exit      "e3-unreachable-id-exit2" 2 "$RC"
assert_status    "e3-unreachable-id-token" "STOP-AFTER-UNREACHABLE-TASK" "$OUTPUT"
assert_no_marker "e3-unreachable-id-no-devloop" "$MARK" 'ran.claude.devloop'

# E4: a task whose dep is PENDING must be ACCEPTED, not refused. Reds against
# the pre-OPS-4 code, which treated any non-`completed` dep as a blocker and
# refused with "this run will never reach it" — false, since a pending dep is
# work this run does first.
#
# (What stood here was a COMMENT WITH NO ASSERTIONS: it described a case, was
# followed only by a fixture restore, and no `e4-` label existed in the file.
# It also encoded the wrong model — "the dep never completes because it is
# escalated" — when engine.rs selects `Pending | Escalated` and REOPENS an
# escalated task on selection. A prose label with nothing behind it is the
# documentation-layer form of the h3 class: it produces the APPEARANCE of
# coverage, and it is why OPS-4 survived a suite built to catch untested lanes.)
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" '- id: 1
  status: pending
  specialist: test
  env_tests: false
  prompt: fixture task one prompt
- id: 2
  status: pending
  specialist: test
  env_tests: false
  deps: [1]
  prompt: fixture task two prompt'
git -C "$TEMPLATE" commit --quiet -am "pending-dep fixture" >/dev/null
run_story -- fixture --stop-after=2
assert_exit      "e4-pending-dep-accepted" 0 "$RC"
assert_status    "e4-pending-dep-stopped"  "STOPPED after task 2" "$OUTPUT"
assert_status    "e4-pending-dep-noted"    "depends on task(s) 1" "$OUTPUT"
assert_marker    "e4-pending-dep-devloops-ran" "$MARK" 'ran.claude.devloop'
assert_no_marker "e4-pending-dep-close-gate-skipped" "$MARK" 'ran.layer-all'

# E4b WITHDRAWN — it exposed a SEPARATE, PRE-EXISTING defect and cannot pass
# until that is fixed. Recorded rather than deleted, and deliberately NOT
# rewritten to assert the broken behaviour: pinning a defect as correct is the
# phantom-E4 mistake in another form.
#
# THE DEFECT (found by this case, reported to @team-lead for a ruling): a story
# containing an ESCALATED task cannot be resumed at all. `dt-story next` reopens
# an escalated task and REWRITES the story file (documented at main.rs:20-22,
# "file rewritten"). The runner calls `next` BEFORE its fresh-start clean-tree
# check, so that write makes the tree dirty and the check exits 2 with:
#   "dirty tree and no resumable devloop for task 1"
# Measured end-to-end here. It makes escalation-is-retryable — the property
# engine.rs:48/:87 implements and the ADR relies on for the escalate/rerun
# recovery loop — unreachable through the runner.
#
# Not fixed in this changeset: the candidate fix (commit the reopen, as the
# audit-remediation path already does for add-task) is a runner behaviour change
# in a lane this task was not scoped to, arriving after the Lead's freeze and
# without review. It is filed instead.
#
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" "$DEFAULT_TASKS"
git -C "$TEMPLATE" commit --quiet -am "single-task fixture" >/dev/null

# E5: valid stop — the story-close gate must NOT run.
run_story -- fixture --stop-after=1
assert_exit      "e5-valid-stop-exit0" 0 "$RC"
assert_status    "e5-valid-stop-message" "STOPPED after task 1" "$OUTPUT"
assert_marker    "e5-valid-stop-devloop-ran" "$MARK" 'ran.claude.devloop'
assert_no_marker "e5-valid-stop-close-gate-skipped" "$MARK" 'ran.layer-all'

# E6: extra argv is refused, not silently dropped (same failure class as R-5).
run_story -- fixture --stop-after=1 --unknown-flag
assert_exit   "e6-extra-argv-exit2" 2 "$RC"
assert_status "e6-extra-argv-token" "UNKNOWN-ARGUMENT" "$OUTPUT"

# E7: `01` must not pass a numeric check and then fail a string comparison.
run_story -- fixture --stop-after=01
assert_exit   "e7-leading-zero-exit0" 0 "$RC"
assert_status "e7-leading-zero-stopped" "STOPPED after task 1" "$OUTPUT"

# =============================================================================
# (F) GATE LANES — R-4 / R-2 defect 3
# =============================================================================
# F1: rc 1 => IMPLEMENTER. Task escalated, escalation record written.
run_story FAKE_LAYER3_RC=1 -- fixture
assert_exit   "f1-rc1-exit1" 1 "$RC"
assert_status "f1-rc1-escalated" "ESCALATED" "$OUTPUT"
assert_marker "f1-rc1-escalation-record" "$(RUN_DIR_OF "$DT")" 'task-1.runner-escalation.*.json'
if [ "$(manifest_status "$FIX" 1)" = "escalated" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[f1-rc1-manifest-escalated] task 1 status is '$(manifest_status "$FIX" 1)', expected escalated"); fi

# F2: rc 2 => OPERATOR. Manifest UNTOUCHED, infra incident, no escalation record.
# This is the collapse R-4 names: layer7.test.sh:8-16 labels exit 2
# PRECONDITION_FAILURE "operator" and exit 1 FAIL "implementer", and
# run-story.sh collapsed both into one escalate() call.
run_story FAKE_LAYER3_RC=2 -- fixture
assert_exit      "f2-rc2-exit2" 2 "$RC"
assert_status    "f2-rc2-token" "PIPELINE-PRECONDITION" "$OUTPUT"
assert_status    "f2-rc2-names-layer" "layer=3" "$OUTPUT"
assert_marker    "f2-rc2-incident-record" "$(RUN_DIR_OF "$DT")" 'task-1.infra-incident.*.json'
# Assert the record's CONTENTS, not merely its existence. Presence-only would
# pass against a ZERO-BYTE record — which is exactly the bug the `|| true`
# removal fixed, so the suite could not otherwise tell the bug from the fix.
# `lane` and `log` are the two fields the operator lane's whole value rests on,
# and the console line that carries the same information is captured nowhere.
f2_rec="$(ls "$(RUN_DIR_OF "$DT")"/task-1.infra-incident.*.json 2>/dev/null | head -n 1)"
assert_status "f2-rc2-record-lane" "pipeline-precondition" "$(jq -r '.lane // "MISSING"' "$f2_rec" 2>/dev/null || echo PARSE-FAILED)"
f2_log="$(jq -r '.log // "MISSING"' "$f2_rec" 2>/dev/null || echo PARSE-FAILED)"
assert_status "f2-rc2-record-log-is-gatelog" "task-1.gate.log" "$f2_log"
if [ -f "$f2_log" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[f2-rc2-record-log-exists] record's log field names '${f2_log}', which does not exist"); fi
assert_no_marker "f2-rc2-no-escalation-record" "$(RUN_DIR_OF "$DT")" 'task-1.runner-escalation.*.json'
if [ "$(manifest_status "$FIX" 1)" = "pending" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[f2-rc2-manifest-untouched] task 1 status is '$(manifest_status "$FIX" 1)', expected pending"); fi

# F3: an unknown non-zero rc (127 = layer script missing) must ALSO default to
# the operator lane — an unknown rc must never blame the implementer.
run_story FAKE_LAYER3_RC=127 -- fixture
assert_exit   "f3-rc127-exit2" 2 "$RC"
assert_status "f3-rc127-token" "PIPELINE-PRECONDITION" "$OUTPUT"

# F4: the split applies to layer 7 as well as 1-6 (env_tests task).
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" '- id: 1
  status: pending
  specialist: test
  env_tests: true
  prompt: fixture task needing env tests'
git -C "$TEMPLATE" commit --quiet -am "env-tests fixture" >/dev/null
run_story FAKE_LAYER7_RC=2 -- fixture
assert_exit   "f4-layer7-rc2-exit2" 2 "$RC"
assert_status "f4-layer7-rc2-token" "PIPELINE-PRECONDITION" "$OUTPUT"
assert_status "f4-layer7-names-layer" "layer=7" "$OUTPUT"
assert_marker "f4-layer7-ran" "$MARK" 'ran.layer7'
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" "$DEFAULT_TASKS"
git -C "$TEMPLATE" commit --quiet -am "restore single-task fixture" >/dev/null

# =============================================================================
# (G) CANARY CLASSIFICATION — R-2 defects 1 & 2, R-3
# =============================================================================
# Every case drives the devloop to a non-zero exit so the canary is consulted.
CANARY_HEALTHY='{"is_error":false,"result":"ok","api_error_status":null}'

# G1: DEFECT 1 — a quota message in the shape production actually emits.
# canary_classify greps case-sensitively for the literal 'session limit', while
# run-story.sh:410-411 (same file, same field) already encodes the real shape as
# 'resets [0-9]+:[0-9]+[ap]m'. This string matches the latter and NOT the
# former, so it classifies as `infra` and the story STOPS instead of sleeping.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":null,"result":"5-hour limit reached ∙ resets 3:00pm"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_status "g1-prose-typo-not-infra" "SESSION-LIMIT" "$OUTPUT"
assert_absent "g1-prose-typo-no-infra-lane" "STORY_RUN: INFRA" "$OUTPUT"
assert_marker "g1-prose-typo-slept" "$MARK" 'sleep.args'
assert_marker "g1-prose-typo-canary-ran" "$MARK" 'ran.claude.canary'

# G2: DEFECT 2 — one byte of stderr. canary_probe redirects 2>&1 into the file
# whose stdout is parsed as JSON, so a single warning line makes well-formed
# output unreadable and every failure becomes `infra`.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":429,"result":"rate limited"}' \
  FAKE_CANARY_STDERR='Warning: something harmless on stderr' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_status "g2-stderr-byte-classified" "SESSION-LIMIT" "$OUTPUT"
assert_absent "g2-stderr-byte-no-infra" "STORY_RUN: INFRA" "$OUTPUT"
assert_marker "g2-stderr-preserved" "$(RUN_DIR_OF "$DT")" 'task-1.canary.err'

# G3 (R-3): api_error_status present-with-429.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":429,"result":"quota"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_status "g3-status-429" "SESSION-LIMIT" "$OUTPUT"

# G4 (R-3): present-with-NULL — what production actually emits, and what jq's
# `//` collapses with absent today.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":null,"result":"Connection error"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_status "g4-status-null" "SESSION-LIMIT" "$OUTPUT"

# G5 (R-3): key ABSENT entirely — must classify identically to G4.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"result":"Connection error"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_status "g5-status-absent" "SESSION-LIMIT" "$OUTPUT"

# G6 (R-3): a non-429 numeric status proves the field is genuinely READ.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":401,"result":"nope"}' -- fixture
assert_exit   "g6-status-401-exit2" 2 "$RC"
assert_status "g6-status-401-auth" "AUTH-EXPIRED" "$OUTPUT"

# G7 (R-3): output that is not JSON at all -> infra (the substrate produced
# nothing classifiable). This is the ONLY remaining infra lane.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='this is not json at all' -- fixture
assert_exit   "g7-non-json-exit2" 2 "$RC"
assert_status "g7-non-json-infra" "STORY_RUN: INFRA" "$OUTPUT"
assert_marker "g7-non-json-incident" "$(RUN_DIR_OF "$DT")" 'task-1.infra-incident.*.json'

# G8 (R-3): EMPTY canary output (probe killed by its timeout) -> infra. The case
# most likely to be missed, because it does not look like a fixture.
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='' FAKE_CANARY_RC=124 -- fixture
assert_exit   "g8-empty-exit2" 2 "$RC"
assert_status "g8-empty-infra" "STORY_RUN: INFRA" "$OUTPUT"

# G9: healthy canary + failed devloop => the environment is fine, so the failure
# is task-specific and escalates (implementer).
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON="$CANARY_HEALTHY" -- fixture
assert_exit   "g9-healthy-exit1" 1 "$RC"
assert_status "g9-healthy-escalates" "devloop-session-error" "$OUTPUT"

# G10: retries exhausted on a POSITIVELY-classified quota outage must route to
# the infra lane, not be recorded against the task. This is the precondition for
# the classify inversion: inverting the default toward retryable while this lane
# terminates in escalate() would convert MORE outages into implementer bugs.
run_story FAKE_DEVLOOP_RCS="1 1 1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":429,"result":"quota"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_exit   "g10-exhausted-exit2" 2 "$RC"
assert_status "g10-exhausted-token" "SESSION-LIMIT-EXHAUSTED" "$OUTPUT"
if [ "$(manifest_status "$FIX" 1)" = "pending" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[g10-exhausted-manifest-untouched] task 1 is '$(manifest_status "$FIX" 1)', expected pending"); fi

# G11: ADR-0035 §6 — the canary runs with --allowedTools "" AND keeps
# --dangerously-skip-permissions. Dropping the latter reintroduces a
# permission-prompt hang misclassified as `infra`. --allowedTools must stay LAST
# (it is variadic, so a following flag would be consumed as its value).
run_story FAKE_DEVLOOP_RCS="1" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON="$CANARY_HEALTHY" -- fixture
canary_argv="$(grep -- '--model haiku' "${MARK}/claude.argv" 2>/dev/null || true)"
assert_status "g11-canary-allowedtools-empty" "--allowedTools '' " "$canary_argv"
assert_status "g11-canary-skip-permissions"   "--dangerously-skip-permissions" "$canary_argv"

# =============================================================================
# (H) TRANSIENT GIT ERRORS — R-2 defect 4 (mechanism M2)
# =============================================================================
# H1: the devloop exits 0 having committed nothing, and `git rev-parse HEAD`
# then fails. `[ "$(git rev-parse HEAD)" = "$head_before" ]` compares "" against
# a sha, decides HEAD MOVED, skips the no-commit escalation, gates, and marks
# the task COMPLETE. A false green produced by a git error.
run_story FAKE_DEVLOOP_RCS="0" FAKE_DEVLOOP_COMMIT=0 FAKE_BREAK_GIT_HEAD=1 -- fixture
assert_exit   "h1-git-error-exit2" 2 "$RC"
assert_status "h1-git-error-token" "GIT-ERROR" "$OUTPUT"
assert_marker "h1-git-error-incident" "$(RUN_DIR_OF "$DT")" 'task-1.infra-incident.*.json'
# Same contents assertion for the git-error lane. Asserting the log file EXISTS
# also pins that the git stderr capture actually happened, which nothing else does.
h1_rec="$(ls "$(RUN_DIR_OF "$DT")"/task-1.infra-incident.*.json 2>/dev/null | head -n 1)"
assert_status "h1-record-lane" "git-error" "$(jq -r '.lane // "MISSING"' "$h1_rec" 2>/dev/null || echo PARSE-FAILED)"
h1_log="$(jq -r '.log // "MISSING"' "$h1_rec" 2>/dev/null || echo PARSE-FAILED)"
assert_status "h1-record-log-is-giterr" "git-error" "$h1_log"
if [ -s "$h1_log" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[h1-record-log-captured-stderr] record's log '${h1_log}' is missing or empty — git's stderr was not captured"); fi
if [ "$(manifest_status "$FIX" 1)" != "completed" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[h1-git-error-not-completed] a git error marked task 1 COMPLETED (false green)"); fi

# H2: the stop hook's own instance of the same mechanism
# (devloop-stop-hook.sh:26-29). `head="$(... || true)"` then `[ -z "$head" ] ||
# [ "$head" != "$START" ]` exits 0 — so a git error ALLOWS the stop, and a
# headless Lead ends its turn with no commit and no escalation file.
hookrepo="$(mktemp -d "${WORK}/hook.XXXXXX")"
git -c init.defaultBranch=main init -q "$hookrepo"
git -C "$hookrepo" config user.email "fixture@example.invalid"
git -C "$hookrepo" config user.name "Fixture Runner"
git -C "$hookrepo" commit --quiet --allow-empty -m "base" >/dev/null
hook_head="$(git -C "$hookrepo" rev-parse HEAD)"
printf 'ref: refs/heads/nonexistent-branch\n' > "${hookrepo}/.git/HEAD"
hook_out="$(env -i PATH="${STUB_BIN}:${PATH}" HOME="$FIXHOME" \
  DEVLOOP_HEADLESS=1 DEVLOOP_START_HEAD="$hook_head" \
  CLAUDE_PROJECT_DIR="$hookrepo" \
  DEVLOOP_STOP_COUNT_FILE="${WORK}/hook.count" \
  DEVLOOP_TEST_MARKERS="${WORK}" \
  bash "$STOP_HOOK" 2>&1)"
assert_status "h2-stop-hook-fails-closed" '"decision":"block"' "$hook_out"
assert_status "h2-stop-hook-distinct-reason" "git" "$hook_out"

# H3: the hook still ALLOWS a stop when the repo is healthy and HEAD moved —
# the control must be proven to stay silent when it should not fire.
#
# ANCHORED POSITIVELY, and it was not originally. `assert_absent
# '"decision":"block"'` alone is satisfied by the hook ALLOWING the stop and
# equally by the hook not existing, erroring, or never running — its expected
# outcome IS the null outcome. Measured: with STOP_HOOK repointed at a
# nonexistent path, h3 PASSED while h2 caught the deletion. Same class as K1.
# The hook's contract (devloop-stop-hook.sh:11) is "Exit 0 with no output =
# allow the stop", so asserting BOTH halves of that contract is what
# distinguishes "allowed" from "never ran".
git -C "$hookrepo" symbolic-ref HEAD refs/heads/main
git -C "$hookrepo" commit --quiet --allow-empty -m "progress" >/dev/null
hook_rc2=0
hook_out2="$(env -i PATH="${STUB_BIN}:${PATH}" HOME="$FIXHOME" \
  DEVLOOP_HEADLESS=1 DEVLOOP_START_HEAD="$hook_head" \
  CLAUDE_PROJECT_DIR="$hookrepo" \
  DEVLOOP_STOP_COUNT_FILE="${WORK}/hook.count2" \
  bash "$STOP_HOOK" 2>&1)" || hook_rc2=$?
assert_absent "h3-stop-hook-allows-progress" '"decision":"block"' "$hook_out2"
assert_exit   "h3-stop-hook-ran-and-exited-0" 0 "$hook_rc2"
if [ -z "$hook_out2" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[h3-stop-hook-silent-allow] expected NO output (contract: exit 0 + no output = allow), got: ${hook_out2}"); fi

# =============================================================================
# (I) PROMPT SPLICING — R-2 defect 5
# =============================================================================
# The prompt is interpolated into `/devloop "%s" --specialist=%s`, so a manifest
# prompt containing a double quote truncates the devloop's instruction and
# silently changes what the task is told to do. Latent while manifests are
# hand-authored; live once /user-story emits them (story task #4 depends on this).
SPLICE_SENTINEL='SPLICE-CANARY-7f3a'
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" '- id: 1
  status: pending
  specialist: test
  env_tests: false
  prompt: "Fix the \"quoted\" thing SPLICE-CANARY-7f3a and also --paired-with=evil"'
git -C "$TEMPLATE" commit --quiet -am "splice fixture" >/dev/null
run_story -- fixture
devloop_argv="$(grep -- '--output-format stream-json' "${MARK}/claude.argv" 2>/dev/null || true)"
# Negative half: no manifest byte reaches the command line.
assert_absent "i1-sentinel-absent-from-argv" "$SPLICE_SENTINEL" "$devloop_argv"
assert_absent "i2-paired-with-not-spliced"   "--paired-with=evil" "$devloop_argv"
# Positive half — absence alone is vacuous: it passes if the runner drops the
# prompt reference entirely and dispatches a task with no instruction at all.
assert_status "i3-prompt-file-referenced" "task-1.prompt" "$devloop_argv"
assert_status "i4-specialist-flag-intact" "--specialist=test" "$devloop_argv"
# And the full prompt survives verbatim on disk, where the instruction points.
prompt_on_disk="$(cat "$(RUN_DIR_OF "$DT")/task-1.prompt" 2>/dev/null || true)"
assert_status "i5-prompt-verbatim-on-disk" "$SPLICE_SENTINEL" "$prompt_on_disk"
assert_status "i6-prompt-quotes-preserved" '"quoted"' "$prompt_on_disk"

# I7: a specialist that is not a plain token is refused, not spliced as flags.
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" '- id: 1
  status: pending
  specialist: "test --paired-with=evil"
  env_tests: false
  prompt: ordinary prompt'
git -C "$TEMPLATE" commit --quiet -am "bad specialist fixture" >/dev/null
run_story -- fixture
assert_exit   "i7-bad-specialist-exit2" 2 "$RC"
assert_status "i7-bad-specialist-token" "INVALID-SPECIALIST" "$OUTPUT"
mk_story "${TEMPLATE}/docs/user-stories/2026-08-13-fixture.md" "$DEFAULT_TASKS"
git -C "$TEMPLATE" commit --quiet -am "restore fixture" >/dev/null

# =============================================================================
# (J) TIMEOUT LANE
# =============================================================================
# run-story.sh:57-59 says "timeout is an infra escalation, not a task verdict";
# the code escalated. Resolving toward the comment.
run_story FAKE_DEVLOOP_RCS="124" FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON="$CANARY_HEALTHY" -- fixture
assert_exit   "j1-timeout-exit2" 2 "$RC"
assert_status "j1-timeout-token" "TIMEOUT" "$OUTPUT"
if [ "$(manifest_status "$FIX" 1)" = "pending" ]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[j1-timeout-manifest-untouched] task 1 is '$(manifest_status "$FIX" 1)', expected pending"); fi

# =============================================================================
# (K) RESUME-POINTER SELECTION — newest_devloop_output's HAPPY path
# =============================================================================
# K1 covers a gap found while hardening that function: every other case drives
# it with docs/devloop-outputs ABSENT or empty, so the branch that actually
# SELECTS a slug had no test — and it carries a shape filter
# (`grep -E '^[0-9]+(\.[0-9]+)? '`) added so a stderr line folded in by find's
# 2>&1 cannot win `sort -rn | head -n 1` and silently discard the real entry.
# A filter on an untested selection path is exactly the "control that never
# runs" shape this suite exists to close, so the positive case is required:
# without it, a filter that rejected EVERY line would still pass the suite,
# because "no resumable output" is the default expectation everywhere else.
run_story FAKE_DEVLOOP_RCS="1 0" FAKE_DEVLOOP_MKOUT=1 FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":429,"result":"quota"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
assert_status "k1-resume-slug-selected" "resume=2026-08-13-fixture-devloop" "$OUTPUT"
assert_marker "k1-resume-slept" "$MARK" 'sleep.args'
# The second attempt must carry --continue with that slug, not start fresh.
resume_argv="$(grep -- '--continue=2026-08-13-fixture-devloop' "${MARK}/claude.argv" 2>/dev/null || true)"
assert_status "k1-resume-continue-flag" "--continue=2026-08-13-fixture-devloop" "$resume_argv"

# =============================================================================
# (L) REFUSAL LANES ADDED BY THIS COMMIT
# =============================================================================
# The story's accepted "full branch enumeration" deferral is scoped to
# PRE-EXISTING branches. A lane added in this commit has never executed once,
# which is a different claim — so each one added here is driven at least once.

# L1 (OPS-1): INCIDENT-RECORD-UNWRITABLE. The bug this closes was that
# `>"$rec"` CREATES the file before jq runs, so a zero-byte record passed every
# glob-based assertion — the suite would have gone green against pre-fix code on
# exactly the property the fix establishes. The jq stub fails only on
# record_infra_incident's call (`--argjson task`), leaving manifest parsing,
# cost telemetry and canary classification untouched.
run_story FAKE_LAYER3_RC=2 FAKE_JQ_FAIL_INCIDENT=1 -- fixture
assert_status "l1-record-unwritable-token" "INCIDENT-RECORD-UNWRITABLE" "$OUTPUT"
assert_marker "l1-record-unwritable-jq-ran" "$MARK" 'ran.jq.incident-fail'
# The contract is "the lane exit below still stands" — a failed record must not
# change the lane's verdict.
assert_exit   "l1-record-unwritable-lane-exit-unchanged" 2 "$RC"
assert_status "l1-record-unwritable-lane-still-named" "PIPELINE-PRECONDITION" "$OUTPUT"

# L2 (OPS-2): STOP-AFTER-UNVERIFIABLE. The DT_STORY seam exists to make this
# testable. Since the exit code cannot carry the diagnosis (clap's
# unrecognized-subcommand collides with dt-story's own EXIT_MALFORMED), the
# MESSAGE is the entire mechanism — so assert the message, not the code.
cat > "${WORK}/dt-story-stale" <<'STUB'
#!/usr/bin/env bash
case "$1" in list-tasks) exit 2 ;; *) exit 0 ;; esac
STUB
chmod +x "${WORK}/dt-story-stale"
run_story DT_STORY="${WORK}/dt-story-stale" -- fixture --stop-after=1
assert_exit   "l2-unverifiable-exit2" 2 "$RC"
assert_status "l2-unverifiable-token" "STOP-AFTER-UNVERIFIABLE" "$OUTPUT"
assert_status "l2-names-not-built"    "not built" "$OUTPUT"
assert_status "l2-names-stale"        "stale" "$OUTPUT"
assert_status "l2-names-malformed"    "manifest" "$OUTPUT"
assert_status "l2-names-disambiguator" "validate" "$OUTPUT"
assert_absent "l2-not-an-id-message"  "STOP-AFTER-UNKNOWN-TASK" "$OUTPUT"

# L3 (OPS-3): tree_dirty's rc-2 branch. Its fix is the one that is MESSAGE-only
# on an already-correct lane, which is the wrong-message class this diff repairs
# twice elsewhere — so an untested version of it would be the same defect.
FIXTURE_PRERUN=corrupt_git_index run_story -- fixture
unset FIXTURE_PRERUN
assert_exit   "l3-tree-dirty-git-error-exit2" 2 "$RC"
assert_status "l3-tree-dirty-git-error-token" "GIT-ERROR" "$OUTPUT"
assert_absent "l3-tree-dirty-not-dirty-message" "dirty tree and no resumable devloop" "$OUTPUT"

# L4 (OPS-3): the pre-devloop head_before read — the M2 site that aborted under
# `set -e` with exit 1 and no lane, which the exit-code header documents as
# "task escalated".
FIXTURE_PRERUN=corrupt_git_head run_story -- fixture
unset FIXTURE_PRERUN
assert_exit      "l4-head-before-exit2" 2 "$RC"
assert_status    "l4-head-before-token" "GIT-ERROR" "$OUTPUT"
assert_no_marker "l4-head-before-no-devloop" "$MARK" 'ran.claude.devloop'

# L5 (CQ-10): the THIRD splice site's second producer. A devloop-created output
# directory whose name carries the splice sentinel must not reach --continue= on
# the session-limit resume path — the path that reads newest_devloop_output
# rather than $slug_file. Without the slug floor the same name is a hard exit 2
# across invocations and silently interpolated within one.
run_story FAKE_DEVLOOP_RCS="1 0" FAKE_DEVLOOP_MKOUT=1 \
  FAKE_DEVLOOP_MKOUT_NAME='2026-08-13-evil" --paired-with=evil' FAKE_DEVLOOP_COMMIT=0 \
  FAKE_CANARY_JSON='{"is_error":true,"api_error_status":429,"result":"quota"}' \
  STORY_SESSION_LIMIT_RETRIES=1 -- fixture
l5_argv="$(cat "${MARK}/claude.argv" 2>/dev/null || true)"
assert_absent "l5-hostile-slug-not-in-argv" "--paired-with=evil" "$l5_argv"
assert_status "l5-hostile-slug-warned"      "outside [0-9A-Za-z._-]" "$OUTPUT"

# L6: INVALID-RESUME-SLUG — the same floor on the $slug_file producer.
SEED_SLUG='bad slug" --paired-with=evil' run_story -- fixture
unset SEED_SLUG
assert_exit   "l6-invalid-resume-slug-exit2" 2 "$RC"
assert_status "l6-invalid-resume-slug-token" "INVALID-RESUME-SLUG" "$OUTPUT"

# L7: UNSAFE-RUN-DIR. Priced in-code as newly refusing runs that work today, so
# it is exercised before it meets an operator.
spacedir="$(mktemp -d "${WORK}/sp ace.XXXXXX")"
run_story DEVLOOP_TMP="$spacedir" -- fixture
assert_exit   "l7-unsafe-run-dir-exit2" 2 "$RC"
assert_status "l7-unsafe-run-dir-token" "UNSAFE-RUN-DIR" "$OUTPUT"

# L8: SEAM-RUN-DIR-INSIDE-REAL-REPO. Uses an EXISTING directory inside the real
# clone (no writes to /work).
run_story DEVLOOP_TMP="${REAL_REPO_ROOT}/scripts" -- fixture
assert_exit   "l8-run-dir-inside-real-repo-exit2" 2 "$RC"
assert_status "l8-run-dir-inside-real-repo-token" "SEAM-RUN-DIR-INSIDE-REAL-REPO" "$OUTPUT"

# NOT COVERED, with reasons (both accepted at Gate 3):
#   STOP-AFTER-NEVER-FIRED — the terminal check sits on the AllDone path, and
#     every scenario that would leave the flag unfired (a dep escalating, a gate
#     going red, a lane exit) leaves the loop EARLIER through escalate() or an
#     exit 2, never through AllDone. (@operations. The earlier reason given here
#     — "the up-front validation makes it unreachable" — was circular: it leaned
#     on the over-aggressive dep check that OPS-4 deleted, so it would have
#     become unfounded the moment that landed, with nothing to flag it.)
#   persist_resume_pointer's git-fault WARN — a documented DELIBERATE exception
#     to the lane rule (it must not hijack the caller's lane), where the comment
#     is better evidence than a contrived test. @operations explicitly accepted.

report_results "scripts/workflow/run-story.test.sh"
