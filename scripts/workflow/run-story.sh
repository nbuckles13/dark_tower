#!/usr/bin/env bash
# run-story.sh — deterministic story runner (v1). DRAFT for review.
#
# Executes a user story's pending tasks serially, one headless /devloop per
# task, on the current branch, in the current tree. No model in the control
# flow: task selection and state transitions go through dt-story; pass/fail
# authority is this script's own validation-pipeline run, never the devloop's
# self-reported outcome.
#
# Intended execution context: inside the devloop container set up by
# devloop.sh (skip-permissions is only safe behind that boundary).
#
# Usage: scripts/workflow/run-story.sh <story-file.md | story-slug> \
#          [--stop-after=N] [--revalidate | --restart <text>]
#
#   --stop-after=N  exit 0 after task N completes (skips the story-close gate).
#                   For staged runs where a human step belongs between tasks.
#
#   --revalidate    operator-intervention retry for the escalated task the run
#                   reaches: re-run ONLY the authoritative gate against the tree
#                   as committed by the prior attempt (no devloop is spawned).
#                   For a failure judged environmental, not diff-caused (e.g. a
#                   since-fixed flaky Layer-7 test). Green -> complete; red ->
#                   re-escalate. Composable with --stop-after.
#   --restart <text>  operator-intervention retry for the escalated task the run
#                   reaches: spawn a FRESH devloop seeded with <text> (the
#                   operator's diagnosis), the prior attempt's commit and its
#                   gate-failure tail, so it starts from the diagnosis and the
#                   existing commit rather than re-implementing. <text> is
#                   REQUIRED. Composable with --stop-after.
#   --revalidate and --restart are MUTUALLY EXCLUSIVE. Both REFUSE if the task
#   the run reaches has no prior escalation, or if that attempt never committed
#   (a devloop-no-commit escalation has nothing to validate or restart from).
#
# Exit: 0  all tasks complete, story-close gate green (or --stop-after reached)
#       1  task escalated (escalation.json path printed) or blocked
#       2  precondition / infra / manifest failure / retry-flag misuse
set -euo pipefail

# Timestamped console logging (UTC). All STORY_RUN lines route through these so
# elapsed time between events is readable in a captured run log. Defined above
# the seam block so its refusals are timestamped like every other STORY_RUN line.
#
# NB these emit `STORY_RUN: <TOKEN> ...`, NOT `STATUS=/REASON=`. That is the
# LAYER-SCRIPT contract (parsed by lang/_common.sh run_and_emit /
# tee_collect_statuses, keyed by docs/runbooks/devloop-validation.md §6.3);
# this script emits no STATUS lines and must not appear to.
slog() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
slogerr() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }

# resolve_run_base <kind> — the base dir for a run-artifact class (ADR-0037 D5).
# TWO kinds with DELIBERATELY DIFFERENT bases that must NOT be collapsed (the
# two-key decomposition): a single-value accessor would re-unify them behind a
# name that reads like correctness, which is why this takes an argument.
#   marker   — per-CONTAINER, EPHEMERAL: the in-flight marker + substrate-probe
#              cache. Stays on ${DEVLOOP_TMP:-/tmp/devloop} — this is what
#              devloop.sh:~715 reads to skip the mid-run CLI update; do NOT move it.
#   run-dir  — per-STORY, HOST-PERSISTENT: cost ledger + durable per-task evidence.
#              Default base moves off container /tmp so it survives teardown.
# Returns the base WITH the /story-runner segment (callers append [/<story>]); the
# .ledger-mount positive-control marker lives one level up, at
# "$(dirname "$(resolve_run_base run-dir)")/.ledger-mount" == the mount root.
#
# PRECEDENCE IS A SECURITY INVARIANT — DEVLOOP_TMP MUST dominate DEVLOOP_STORY_RUN_BASE.
# Two properties rest on it: (1) under DEVLOOP_TEST the harness sets DEVLOOP_TMP to a
# mktemp dir to isolate, and __seam_assert_run_dir_isolated validates DEVLOOP_TMP —
# if the carrier outranked it, the seam would validate one var while another placed
# evidence (the S-1 hazard, back door); (2) the carrier gets NO CI-presence clause
# precisely because DEVLOOP_TMP dominates and the harness's env -i scrubs it, so an
# ambient carrier leaking into a CI job is overridden and inert. Flip the order and
# both break. The carrier is a DEFAULT the launcher supplies, never an override; the
# isolation knob is DEVLOOP_TMP and it wins.
#
# DEVLOOP_STORY_RUN_BASE is NOT a seam (see the seam header): a seam refuses when
# present WITHOUT the sentinel, but devloop.sh sets this on every production
# container (DEVLOOP_TEST unset), so enrolling it would refuse every production run.
# A var the production launcher sets can never carry "present without sentinel =
# tamper". It composes RUN_DIR in production only and cannot reach the destructive
# git paths (those key on REPO_ROOT). The unguarded ${HOME} is deliberate: with both
# vars unset, `set -u` makes an unset HOME a HARD ERROR, not a silent empty expansion.
resolve_run_base() {
  case "${1:-}" in
    marker)  printf '%s' "${DEVLOOP_TMP:-/tmp/devloop}/story-runner" ;;
    run-dir) printf '%s' "${DEVLOOP_TMP:-${DEVLOOP_STORY_RUN_BASE:-${HOME}/.cache/devloop/story-runs}}/story-runner" ;;
    *) slogerr "STORY_RUN: RESOLVE-RUN-BASE-BAD-KIND '${1:-}' — internal error, expected 'marker' or 'run-dir'."; exit 2 ;;
  esac
}

# =============================================================================
# TEST SEAMS (security trust boundary; ADR-0035 §12)
# =============================================================================
# TWO seams, and the count is a CONSEQUENCE, not a preference. ADR-0035 §12
# names five (repo-root, DT_STORY, layer-script dir, preflight bypass,
# audit-glob root). This script derives REPO_ROOT from BASH_SOURCE and `cd`s
# into it, after which EVERY filesystem path it touches is CWD-relative:
# scripts/workflow/preflight-story.sh, scripts/layer${n}.sh, scripts/layer7.sh,
# ./scripts/layer-all.sh, scripts/lang/*/audit.sh,
# scripts/lang/<lang>/audit-remediation.md, docs/user-stories/*,
# docs/devloop-outputs/*, .devloop-escalation.json, audit-suppressions.toml.
# So the root redirect SUBSUMES three of the five. The single exception is
# target/release/dt-story — a BUILT ARTIFACT that must keep pointing at the real
# binary while the tree moves, which is the whole reason it needs its own seam.
# DO NOT add a third seam for a path that is already CWD-relative.
#
#   STORY_REPO_ROOT — repo root override (default: BASH_SOURCE-derived)
#   DT_STORY        — dt-story binary override (default: target/release/dt-story)
#
# DEVLOOP_TMP is NOT a seam. It is a PRE-EXISTING redirect (see RUN_DIR /
# RUN_BASE below) that predates this block; the sentinel adds a CONSTRAINT on
# it, not a redirect through it. That is why the count above stays two.
#
# DEVLOOP_STORY_RUN_BASE (ADR-0037 D5) is ALSO not a seam — same class as
# DEVLOOP_TMP, so the count stays two. It is a production CARRIER: devloop.sh sets
# it via -e to the container ledger mount target on EVERY production container
# (where DEVLOOP_TEST is unset). A seam's defining behaviour is "present without
# the exact sentinel => exit 2"; a var the production launcher sets on every run
# can never carry that "present without sentinel = tamper" signal, so enrolling it
# in __any_seam_override_present would refuse every production run. It composes the
# run-dir base ONLY in production (DEVLOOP_TMP dominates it under the sentinel — see
# resolve_run_base) and cannot reach the destructive git reset/clean paths (those
# key on REPO_ROOT). The evidence-interleaving hazard it could otherwise open under
# the sentinel is closed by the H2 sibling clause in __seam_assert_run_dir_isolated,
# NOT by a seam refusal.
#
# Overrides are read ONLY when DEVLOOP_TEST is set EXACTLY to "1".
# DO NOT change the gating to a truthy `-n` test — DEVLOOP_TEST=0 must NOT
# enable overrides (audit-suppressions-check.sh:35-36 closes the same bypass).
# An override present WITHOUT the exact sentinel is a tamper/misconfig signal:
# fail LOUD, never silently honor and never silently ignore.
#
# Ordering below is load-bearing: every refusal happens BEFORE the `cd`, and
# before any git command THAT COULD ACT ON THE REAL TREE — so a misconfigured
# seam cannot reach the destructive paths (`git reset --hard` / `git clean -fdq`)
# this script contains. Check (4) does run git, deliberately: `git -C
# "$__seam_root" rev-parse --show-toplevel` is read-only and explicitly `-C`
# scoped to the CANDIDATE root, never to the CWD. Do not read this as "the block
# is git-free" and add an unscoped call on that basis.
# -----------------------------------------------------------------------------

__test_sentinel_active() { [[ "${DEVLOOP_TEST:-}" == "1" ]]; }

# True if any test-injection override env is set (regardless of sentinel).
__any_seam_override_present() {
  [[ -n "${STORY_REPO_ROOT:-}" ]] && return 0
  [[ -n "${DT_STORY:-}" ]] && return 0
  return 1
}

# (1) CI presence-rejection. PRESENCE-based and INDEPENDENT of DEVLOOP_TEST —
# the LAYER_SCRIPT_DIR shape (lang/_common.sh:215-219), not the
# DEVLOOP_HELPER_SOCKET shape. _common.sh withholds a CI clause only where
# CI-inertness is guaranteed by assert_no_ci_sentinel_leak; this script sources
# no _common.sh, so that premise is absent and the clause is required. Note
# scripts/workflow/run-story.test.sh IS on CI's path (via layer3.sh) and does
# exercise these seams there — legitimately, because `env -i` scrubs
# GITHUB_ACTIONS from its children (the layer-all.test.sh:73-77 pattern). This
# clause defends against AMBIENT LEAKAGE into a process that inherited the CI
# environment, not against a deliberate scrub.
if [ -n "${GITHUB_ACTIONS:-}" ] && __any_seam_override_present; then
  slogerr "STORY_RUN: SEAM-SET-IN-CI — a run-story test seam (STORY_REPO_ROOT / DT_STORY) is set in a CI job. These are LOCAL-ONLY test seams; nothing legitimate sets them in CI. Find and remove whatever exported it; do NOT unset-and-rerun blindly."
  exit 2
fi

# (2) Trust-boundary guard: an override env present without the EXACT sentinel.
if __any_seam_override_present && ! __test_sentinel_active; then
  slogerr "STORY_RUN: SEAM-OVERRIDE-WITHOUT-TEST-SENTINEL — a test-injection override (STORY_REPO_ROOT / DT_STORY) is set but DEVLOOP_TEST != \"1\" — refusing to honor it (possible env injection). DT_STORY in particular selects the binary that runs 'complete'/'escalate' against the real manifest. Investigate; do not unset-and-rerun blindly."
  exit 2
fi

# (3) Resolve the repo root: real (BASH_SOURCE-derived) or seam.
# `pwd -P` on both sides so the containment comparisons below and the
# under-the-repo check in __seam_assert_run_dir_isolated are physical-path sound.
REAL_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
REPO_ROOT="$REAL_REPO_ROOT"
__seam_dt_story=""

# (4) Containment (R-1). Setting the redirect is not the same as being
# redirected: git resolves UPWARD from the working directory, so a root pointing
# INSIDE the real clone provides no containment at all — measured, `git reset
# --hard` from such a directory reverts the real tree and `git clean -fdq`
# deletes the fixture's own contents (this script reaches exactly that pair
# below). Containment holds only when the redirected root IS ITS OWN git
# top-level AND DIFFERS from the real one; the second half is load-bearing
# because a root set to the real repository is a no-op redirect that would
# otherwise pass.
#
# Comparisons use `-ef` (device+inode), NEVER string equality: a fixture reached
# via a symlinked path (mktemp -d under a symlinked /tmp) resolves
# --show-toplevel to the real path, so `=` reports not-equal for an identical
# directory — the suite would red spuriously and the obvious "fix" is to weaken
# the check.
#
# ORDERING IS LOAD-BEARING, and `-ef` is why: `[ /existing -ef /missing ]` is
# FALSE, so the "differs from the real root" test below evaluates TRUE (pass)
# whenever either operand does not exist. It is safe ONLY because the
# own-toplevel check above it has already rejected a nonexistent/non-repo root.
# Do not reorder, short-circuit or merge these two tests.
# (__seam_assert_run_dir_isolated rests on the same fail-open primitive; see the
# note there, which is the shared half of this comment.)
if __test_sentinel_active; then
  __seam_dt_story="${DT_STORY:-}"
  if [ -n "${STORY_REPO_ROOT:-}" ]; then
    if ! __seam_root="$(cd "${STORY_REPO_ROOT}" 2>/dev/null && pwd -P)"; then
      slogerr "STORY_RUN: SEAM-ROOT-UNUSABLE — STORY_REPO_ROOT='${STORY_REPO_ROOT}' is not a directory this process can enter."
      exit 2
    fi
    __seam_top="$(git -C "$__seam_root" rev-parse --show-toplevel 2>/dev/null || true)"
    if [ -z "$__seam_top" ] || ! [ "$__seam_top" -ef "$__seam_root" ]; then
      slogerr "STORY_RUN: SEAM-ROOT-NOT-OWN-TOPLEVEL — STORY_REPO_ROOT='${__seam_root}' is not its own git top-level (git says: '${__seam_top:-<not a git repository>}'). A root inside another clone gives NO containment: this script runs 'git reset --hard' and 'git clean -fdq', which would act on the enclosing repository. Use a standalone fixture repo (mktemp -d + git init)."
      exit 2
    fi
    if [ "$__seam_root" -ef "$REAL_REPO_ROOT" ]; then
      slogerr "STORY_RUN: SEAM-ROOT-IS-REAL-REPO — STORY_REPO_ROOT='${__seam_root}' resolves to the real repository ('${REAL_REPO_ROOT}'). That is a no-op redirect, not containment: the destructive paths would run against the tree the seam exists to protect."
      exit 2
    fi
    REPO_ROOT="$__seam_root"
  fi
fi

# (5) DEVLOOP_TMP containment. NOT a seam (see the header) — a CONSTRAINT on a
# pre-existing redirect, enforced only under the sentinel. Two failures, both
# measured: RUN_BASE/.run-in-flight is removed by this script's own EXIT trap,
# so an un-redirected test invocation DELETES A LIVE RUN'S marker (a host attach
# then updates the CLI, and the per-task substrate assertion kills that story
# with SUBSTRATE-CHANGED); and the run dir is where every task-N.* evidence
# artifact lands, so a shared RUN_BASE interleaves fixture canary/incident/cost
# records into the live run's evidence directory.
#
# The `.run-in-flight` check states the property POSITIVELY ("no other runner
# owns this run dir"). A denylist of one path would catch today's hazard but not
# a harness passing the AMBIENT DEVLOOP_TMP through — which in a container where
# devloop.sh set DEVLOOP_TMP=/tmp/devloop-<slug> clobbers a real run dir.
# Deliberately sentinel-gated: refusing unconditionally on a pre-existing marker
# would block recovery after a SIGKILLed run leaves a stale one, since the EXIT
# trap cannot fire on a kill.
__seam_assert_run_dir_isolated() {
  local dt resolved default_base
  dt="${DEVLOOP_TMP:-}"
  if [ -z "$dt" ]; then
    slogerr "STORY_RUN: SEAM-RUN-DIR-NOT-REDIRECTED — DEVLOOP_TEST=1 but DEVLOOP_TMP is unset, so BOTH per-container defaults would be live: the in-flight marker + probe cache default to /tmp/devloop, and the evidence/ledger defaults to the production ledger base — either would collide with a live run. Redirect it (mktemp -d)."
    exit 2
  fi
  if ! resolved="$(cd "$dt" 2>/dev/null && pwd -P)"; then
    slogerr "STORY_RUN: SEAM-RUN-DIR-UNUSABLE — DEVLOOP_TMP='${dt}' is not a directory this process can enter."
    exit 2
  fi
  # Shared fail-open note (see the ordering comment at (4)): `-ef` is FALSE when
  # either operand is missing. Here that is decided EXPLICITLY rather than
  # inherited — an absent /tmp/devloop means no live run can be sharing it, so
  # there is nothing to collide with and this passes.
  # LITERAL, never an env indirection. An overridable "what counts as the
  # default" is an undeclared third override that RELAXES this check rather than
  # redirecting a path: pointed at a nonexistent path it makes the `-ef` below
  # compare against a missing operand, which is false, so a run dir that
  # genuinely IS /tmp/devloop passes the guard. That reopens the live-run
  # marker-deletion hazard this function exists to close, and it would escape
  # both halves of the seam contract — it is not in __any_seam_override_present,
  # so neither the fail-loud-without-sentinel guard nor the CI presence
  # rejection would cover it. If this ever needs to be configurable it enters
  # through that enumeration and gets the CI clause, like the other two.
  # H1 — MARKER-deletion hazard. This literal is the MARKER base default
  # (resolve_run_base marker), and it STAYS a literal: :200-209 argues making it
  # configurable would relax the check. Post-decomposition /tmp/devloop holds the
  # in-flight marker + substrate-probe cache (the ledger moved to the run-dir base);
  # so this message now names the marker hazard specifically, and H2 below names the
  # evidence hazard that moved with the ledger.
  default_base="/tmp/devloop"
  if [ -d "$default_base" ] && [ "$resolved" -ef "$default_base" ]; then
    slogerr "STORY_RUN: SEAM-RUN-DIR-NOT-REDIRECTED — DEVLOOP_TMP resolves to the default '${default_base}', where a live run keeps its in-flight marker and substrate-probe cache; this process's EXIT trap would delete that marker. Redirect it (mktemp -d)."
    exit 2
  fi
  # H2 — EVIDENCE-interleaving hazard, which moved to the run-dir (ledger) base
  # under the two-key decomposition and is NO LONGER guarded by the /tmp/devloop
  # literal above. A DEVLOOP_TEST run whose DEVLOOP_TMP resolves to the production
  # ledger base lands RUN_DIR identical to a live run's — and that dir now SURVIVES
  # teardown, so fixture canary/incident/cost records durably interleave into it.
  # (The :272 in-use check only covered the run dir incidentally, while marker and
  # run dir shared a base; the decomposition ended that co-location silently.)
  #
  # LEFT operand is `$resolved` (this test's own DEVLOOP_TMP, env-following — same as
  # H1). RIGHT operands are two LITERALS, OR'd, and MUST NOT derive via
  # resolve_run_base: that reads DEVLOOP_STORY_RUN_BASE/$HOME, so a nonexistent
  # target would make `-ef` compare a missing operand (FALSE => pass) and a
  # DEVLOOP_TMP aimed at the real ledger would sail through — the :200-209
  # relaxation, one base over. The comparison FLOOR must be a value this test
  # process's env cannot influence.
  #   (a) container base — env-IMMUNE floor;
  #   (b) ${HOME} form   — adds the STORY_RUNNER_ALLOW_HOST host-lane, where the real
  #       base is the operator's $HOME. OR-ing (b) atop (a) can only ADD a refusal,
  #       never subtract one (a phantom (b) simply never fires) — a future edit that
  #       makes (b) REPLACE rather than AUGMENT (a) reopens the original hole; keep
  #       (a) as the standalone floor.
  # SYMMETRY vs H1 (:210), stated precisely so the host-lane gap stays visible: this
  # clause and H1 are symmetric in FORM (both a test-resolved LEFT `-ef` a non-relaxable
  # literal RIGHT, same reason) but NOT in COVERAGE — /tmp/devloop is the marker base on
  # EVERY lane incl. the host hatch, whereas /home/dev/.cache/... is the ledger base
  # ONLY in-container. That coverage gap is exactly WHY clause (b) exists (it closes the
  # host lane the container literal misses). Do not "simplify" to a bare symmetric claim.
  # `-ef` is device+inode EXACT identity, NOT prefix — a DEVLOOP_TMP at a broad
  # parent does not sweep fixtures beneath it. Own fresh-host argument: an absent
  # literal ⇒ no live run there, so a missing-operand pass is correct; a live run
  # necessarily creates the dir, so the check is present exactly when it can matter.
  __ledger_base_container="/home/dev/.cache/devloop/story-runs"
  if [ -d "$__ledger_base_container" ] && [ "$resolved" -ef "$__ledger_base_container" ]; then
    slogerr "STORY_RUN: SEAM-RUN-DIR-IS-LEDGER-BASE — DEVLOOP_TMP='${resolved}' resolves to the container production ledger base ('${__ledger_base_container}'), so this test's RUN_DIR would land in a live run's host-persistent evidence dir (which survives teardown) and interleave fixture records into it. Redirect DEVLOOP_TMP (mktemp -d)."
    exit 2
  fi
  if [ -n "${HOME:-}" ]; then
    __ledger_base_host="${HOME}/.cache/devloop/story-runs"
    if [ -d "$__ledger_base_host" ] && [ "$resolved" -ef "$__ledger_base_host" ]; then
      slogerr "STORY_RUN: SEAM-RUN-DIR-IS-LEDGER-BASE-HOST — DEVLOOP_TMP='${resolved}' resolves to the host-lane production ledger base ('${__ledger_base_host}', the STORY_RUNNER_ALLOW_HOST base) — same evidence-interleaving hazard as the container case. Redirect DEVLOOP_TMP (mktemp -d)."
      exit 2
    fi
  fi
  case "${resolved}/" in
    "${REAL_REPO_ROOT}"/*)
      slogerr "STORY_RUN: SEAM-RUN-DIR-INSIDE-REAL-REPO — DEVLOOP_TMP='${resolved}' resolves inside the real repository ('${REAL_REPO_ROOT}'), so run artifacts would land in the operator's work tree as untracked files."
      exit 2
      ;;
  esac
  if [ -e "${resolved}/story-runner/.run-in-flight" ]; then
    slogerr "STORY_RUN: SEAM-RUN-DIR-IN-USE — '${resolved}/story-runner/.run-in-flight' already exists, so another runner owns this run dir. This process's EXIT trap would delete that marker and its task artifacts would interleave with that run's evidence. Redirect DEVLOOP_TMP (mktemp -d). If a killed run left this marker stale, remove it deliberately."
    exit 2
  fi
}
if __test_sentinel_active; then __seam_assert_run_dir_isolated; fi

# dt-story binary. SEAM (see the header): a BUILT ARTIFACT that must NOT move
# with the tree, which is why the root redirect cannot cover it.
# Deliberately NOT written as "${DT_STORY:-target/release/dt-story}": that form
# would silently honor an ambient DT_STORY with no sentinel and no fail-loud —
# and this variable selects the binary that executes `complete` and `escalate`
# against the real manifest. The (2) guard above is the only thing between an
# inherited env var and that.
DT_STORY="target/release/dt-story"

# TWO SLUG CLASSES, deliberately different, each named so neither drifts and
# neither gets "helpfully" unified into the other.
#
# SLUG_CLASS_RESUME — the WIDER class. Guards shell command-line
# interpolation of an EPHEMERAL resume value (`--continue=%s`) read off the
# filesystem. Its job is "this cannot present as a flag or split the command
# line", not "this is a well-formed manifest value". Collapsed here from four
# literal copies.
#
# SLUG_CLASS_CANONICAL — the NARROW class, MIRRORING the Rust
# `manifest::SLUG_PATTERN`, which is the source of truth. Applied only where
# a slug is about to enter the durable manifest.
#
# For WHICH literals are pinned together and why the resume class is excluded,
# see the SCOPE comment in scripts/guards/simple/validate-slug-class-sync.sh —
# that guard is the one home for the enumeration. Restating it here would give
# the coverage list two homes, which is the failure this pair guards against.
#
# ANCHOR CONVENTION DIVERGES between these two, deliberately and unavoidably:
# SLUG_CLASS_CANONICAL EMBEDS `^...$` (it is compared byte-for-byte against the
# Rust literal, which embeds them), while SLUG_CLASS_RESUME OMITS them and each
# use site supplies its own (it is interpolated into larger patterns). Check
# which you are using before adding a third use site.
readonly SLUG_CLASS_RESUME='[0-9A-Za-z._-]+'
readonly SLUG_CLASS_CANONICAL='^[a-z0-9]+(-[a-z0-9]+)*$'
if [ -n "$__seam_dt_story" ]; then DT_STORY="$__seam_dt_story"; fi

# (6) Enter the (possibly redirected) root. Nothing above this line touches the
# working tree.
cd "$REPO_ROOT"

# Seam-active announcement, beside the substrate-bound line below: a seam-active
# artifact tree must never be readable as production evidence. Deliberately
# AFTER the containment checks — a run dying in (4)/(5) exits under its own
# distinct token, and announcing a seam the runner just refused would be worse
# than announcing nothing.
if __test_sentinel_active && __any_seam_override_present; then
  slog "STORY_RUN: TEST SEAMS ACTIVE — repo_root=${REPO_ROOT} dt_story=${DT_STORY} run_dir_base=${DEVLOOP_TMP:-<unset>} (NOT a production run)"
fi

ARG="${1:?usage: run-story.sh <story-file.md | story-slug> [--stop-after=N] [--revalidate | --restart <text> | --finish | --interactive]}"
shift
# Flag grammar (order-independent after the story arg). Extra/unknown argv is
# REFUSED, not silently dropped — a flag that silently does nothing is the same
# failure class R-5 names. --revalidate / --restart are the operator-intervention
# retry flags (see the usage header); they slot into the SAME accepted grammar as
# --stop-after and compose with it.
STOP_AFTER=""
SAW_STOP_AFTER=0   # PRESENCE, tracked separately from the value: `--stop-after=`
                   # strips to empty, and testing `-n` on the STRIPPED value would
                   # let an empty value skip validation and silently do nothing
                   # (the R-5 failure class). Validate on presence, not on `-n`.
REVALIDATE=0
RESTART=0
RESTART_TEXT=""
FINISH=0        # D6/f: model-free commit-intent finisher (reviewed-but-uncommitted)
INTERACTIVE=0   # D6/g: attach claude to the operator TTY for the escalated task
while [ "$#" -gt 0 ]; do
  case "$1" in
    --stop-after=*)
      # A repeated flag silently last-wins, which is the same "flag that does
      # nothing" class this grammar refuses extra argv for. Refuse the duplicate.
      [ "$SAW_STOP_AFTER" -eq 0 ] || { slogerr "STORY_RUN: DUPLICATE-FLAG — --stop-after given more than once; a silently-last-wins flag is the R-5 failure class. Pass it once."; exit 2; }
      SAW_STOP_AFTER=1; STOP_AFTER="${1#--stop-after=}" ;;
    --revalidate)
      [ "$REVALIDATE" -eq 0 ] || { slogerr "STORY_RUN: DUPLICATE-FLAG — --revalidate given more than once."; exit 2; }
      REVALIDATE=1 ;;
    # --restart takes its diagnosis text as the NEXT argv token (never inline on
    # this flag's own token beyond the =form), so an empty/missing text is
    # detectable and refused below rather than silently interpolated. The next
    # token must NOT look like a flag: consuming a following `--revalidate` /
    # `--stop-after=` as the diagnosis text would bypass the mutual-exclusion and
    # stop-after handling (S-3). A diagnosis that genuinely starts with `--` goes
    # through the =form.
    --restart)
      [ "$RESTART" -eq 0 ] || { slogerr "STORY_RUN: DUPLICATE-FLAG — --restart given more than once."; exit 2; }
      RESTART=1
      if [ "$#" -ge 2 ] && [ "${2#--}" = "$2" ]; then
        RESTART_TEXT="$2"; shift
      elif [ "$#" -ge 2 ]; then
        slogerr "STORY_RUN: RESTART-FLAG-AS-TEXT — --restart was followed by '$2', which is a flag, not a diagnosis; consuming it as the text would bypass the mutual-exclusion / --stop-after checks. If the diagnosis genuinely starts with '--', pass it as --restart=<text>."
        exit 2
      else
        RESTART_TEXT=""
      fi ;;
    --restart=*)
      [ "$RESTART" -eq 0 ] || { slogerr "STORY_RUN: DUPLICATE-FLAG — --restart given more than once."; exit 2; }
      RESTART=1; RESTART_TEXT="${1#--restart=}" ;;
    # --finish / --interactive: booleans, no text (unlike --restart), so no
    # next-token handling. Duplicate-flag guard per R-5. Both are operator-
    # intervention retry flags in the same family and join the mutual-exclusion +
    # RETRY_APPLIED machinery below.
    --finish)
      [ "$FINISH" -eq 0 ] || { slogerr "STORY_RUN: DUPLICATE-FLAG — --finish given more than once."; exit 2; }
      FINISH=1 ;;
    --interactive)
      [ "$INTERACTIVE" -eq 0 ] || { slogerr "STORY_RUN: DUPLICATE-FLAG — --interactive given more than once."; exit 2; }
      INTERACTIVE=1 ;;
    *) slogerr "STORY_RUN: UNKNOWN-ARGUMENT '$1' — usage: run-story.sh <story-file.md | story-slug> [--stop-after=N] [--revalidate | --restart <text> | --finish | --interactive]"; exit 2 ;;
  esac
  shift
done
if [ "$SAW_STOP_AFTER" -eq 1 ]; then
  [[ "$STOP_AFTER" =~ ^[0-9]+$ ]] || { slogerr "STORY_RUN: --stop-after needs a task id"; exit 2; }
  # Normalize: the comparison at the stop check is a STRING compare, so "01"
  # would pass a numeric lookup here and then never match id "1".
  STOP_AFTER="$((10#$STOP_AFTER))"
fi
# Retry-flag misuse, each a distinct STORY_RUN token + exit 2. These are the
# up-front (argv-only) refusals; NO-ESCALATED-TASK and NO-COMMIT-REFUSED need the
# manifest and fire inside the loop.
# The specific --revalidate+--restart pairing keeps its own tailored message (its
# guidance names what each does), checked FIRST so that combo hits it rather than the
# generic one. Every other multi-flag combo (any involving --finish/--interactive)
# hits the general count>1 check below — one check, not six pairwise ifs (DRY C4).
if [ "$REVALIDATE" -eq 1 ] && [ "$RESTART" -eq 1 ]; then
  slogerr "STORY_RUN: REVALIDATE-WITH-RESTART — --revalidate and --restart are mutually exclusive: the first re-runs only the gate, the second spawns a fresh devloop. Pick one."
  exit 2
fi
if [ "$((REVALIDATE + RESTART + FINISH + INTERACTIVE))" -gt 1 ]; then
  __retry_set=""
  [ "$REVALIDATE" -eq 1 ] && __retry_set="${__retry_set} --revalidate"
  [ "$RESTART" -eq 1 ] && __retry_set="${__retry_set} --restart"
  [ "$FINISH" -eq 1 ] && __retry_set="${__retry_set} --finish"
  [ "$INTERACTIVE" -eq 1 ] && __retry_set="${__retry_set} --interactive"
  slogerr "STORY_RUN: MULTIPLE-RETRY-FLAGS — the operator-intervention retry flags are mutually exclusive; given:${__retry_set}. Each is a distinct retry on the first-reached escalated task — pick one."
  exit 2
fi
if [ "$RESTART" -eq 1 ]; then
  if [ -z "$RESTART_TEXT" ]; then
    slogerr "STORY_RUN: RESTART-EMPTY-TEXT — --restart requires a non-empty diagnosis text (it is spliced into the fresh devloop's prompt so it starts from the diagnosis)."
    exit 2
  fi
  # FOURTH prompt-splice site (alongside the specialist token, the --continue
  # slug and the prompt file). SAME discipline: validate BEFORE interpolation,
  # and the value reaches the model ONLY via the on-disk prompt file, never the
  # claude command line (mirrors the R-2 defect-5 out-of-band fix). A code-fence
  # line would break the prompt/manifest contract the file is read under, so it
  # is refused loudly rather than normalised.
  if printf '%s' "$RESTART_TEXT" | grep -qF '```'; then
    slogerr "STORY_RUN: RESTART-UNSAFE-TEXT — the --restart text contains a code fence (\`\`\`), which would break the prompt/manifest contract the devloop reads the prompt file under. Remove it; the text is a diagnosis paragraph, not a code block."
    exit 2
  fi
fi
if [ -f "$ARG" ]; then
  STORY_FILE="$ARG"
else
  # Slug form: suffix match against docs/user-stories/, mirroring /close-story.
  shopt -s nullglob
  matches=(docs/user-stories/*"${ARG}".md)
  shopt -u nullglob
  case "${#matches[@]}" in
    0) slogerr "STORY_RUN: no story file matches '${ARG}'"; exit 2 ;;
    1) STORY_FILE="${matches[0]}" ;;
    *) slogerr "STORY_RUN: ambiguous slug '${ARG}' matches: ${matches[*]}"; exit 2 ;;
  esac
fi
# (DT_STORY is resolved in the seam block at the top of this file — a built
# artifact that must not move with the tree. Do not re-assign it here.)
# RUN_DIR — per-STORY, host-persistent ledger/evidence base (ADR-0037 D5), via
# resolve_run_base. NOT the same base as RUN_BASE/INFLIGHT below: those are the
# per-CONTAINER marker on ${DEVLOOP_TMP:-/tmp/devloop} and MUST stay there (the
# two-key decomposition — do not "unify" the two). The shared knob is DEVLOOP_TMP:
# when set (always, under DEVLOOP_TEST) both bases collapse onto it and the seam
# guards cover RUN_DIR; when unset (production) RUN_DIR defaults to the persistent
# mount while the marker stays ephemeral. resolve_run_base carries the precedence
# rule and why DEVLOOP_TMP must dominate.
RUN_DIR="$(resolve_run_base run-dir)/$(basename "${STORY_FILE%.md}")"

# RUN_DIR character floor. UNCONDITIONAL, not sentinel-gated: the hazard exists
# in production. RUN_DIR is composed from DEVLOOP_TMP / DEVLOOP_STORY_RUN_BASE /
# $HOME (env vars with no character constraint) and `basename "${STORY_FILE%.md}"`,
# which on the `-f
# "$ARG"` branch above is OPERATOR-SUPPLIED ARGV — so
# `run-story.sh '/path/my"story.md'` puts a double quote into it. That path is
# the one piece of variable content inside the quoted `/devloop "..."` argument
# below, so without this floor the R-2 defect-5 fix would still have an
# unconstrained byte source reaching the command line, and its stated guarantee
# ("no unconstrained byte reaches the quoted argument") would be false.
#
# PRICED, because this NEWLY REFUSES RUNS THAT SUCCEED TODAY. The class excludes
# a space, so `DEVLOOP_TMP=/home/some user/tmp` or a story file named
# `my story.md` hard-fails here having worked before this commit. That is the
# intended trade — the alternative is an operator-supplied byte splicing into a
# model-facing instruction, which is R-2 defect 5 with a different source — but
# it is a behaviour change in a refusal lane, and the next person to trip on it
# will be tempted to widen the class to "just allow spaces". Widening it
# reopens the defect for every character added. Redirect DEVLOOP_TMP or rename
# the story file instead.
if ! [[ "$RUN_DIR" =~ ^[A-Za-z0-9._/-]+$ ]]; then
  slogerr "STORY_RUN: UNSAFE-RUN-DIR — the resolved run dir '${RUN_DIR}' contains characters outside [A-Za-z0-9._/-] (a space is the common case). This path is referenced inside the devloop instruction, so an unconstrained byte here changes what the task is told to do. NOTE this refuses some paths that worked before this check existed — that is deliberate; the offending byte may come from DEVLOOP_TMP, DEVLOOP_STORY_RUN_BASE, or \$HOME (on the STORY_RUNNER_ALLOW_HOST host lane) — redirect to a space-free path, or rename the story file. Do NOT widen the character class."
  exit 2
fi

# Persistence positive control (ADR-0037 D5 / O-3). A plain writability check is
# VACUOUS for D5's goal: it passes on a dir that is writable but NOT the
# host-persistent mount (a failed helper launch, or a container created before this
# change via devloop.sh's already-running short-circuit — no mount was added), in
# which case the ledger silently evaporates on destroy, the exact story-1 failure D5
# exists to fix. devloop.sh writes .ledger-mount host-side into the mount root before
# `podman run`, so its presence PROVES the bind happened.
#
# GATED on the IN-CONTAINER production lane: DEVLOOP_TMP unset (so not a test / not a
# DEVLOOP_TMP redirect) AND STORY_RUNNER_ALLOW_HOST != 1 (code-reviewer F1). The
# host-hatch lane (STORY_RUNNER_ALLOW_HOST=1, no container) legitimately resolves the
# run dir to ${HOME}/.cache/devloop/story-runs, which is the operator's REAL home —
# inherently durable, no container to mount a .ledger-mount into — so asserting the
# marker there is wrong (the writable-but-not-mount property and the will-it-survive
# property diverge exactly on that lane) and would refuse a supported lane with a
# nonsensical "devloop.sh --recreate" (pre-D5 that lane ran, resolving to /tmp/devloop).
# ALLOW_HOST is the right discriminator: set only on the host lane, never in-container,
# so it separates "host lane, skip" from "container mount failed, still catch" — where
# gating on the carrier would wrongly skip a pre-D5 image that leaves the carrier unset.
# The marker lives at the mount ROOT, one level above the /story-runner segment.
if [ -z "${DEVLOOP_TMP:-}" ] && [ "${STORY_RUNNER_ALLOW_HOST:-}" != "1" ]; then
  __ledger_mount_root="$(dirname "$(resolve_run_base run-dir)")"
  if [ ! -f "${__ledger_mount_root}/.ledger-mount" ]; then
    slogerr "STORY_RUN: LEDGER-NOT-PERSISTENT — the run-dir base '${__ledger_mount_root}' has no .ledger-mount marker, so it is NOT the host-persistent bind mount (writable, but the ledger would die with the container — the failure D5 exists to fix). This container predates the D5 mount or the mount failed. Recover on the host: infra/devloop/devloop.sh --recreate ${DEVLOOP_SLUG:-<devloop-slug>}. NOT falling back to /tmp."
    exit 2
  fi
fi

# Create-and-writable, loud on failure — NEVER a silent /tmp fallback (D5). A bare
# `mkdir -p` under `set -e` would abort with no STORY_RUN token; this names the path
# and the host-side recovery.
if ! mkdir -p "$RUN_DIR" 2>/dev/null || [ ! -w "$RUN_DIR" ]; then
  slogerr "STORY_RUN: RUN-DIR-UNWRITABLE — could not create or write the run dir '${RUN_DIR}' (host-persistent ledger, ADR-0037 D5). A podman-created mount root can be root-owned under --userns=keep-id. Fix perms on ${__ledger_mount_root:-the mount}, or recover on the host: infra/devloop/devloop.sh --recreate ${DEVLOOP_SLUG:-<devloop-slug>}. NOT falling back to /tmp — that lost story-1's ledgers."
  exit 2
fi

# Announce the resolved run dir — ONE unconditional line, three consumers: the test
# harness greps it and ASSERTS it equals its own RUN_DIR_OF (an agreement check, so a
# drift between the runner's resolution and the harness's restatement goes red — see
# O2; the harness restates rather than derives, and the single agreement assertion
# suffices because every case shares $DT + the fixed slug); an operator sees where the
# ledger landed (a redirected/wrong base is then visible in the log, not silent); and
# `source=` is the precedence-branch discriminator. Emitted AFTER the writability
# check succeeds (a usable dir exists to
# point at) — genuinely ABSENT on the UNSAFE-RUN-DIR / RUN-DIR-UNWRITABLE / LEDGER-
# NOT-PERSISTENT refusal lanes. VOCABULARY IS `STORY_RUN:`, NOT `STATUS=`/`REASON=`:
# a STATUS= line would cast a spurious vote in Layer-3's verdict via
# tee_collect_statuses (docs/runbooks/devloop-validation.md §6.3) AND break the
# harness read-back. Do not change the token or the leading path field.
if [ -n "${DEVLOOP_TMP:-}" ]; then __run_dir_source="DEVLOOP_TMP"
elif [ -n "${DEVLOOP_STORY_RUN_BASE:-}" ]; then __run_dir_source="DEVLOOP_STORY_RUN_BASE"
else __run_dir_source="default"; fi
slog "STORY_RUN: RUN-DIR ${RUN_DIR} source=${__run_dir_source}"

# --- --stop-after validated against the MANIFEST, not as a bare integer (R-5) --
# A number that names no task, or names one the story never reaches, was
# silently ignored and the runner continued to the end. A flag that silently
# does nothing is the same failure class as a gate that passes without running.
#
# Validated HERE — before preflight and before the loop — so a typo dies at
# second zero rather than after a full story. The id set comes from `dt-story
# list-tasks` (a read-only projection of the already-parsed manifest), never a
# bash-side YAML parse: a second parser would drift the moment the schema moves.
#
# Any non-zero from list-tasks means "this binary cannot answer", NOT
# "id not found". Preflight has not run at this point, so the rc space here is
# 0 ok / 2 stale-or-malformed / 127 not built / 126 not executable, and clap's
# unrecognized-subcommand exit collides with dt-story's own EXIT_MALFORMED —
# so the message names all three causes and hands over the one command that
# separates them rather than guessing.
if [ -n "$STOP_AFTER" ]; then
  if ! stop_tasks_json="$("$DT_STORY" list-tasks "$STORY_FILE" 2>/dev/null)" \
     || [ -z "$stop_tasks_json" ]; then
    slogerr "STORY_RUN: STOP-AFTER-UNVERIFIABLE — '${DT_STORY} list-tasks' could not answer, so --stop-after cannot be validated. Causes: dt-story not built, a stale binary predating the 'list-tasks' verb, or an unparseable manifest. To tell them apart run: ${DT_STORY} validate ${STORY_FILE} — green means the binary is stale (cargo build --release -p dt-guard -p dt-story), red means the manifest is."
    exit 2
  fi
  # Branch on jq's rc for the same reason as stop_blockers below: empty is
  # indistinguishable from "no task with that id", so discarding the rc would
  # send an operator after a typo in a correct flag. Narrow but precise —
  # list-tasks has already exited 0 with non-empty stdout, and its contract says
  # rc 0 carries a JSON array, so a failure here means the binary violated its
  # own contract.
  if ! stop_status="$(jq -r --argjson id "$STOP_AFTER" \
      '.[] | select(.id == $id) | .status' <<<"$stop_tasks_json" 2>&1)"; then
    slogerr "STORY_RUN: STOP-AFTER-UNVERIFIABLE — could not read task status from '${DT_STORY} list-tasks' output while validating --stop-after=${STOP_AFTER}: ${stop_status}. The binary exited 0 with output that is not the JSON array its contract declares; rebuild with cargo build --release -p dt-guard -p dt-story and investigate."
    exit 2
  fi
  if [ -z "$stop_status" ]; then
    slogerr "STORY_RUN: STOP-AFTER-UNKNOWN-TASK — --stop-after=${STOP_AFTER} names no task in ${STORY_FILE}. Task ids present: $(jq -r 'map(.id | tostring) | sort | join(", ")' <<<"$stop_tasks_json" 2>/dev/null)"
    exit 2
  fi
  case "$stop_status" in
    pending|escalated) : ;;
    *)
      slogerr "STORY_RUN: STOP-AFTER-UNREACHABLE-TASK — --stop-after=${STOP_AFTER} names a task already '${stop_status}', so this run will never reach it and the flag would silently do nothing. Ids the run can still reach: $(jq -r '[.[] | select(.status == "pending" or .status == "escalated") | .id | tostring] | join(", ")' <<<"$stop_tasks_json" 2>/dev/null)"
      exit 2
      ;;
  esac
  # DEPS ARE NOT AN UP-FRONT BLOCKER. This previously refused any task with a
  # dep that was not yet `completed` — but a PENDING dep is not a blocker, it is
  # work THIS RUN DOES FIRST. Measured against this story's own manifest, that
  # made `--stop-after=3` and `--stop-after=4` both refuse with "this run will
  # never reach it", a sentence that is false: the run executes task 1, then 4,
  # and stops exactly where asked.
  #
  # That inverted R-5. The requirement is "halts when told to, and refuses
  # loudly when it CANNOT"; refusing when it CAN is the more damaging direction,
  # because staging a run around a human step is the flag's entire purpose and
  # the message actively misinforms. It is also the wrong-message class this
  # change repairs three times elsewhere — introduced here, in the fix for R-5.
  #
  # After `dt-story validate` there is no up-front dep state that makes a
  # pending/escalated task unreachable: cycles and unknown dep ids are already
  # rejected, so every remaining dep is one this run will attempt. The genuine
  # failure (a dep escalates, so the run exits before reaching the stop task) is
  # a RUNTIME outcome with its own lane via escalate() — not something an
  # up-front check can predict, and not something it should pretend to.
  #
  # The dep relationship is still worth SEEING when staging a run; only the
  # refusal was wrong. Informational only, never a refusal.
  stop_deps="$(jq -r --argjson id "$STOP_AFTER" \
      '[ (.[] | select(.id == $id) | .deps[]) | tostring ] | join(", ")' \
      <<<"$stop_tasks_json" 2>/dev/null || true)"
  if [ -n "$stop_deps" ]; then
    slog "STORY_RUN: --stop-after=${STOP_AFTER} depends on task(s) ${stop_deps}, which this run will execute first."
  fi
  STOP_AFTER_FIRED=0
fi

# Per-devloop wall-clock ceiling. A wedged headless session must not hold the
# story forever; timeout is an infra escalation, not a task verdict.
# (Applies per claude invocation — session-limit sleeps don't consume it.)
TASK_TIMEOUT="${STORY_TASK_TIMEOUT:-14400}"

# Session-limit waits per task before falling through to escalation.
SESSION_LIMIT_RETRIES="${STORY_SESSION_LIMIT_RETRIES:-2}"

# Model for devloop sessions (Lead + teammates inherit). Default Opus: the
# ~200-devloop baseline ran on Opus-class models (comparability), the gate
# structure catches implementation mistakes regardless, and quota windows —
# not model capability — are the binding constraint on story throughput.
#
# The default is the ALIAS `opus`, never a generation-pinned id. A pinned id
# rots in two directions the runner cannot detect: it silently executes a
# weaker model than intended across a whole unattended story, or it hard-fails
# every task once the id is retired. Same form as the canary's `--model haiku`
# below. Per-seat tiering (ADR-0035 §F) is a separate decision and needs a
# per-teammate override in the devloop skill, not a change to this default.
STORY_MODEL="${STORY_MODEL:-opus}"

scripts/workflow/preflight-story.sh "$STORY_FILE"

# Runner-scoped, per-invocation (security 2026-08-10): the unlimited print-mode
# background wait is a RUNNER dependency, not an operator preference, so it is
# exported here rather than living in global config. Preflight still writes it to
# settings.json in-container (ephemeral $HOME); on the host-hatch path this export
# is the only carrier, which is why it must not be conditional.
export CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0

# Substrate version binding. preflight probes ONCE per invocation, but the CLI can
# move underneath a run: infra/devloop/devloop.sh updates it on every attach,
# against the RUNNING container (the attach path is also the credential-recovery
# path, so it cannot simply be blocked — see devloop.sh). The probe validates a
# version; detecting that the validated version CHANGED is just `claude --version`,
# so assert it per task and fail loud at the boundary rather than silently running
# tasks on unprobed code.
cli_version() { claude --version 2>/dev/null | head -n 1; }
STORY_CLI_VERSION="$(cli_version)"
slog "STORY_RUN: substrate bound to claude ${STORY_CLI_VERSION:-unknown}"

# In-flight marker: lets a host-side attach see that a run owns this container and
# skip its CLI update. Removed on every exit path, including escalation.
#
# PER-CONTAINER, ephemeral — the MARKER base (resolve_run_base marker), NOT the
# ledger base. It stays on ${DEVLOOP_TMP:-/tmp/devloop}: this is the exact path
# devloop.sh:~715 reads (INFLIGHT_MARKER) to skip the mid-run CLI update, so moving
# it silently breaks that detection (the CLI updates under a live run and the story
# later dies on SUBSTRATE-CHANGED, wrong cause). Deliberately a DIFFERENT base than
# RUN_DIR in production (two-key decomposition); do not unify them. validate-run-dir-
# path-sync.sh statically binds this composition to devloop.sh's INFLIGHT_MARKER.
RUN_BASE="$(resolve_run_base marker)"
INFLIGHT="$RUN_BASE/.run-in-flight"
mkdir -p "$RUN_BASE"
printf 'story=%s pid=%s started=%s\n' \
  "$(basename "${STORY_FILE%.md}")" "$$" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$INFLIGHT"
trap 'rm -f "$INFLIGHT"' EXIT

# Concurrent-same-story guard (ADR-0037 D5 / ops). The ledger mount is FLAT/shared,
# so two containers running the SAME story share one RUN_DIR — which the per-container
# in-flight marker above CANNOT see (it lives on the per-slug /tmp/devloop). Without
# this, their cost-ledger.jsonl interleaves (a wrong story total) and
# latest_escalation_record could select a foreign attempt's baseline (breaking the
# S-10 HEAD binding --finish/--revalidate rest on). Write a per-story owner stamp and
# refuse if a DIFFERENT live owner holds it. The stamp records the container slug so
# the ledger's single-owner invariant (see report_task_cost) is checkable after the
# fact. A stale stamp from a KILLED run (its EXIT trap could not fire) is removable
# deliberately — same discipline as SEAM-RUN-DIR-IN-USE at the seam block; never
# block recovery forever on a stamp nobody owns.
OWNER_FILE="$RUN_DIR/.owner"
__owner_self="slug=${DEVLOOP_SLUG:-<none>} pid=$$ started=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
if [ -f "$OWNER_FILE" ]; then
  __owner_prev_slug="$(sed -n 's/^slug=\([^ ]*\).*/\1/p' "$OWNER_FILE" 2>/dev/null || true)"
  if [ "${__owner_prev_slug:-}" != "${DEVLOOP_SLUG:-<none>}" ]; then
    slogerr "STORY_RUN: STORY-DIR-IN-USE — the run dir '${RUN_DIR}' is owned by a different devloop ('$(cat "$OWNER_FILE" 2>/dev/null || echo unknown)'); another container is running this SAME story on the shared ledger mount. Their evidence + cost ledger would interleave. Run this story in ONE container at a time. If that owner is a KILLED run (no live process), remove ${OWNER_FILE} deliberately and rerun."
    exit 2
  fi
fi
printf '%s\n' "$__owner_self" >"$OWNER_FILE"

# Best-effort cost telemetry: sum every result event in the task's log (all
# attempts, including cross-invocation resumes, append to the same file).
# Telemetry only — never routes control flow; classification uses the canary.
# LEDGER INVARIANT (ADR-0037 D5, stated ONCE — honored here, in append_lane_cost,
# and in the story-close rollup). An entry with no `kind` or kind="devloop" is a
# cumulative SUPERSET of this task's attempts within this story FOR A SINGLE LIVE
# OWNER — report_task_cost greps the whole accumulated tasklog, so later entries
# subsume earlier ones (that is why the rollup takes the last devloop entry). Two
# concurrent runs over one story would produce accumulations that are supersets of
# each other in NEITHER direction — which is exactly what the .owner guard prevents.
# An entry with any other `kind` is a per-lane SINGLETON that never supersedes a
# devloop entry. Unmeasured lanes carry cost fields null+measured:false (never 0).
# `kind` is present on EVERY entry so the rollup can partition without null-handling.
# append_unavailable_cost <id> <reason> — persist a per-lane singleton marking a task
# whose devloop cost was LOST but recoverably known-lost (obs F1). Without it the loud
# COST-UNAVAILABLE line vanishes with the console and the persistent rollup reads the
# task as free with unmeasured_tasks=0 — affirmatively asserting nothing was lost. As
# a measured:false entry the rollup's unmeasured branch counts it, so the headline
# STORY-COST carries unmeasured_tasks>=1. NOT for the append-failed path (it can't
# self-record by definition — that stays loud-line-only).
append_unavailable_cost() {
  local id="$1" reason="$2" entry
  entry="$(jq -n -c --argjson task "$id" --arg reason "$reason" \
    '{task:$task, kind:"unavailable", measured:false, reason:$reason,
      usd:null, output_tokens:null, cache_read_tokens:null, turns:null, api_minutes:null}' 2>/dev/null || true)"
  [ -n "$entry" ] && echo "$entry" >>"$RUN_DIR/cost-ledger.jsonl" || true
}

report_task_cost() {
  local id="$1" summary
  if [ ! -s "$RUN_DIR/task-${id}.devloop.log" ]; then
    # No session log to derive from. On the devloop path this means no result event
    # was captured — loud, not silent (O6): a persistent ledger makes "task N was
    # free" and "we lost task N's cost" indistinguishable otherwise. Also persist an
    # `unavailable` entry so the loss survives into the rollup (obs F1).
    slogerr "STORY_RUN: COST-UNAVAILABLE task=${id} reason=no-session-log — no ${RUN_DIR}/task-${id}.devloop.log to derive cost from; the ledger gains no devloop entry for this task."
    append_unavailable_cost "$id" "no-session-log"
    return 0
  fi
  # `tier` (ADR-0037 D2 / obs O3) is the loop global floored ONCE at task
  # selection (read adjacent to specialist), NOT re-read from the manifest here —
  # the manifest is mutable after the run, and two reads would drift; the D2
  # trial's tier-vs-cost conclusion rests on this field. On the FRESH lane it is
  # the exact tier interpolated onto the /devloop line (what the task ran at). On
  # the --continue / RESTART_FROM_TREE resume lane no --tier is passed and the
  # resumed devloop derives its gate shape from main.md's Tier row, so this
  # records the manifest's CURRENT tier — which the resumed session is expected
  # to match via that row, and which diverges only if the manifest was hand-edited
  # between the interrupted attempt and the resume. Provenance, same reading as the
  # Loop State Tier row. `set -u`-safe: bound before any report_task_cost call
  # site in this loop iteration.
  summary="$( (grep -h '"type":"result"' "$RUN_DIR/task-${id}.devloop.log" 2>/dev/null || true) \
    | jq -s -c --argjson task "$id" --arg tier "$tier" 'select(length > 0) | {
        task: $task, kind: "devloop", tier: $tier, measured: true, attempts: length,
        usd: (map(.total_cost_usd // 0) | add * 100 | round / 100),
        output_tokens: (map(.usage.output_tokens // 0) | add),
        cache_read_tokens: (map(.usage.cache_read_input_tokens // 0) | add),
        turns: (map(.num_turns // 0) | add),
        api_minutes: ((map(.duration_api_ms // 0) | add) / 60000 | round)
      }' 2>/dev/null)" || true
  if [ -z "$summary" ]; then
    slogerr "STORY_RUN: COST-UNAVAILABLE task=${id} reason=no-result-events — the session log has no parseable \"type\":\"result\" event (truncated log, jq failure, or a session that emitted none); no devloop cost entry written."
    append_unavailable_cost "$id" "no-result-events"
    return 0
  fi
  if ! echo "$summary" >>"$RUN_DIR/cost-ledger.jsonl"; then
    slogerr "STORY_RUN: COST-UNAVAILABLE task=${id} reason=append-failed — could not append the devloop cost entry to ${RUN_DIR}/cost-ledger.jsonl (disk/permissions)."
    return 0
  fi
  slog "STORY_RUN: COST $(jq -r '"task=\(.task) kind=\(.kind) attempts=\(.attempts) usd=\(.usd) output_tokens=\(.output_tokens) cache_read_tokens=\(.cache_read_tokens) turns=\(.turns) api_minutes=\(.api_minutes)"' <<<"$summary")"
}

# append_lane_cost <id> <kind> [wall_secs] — explicit ledger entry for a lane that
# ran NO model session (revalidate-gate-only, finish, interactive), so
# report_task_cost has nothing to derive and would WRONGLY re-grep a prior attempt's
# stale tasklog (the O2a hazard). Per the invariant above, these are per-lane
# singletons that never supersede a devloop entry.
#   revalidate-gate-only / finish — model-free, TRUE zero cost (measured:true).
#   interactive — real but UNMEASURED cost (usd:null + measured:false, NEVER 0;
#     wall_clock_seconds is the one magnitude that survives without a session log).
append_lane_cost() {
  local id="$1" kind="$2" wall="${3:-0}" entry
  # -c: one compact object per line — the ledger is JSONL, and the story-close
  # rollup + tests read it line-oriented.
  if [ "$kind" = "interactive" ]; then
    entry="$(jq -n -c --argjson task "$id" --argjson wall "${wall:-0}" \
      '{task:$task, kind:"interactive", measured:false, cost_source:"attached-session-no-stream-json",
        usd:null, output_tokens:null, cache_read_tokens:null, turns:null, api_minutes:null,
        wall_clock_seconds:$wall}' 2>/dev/null || true)"
  else
    entry="$(jq -n -c --argjson task "$id" --arg kind "$kind" \
      '{task:$task, kind:$kind, measured:true, attempts:1,
        usd:0, output_tokens:0, cache_read_tokens:0, turns:0, api_minutes:0}' 2>/dev/null || true)"
  fi
  if [ -z "$entry" ] || ! echo "$entry" >>"$RUN_DIR/cost-ledger.jsonl"; then
    slogerr "STORY_RUN: COST-UNAVAILABLE task=${id} kind=${kind} reason=append-failed — could not write the lane cost entry."
    return 0
  fi
  if [ "$kind" = "interactive" ]; then
    slog "STORY_RUN: COST task=${id} kind=interactive measured=false wall_clock_seconds=${wall:-0} (attached session; cost unmeasured, NOT zero)"
  else
    slog "STORY_RUN: COST task=${id} kind=${kind} usd=0 measured=true (no devloop session)"
  fi
}

escalate() {
  local id="$1" reason="$2" log="$3" rec
  # Cost (O2a): the interactive lane has no stream-json tasklog, so report_task_cost
  # would either find nothing or re-grep a PRIOR attempt's stale log and attribute it
  # here — a false number, worse than a gap. Record it explicitly as unmeasured.
  if [ "${INTERACTIVE_SPAWN:-0}" -eq 1 ]; then
    append_lane_cost "$id" interactive "${INTERACTIVE_WALL_SECS:-0}"
  else
    report_task_cost "$id"
  fi
  # Per-task, per-attempt record. A single shared escalation.json was overwritten
  # by each later escalation: Run #1 escalated tasks 61/64/66 (ledger attempts=2)
  # but only ONE runner record survived, so the escalation history — the evidence
  # you most want, since an escalated task never commits and therefore leaves
  # nothing in docs/devloop-outputs — was destroyed by the next escalation.
  # escalation.json is kept as a stable "latest" pointer (documented at :19).
  rec="$RUN_DIR/task-${id}.runner-escalation.$(date -u +%Y%m%dT%H%M%SZ).json"
  "$DT_STORY" escalate "$STORY_FILE" "$id" --reason "$reason" --log "$log" \
    --out "$rec"
  cp -f "$rec" "$RUN_DIR/escalation.json"
  # Persist THIS attempt's baseline (HEAD before the devloop ran) beside the
  # record. The diagnosed-retry flags read it to (a) recover the prior attempt's
  # output-doc slug via the SAME --diff-filter=A commit-range derivation the
  # completion path uses, and (b) confirm the attempt actually COMMITTED
  # (baseline != HEAD), which — with the reason — is the POSITIVE evidence the
  # no-commit refusal requires. A missing/empty sidecar is NOT read as
  # "committed": the retry flags refuse (NO-COMMIT-EVIDENCE) when it is absent,
  # so a failed `|| true` write fails closed, never open.
  printf '%s' "${head_before:-}" > "$RUN_DIR/task-${id}.head-before" || true
  slogerr "STORY_RUN: ESCALATED task=${id} reason=${reason} log=${log}"
  slogerr "STORY_RUN: escalation record: ${rec} (latest also at ${RUN_DIR}/escalation.json)"
  # Diagnosed-retry guidance. A COMMITTED attempt can be retried without
  # re-implementing; a devloop-no-commit attempt committed nothing, so neither
  # flag applies to it. Name the two flags here rather than a bare "rerun".
  if [ "$reason" = "devloop-no-commit" ]; then
    slogerr "STORY_RUN: recover: this attempt committed nothing to retry from — fix the task and rerun the runner (the diagnosed-retry flags --revalidate/--restart require a committed attempt)."
  else
    slogerr "STORY_RUN: recover after diagnosing: rerun with --revalidate (re-run the gate ONLY, for an environmental failure) OR --restart 'what to fix' (fresh devloop from THIS commit). A plain rerun re-implements from scratch."
  fi
  # Auto-suggest the interactive lane after the 2nd+ escalation of THIS task (count
  # the per-attempt records this escalate() writes). A SUGGESTION only — never an
  # auto-switch into a lane that needs a human who may not be present (that is the
  # operator's call, and --interactive refuses without a TTY).
  local __esc_count
  __esc_count="$(ls -1 "$RUN_DIR"/task-"${id}".runner-escalation.*.json 2>/dev/null | grep -c . || true)"
  if [ "${__esc_count:-0}" -ge 2 ]; then
    slogerr "STORY_RUN: SUGGEST task=${id} has now escalated ${__esc_count} times — consider re-running with --interactive to drive it attached to your TTY (the Lead asks you in-session instead of escalating), rather than another headless attempt."
  fi
  exit 1
}

# Durable incident record for the lanes that deliberately DO NOT touch the
# manifest (auth-expired, infra, and any future ceiling lane). escalate() is not
# usable there — it marks the task escalated, and "manifest untouched" is the
# whole contract of those lanes — so they previously exited leaving nothing but a
# console line. Run #1's auth-expired tasks (61, 66) left no record at all, and
# their canary was overwritten by the next attempt because the canary path is
# fixed per task. This writes a per-attempt record and preserves the canary that
# drove the classification. Never reads or writes the manifest.
# LANE VOCABULARY (closed set). A `lane` value names WHAT BROKE and therefore
# what the operator does next. Nothing else declares this domain, so add a row
# here before inventing a seventh spelling — the precedent is canary_classify's
# own enum comment below.
#   auth-expired            OAuth credentials rejected  -> devloop.sh --refresh-creds on the host
#   infra                   canary produced nothing parseable -> check API reachability, rerun
#   session-limit-exhausted quota window outlasted the retries -> rerun after the reset
#   devloop-timeout         headless session exceeded TASK_TIMEOUT -> investigate/raise the ceiling
#   pipeline-precondition   a gate layer reported operator-class -> fix the environment
#   git-error               git itself failed -> read the captured stderr; fix the repo/disk/lock
# (NB `devloop-timeout` also exists as a pre-change `escalate` REASON. Different
# record types and filenames, so the two never collide in one artifact.)
#
# Args: $1=task id  $2=lane (above)  $3=detail (fixed strings + ids + paths ONLY —
#       never gate output, model prose or CLI stderr)  $4=log path (optional)
record_infra_incident() {
  local id="$1" lane="$2" detail="$3" log="${4:-}" ts rec canary canary_err
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  rec="$RUN_DIR/task-${id}.infra-incident.${ts}.json"
  canary="$RUN_DIR/task-${id}.canary.${ts}.json"
  if [ -f "$RUN_DIR/task-${id}.canary.json" ]; then
    cp -f "$RUN_DIR/task-${id}.canary.json" "$canary"
  else
    canary=""
  fi
  # The canary's stderr is preserved per-attempt for the same reason the canary
  # itself is: the fixed per-task path is overwritten by the next attempt, and
  # when a canary is unclassifiable its stderr is the only evidence of why.
  canary_err="$RUN_DIR/task-${id}.canary.err.${ts}"
  if [ -f "$RUN_DIR/task-${id}.canary.err" ]; then
    cp -f "$RUN_DIR/task-${id}.canary.err" "$canary_err"
  else
    canary_err=""
  fi
  jq -n --argjson task "$id" --arg lane "$lane" --arg detail "$detail" \
        --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg canary "$canary" \
        --arg canary_err "$canary_err" --arg log "$log" \
        --arg cli "$STORY_CLI_VERSION" \
    '{task:$task, lane:$lane, at:$at, detail:$detail, cli:$cli,
      canary:(if $canary == "" then null else $canary end),
      canary_stderr:(if $canary_err == "" then null else $canary_err end),
      log:(if $log == "" then null else $log end)}' \
    >"$rec" || {
    # `if !`-guarded, NOT bare. Removing the old `|| true` was right — it masked
    # the write failure — but a BARE `jq >"$rec"` under `set -e` aborts the whole
    # script with jq's rc before the check below can run, so the lane exits
    # through no lane at all and the loud message never prints. Found by driving
    # this branch: the fix for a masked failure had made the report unreachable.
    slogerr "STORY_RUN: INCIDENT-RECORD-UNWRITABLE — jq failed writing the incident record at ${rec} (lane=${lane}). The lane exit below still stands; the durable evidence for it does NOT exist. Investigate disk/permissions on ${RUN_DIR}."
    return 0
  }
  # NOT `|| true`. For every lane above the manifest is untouched and nothing
  # lands in docs/devloop-outputs/, so this record is the ENTIRE durable trace.
  # The redirect creates the file before jq runs, so a jq failure leaves a
  # ZERO-BYTE record whose path is announced below as though it were evidence —
  # recreating the exact Run #1 loss this function was written to fix, and worse
  # than no record, because the operator's first move is spent on it.
  if [ ! -s "$rec" ] || ! jq -e . <"$rec" >/dev/null 2>&1; then
    slogerr "STORY_RUN: INCIDENT-RECORD-UNWRITABLE — failed to write a parseable incident record at ${rec} (lane=${lane}). The lane exit below still stands; the durable evidence for it does NOT exist. Investigate disk/permissions on ${RUN_DIR}."
    return 0
  fi
  slogerr "STORY_RUN: incident record: ${rec}"
  return 0
}

# Canary probe: a minimal haiku request whose single-object JSON output is
# classifiable by field access. Consulted only to explain a devloop failure —
# when the environment is broken (quota, auth, network) the canary fails the
# same way, and classifying its 2-line output replaces signature-grepping the
# devloop's stream log (which misclassified two incidents: attempt-boundary
# contamination 2026-08-04, result line buried by teammate wind-down events
# 2026-08-05).
canary_probe() {
  local out="$1"
  # stderr goes to its OWN file, never merged into $out. `>"$out" 2>&1` merged
  # them, so a single warning line on stderr made the JSON this file's stdout is
  # parsed as unreadable, and EVERY failure classified as `infra`. Redirected
  # rather than discarded: when a canary is unclassifiable that stderr is the
  # only evidence of why, so 2>/dev/null would make the infra lane less
  # debuggable than the bug it replaces. $RUN_DIR is under DEVLOOP_TMP, outside
  # the work tree, so `git add` and the amend below cannot sweep it.
  #
  # --allowedTools "" MUST STAY LAST: the option is variadic
  # (`--allowedTools <tools...>`), so a flag appended after it in the natural
  # place is swallowed as its value — which would silently drop
  # --dangerously-skip-permissions and reintroduce the permission-prompt hang
  # that ADR-0035 §6 keeps that flag to prevent, misclassified here as a timeout
  # and then as `infra`. Measured parse-safe on claude 2.1.229 (rc 0, empty
  # stderr, well-formed single-object JSON, permission_denials []).
  timeout 120 claude -p "Reply with exactly: ok" --model haiku \
    --output-format json --dangerously-skip-permissions \
    --allowedTools "" >"$out" 2>"${out%.json}.err"
}

# Classify a canary output file: healthy | session-limit | auth-expired | infra
canary_classify() {
  local out="$1" is_err status text lower
  # (1) Not a JSON object -> infra. This is now the ONLY infra lane, and the
  # narrowing is honest: if the API returned a well-formed error object then the
  # API is REACHABLE, so `infra` ("unreachable or unclassifiable") is wrong for
  # it. Covers non-JSON, truncated, and EMPTY output (a probe killed by its own
  # `timeout 120` leaves zero bytes).
  #
  # This narrowing is sound ONLY BECAUSE canary_probe no longer merges stderr
  # into $out. Before that fix, one warning line from a perfectly healthy API
  # produced exactly the input this lane now reads as "the substrate produced
  # nothing" — so anyone who "simplifies" the probe's redirect back re-widens
  # this lane silently.
  # `[ -s ]` FIRST and separately: `jq -e` returns 0 on EMPTY input (measured:
  # `printf '' | jq -e 'type=="object"'` -> 0), so the type assertion alone is
  # vacuous against a probe that produced nothing — exactly the case a
  # `timeout 120` kill leaves behind.
  [ -s "$out" ] || { echo infra; return 0; }
  jq -e 'type == "object"' "$out" >/dev/null 2>&1 || { echo infra; return 0; }
  is_err="$(jq -r '.is_error // false' "$out" 2>/dev/null)" || { echo infra; return 0; }
  # NOTE null-vs-absent: these classify IDENTICALLY and deliberately — both mean
  # "no status field to classify on". Production emits present-with-null. Both
  # shapes are pinned by fixtures so the equivalence is asserted rather than left
  # to jq's `//` collapsing them by accident.
  status="$(jq -r 'if (.api_error_status == null) then "" else (.api_error_status | tostring) end' "$out" 2>/dev/null)" || true
  text="$(jq -r '[.result?, .error?, .message?] | map(select(type == "string")) | join(" ")' "$out" 2>/dev/null)" || true
  lower="$(printf '%s' "$text" | tr '[:upper:]' '[:lower:]')"

  if [ "$is_err" = "false" ] && [ -n "$text" ]; then echo healthy; return 0; fi

  # (2) Auth, structured first then prose. Case-INsensitive and multi-phrase:
  # the previous single case-sensitive literal was the defect.
  case "$status" in 401|403) echo auth-expired; return 0 ;; esac
  case "$lower" in
    *authenticat*|*oauth*|*credential*|*"api key"*|*unauthorized*|*"please log in"*)
      echo auth-expired; return 0 ;;
  esac

  # (3) Explicit quota signal.
  [ "$status" = "429" ] && { echo session-limit; return 0; }

  # (4) DEFAULT: anything else that PARSED as an error object is retryable
  # (ADR-0035 §6 inversion). The old code grepped `.result` case-sensitively for
  # the literal 'session limit', while the session-limit lane below already
  # encodes the real message shape as 'resets [0-9]+:[0-9]+[ap]m' — so
  # "5-hour limit reached . resets 3:00pm" matched the latter and NOT the
  # former, fell through to `infra`, and the story STOPPED INSTEAD OF SLEEPING.
  # Sleeping unnecessarily costs a retry slot; dying unnecessarily costs the
  # story. Safe only because retry exhaustion now routes to the infra lane
  # rather than escalating the task.
  echo session-limit
}

# Advisory-remediation gate (runs when all tracked tasks are done, before the
# story-close gate). Ambient advisories accumulate independent of the story's
# diff, so the first-ever close can go audit-red on unrelated CVEs. Rather than
# hard-fail (breaking the "come back to done" promise), auto-append a scoped
# remediation devloop per red language and let the loop run it.
#
# Language-agnostic: iterates scripts/lang/*/audit.sh (never hardcodes rust/ts),
# forces the scan (the normal gate is dep-change-gated), and fails LOUDLY if a
# language's audit is red but declares no remediation owner.

# Set AUDIT_RED to newline-separated red languages, forcing every scan (the
# normal per-lang gate is dep-change-gated). Called directly (NOT in $()) so a
# `return 2` propagates. Per-audit.sh exit contract: 0 = OK/N-A/skipped (clean),
# 1 = FAIL (advisory), >=2 = precondition/unknown (infra, not a fixable advisory).
AUDIT_RED=""
audit_scan() {
    AUDIT_RED=""
    local sh lang rc
    for sh in scripts/lang/*/audit.sh; do
        [ -f "$sh" ] || continue
        lang="$(basename "$(dirname "$sh")")"
        set +e
        DEVLOOP_AUDIT_FORCE_RUN=1 "$sh" >"$RUN_DIR/audit-${lang}.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq 1 ]; then
            AUDIT_RED+="${lang}"$'\n'
        elif [ "$rc" -ge 2 ]; then
            slogerr "STORY_RUN: audit for ${lang} exited ${rc} (precondition/unknown, not an advisory) — see $RUN_DIR/audit-${lang}.log"
            return 2
        fi
    done
    return 0
}

# Echo a language's declared remediation owner (audit-remediation.md line-1
# `# owner: <specialist>`), or return 2 (loud) if the file/header is missing —
# the "audit red for a language we don't understand" fail-loud case. Call via
# `owner="$(audit_owner_for X)" || exit 2` so the return propagates.
audit_owner_for() {
    local lang="$1" file owner
    file="scripts/lang/${lang}/audit-remediation.md"
    if [ ! -f "$file" ]; then
        slogerr "STORY_RUN: audit red for '${lang}' but ${file} is missing — declare its remediation owner + prompt, or fix the advisory by hand"
        return 2
    fi
    owner="$(sed -n '1s/^# owner: *//p' "$file")"
    if [ -z "$owner" ]; then
        slogerr "STORY_RUN: ${file} line 1 must be '# owner: <specialist>' — got: $(head -n1 "$file")"
        return 2
    fi
    printf '%s' "$owner"
}

# Persist the --continue pointer for the current task if its devloop created
# an output dir and nothing was committed. Uses the per-task vars set in the
# main loop. Safe to call on any exit path; no-op if already persisted.
# --- Fail-open git reads (R-2 defect 4, mechanism M2) -------------------------
# A conditional comparing a command substitution against a known value fails
# OPEN: when the command errors the substitution is empty, the comparison takes
# the "not equal" branch, and the runner proceeds as though it had learned
# something. Four sites had this shape, plus one that aborted under `set -e`
# with no lane at all, plus the same mechanism in devloop-stop-hook.sh.
#
# __drop_if_empty — remove a capture file a SUCCESSFUL call left empty.
#
# `2>"$errfile"` creates the file whether or not git fails, so without this every
# healthy task left a zero-byte `task-N.git-error.<ts>.log` in the evidence dir —
# one per task, timestamped, indistinguishable at a glance from a real incident.
# That inverts the property the rest of this change is built on: not a missing
# record, a FALSE one, and it degrades the genuine artifact by making the name
# routine background noise.
#
# The `/dev/null` guard is load-bearing, not defensive: both helpers below
# DEFAULT errfile to /dev/null, so an unguarded `rm -f` here would delete
# /dev/null for any caller that omitted the argument.
__drop_if_empty() {
  local f="${1:-}"
  [ -n "$f" ] || return 0
  [ "$f" != /dev/null ] || return 0
  if [ -e "$f" ] && [ ! -s "$f" ]; then rm -f "$f"; fi
  return 0
}

# git_head — echo HEAD's sha, or return non-zero with git's stderr captured.
# Never conflate "git failed" with "HEAD moved": that reading marks an
# uncommitted task COMPLETE, a false green produced by an environment fault.
# Args: $1=path to write git's stderr to (optional)
git_head() {
  local errfile="${1:-/dev/null}" sha
  if ! sha="$(git rev-parse --verify HEAD 2>"$errfile")"; then
    return 1
  fi
  __drop_if_empty "$errfile"
  printf '%s' "$sha"
}

# tree_dirty — 0 dirty, 1 clean, 2 GIT ERROR. `git diff --quiet` is tri-state
# (0 clean / 1 differences / >1 error) and the old conditional read every
# non-zero as "dirty", so a git fault exited under a message naming the wrong
# cause. Callers MUST use `rc=0; tree_dirty || rc=$?` — a bare call would abort
# under `set -e` on the clean branch.
# Args: $1=path to write git's stderr to (optional)
tree_dirty() {
  local errfile="${1:-/dev/null}" rc
  git diff --quiet 2>"$errfile"; rc=$?
  [ "$rc" -gt 1 ] && return 2
  if [ "$rc" -eq 1 ]; then __drop_if_empty "$errfile"; return 0; fi
  git diff --cached --quiet 2>"$errfile"; rc=$?
  [ "$rc" -gt 1 ] && return 2
  if [ "$rc" -eq 1 ]; then __drop_if_empty "$errfile"; return 0; fi
  __drop_if_empty "$errfile"
  return 1
}

# git_error_lane — record + exit 2. A git failure is unambiguously environment,
# so it routes to the OPERATOR lane; landing it on escalate() would violate R-4
# inside the change that fixes R-4. The captured stderr is the whole diagnosis:
# "not a git repository", an index.lock held by another process, a corrupt
# object store and a safe.directory refusal are four different operator actions
# behind one lane value.
# Args: $1=task id  $2=what failed  $3=captured-stderr path
git_error_lane() {
  local id="$1" what="$2" errfile="$3"
  record_infra_incident "$id" git-error \
    "git failed while ${what}; refusing to infer repository state from a failed read" \
    "$errfile"
  slogerr "STORY_RUN: GIT-ERROR task=${id} — git failed while ${what}. The runner will NOT guess whether the task committed. Manifest untouched; fix the repository (see ${errfile}) and rerun."
  exit 2
}

# newest_devloop_output — slug of the most recent docs/devloop-outputs/ dir
# created since $start_marker, or empty.
#
# Exists because `find ... | sort -rn | head -n 1 | cut ...` inside a `d="$(...)"`
# assignment is fatal under `set -euo pipefail` when docs/devloop-outputs is
# ABSENT: find exits non-zero, pipefail propagates it, and the assignment kills
# the runner with exit 1, no message and no lane — while :19-20 documents exit 1
# as "task escalated". The directory is legitimately absent after the
# session-limit lane's own `git clean -fdq` removes it (git does not track empty
# directories), so this is reachable, not theoretical. Found by the test suite.
#
# Absence is a normal state meaning "no devloop has produced output yet", so it
# is answered, not raised — but it must be answered EXPLICITLY rather than by a
# masked pipeline failure.
newest_devloop_output() {
  local marker="$1" raw
  # Absence is a NORMAL state ("no devloop has produced output yet"), so it is
  # ANSWERED (empty result), not raised. Callers already treat empty as
  # "start fresh". It must be answered explicitly rather than arriving as a
  # masked pipeline failure, which is what killed the runner.
  [ -d docs/devloop-outputs ] || return 0
  # find's rc is captured BEFORE the sort|head pipeline, deliberately: a genuine
  # enumeration failure (permissions, I/O) is LOUD, while `head -n 1` closing
  # the pipe on `sort` is legitimate and must not be read as an error. A single
  # `|| true` over the whole pipeline would conflate the two and silently lose a
  # resume pointer; splitting them keeps the SIGPIPE tolerance without buying
  # blanket masking with it.
  #
  # Form matters here. This captures find into a variable and checks its rc
  # BEFORE any pipe, rather than reading ${PIPESTATUS[0]} (correct only if
  # captured on the very next line — any intervening test or assignment clobbers
  # it) or inferring rc from a $( ) wrapping the whole pipeline (NOT correct
  # under pipefail: that re-conflates find's failure with head's pipe-close,
  # which is exactly the distinction above).
  #
  # `2>&1` folds find's stderr into $raw so the WARN can quote it, which means on
  # the exit-0 path a stray stderr line can reach the pipeline. GNU find emits
  # some warnings to stderr and still exits 0, so this is not only a failure-path
  # concern; the live risk is a future find adding one, at which point the
  # behaviour changes on every run at once — the same substrate-moves-underneath
  # hazard this script already treats as first-class for the CLI version.
  #
  # The grep below is a SHAPE FLOOR: only `%T@ %f` lines survive, so diagnostic
  # text cannot enter the sort and therefore cannot win the selection.
  #
  # Measured, because the reasoning is subtler than it looks and the wrong
  # conclusion is reassuring: `sort -rn` coerces a leading non-numeric to 0, so a
  # typical `find: warning: …` line LOSES to a real epoch-prefixed entry and the
  # correct dir is still selected. The floor is not defending against that case.
  # It defends against a stderr line that happens to START WITH A LARGER NUMBER,
  # which would win `head -n 1` and DISCARD the genuine directory — and the
  # downstream `[ -f docs/devloop-outputs/<slug>/main.md ]` gate would then
  # convert that into a SILENT "start fresh", losing a resume pointer with no
  # WARN. That gate makes the outcome SAFE, not VISIBLE, and the enumeration
  # branch above deliberately makes the identical consequence loud. With the
  # floor the gate goes back to being defence in depth rather than load-bearing.
  if ! raw="$(find docs/devloop-outputs -mindepth 1 -maxdepth 1 -type d \
        -newer "$marker" -printf '%T@ %f\n' 2>&1)"; then
    slogerr "STORY_RUN: WARN could not enumerate docs/devloop-outputs (${raw}) — treating as no resumable devloop output, so this task will start FRESH rather than resuming. If an interrupted devloop exists, its resume pointer is being lost; investigate before rerunning."
    return 0
  fi
  [ -n "$raw" ] || return 0
  # Surface anything the shape floor drops. The comment above identifies the
  # exposure (a future find warning on the exit-0 path changes behaviour on every
  # run at once) and silence would leave that invisible — while the enumeration
  # branch three lines up is deliberately loud about the identical consequence.
  # This script binds and re-asserts the CLI version per task precisely so a
  # substrate change cannot move underneath it quietly; this is the same event.
  local shaped dropped
  shaped="$(printf '%s\n' "$raw" | grep -E '^[0-9]+(\.[0-9]+)? ' || true)"
  dropped="$(printf '%s\n' "$raw" | grep -cvE '^[0-9]+(\.[0-9]+)? ' || true)"
  if [ "${dropped:-0}" -gt 0 ]; then
    slogerr "STORY_RUN: WARN find emitted ${dropped} non-'%T@ %f' line(s) while enumerating docs/devloop-outputs and they were discarded before resume selection. Selection is unaffected, but a find that warns on every invocation would change resume behaviour repo-wide: $(printf '%s\n' "$raw" | grep -vE '^[0-9]+(\.[0-9]+)? ' | head -n 3 | tr '\n' ' ')"
  fi
  # SLUG FLOOR (third splice site, second producer). The selected name reaches
  # `--continue=%s`, and the value read from $slug_file is already validated
  # against this exact class on the other producer path — so without this, the
  # SAME name is a hard exit 2 across invocations and silently interpolated
  # within one, which is the shape that survives review. Applied HERE rather
  # than at the two call sites so both get it by construction; validating only
  # the session-limit caller would leave persist_resume_pointer writing an
  # unvalidated value into $slug_file for the next run to reject.
  #
  # These directories are created by devloops — i.e. by models — the same trust
  # level as the manifest prompts R-2 defect 5 keeps off the command line.
  #
  # Filter-and-WARN rather than exit: symmetric with the enumeration branch
  # above, which also declines to kill the run over a resume-selection problem.
  # A malformed directory name is not necessarily hostile, but it must never be
  # interpolated, and losing a resume pointer must not be silent.
  local safe unsafe
  safe="$(printf '%s\n' "$shaped" | grep -E "^[0-9]+(\.[0-9]+)? ${SLUG_CLASS_RESUME}$" || true)"
  unsafe="$(printf '%s\n' "$shaped" | grep -cvE "^[0-9]+(\.[0-9]+)? ${SLUG_CLASS_RESUME}$" || true)"
  if [ "${unsafe:-0}" -gt 0 ] && [ -n "$shaped" ]; then
    slogerr "STORY_RUN: WARN ${unsafe} docs/devloop-outputs entry name(s) are outside ${SLUG_CLASS_RESUME} and were excluded from resume selection — they would be interpolated into the devloop's --continue flag. If one of them is this task's interrupted devloop, its resume pointer is being lost; rename the directory. Offending: $(printf '%s\n' "$shaped" | grep -vE "^[0-9]+(\.[0-9]+)? ${SLUG_CLASS_RESUME}$" | head -n 3 | cut -d' ' -f2- | tr '\n' ' ')"
  fi
  printf '%s\n' "$safe" | sort -rn | head -n 1 | cut -d' ' -f2- || true
}

persist_resume_pointer() {
  [ -s "$slug_file" ] && return 0
  local now_head d
  # DELIBERATE EXCEPTION to git_error_lane. This runs INSIDE the auth-expired
  # and infra arms, BEFORE they write their own incident records — exiting here
  # on a git fault would destroy the precise classification the canary just paid
  # for (auth-expired, whose recovery is `devloop.sh --refresh-creds`) on the
  # one path where that record is the only durable trace. So: fail LOUD and
  # RETURN, letting the caller's lane finish and record. Do not "fix the
  # inconsistency" by routing this to the operator lane.
  if ! now_head="$(git_head)"; then
    slogerr "STORY_RUN: WARN task-resume pointer could not be checked — 'git rev-parse HEAD' failed. Persisting the pointer anyway (a stale pointer costs one resume attempt; a missing one loses the interrupted devloop)."
  elif [ "$now_head" != "$head_before" ]; then
    return 0
  fi
  d="$(newest_devloop_output "$start_marker")"
  if [ -n "$d" ] && [ -f "docs/devloop-outputs/${d}/main.md" ]; then
    echo "$d" >"$slug_file"
  fi
  return 0
}

# latest_escalation_record <id> — path of the newest runner-escalation record for
# this task, or empty. escalate() names each record with an embedded UTC
# timestamp (task-N.runner-escalation.<ts>.json), so a lexical sort orders them
# by time; the diagnosed-retry flags read the newest one's `reason` to decide
# whether the task the run reached was escalated at all, and whether its prior
# attempt committed.
latest_escalation_record() {
  local id="$1"
  # `|| true`: with no matching record `ls` exits non-zero, and under
  # `set -euo pipefail` that would abort the caller's `esc_rec="$( … )"`
  # assignment before the NO-ESCALATED-TASK lane can report it. "No record" is a
  # normal, answered state here (empty stdout), not a failure.
  ls -1 "$RUN_DIR"/task-"${id}".runner-escalation.*.json 2>/dev/null | sort | tail -n 1 || true
}

# run_gate <id> — the AUTHORITATIVE per-task gate: layers 1-6 then layer 7,
# appended to $gatelog. Factored so the normal completion path and --revalidate
# run the IDENTICAL invocation (ADR-0035 §3 — pass/fail authority is this gate,
# never the devloop's verdict). Sets the globals gate_rc / gate_layer /
# gate_layers that the caller's operator-vs-implementer split reads.
run_gate() {
  local id="$1" n rc gate_start
  # EVERY task runs layers 1-6 then layer 7 (2026-08-20 — layer 7 is now
  # UNCONDITIONAL, no longer gated on an env_tests tag; that manifest field was
  # removed). The `1-6+7` label is kept deliberately: layer 7 is a separate
  # ~10-15min cost envelope, so the `+7` preserves that distinction — it no
  # longer signals conditionality.
  gate_layers="1-6+7"
  slog "STORY_RUN: GATE task=${id} layers=${gate_layers} running (log=${gatelog})"
  gate_start="$(date +%s)"
  gate_rc=0
  gate_layer=0
  for n in 1 2 3 4 5 6; do
    set +e
    "scripts/layer${n}.sh" >>"$gatelog" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then gate_rc=$rc; gate_layer=$n; break; fi
  done
  if [ "$gate_rc" -eq 0 ]; then
    set +e
    scripts/layer7.sh >>"$gatelog" 2>&1
    gate_rc=$?
    set -e
    [ "$gate_rc" -ne 0 ] && gate_layer=7
  fi
  slog "STORY_RUN: GATE task=${id} layers=${gate_layers} rc=${gate_rc} elapsed=$(( $(date +%s) - gate_start ))s"
}

# run_full_gate <logfile> — the AUTHORITATIVE FULL-pipeline run in ONE pass via
# layer-all.sh, appended to <logfile>; returns its rc. The ONE home of this
# invocation (DRY F2): the pass/fail authority (ADR-0035 §3) had two callers — the
# --finish lane and the story-close gate — exactly the shape run_gate exists to
# collapse. layer-all.sh (NOT the per-layer run_gate) is REQUIRED where a commit
# follows, because its EXIT trap is the ONLY producer of the Gate-2 verdict the
# pre-commit hook reads at ${DEVLOOP_TMP}/gate2-verdict. DEVLOOP_FAIL_FAST=0 forces
# run-all: this runs from the runner's own shell where DEVLOOP_HEADLESS is unset, so
# without it layer-all.sh would classify the run interactive and fail-fast. Callers
# capture the rc under set -e via `rc=0; run_full_gate "$log" || rc=$?`, and
# pre-truncate the log themselves if they want fresh output (finish appends to the
# task gate log; story-close truncates its own).
run_full_gate() {
  local log="$1" rc
  set +e
  DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh >>"$log" 2>&1
  rc=$?
  set -e
  return "$rc"
}

# complete_task <id> <baseline-head> [cost_mode] — slug resolution + dt-story
# complete + manifest bump + cleanup + suppression-visibility note. Factored so
# every completion path shares ONE implementation of the closed-set slug derivation
# and the amend-or-chore commit; the differences are the commit-range baseline (the
# devloop's head_before on the normal, --interactive AND --finish paths; only
# --revalidate reads the prior attempt's persisted `task-N.head-before` sidecar) and
# the cost_mode. cost_mode selects
# the ledger entry (see the invariant at report_task_cost):
#   devloop (default)     — derive cost from the session log via report_task_cost;
#   gate-only             — --revalidate: model-free true-zero entry;
#   finish                — --finish: model-free true-zero entry;
#   interactive           — --interactive: UNMEASURED entry (usd:null + wall_clock).
# Callers: the normal completion path, --revalidate, --finish, and --interactive.
# Reuses continue_slug / slug_file / start_marker / stop_count_file / gatelog as the
# per-task globals the caller set.
complete_task() {
  local id="$1" baseline="$2" cost_mode="${3:-devloop}"
  local slug_git_err slug_derive_ok slug_raw slug_candidates slug_count
  local task_slug slug_src slug_cause slug_rejected added

  # SLUG RESOLUTION — a CLOSED SET of outcomes. Every completion emits exactly
  # one, so "none of them appeared" is itself detectable. Primary derivation is
  # the task's own commit range, NOT a directory mtime. `--diff-filter=A`
  # restricts to output dirs CREATED in this range; `:(glob)` pins `*` to one
  # path segment (git's fnmatch lacks FNM_PATHNAME). Capture git's rc SEPARATELY
  # from the pipeline so a genuine enumeration failure stays distinguishable from
  # "nothing was added" — folding a git fault into `no-record-in-range` would
  # print a message that is affirmatively false. See the story-task-4 comments in
  # the git history for the measured cases behind each clause.
  slug_git_err="$RUN_DIR/task-${id}.slug-derive.err"
  slug_derive_ok=1
  if ! slug_raw="$(git diff --name-only --diff-filter=A "$baseline" HEAD \
        -- ':(glob)docs/devloop-outputs/*/main.md' 2>"$slug_git_err")"; then
    slug_derive_ok=0
  fi
  slug_candidates="$(printf '%s\n' "$slug_raw" | grep . | cut -d/ -f3 | sort -u || true)"
  slug_count="$(printf '%s' "$slug_candidates" | grep -c . || true)"

  # ONE outcome per completion. The arms are MUTUALLY EXCLUSIVE and `slug_cause`
  # is set exactly once, so a reader (or a test) classifying by `cause=` gets a
  # single answer. Keep this a partition — see the git-history comment about the
  # earlier unsafe-class fall-through that emitted a second, contradictory cause.
  task_slug=""; slug_src=""; slug_cause=""; slug_rejected=""
  if [ "$slug_derive_ok" -ne 1 ]; then
    slug_cause="git-error"
  elif [ "${slug_count:-0}" -gt 1 ]; then
    slug_cause="ambiguous"
  elif [ "${slug_count:-0}" -eq 1 ]; then
    task_slug="$slug_candidates"; slug_src="commit-range"
  elif [ -n "$continue_slug" ]; then
    task_slug="$continue_slug"; slug_src="resume-pointer"
  else
    slug_cause="no-record-in-range"
  fi

  # Write-time floor on the NARROW class. Pinned against the canonical Rust class
  # by scripts/guards/simple/validate-slug-class-sync.sh. Applied here so a
  # rejected slug can never reach `complete` and kill the run under `set -e`: the
  # task's work is committed and its gates are green, so erroring would be a
  # self-inflicted operator-lane red of exactly the class R-4 exists to prevent.
  if [ -n "$task_slug" ] && ! [[ "$task_slug" =~ $SLUG_CLASS_CANONICAL ]]; then
    slug_rejected="$task_slug"
    slug_cause="unsafe-class"
    task_slug=""; slug_src=""
  fi

  # Fold the manifest bump into the devloop's own commit. No --commit sha in the
  # manifest: amending changes the sha. Fall back to a separate chore commit if
  # the amend is rejected (e.g. a hook that pins devloop-commit trees).
  if [ -n "$task_slug" ]; then
    "$DT_STORY" complete "$STORY_FILE" "$id" --slug "$task_slug"
  else
    # Answered, not raised — but never silent, and never more than one cause.
    case "$slug_cause" in
      git-error)
        slogerr "STORY_RUN: NO-SLUG task=${id} cause=git-error — enumerating this task's commit range for a devloop output failed; the slug could not be derived at all (this is NOT 'no record exists'). git stderr: ${slug_git_err}. Recording completion without a slug. Repair with: ${DT_STORY} complete ${STORY_FILE} ${id} --slug <slug>" ;;
      ambiguous)
        slogerr "STORY_RUN: NO-SLUG task=${id} cause=ambiguous candidates=${slug_count} ($(printf '%s' "$slug_candidates" | tr '\n' ' ')) — more than one devloop output was CREATED in this task's commit range, so which one produced it is not decidable here. Recording completion without a slug. Repair with: ${DT_STORY} complete ${STORY_FILE} ${id} --slug <slug>" ;;
      unsafe-class)
        slogerr "STORY_RUN: NO-SLUG task=${id} cause=unsafe-class candidate='${slug_rejected}' — outside ${SLUG_CLASS_CANONICAL}, so it would be rejected by dt-story and kill the run at a point where this task's work is already committed. Recording completion without a slug. Repair with: ${DT_STORY} complete ${STORY_FILE} ${id} --slug <slug>" ;;
      no-record-in-range)
        slogerr "STORY_RUN: NO-SLUG task=${id} cause=no-record-in-range — no docs/devloop-outputs/*/main.md was ADDED between ${baseline} and HEAD, AND no resume pointer survived. (This does NOT mean the devloop wrote no record: on a resumed task the record exists and was committed by an earlier run.) Recording completion without a slug. Repair with: ${DT_STORY} complete ${STORY_FILE} ${id} --slug <slug>" ;;
      *)
        # NO CATCH-ALL INTO A SPECIFIC CLAIM — an unrecognised cause makes NO
        # environmental assertion at all (see the git-history comment).
        slogerr "STORY_RUN: NO-SLUG task=${id} cause=unclassified='${slug_cause}' — the slug classifier produced a cause this emitter has no arm for, so no claim is made about why. This is a RUNNER BUG; the completion itself is sound. Repair with: ${DT_STORY} complete ${STORY_FILE} ${id} --slug <slug>" ;;
    esac
    "$DT_STORY" complete "$STORY_FILE" "$id"
  fi
  git add "$STORY_FILE"
  if ! git commit --quiet --amend --no-edit; then
    git commit --quiet -m "chore(story): task #${id} complete (run-story manifest bump)"
  fi
  rm -f "$slug_file" "$start_marker" "$stop_count_file"
  slog "STORY_RUN: COMPLETE task=${id} commit=$(git rev-parse --short HEAD) slug=${task_slug:-none} src=${slug_src:-none} cause=${slug_cause:-none}"

  # Cost ledger. A gate-only (--revalidate), finish, or interactive completion spawned
  # no devloop session for THIS attempt, so report_task_cost would find nothing or
  # (wrongly) re-derive a PRIOR attempt's cost from its leftover log; route through
  # append_lane_cost (per-lane singleton entry) instead. See the ledger invariant.
  case "$cost_mode" in
    gate-only)   append_lane_cost "$id" revalidate-gate-only ;;
    finish)      append_lane_cost "$id" finish ;;
    interactive) append_lane_cost "$id" interactive "${INTERACTIVE_WALL_SECS:-0}" ;;
    *)           report_task_cost "$id" ;;
  esac

  # Suppression-visibility monitor: if this task's commit ADDED audit-suppression
  # entries, surface it — a cleared advisory via suppression (vs a real fix) is a
  # governed but deliberate choice that should be loud, not buried in a diff.
  if git show HEAD --format= --name-only 2>/dev/null | grep -q '^audit-suppressions\.toml$'; then
    added="$(git show HEAD -- audit-suppressions.toml 2>/dev/null | grep -cE '^\+[[:space:]]*id[[:space:]]*=' || true)"
    if [ "${added:-0}" -gt 0 ]; then
      slog "STORY_RUN: NOTE task=${id} added ${added} audit suppression(s) — verify security-reviewed with exposure analysis + expiry"
    fi
  fi
}

# working_tree_status <id> <errfile> <what> — porcelain of the tree EXCLUDING the
# manifest, routing a git fault to the operator lane (with <what> naming the caller's
# context in the git-error message, so each site keeps its own diagnostic). Shared by
# ALL THREE byte-identical sites — --restart's dirty probe, --revalidate's clean-tree
# attestation, and --finish's post-stage remainder check — collapsed here beside
# git_head/tree_dirty (C7).
working_tree_status() {
  local id="$1" errfile="$2" what="${3:-checking the work tree}" out
  if ! out="$(git status --porcelain -- . ":(exclude)$STORY_FILE" 2>"$errfile")"; then
    git_error_lane "$id" "$what" "$errfile"
  fi
  __drop_if_empty "$errfile"
  printf '%s' "$out"
}

# finish_lane <id> — D6/f model-free finisher. Covers the reviewed-but-UNCOMMITTED
# case (the terminal-phase crash after Gate-3 close), the INVERSE of
# --revalidate/--restart (which require a committed prior attempt) — so it does NOT
# run the NO-COMMIT-EVIDENCE/REFUSED checks; its precondition is the commit-intent
# the devloop wrote at Gate-3 close. Fail-closed throughout; every refusal a distinct
# STORY_RUN token that falls back to resume; the index is reset on any post-stage
# refusal so a failed finish doesn't wedge the next run. Reuses complete_task
# (slug/complete/amend/ledger) — the gate is layer-all (NEW-1), not run_gate.
# Uses per-task globals set by the caller (head_before, gatelog, STORY_FILE, RUN_DIR).
finish_lane() {
  local id="$1"
  local intent="$RUN_DIR/task-${id}.commit-intent.json"
  [ -f "$intent" ] || {
    slogerr "STORY_RUN: FINISH-NO-INTENT task=${id} — no commit-intent at ${intent}. Gate-3 never closed with an intent (a terminal-phase crash BEFORE the intent was written, or a pre-feature escalation). Rerun WITHOUT --finish to resume."
    exit 2
  }
  if ! jq -e . "$intent" >/dev/null 2>&1; then
    slogerr "STORY_RUN: FINISH-INTENT-MALFORMED task=${id} — ${intent} is not parseable JSON. Refusing (a malformed intent must not silently commit an empty/partial changeset). Rerun WITHOUT --finish to resume."
    exit 2
  fi
  # Bindings (S-10): story + task + HEAD, else a stale intent could replay onto a
  # different task or a moved tree. NO expiry check — the bindings ARE the control;
  # an age check is a strictly weaker second control whose only reachable effect is
  # a false refusal on a task legitimately picked up weeks later.
  local i_story i_task i_head i_slug i_msg
  i_story="$(jq -r '.story // ""' "$intent")"
  i_task="$(jq -r '.task_id // ""' "$intent")"
  i_head="$(jq -r '.head // ""' "$intent")"
  i_slug="$(jq -r '.slug // ""' "$intent")"
  i_msg="$(jq -r '.message // ""' "$intent")"
  if [ "$i_story" != "$STORY_FILE" ] || [ "$i_task" != "$id" ] || [ "$i_head" != "$head_before" ]; then
    slogerr "STORY_RUN: FINISH-STALE-INTENT task=${id} — the intent binds to story='${i_story}' task='${i_task}' head='${i_head}', but this run is story='${STORY_FILE}' task='${id}' head='${head_before}'. A stale intent must not replay onto a moved tree or a different task. Rerun WITHOUT --finish to resume."
    exit 2
  fi
  [ -n "$i_msg" ] || {
    slogerr "STORY_RUN: FINISH-INTENT-MALFORMED task=${id} — the intent carries no commit message. Refusing. Rerun WITHOUT --finish to resume."
    exit 2
  }
  # Trailer vocabulary (S-9): Approved-Cross-Boundary: is the ADR-0024 §6.7 owner
  # co-sign record — a well-formed trailer naming an uninvolved specialist is a
  # governance bypass, not a formatting nit. Refuse any Approved-Cross-Boundary
  # trailer whose specialist is outside the known set.
  local bad_trailer
  bad_trailer="$(printf '%s\n' "$i_msg" | grep -E '^Approved-Cross-Boundary:' \
    | grep -vE '^Approved-Cross-Boundary: (auth-controller|global-controller|meeting-controller|media-handler|database|protocol|infrastructure|client|security|observability|operations|test|code-reviewer|dry-reviewer|semantic-guard) ' || true)"
  if [ -n "$bad_trailer" ]; then
    slogerr "STORY_RUN: FINISH-BAD-TRAILER task=${id} — the commit-intent message carries an Approved-Cross-Boundary: trailer naming an unknown/uninvolved specialist: $(printf '%s' "$bad_trailer" | head -n1). That trailer is the owner co-sign record; refusing to replay it. Rerun WITHOUT --finish to resume."
    exit 2
  fi
  # File list. Read into an array; validate EACH path at parse time (S-7). git's
  # `--` stops OPTION parsing but NOT pathspec magic, so ':/' / ':(glob)' / '.' each
  # stage the whole tree and pass a naive check — reject them here, before `git add`.
  local files=() f had_output_doc=0
  while IFS= read -r f; do [ -n "$f" ] && files+=("$f"); done < <(jq -r '.files[]? // empty' "$intent")
  [ "${#files[@]}" -gt 0 ] || {
    slogerr "STORY_RUN: FINISH-INTENT-MALFORMED task=${id} — the intent lists no files. Refusing. Rerun WITHOUT --finish to resume."
    exit 2
  }
  for f in "${files[@]}"; do
    case "$f" in
      /*|.|"" ) : "reject" ;;
      :*) : "reject pathspec magic" ;;
      *) if [[ "$f" == *".."* ]] || printf '%s' "$f" | grep -qE '[[:cntrl:]]'; then : "reject"; else
           case "$f" in docs/devloop-outputs/*/main.md) had_output_doc=1 ;; esac
           continue
         fi ;;
    esac
    slogerr "STORY_RUN: FINISH-UNSAFE-PATH task=${id} — intent file '${f}' is absolute, '.', empty, contains '..', a control char, or a git pathspec-magic prefix (':', ':/', ':(...)') — any of which would stage far more than intended. Refusing. Rerun WITHOUT --finish to resume."
    exit 2
  done
  if [ "$had_output_doc" -ne 1 ]; then
    slogerr "STORY_RUN: FINISH-NO-SLUG task=${id} — the intent's files include no docs/devloop-outputs/*/main.md, so complete_task cannot derive the output-doc slug. Refusing (an intent that would complete without a slug is malformed). Rerun WITHOUT --finish to resume."
    exit 2
  fi
  RETRY_APPLIED=1
  # AUTHORITATIVE gate = layer-all.sh (NEW-1), NOT run_gate: the per-layer run_gate
  # emits no Gate-2 verdict, and the `git commit` below stages a devloop-shaped
  # changeset (main.md at Phase=complete), which the pre-commit hook requires a
  # verdict for and blocks fail-closed without. layer-all.sh's EXIT trap produces
  # the verdict at ${DEVLOOP_TMP}/gate2-verdict, binding it to exactly this tree.
  # DEVLOOP_FAIL_FAST=0 mirrors the story-close gate (one-pass full report).
  slog "STORY_RUN: FINISH task=${id} — re-running the authoritative gate (layer-all) with NO model turn (log=${gatelog})"
  local fin_rc=0 fin_start
  fin_start="$(date +%s)"
  run_full_gate "$gatelog" || fin_rc=$?
  slog "STORY_RUN: FINISH task=${id} gate rc=${fin_rc} elapsed=$(( $(date +%s) - fin_start ))s"
  if [ "$fin_rc" -ne 0 ]; then
    tail -n 50 "$gatelog"
    escalate "$id" "finish-pipeline-red" "$gatelog"
  fi
  # RE-DERIVE the index from the intent (security F-1). The SKILL producer stages
  # (`git add -A`) BEFORE it writes the intent and commits, so a terminal-phase crash
  # — the exact case --finish exists for — leaves the index STAGED. A pre-stage
  # "index must be clean" assertion (the old FINISH-INDEX-DIRTY lane) would refuse
  # that headline scenario. Instead do a MIXED reset (worktree UNTOUCHED — the
  # reviewed content stays exactly as reviewed) and rebuild the index solely from the
  # intent's file list below. This satisfies S-8's ACTUAL purpose ("a staged change
  # the intent doesn't list must not ride into the commit") more strongly: a file the
  # operator had staged that the intent omits becomes an unstaged working-tree change.
  #
  # NO pre-stage index precondition here BY DESIGN (ops): with the reset normalising
  # the starting index, the POST-STAGE remainder check below is now the SOLE defense
  # against a pre-staged foreign change (it surfaces as an unstaged remainder → the
  # FINISH-DIRTY-REMAINDER test is therefore load-bearing). Do NOT re-add an index
  # precondition, and do NOT read the absence of one as a gap.
  git reset -q
  # Stage exactly the intent's files, per-file so a path the intent lists but the
  # tree no longer has does NOT abort the lane under `set -e` (`git add` errors on an
  # unmatched pathspec) — a missing file is a MISMATCH, caught by the set-equality
  # check below, not a crash. `--` mandatory (leading-'-' safety); magic pre-rejected.
  local __add_f
  for __add_f in "${files[@]}"; do git add -- "$__add_f" 2>/dev/null || true; done
  # Set equality on the staged PATH set (S-18): missing OR extra both fail.
  # Newline-safe WITHOUT -z ONLY because FINISH-UNSAFE-PATH rejects control chars in
  # files[] (security F-2); if that floor is ever relaxed, switch this AND the --raw
  # comparison below to -z (and the SKILL's capture) together — do not switch one
  # side. Note: --name-only quote-escapes non-ASCII (core.quotePath) while files[] are
  # raw JSON, so a non-ASCII path yields a (fail-closed) FINISH-FILE-MISMATCH, not a
  # clean refusal — acceptable, and another reason the floor stays strict.
  local staged_paths expected_paths
  staged_paths="$(git diff --cached --name-only | sort -u)"
  expected_paths="$(printf '%s\n' "${files[@]}" | sort -u)"
  if [ "$staged_paths" != "$expected_paths" ]; then
    git reset -q
    slogerr "STORY_RUN: FINISH-FILE-MISMATCH task=${id} — the staged path set does not equal the intent's file list (a file changed/added/removed since Gate-3 close). Refusing rather than committing a set the review didn't approve. Likely cause: an edit after Gate-3. EITHER revert the stray edit and rerun --finish (cheap), OR rerun WITHOUT --finish to resume so the edit is re-reviewed (re-hydrates the transcript). expected=[$(printf '%s' "$expected_paths" | tr '\n' ' ')] staged=[$(printf '%s' "$staged_paths" | tr '\n' ' ')]"
    exit 2
  fi
  # Content/status/mode: the staged raw must equal the intent's captured staged raw.
  # Distinct token from the path-set mismatch (different repair — ops O-16).
  local staged_raw intent_raw
  staged_raw="$(git diff --cached --raw --no-abbrev | sort)"
  intent_raw="$(jq -r '.raw // ""' "$intent" | sort)"
  if [ "$staged_raw" != "$intent_raw" ]; then
    git reset -q
    slogerr "STORY_RUN: FINISH-CONTENT-MISMATCH task=${id} — the staged content/mode does not match what Gate-3 reviewed (same file set, changed bytes). Refusing. EITHER revert the post-Gate-3 edit and rerun --finish (cheap), OR rerun WITHOUT --finish to resume for re-review (expensive)."
    exit 2
  fi
  # Post-stage remainder (ops, --revalidate parity): layer-all ran over the WORKING
  # TREE, so a file dirty-but-UNSTAGED outside the intent set was in the green but
  # won't be in the commit — a green not reproducible from the commit. Refuse before
  # committing, in the right place with the right message (NEW-1 would otherwise
  # backstop it with a confusing Gate-2 signature failure).
  local remainder rem_err="$RUN_DIR/task-${id}.finish-remainder.err"
  remainder="$(working_tree_status "$id" "$rem_err" "checking for uncommitted work outside the intent before finishing")"
  # Drop the STAGED-index entries from the remainder; anything left is uncommitted
  # work outside the intent. The staged class is `XY path` with X in [MADRCT] and a
  # blank Y (` `) — `T` (typechange: file↔symlink↔gitlink) included (ops F1: omitting
  # it false-refuses a legitimately-staged typechange). `U` (unmerged) deliberately
  # EXCLUDED — an unmerged path should refuse. Any `X  path` this drops is NECESSARILY
  # an intent file, because the path-set equality check above already proved
  # staged==intent — so this cannot mask a stray staged file (do not "fix" it to match
  # paths instead of status).
  local rem_left
  rem_left="$(printf '%s\n' "$remainder" | grep -vE '^[MADRCT]  ' | grep -v '^$' || true)"
  if [ -n "$rem_left" ]; then
    git reset -q
    slogerr "STORY_RUN: FINISH-DIRTY-REMAINDER task=${id} — the work tree has changes outside the intent's file set (unstaged/untracked), which layer-all validated but this commit would NOT carry — a green not reproducible from the commit (--revalidate's REVALIDATE-DIRTY-TREE reasoning). Commit or clean them, then rerun. Offending: $(printf '%s' "$rem_left" | head -n 5 | tr '\n' ';')"
    exit 2
  fi
  # Commit with the recorded message via -F (S-9: never argv/eval). complete_task
  # then amends the manifest bump onto it (index-vs-HEAD; only $STORY_FILE staged, so
  # gate2_is_devloop_mainmd rejects it and the pre-commit hook no-ops — verified).
  local msg_file="$RUN_DIR/task-${id}.finish-message"
  printf '%s\n' "$i_msg" >"$msg_file"
  git commit -q -F "$msg_file"
  # ACCEPT-PATH receipt (S-20): bounded — counts + verdicts, not per-path enumeration.
  slog "STORY_RUN: FINISH-VERIFIED task=${id} intent=${intent} head=${head_before} paths=${#files[@]} pathset_ok=true content_ok=true committed=$(git rev-parse --short HEAD 2>/dev/null || echo '<unreadable>')"
  complete_task "$id" "$head_before" finish
  slog "STORY_RUN: FINISH task=${id} — committed the reviewed intent with no model turn; task completed."
}

# build_devloop_prompt <mode> <id> — the ONE home of the /devloop prompt (C5), so
# the R-2 defect-5 out-of-band discipline (no manifest byte on the command line; the
# prompt reaches the model only via $prompt_file) lives in one place. The two lanes
# differ ONLY in the "HEADLESS RUN … Headless Mode" prefix (headless sets it so the
# Lead escalates instead of asking; interactive omits it so the Lead asks the human)
# and in the claude flags at the spawn site — NOT in the prompt body. Reads the
# per-task globals continue_slug / prompt_file / specialist / tier the caller set.
build_devloop_prompt() {
  local mode="$1" id="$2" prefix_resume="" prefix_fresh=""
  if [ "$mode" = headless ]; then
    prefix_resume="HEADLESS RUN (run-story task #${id}, resumed): follow the devloop skill including its Headless Mode section.\n"
    prefix_fresh="HEADLESS RUN (run-story task #${id}): follow the devloop skill including its Headless Mode section.\n"
  fi
  if [ -n "$continue_slug" ]; then
    # RESUME lane carries NO --tier BY DESIGN (ADR-0037 D2 / obs O1b): the
    # resumed devloop re-derives its gate shape from the main.md Loop State
    # `Tier` row (which the /devloop SKILL names as the authoritative record of
    # what this attempt did), NOT from a re-passed flag. Threading --tier here
    # would be a second, drifting source; the omission is deliberate — do not
    # "fix" it. (RESTART_FROM_TREE takes this lane too.)
    printf '%b/devloop "This devloop was interrupted before completion. Resume from main.md state: finish incomplete phases, then gates and commit as normal." --continue=%s' \
      "$prefix_resume" "$continue_slug"
  else
    printf '%bThe task description for this devloop is the EXACT, COMPLETE contents of the file %s. Read that file now and treat its full text AS the /devloop prompt argument — do not paraphrase, summarise or truncate it, and do not re-quote it.\n/devloop "See %s — that file'"'"'s exact contents are this devloop'"'"'s task description." --specialist=%s --tier=%s' \
      "$prefix_fresh" "$prompt_file" "$prompt_file" "$specialist" "$tier"
  fi
}

RETRY_APPLIED=0
INTERACTIVE_SPAWN=0       # set when --interactive reaches the escalated task; the
INTERACTIVE_WALL_SECS=0   # spawn site branches on it and records wall-clock for the ledger
while :; do
  set +e
  task_json="$("$DT_STORY" next "$STORY_FILE")"
  next_rc=$?
  set -e
  case "$next_rc" in
    0) ;;
    3) # All tracked tasks done — advisory-remediation gate before story close.
       audit_scan || exit 2
       if [ -z "$AUDIT_RED" ]; then
         break   # every language's forced audit is clean → proceed to close gate
       fi
       # Handle one red language per iteration; the loop re-audits after each
       # remediation devloop, so multiple red languages resolve serially.
       red_lang="$(printf '%s' "$AUDIT_RED" | head -n1)"
       red_owner="$(audit_owner_for "$red_lang")" || exit 2
       set +e
       add_id="$("$DT_STORY" add-task "$STORY_FILE" \
         --specialist "$red_owner" \
         --prompt-file <(tail -n +2 "scripts/lang/${red_lang}/audit-remediation.md") \
         --tag "audit-remediation-${red_lang}")"
       add_rc=$?
       set -e
       case "$add_rc" in
         0) # Commit the manifest append before continuing — otherwise the
            # appended task's fresh-start clean-tree check trips on the
            # runner's own story-file write.
            git add "$STORY_FILE"
            git commit --quiet -m "chore(story): append audit-remediation task #${add_id} (${red_lang} advisory)"
            slog "STORY_RUN: AUDIT-REMEDIATION ${red_lang} red → appended task #${add_id} (owner ${red_owner})"
            continue ;;   # loop picks up the appended remediation task
         4) # A remediation task for this language already exists but the audit
            # is still red — the auto-fix didn't resolve it. Human judgment
            # needed (patch unavailable, or a governed suppression decision).
            slogerr "STORY_RUN: AUDIT-REMEDIATION ${red_lang} still red after remediation task #${add_id} — fix the advisory or add a governed suppression (docs/contributor/audit-suppressions.md), then rerun"
            slogerr "STORY_RUN: audit log: $RUN_DIR/audit-${red_lang}.log"
            exit 1 ;;
         *) slogerr "STORY_RUN: dt-story add-task failed rc=${add_rc}"; exit 2 ;;
       esac
       ;;
    4) slogerr "STORY_RUN: BLOCKED — pending tasks with unsatisfied deps (see dt-story stderr)"; exit 1 ;;
    *) slogerr "STORY_RUN: manifest error (dt-story next rc=${next_rc})"; exit 2 ;;
  esac

  id="$(jq -r .id <<<"$task_json")"
  specialist="$(jq -r .specialist <<<"$task_json")"
  # The specialist is the one manifest value that MUST remain on the command
  # line (it is a flag value, not free text), so it gets a character-class
  # floor. dt-story constrains it only to non-empty-after-trim, so
  # `test --paired-with=x` is an accepted specialist today and would splice as
  # extra flags. Deliberately NOT an `.claude/agents/<name>.md` existence check:
  # under an active STORY_REPO_ROOT that path resolves inside the fixture.
  if ! [[ "$specialist" =~ ^[a-z][a-z0-9-]*$ ]]; then
    slogerr "STORY_RUN: INVALID-SPECIALIST task=${id} — specialist '${specialist}' is not a plain token ([a-z][a-z0-9-]*). It is interpolated as a flag value, so anything else changes what the devloop is told to do."
    exit 2
  fi
  # Read tier ADJACENT to specialist (ADR-0037 D2) — bind it HERE so every
  # downstream site sees it under `set -u`, including report_task_cost on the
  # --finish / --revalidate / escalate lanes that reach it early in this
  # iteration (a later read would be an unbound-variable abort there).
  tier="$(jq -r .tier <<<"$task_json")"
  # TIER FLOOR. This `^(full|light)$` alternation is a DELIBERATE SECOND COPY of
  # manifest::Tier's value set {full,light} — mirroring the specialist floor
  # above — and it earns its keep the way `commit` and `slug` earn their opposite
  # rules in manifest.rs:158-176: different provenance, different job, not an
  # inconsistency to tidy.
  #   (a) It is a second copy of the set the Rust `manifest::Tier` enum owns as
  #       the SSoT.
  #   (b) It does TWO jobs, and its failure IS reachable: command-line
  #       interpolation safety (tier is spliced onto the /devloop line, like
  #       specialist) AND presence/staleness detection. A dt-story predating the
  #       `tier` field emits a `next` payload with no tier key, so `jq -r .tier`
  #       yields the literal string `null`; this floor is what stops `--tier=null`
  #       reaching the /devloop line. A `null` here can ONLY mean a stale binary
  #       (a current dt-story always serializes a concrete tier on the non-Option
  #       RunnableTask field), so it is an environment fault → exit 2, NEVER a
  #       "use default" (that is serde's job upstream and yields "full", never
  #       null). A plain token class ([a-z]+) cannot substitute — it would ACCEPT
  #       `null` — precisely because of this second job.
  #   (c) ADDING A TIER to manifest::Tier requires WIDENING THIS LITERAL too.
  #   (d) A sync guard was considered and DECLINED as disproportionate: drift
  #       fails loud in BOTH directions (Rust widens → this floor rejects the new
  #       value at run time; Rust narrows → the value is unreachable), unlike the
  #       slug class whose drift was silent. security(exit-2) vs ops(full+warn)
  #       was resolved to exit-2 (fail-loud beats masking a stale environment).
  if ! [[ "$tier" =~ ^(full|light)$ ]]; then
    slogerr "STORY_RUN: INVALID-TIER task=${id} — tier '${tier}' is not 'full' or 'light'. A tier of 'null' means a STALE dt-story that predates the tier field (rebuild: cargo build --release -p dt-guard -p dt-story); any other value is a malformed manifest.tier the serde enum manifest::Tier should have rejected. Not coercing to full — that would mask the fault."
    exit 2
  fi
  prompt_file="$RUN_DIR/task-${id}.prompt"
  jq -r .prompt <<<"$task_json" >"$prompt_file"

  tasklog="$RUN_DIR/task-${id}.devloop.log"
  gatelog="$RUN_DIR/task-${id}.gate.log"
  giterr="$RUN_DIR/task-${id}.git-error.$(date -u +%Y%m%dT%H%M%SZ).log"
  if ! head_before="$(git_head "$giterr")"; then
    git_error_lane "$id" "reading HEAD before the devloop started" "$giterr"
  fi
  stop_count_file="$RUN_DIR/task-${id}.stopblocks"
  slug_file="$RUN_DIR/task-${id}.slug"
  start_marker="$RUN_DIR/task-${id}.start"
  rm -f .devloop-escalation.json "$stop_count_file"

  # Resume detection: a persisted slug (written on any uncommitted exit) means
  # this task has an interrupted devloop to continue instead of a fresh start.
  continue_slug=""

  # Per-iteration reset: --interactive/--finish act on the FIRST-reached escalated
  # task ONCE (RETRY_APPLIED gate), so INTERACTIVE_SPAWN must NOT leak into the next
  # task's spawn. Reset here; the retry block re-sets it only for the escalated task.
  INTERACTIVE_SPAWN=0
  INTERACTIVE_WALL_SECS=0

  # --- OPERATOR-INTERVENTION RETRY (--revalidate / --restart) ------------------
  # A run halts at its FIRST escalation, and `dt-story next` reopens an escalated
  # task on selection (status back to pending, escalation cleared, file
  # rewritten) — so the escalated task is exactly the FIRST task this loop
  # reaches, and by here `next` has already reopened it. Both flags therefore act
  # on THIS task, once (RETRY_APPLIED gate): after handling it, subsequent tasks
  # run the normal fresh-devloop path.
  #
  # The manifest no longer shows the escalation (next cleared it), but the
  # runner-escalation record escalate() wrote in a PRIOR run survives in RUN_DIR
  # and is the evidence both flags key on: no record => the task the run reached
  # was never escalated (NO-ESCALATED-TASK); reason == devloop-no-commit (or a
  # persisted baseline equal to HEAD) => the prior attempt committed nothing, so
  # there is nothing to validate or restart from (NO-COMMIT-REFUSED, masked-
  # failure protection). This block sits BEFORE the fresh-start clean-tree check
  # deliberately: next's reopen dirtied the manifest, which that check would
  # otherwise reject.
  RESTART_FROM_TREE=0
  if { [ "$REVALIDATE" -eq 1 ] || [ "$RESTART" -eq 1 ] || [ "$FINISH" -eq 1 ] || [ "$INTERACTIVE" -eq 1 ]; } && [ "$RETRY_APPLIED" -eq 0 ]; then
    esc_rec="$(latest_escalation_record "$id")"
    if [ -z "$esc_rec" ]; then
      slogerr "STORY_RUN: NO-ESCALATED-TASK — the task the run reached (task ${id}) has no prior runner-escalation record in ${RUN_DIR}, so there is nothing for --revalidate/--restart/--finish/--interactive to act on. These flags retry a task an EARLIER run escalated; run the runner without them to make forward progress."
      exit 2
    fi
    esc_reason="$(jq -r '.reason // ""' "$esc_rec" 2>/dev/null || true)"

    # --finish: reviewed-but-UNCOMMITTED, its OWN precondition branch (the commit
    # intent), NOT the committed-attempt evidence checks below. Ends in `continue`
    # (success) or exits/escalates.
    if [ "$FINISH" -eq 1 ]; then
      finish_lane "$id"
      if [ -n "$STOP_AFTER" ] && [ "$id" = "$STOP_AFTER" ]; then
        STOP_AFTER_FIRED=1
        slog "STORY_RUN: STOPPED after task ${id} (--stop-after) — story-close gate NOT run; rerun without the flag to continue"
        exit 0
      fi
      continue
    fi

    # --interactive: mark the spawn and fall through to the normal fresh/resume path.
    # The commit-evidence checks below are REVALIDATE/RESTART-only (interactive cares
    # only about HEAD movement + gate rc, whatever the human did), so they are guarded
    # to skip for interactive. The spawn site branches on INTERACTIVE_SPAWN.
    if [ "$INTERACTIVE" -eq 1 ]; then
      RETRY_APPLIED=1
      INTERACTIVE_SPAWN=1
      # `dt-story next` reopened the escalated task, dirtying the manifest. Commit
      # that reopen so the fresh-start clean-tree check below passes (mirrors
      # --restart's fresh reopen-commit), then re-read head_before as this attempt's
      # baseline. Any resume pointer is preserved — if one exists the attached spawn
      # --continues and the resume path (which tolerates a dirty tree) is taken;
      # otherwise this leaves only the reopen committed and the tree clean for a fresh
      # attached start. (Uncommitted prior implementation work with NO resume pointer
      # still refuses at the clean-tree check — the operator cleans up, as with --restart.)
      git add "$STORY_FILE"
      git commit --quiet -m "chore(story): reopen task #${id} for operator --interactive" || true
      if ! head_before="$(git_head "$giterr")"; then
        git_error_lane "$id" "reading HEAD after committing the interactive reopen" "$giterr"
      fi
      slog "STORY_RUN: INTERACTIVE task=${id} — attaching claude to your TTY (no timeout, Stop hook inert); after you exit, the runner gates/commits/completes or escalates on HEAD movement + gate rc, unchanged."
    fi
    # --- REVALIDATE / RESTART body (committed-attempt lanes) --------------------
    # Guarded so --interactive (which fell through above with INTERACTIVE_SPAWN set)
    # skips it: interactive needs no committed-prior-attempt evidence — it cares only
    # about HEAD movement + gate rc after the human exits. --finish already
    # `continue`d. So only REVALIDATE/RESTART reach this body.
    if [ "$REVALIDATE" -eq 1 ] || [ "$RESTART" -eq 1 ]; then
    # Establish the prior attempt's baseline (HEAD before its devloop ran) from
    # the sidecar escalate() persists. cur_head is HEAD at loop entry, which — on
    # a committed prior attempt that then escalated (e.g. pipeline-red) — is that
    # attempt's own commit, since escalate() adds no commits of its own.
    reval_baseline=""
    bfile="$RUN_DIR/task-${id}.head-before"
    [ -f "$bfile" ] && reval_baseline="$(cat "$bfile" 2>/dev/null || true)"
    cur_head="$head_before"
    # FAIL CLOSED (S-1). The refusal must rest on POSITIVE evidence of a commit,
    # never on the ABSENCE of a no-commit signal. `devloop-no-commit` is not the
    # only reason that can name an uncommitted attempt: devloop-escalated and
    # devloop-session-error escalate BEFORE the head_after==head_before check, so
    # for those the baseline sidecar is the only evidence — and an absent/empty
    # sidecar (a record predating this feature, a `|| true` write that failed) or
    # an unreadable record (esc_reason="") must NOT be read as "committed".
    # So: (1) require a readable reason AND a persisted baseline at all; then
    # (2) refuse on devloop-no-commit or baseline==HEAD.
    if [ -z "$esc_reason" ] || [ -z "$reval_baseline" ]; then
      slogerr "STORY_RUN: NO-COMMIT-EVIDENCE — cannot establish that task ${id}'s prior attempt committed: reason='${esc_reason:-<unreadable>}', baseline sidecar '${bfile}' is $( [ -s "$bfile" ] && echo present-but-empty || echo absent ). Refusing rather than assuming a commit (masked-failure protection). This can happen for an escalation recorded before this feature existed; fix the task and rerun the runner WITHOUT a retry flag. record: ${esc_rec}"
      exit 2
    fi
    # Probe for uncommitted work up front. For --restart a dirty tree ALWAYS
    # selects tree mode: baseline != HEAD does NOT prove the attempt committed —
    # operator commits (a runner fix, a suppression renewal) land between
    # attempts, and treating them as attempt evidence took the fresh path and
    # deleted a live resume pointer (task 24, 2026-09-08). Dirty tree = work
    # that must not be destroyed, whatever the commit history says.
    restart_dirty=""
    if [ "$RESTART" -eq 1 ]; then
      restart_dirty="$(working_tree_status "$id" "$giterr" "checking for uncommitted work before restart")"
      if [ -n "$restart_dirty" ]; then
        RESTART_FROM_TREE=1
      fi
    fi
    if [ "$RESTART_FROM_TREE" -eq 0 ] && { [ "$esc_reason" = "devloop-no-commit" ] || [ "$reval_baseline" = "$cur_head" ]; }; then
      # --restart's real requirement is SOMETHING TO RESTART FROM, and an
      # uncommitted-but-dirty tree is something: a devloop that escalated with
      # its full implementation in the working tree (ADR-0036 story-1 tasks 3,
      # 4, 24 all did). --revalidate genuinely needs a commit (it gates and
      # then records one), so its refusal stands. For --restart, refuse only
      # when there is NEITHER a commit NOR uncommitted work — that is the
      # masked-failure case with nothing behind it.
      if [ "$RESTART" -eq 1 ]; then
        if [ -n "$restart_dirty" ]; then
          RESTART_FROM_TREE=1
          slog "STORY_RUN: RESTART-FROM-TREE task=${id} — prior attempt committed nothing but left uncommitted work in the tree ($(printf '%s' "$restart_dirty" | grep -c .) entries); the operator diagnosis will be delivered to the RESUMED devloop over that tree."
        else
          slogerr "STORY_RUN: NO-COMMIT-REFUSED — task ${id}'s prior attempt (reason=${esc_reason}) committed nothing AND the tree is clean, so there is nothing to restart from. This is masked-failure protection: fix the task and rerun the runner WITHOUT a retry flag. record: ${esc_rec}"
          exit 2
        fi
      else
        slogerr "STORY_RUN: NO-COMMIT-REFUSED — task ${id}'s prior attempt (reason=${esc_reason}) committed nothing (head_after == head_before), so there is no committed work to validate. This is masked-failure protection: fix the task and rerun the runner WITHOUT a retry flag. record: ${esc_rec}"
        exit 2
      fi
    fi
    # A committed prior attempt is now positively established: a readable reason
    # that is not devloop-no-commit, and a persisted baseline that differs from
    # HEAD (i.e. a commit landed in between).
    RETRY_APPLIED=1

    if [ "$REVALIDATE" -eq 1 ]; then
      slog "STORY_RUN: REVALIDATE task=${id} — re-running the authoritative gate against the committed prior attempt (reason=${esc_reason}); no devloop is spawned."
      # ATTEST THE TREE (S-5). --revalidate promises to gate "the tree as
      # committed by the prior attempt", and complete_task stages only
      # $STORY_FILE — so any OTHER uncommitted edit (an operator hand-fix, junk a
      # prior devloop left) would be validated green but never recorded, a green
      # not reproducible from the commit. Refuse unless the tree is clean apart
      # from the manifest reopen next() just made. Route a git read failure to the
      # operator lane, consistent with tree_dirty's rc>1 handling.
      reval_status="$(working_tree_status "$id" "$giterr" "checking the tree is clean before revalidation")"
      if [ -n "$reval_status" ]; then
        slogerr "STORY_RUN: REVALIDATE-DIRTY-TREE task=${id} — the work tree has uncommitted changes beyond the manifest reopen, so the gate would validate work this completion (which stages only ${STORY_FILE}) will NOT record — a green not reproducible from the commit. Commit or clean the tree, then rerun. Offending: $(printf '%s' "$reval_status" | head -n 5 | tr '\n' ';')"
        exit 2
      fi
      run_gate "$id"
      if [ "$gate_rc" -eq 0 ]; then
        # Green: complete the task, recovering the slug from the PRIOR attempt's
        # commit range (baseline..HEAD) via the shared completion path; record a
        # gate-only, zero-cost ledger entry.
        complete_task "$id" "$reval_baseline" gate-only
        slog "STORY_RUN: REVALIDATE task=${id} — gate green; task completed (gate-only, zero devloop cost)."
        if [ -n "$STOP_AFTER" ] && [ "$id" = "$STOP_AFTER" ]; then
          STOP_AFTER_FIRED=1
          slog "STORY_RUN: STOPPED after task ${id} (--stop-after) — story-close gate NOT run; rerun without the flag to continue"
          exit 0
        fi
        continue
      else
        tail -n 50 "$gatelog"
        # Red: re-escalate with the fresh gate log. Set head_before to the
        # ORIGINAL baseline first so escalate()'s re-persisted sidecar keeps
        # pointing at the real pre-attempt baseline (a --revalidate spawns no
        # devloop, so cur_head is NOT a new baseline — persisting it would make a
        # subsequent --revalidate see baseline == HEAD and wrongly refuse).
        head_before="$reval_baseline"
        escalate "$id" "revalidate-pipeline-red-layer${gate_layer}" "$gatelog"
      fi
    else
      # --restart: seed a FRESH devloop with the operator diagnosis and the prior
      # attempt's evidence, then fall through to the normal fresh-start path.
      if [ "$RESTART_FROM_TREE" -eq 1 ]; then
        slog "STORY_RUN: RESTART task=${id} — delivering the operator diagnosis to the resumed devloop; uncommitted work stays in the tree."
        prior_commit="<none — the prior attempt committed nothing; its full implementation is uncommitted in the working tree>"
      else
        slog "STORY_RUN: RESTART task=${id} — spawning a fresh devloop seeded with the operator diagnosis and the prior attempt's commit; discarding any resume pointer."
        prior_commit="$cur_head"
      fi
      # Prior attempt's output-doc slug (best-effort), via the same commit-range
      # derivation the completion path uses. Apply the SAME canonical slug floor
      # every other slug site applies (S-6): it is `cut`-derived and interpolated
      # into the prompt, so an out-of-class value must not pass unfiltered.
      if [ "$RESTART_FROM_TREE" -eq 1 ] && [ -s "$slug_file" ]; then
        prior_slug="$(cat "$slug_file" 2>/dev/null || true)"
      else
        prior_slug="$(git diff --name-only --diff-filter=A "$reval_baseline" HEAD \
          -- ':(glob)docs/devloop-outputs/*/main.md' 2>/dev/null | grep . | head -n1 | cut -d/ -f3 || true)"
      fi
      [[ -n "$prior_slug" && "$prior_slug" =~ $SLUG_CLASS_CANONICAL ]] || prior_slug=""
      [ -n "$prior_slug" ] || prior_slug="<unknown>"
      # The escalation's gate-failure tail (STATUS=FAIL / REASON= lines). This is
      # STDOUT OF scripts/layer*.sh — test/guard/audit output, much of it written
      # by the PRIOR devloop's own model — so it is a model->model channel and is
      # CONTAINED (S-2), not trusted: cap each line, neutralise code fences, and
      # `> `-quote every line so an injected `REASON=...` reads as quoted evidence,
      # not prompt structure. The operator's own (validated) diagnosis is placed
      # LAST so it, not this untrusted tail, has the final say.
      esc_gatelog="$RUN_DIR/task-${id}.gate.log"
      gate_tail=""
      [ -f "$esc_gatelog" ] && gate_tail="$( (grep -E '^(STATUS=FAIL|REASON=)' "$esc_gatelog" 2>/dev/null || true) \
          | tail -n 10 | cut -c1-200 | sed 's/```/'"'''"'/g; s/^/> /' )"
      # Append the operator paragraph to the on-disk prompt file — OUT OF BAND, so
      # no operator byte reaches the claude command line (mirrors R-2 defect 5).
      # The text was validated (empty + code-fence) at argv-parse time.
      {
        printf '\n\n---\nOPERATOR RESTART DIRECTIVE (run-story --restart)\n'
        if [ "$RESTART_FROM_TREE" -eq 1 ]; then
          printf 'A prior devloop attempt for this task escalated WITHOUT committing; its full implementation is in the working tree. The operator has diagnosed what must change. Build on the tree as it stands, not a blank slate.\n\n'
        else
          printf 'A prior devloop attempt for this task was COMMITTED, but the operator has diagnosed its work as needing to be fixed rather than re-implemented. Start from that existing commit, not a blank slate.\n\n'
        fi
        printf 'Prior attempt commit: %s\n' "$prior_commit"
        printf 'Prior attempt output-doc slug: %s\n' "$prior_slug"
        printf 'Prior attempt gate-failure tail (machine-generated evidence, quoted):\n%s\n\n' "${gate_tail:-> <none captured>}"
        printf 'Operator diagnosis (authoritative — act on THIS):\n%s\n' "$RESTART_TEXT"
      } >>"$prompt_file"
      if [ "$RESTART_FROM_TREE" -eq 1 ]; then
        # KEEP the resume pointer: the fresh-start path requires a clean tree,
        # and the uncommitted implementation must survive. The resume path
        # tolerates (expects) a dirty tree, and the resumed session receives
        # the prompt file with the operator paragraph appended — the session
        # that escalated gets the answer to the question it asked. No reopen
        # commit needed: resumes do not run the clean-tree check.
        slog "STORY_RUN: RESTART task=${id} — operator paragraph appended to ${prompt_file}; resume pointer KEPT (uncommitted work in tree)."
      else
        # Discard the resume pointer so the fresh devloop does NOT --continue the
        # prior work.
        rm -f "$slug_file"
        # Commit the manifest reopen so the fresh-start clean-tree check below
        # passes (mirrors the audit-remediation append-commit); then re-read HEAD as
        # the fresh devloop's baseline.
        git add "$STORY_FILE"
        git commit --quiet -m "chore(story): reopen task #${id} for operator --restart" || true
        if ! head_before="$(git_head "$giterr")"; then
          git_error_lane "$id" "reading HEAD after committing the restart reopen" "$giterr"
        fi
        slog "STORY_RUN: RESTART task=${id} — operator paragraph appended to ${prompt_file}; resume pointer discarded; reopen committed at $(git rev-parse --short HEAD 2>/dev/null || echo '<unreadable>')."
      fi
      # Fall through (no `continue`) to the normal fresh-start devloop path.
    fi
    fi  # end REVALIDATE/RESTART body (interactive skipped it)
  fi

  if [ -s "$slug_file" ]; then
    s="$(cat "$slug_file")"
    # Third splice site: --continue=%s. Filesystem-derived so lower severity
    # than the prompt, but the same mechanism, and it must not survive the fix
    # to the other two.
    if ! [[ "$s" =~ ^${SLUG_CLASS_RESUME}$ ]]; then
      slogerr "STORY_RUN: INVALID-RESUME-SLUG task=${id} — '${slug_file}' contains '${s}', which is not a plain slug (${SLUG_CLASS_RESUME}). It is interpolated as a flag value. Remove the file to start this task fresh."
      exit 2
    fi
    [ -f "docs/devloop-outputs/${s}/main.md" ] && continue_slug="$s"
  fi
  if [ -z "$continue_slug" ]; then
    # Fresh starts require a clean tree; resumes tolerate (expect) the
    # interrupted devloop's uncommitted work.
    dirty_rc=0; tree_dirty "$giterr" || dirty_rc=$?
    case "$dirty_rc" in
      2) git_error_lane "$id" "checking whether the work tree is clean" "$giterr" ;;
      0) slogerr "STORY_RUN: dirty tree and no resumable devloop for task ${id} — clean up, or point ${slug_file} at the interrupted devloop dir"
         exit 2 ;;
    esac
  fi

  # Per-task substrate assertion (see cli_version above). A version change mid-run
  # means the remaining tasks would execute on a substrate the probe never
  # validated — infra lane, not a task verdict: manifest untouched, rerun
  # re-probes the new version in preflight and resumes.
  # (No resume pointer to persist here: no devloop has run this iteration, and an
  # existing pointer from an earlier attempt is already on disk.)
  now_version="$(cli_version)"
  if [ "$now_version" != "$STORY_CLI_VERSION" ]; then
    slogerr "STORY_RUN: SUBSTRATE-CHANGED task=${id} — claude moved from '${STORY_CLI_VERSION}' to '${now_version}' mid-run (an attach updates the CLI in the running container). The new version is UNPROBED; manifest untouched, rerun to re-probe and resume."
    exit 2
  fi

  slog "STORY_RUN: START task=${id} specialist=${specialist} tier=${tier} resume=${continue_slug:-no} log=${tasklog}"
  touch "$start_marker"
  limit_waits=0

  # PROMPT INTERPOLATION (R-2 defect 5) — see build_devloop_prompt: the prompt lives
  # on disk at $prompt_file and no manifest byte reaches the command line; the path
  # is constrained by the RUN_DIR character floor above. The two spawn lanes below
  # share that builder.

  if [ "${INTERACTIVE_SPAWN:-0}" -eq 1 ]; then
    # --- D6/g attached spawn (branch at ENTRY, code-reviewer: fail-CLOSED). The
    # canary/session-limit retry machinery below is headless-only recovery — a human
    # is present here, so this lane STRUCTURALLY never enters it (threading
    # !INTERACTIVE guards through it would fail OPEN on a future edit).
    # TTY GATE (S-13/O-9): with the timeout dropped and the Stop hook inert, a spawn
    # with no controlling terminal would hang forever holding the container, cluster,
    # and in-flight marker. This premise ("a human is attached") must be CHECKED, not
    # assumed. Never auto-selected — only this explicit operator flag reaches here.
    if [ ! -t 0 ] || [ ! -t 1 ]; then
      slogerr "STORY_RUN: INTERACTIVE-NO-TTY task=${id} — --interactive attaches claude to your terminal, but stdin/stdout is not a TTY (nohup, a background job, or 'podman exec' without -t). With the task timeout dropped and the Stop hook inert, that spawn would hang unattended forever. Run it attached (devloop.sh execs with -it), or use --revalidate/--restart."
      exit 2
    fi
    interactive_prompt="$(build_devloop_prompt interactive "$id")"
    slog "STORY_RUN: INTERACTIVE task=${id} — you are driving; /exit or Ctrl-D when done. The runner then gates/commits/completes or escalates on HEAD movement + gate rc."
    __iw_start="$(date +%s)"
    set +e
    # env -u DEVLOOP_HEADLESS (NOT mere omission): an operator who exported it into
    # the runner's shell must not silently re-arm the Stop hook (which self-gates on
    # it, devloop-stop-hook.sh:14). No DEVLOOP_START_HEAD → the hook is doubly inert.
    # No -p (attached, interactive), no timeout, no stream-json redirect. And
    # --dangerously-skip-permissions is DROPPED (S-15): its ADR-0035 §6 justification
    # is "no human is present to answer the prompt", false by construction here — a
    # prompt the operator can answer beats one bypassed. Preflight's container-boundary
    # gate is unchanged, so this is no bypass.
    env -u DEVLOOP_HEADLESS claude "$interactive_prompt" --model "$STORY_MODEL"
    claude_rc=$?
    set -e
    INTERACTIVE_WALL_SECS=$(( $(date +%s) - __iw_start ))
    # Fall straight through to the UNCHANGED post-loop machinery (escalation-file
    # check, no-commit check, run_gate, complete). claude_rc here is the human's exit,
    # NOT a substrate fault — the session-error lane below is guarded to skip it.
  else
    while :; do
    task_prompt="$(build_devloop_prompt headless "$id")"

    # stream-json + verbose: default text mode prints only the final result at
    # session end, leaving the log empty for the whole run. JSONL events make
    # `tail -f` useful; filter with e.g.
    #   jq -r 'select(.type=="assistant") | .message.content[]? | .text? // empty'
    # DEVLOOP_COMMIT_INTENT_FILE (D6/f): where the devloop writes its Gate-3-close
    # commit-intent (env-passed, same precedent as DEVLOOP_START_HEAD); --finish reads
    # it back. Per-task path under RUN_DIR so it persists with the ledger.
    set +e
    DEVLOOP_HEADLESS=1 DEVLOOP_START_HEAD="$head_before" \
      DEVLOOP_STOP_COUNT_FILE="$stop_count_file" \
      DEVLOOP_COMMIT_INTENT_FILE="$RUN_DIR/task-${id}.commit-intent.json" \
      timeout "$TASK_TIMEOUT" claude -p "$task_prompt" \
      --model "$STORY_MODEL" \
      --output-format stream-json --verbose \
      --dangerously-skip-permissions >>"$tasklog" 2>&1
    claude_rc=$?
    set -e

    if [ "$claude_rc" -ne 0 ]; then
      cfile="$RUN_DIR/task-${id}.canary.json"
      canary_probe "$cfile" || true
      canary_class="$(canary_classify "$cfile")"
      case "$canary_class" in
        auth-expired)
          # Pure infra, needs a human (host /login if the refresh token itself
          # expired, then devloop.sh --refresh-creds, which copies fresh creds
          # into the RUNNING container — no restart, so RUN_DIR state survives).
          # No manifest edit — the task stays pending
          # and the resume pointer survives, so a rerun picks up cleanly.
          persist_resume_pointer
          record_infra_incident "$id" auth-expired "OAuth credentials rejected; recovery is devloop.sh --refresh-creds on the host"
          slogerr "STORY_RUN: AUTH-EXPIRED task=${id} — on the host run: /login (if needed) then ./infra/devloop/devloop.sh --refresh-creds ${DEVLOOP_SLUG:-<devloop-slug>}, then rerun to resume (that slug is the container's, not the story's; devloop.sh --run-story exports it as DEVLOOP_SLUG)"
          exit 2
          ;;
        infra)
          persist_resume_pointer
          record_infra_incident "$id" infra "API unreachable or canary unclassifiable"
          slogerr "STORY_RUN: INFRA task=${id} — API unreachable or canary unclassifiable (${cfile}); manifest untouched, rerun to resume"
          exit 2
          ;;
        session-limit)
          # Infra, not a task failure: wait out the window, resume the same
          # devloop via --continue. Retries exhausted falls through to the
          # generic escalation below.
          if [ "$limit_waits" -lt "$SESSION_LIMIT_RETRIES" ]; then
            limit_waits=$((limit_waits + 1))
            sleep_secs=3600
            reset_txt="$(jq -r '.result // empty' "$cfile" 2>/dev/null \
              | grep -o 'resets [0-9]\+:[0-9]\+[ap]m' | cut -d' ' -f2 || true)"
            if [ -n "$reset_txt" ]; then
              reset_epoch="$(date -u -d "$reset_txt" +%s 2>/dev/null || echo 0)"
              now_epoch="$(date -u +%s)"
              if [ "$reset_epoch" -gt 0 ]; then
                [ "$reset_epoch" -le "$now_epoch" ] && reset_epoch=$((reset_epoch + 86400))
                sleep_secs=$((reset_epoch - now_epoch + 300))
              fi
            fi
            # Sanity cap only: the reset time comes from the API's own
            # message, and daily/weekly resets can be many hours out. A
            # relaunch into a still-closed window dies on its first request
            # in seconds, so a wrong sleep costs a retry slot, not dollars.
            [ "$sleep_secs" -gt 93600 ] && sleep_secs=93600
            if [ -z "$continue_slug" ]; then
              # Resume target: the devloop output dir this task created.
              continue_slug="$(newest_devloop_output "$start_marker")"
              if [ -n "$continue_slug" ] && [ -f "docs/devloop-outputs/${continue_slug}/main.md" ]; then
                echo "$continue_slug" >"$slug_file"
              else
                continue_slug=""   # nothing to resume — clean up and retry fresh
                git reset --hard -q "$head_before"
                git clean -fdq
              fi
            fi
            slog "STORY_RUN: SESSION-LIMIT task=${id} wait=${sleep_secs}s resume=${continue_slug:-fresh} (${limit_waits}/${SESSION_LIMIT_RETRIES})"
            sleep "$sleep_secs"
            continue
          fi
          ;;
        healthy)
          # Environment fine — the failure was task/session-specific.
          # Fall through to the escalation checks below.
          ;;
      esac
    fi
    break
    done
  fi  # end interactive-vs-headless spawn branch

  # Persist the resume pointer on any uncommitted exit, so a later runner
  # invocation (after human intervention) resumes via --continue instead of
  # starting fresh over the partial work. Committed exits don't resume.
  persist_resume_pointer

  # Order matters: an explicit escalation file is the most informative signal;
  # timeout and session-error are infra-lane; no-commit catches a devloop that
  # ended "cleanly" without doing anything (a false-green would otherwise pass
  # the pipeline on an unchanged tree).
  if [ -f .devloop-escalation.json ]; then
    mv .devloop-escalation.json "$RUN_DIR/task-${id}.escalation.json"
    escalate "$id" devloop-escalated "$RUN_DIR/task-${id}.escalation.json"
  fi
  # Timeout is INFRA, resolving toward this file's own comment above ("timeout is
  # an infra escalation, not a task verdict") — the code contradicted it. A
  # wedged 4h headless session is a substrate condition and TASK_TIMEOUT is
  # operator-configured.
  # The three claude_rc-driven lanes (timeout / session-limit-exhausted /
  # session-error) are HEADLESS recovery — they exist because no human can react to
  # a wedge, quota, or crash. The interactive lane STRUCTURALLY skipped the canary
  # loop, has no `timeout` wrapper, and its claude_rc is the HUMAN'S exit (Ctrl-D=0,
  # Ctrl-C=130), not a substrate fault — so O-10/S-14: for interactive, only HEAD
  # movement + gate rc are authority. Skip all three; a non-zero human exit is noted
  # (evidence, not a session-error) and falls through to the no-commit/gate machinery.
  if [ "${INTERACTIVE_SPAWN:-0}" -ne 1 ]; then
    if [ "$claude_rc" -eq 124 ]; then
      record_infra_incident "$id" devloop-timeout \
        "headless devloop exceeded STORY_TASK_TIMEOUT=${TASK_TIMEOUT}s" "$tasklog"
      slogerr "STORY_RUN: TIMEOUT task=${id} — the headless devloop exceeded STORY_TASK_TIMEOUT=${TASK_TIMEOUT}s. Manifest untouched; rerun to resume via --continue, or raise STORY_TASK_TIMEOUT. Log: ${tasklog}"
      exit 2
    fi
    # Retry exhaustion on a POSITIVELY-classified quota outage is infra, not a task
    # verdict. Without this the classify inversion above would convert MORE
    # outages into recorded implementer bugs, so the two must stay together.
    if [ "$claude_rc" -ne 0 ] && [ "${canary_class:-}" = "session-limit" ]; then
      record_infra_incident "$id" session-limit-exhausted \
        "quota window outlasted STORY_SESSION_LIMIT_RETRIES=${SESSION_LIMIT_RETRIES} retries" "$tasklog"
      slogerr "STORY_RUN: SESSION-LIMIT-EXHAUSTED task=${id} — the quota window outlasted ${SESSION_LIMIT_RETRIES} retry wait(s). This is an environment condition, NOT a task failure: manifest untouched, resume pointer persisted. Rerun after the window resets. Log: ${tasklog}"
      exit 2
    fi
    [ "$claude_rc" -ne 0 ] && escalate "$id" devloop-session-error "$tasklog"
  elif [ "$claude_rc" -ne 0 ]; then
    # Interactive: record the human's non-zero exit as EVIDENCE (a session that never
    # started — bad flag, expired creds — exits non-zero with HEAD unmoved and lands
    # in devloop-no-commit below, whose record would otherwise say "made no commit"
    # when the truth is "never started"). Not itself an escalation.
    slog "STORY_RUN: INTERACTIVE task=${id} — attached claude exited rc=${claude_rc} (recorded; HEAD movement + gate rc decide the outcome, not this rc)."
  fi
  if ! head_after="$(git_head "$giterr")"; then
    git_error_lane "$id" "reading HEAD after the devloop finished" "$giterr"
  fi
  [ "$head_after" = "$head_before" ] && escalate "$id" devloop-no-commit "$tasklog"

  # Gate: EVERY task runs layers 1-6 then layer 7 (the authoritative pass/fail
  # run, ADR-0035 §3). Factored into run_gate so --revalidate uses the IDENTICAL
  # invocation; run_gate sets gate_rc / gate_layer, read by the split below. Full
  # pipeline incl. layer 7 also runs once at story close.
  run_gate "$id"
  # OPERATOR vs IMPLEMENTER (R-4). The pipeline already draws and names this
  # line: lang/_common.sh status_to_exit_code maps FAIL -> 1 (implementer) and
  # PRECONDITION_FAILURE | FAIL-MISSING-VERB | UNKNOWN -> 2 (operator), with a
  # fail-closed `*` backstop. Both were collapsed into one escalate() call here,
  # so a broken cluster was recorded against the task and the next session hunted
  # a bug in a diff that was fine.
  #
  # The split is rc 1 vs ANY OTHER non-zero — not rc 1 vs rc 2. 127 (layer script
  # missing), 137 (OOM-killed cargo) and 126 are environment, and an UNKNOWN rc
  # must default to not blaming the implementer, mirroring _common.sh's own
  # fail-closed backstop so runner and pipeline fail closed in the same direction.
  #
  # SCOPE, stated so this comment does not overclaim: this routes what the
  # pipeline DECLARES and what the runner OBSERVES. It cannot route what a layer
  # MISDECLARES — a wrapper that exits 1 on a missing guard binary arrives here as
  # rc 1 and is blamed on the task (preflight now asserts those binaries up front,
  # which converts the likeliest instance into a refusal at start). Nor what a
  # model asserts in prose: a Lead that writes .devloop-escalation.json because
  # git broke still lands on the implementer lane above, and fixing that would
  # require reading model-authored prose, which ADR-0035's no-model-in-the-control-
  # flow property forbids. R-4 is the named instance of that class, not all of it.
  if [ "$gate_rc" -eq 1 ]; then
    tail -n 50 "$gatelog"
    escalate "$id" "pipeline-red-layer${gate_layer}" "$gatelog"
  elif [ "$gate_rc" -ne 0 ]; then
    tail -n 50 "$gatelog"
    # This lane's state is UNLIKE the other manifest-untouched lanes: the
    # no-commit check above has already proven HEAD moved, so the task's work IS
    # committed. Say so, and drop any stale resume pointer — the devloop finished
    # and committed, so a rerun must not --continue into a completed devloop.
    rm -f "$slug_file"
    record_infra_incident "$id" pipeline-precondition \
      "layer ${gate_layer} exited ${gate_rc} (operator-class per lang/_common.sh status_to_exit_code); task work is committed, manifest untouched" \
      "$gatelog"
    slogerr "STORY_RUN: PIPELINE-PRECONDITION task=${id} layer=${gate_layer} rc=${gate_rc} — operator-class gate failure (see scripts/lang/_common.sh status_to_exit_code), NOT a task verdict."
    slogerr "STORY_RUN: this task's work IS COMMITTED at $(git rev-parse --short HEAD 2>/dev/null || echo '<unreadable>'); the manifest is UNTOUCHED, so a rerun starts a FRESH devloop for a task already on the branch."
    slogerr "STORY_RUN: recover by EITHER (1) fixing the environment, rerunning scripts/layer${gate_layer}.sh by hand and, if green, '${DT_STORY} complete ${STORY_FILE} ${id}' — OR (2) fixing the environment and rerunning the runner, accepting that task ${id} is re-implemented on top of its own commit."
    slogerr "STORY_RUN: gate log: ${gatelog}"
    exit 2
  fi

  # Slug resolution + dt-story complete + manifest bump + suppression note,
  # factored into complete_task (shared with --revalidate). The devloop's own
  # head_before is the commit-range baseline on this path; cost is derived from
  # the session log. On the interactive lane there is no session log, so pass the
  # `interactive` cost_mode: complete_task records an UNMEASURED (usd:null) entry via
  # append_lane_cost instead of report_task_cost re-grepping a stale/absent log (O2a).
  if [ "${INTERACTIVE_SPAWN:-0}" -eq 1 ]; then
    complete_task "$id" "$head_before" interactive
  else
    complete_task "$id" "$head_before"
  fi

  if [ -n "$STOP_AFTER" ] && [ "$id" = "$STOP_AFTER" ]; then
    STOP_AFTER_FIRED=1
    slog "STORY_RUN: STOPPED after task ${id} (--stop-after) — story-close gate NOT run; rerun without the flag to continue"
    exit 0
  fi
done

# Terminal never-fired check (R-5). The up-front validation above rejects an id
# that names no task or one the run cannot reach; this closes the remaining
# case — the loop finished without the id ever being SELECTED. Fires BEFORE the
# story-close gate, which brings up a cluster and runs for many minutes: a flag
# that silently did nothing must not also cost that.
if [ -n "$STOP_AFTER" ] && [ "${STOP_AFTER_FIRED:-0}" -ne 1 ]; then
  slogerr "STORY_RUN: STOP-AFTER-NEVER-FIRED — every task completed but --stop-after=${STOP_AFTER} was never reached, so the flag silently did nothing. NOTE: the story itself is fine — all tasks completed and committed; only the story-close gate was skipped. Rerun without --stop-after to run just that gate."
  exit 2
fi

# Story-close gate: the full pipeline, layer 7 included, on the final tree.
# This runs silently (redirected) for many minutes — layer 7 brings up the
# cluster — so mark its start/end, mirroring the per-task GATE lines, or a
# quiet story-close reads as a hang.
closelog="$RUN_DIR/story-close.gate.log"
slog "STORY_RUN: STORY-CLOSE GATE running — full layer-all.sh incl. layer 7 (log=${closelog})"
close_start="$(date +%s)"
# The authoritative full-pipeline run + its DEVLOOP_FAIL_FAST=0 rationale live in
# run_full_gate (DRY F2 — one home for the two callers). Truncate our own log first
# (run_full_gate appends), preserving this gate's fresh-log-per-close behaviour.
# (S3 context retained: this runs from the runner's own shell where DEVLOOP_HEADLESS
# is unset, so run_full_gate's inline DEVLOOP_FAIL_FAST=0 is what forces RUN-ALL and
# also overrides an ambient =1 a developer may have exported — see devloop main.md S3.)
: >"$closelog"
rc=0
run_full_gate "$closelog" || rc=$?
slog "STORY_RUN: STORY-CLOSE GATE rc=${rc} elapsed=$(( $(date +%s) - close_start ))s"
if [ "$rc" -ne 0 ]; then
  tail -n 50 "$closelog"
  slogerr "STORY_RUN: story-close gate red (log=${closelog})"
  exit "$rc"
fi

# Story-level cost rollup, honoring the LEDGER INVARIANT (see report_task_cost):
# per task, take the LAST devloop-kind entry (cumulative superset of that task's
# attempts) PLUS any per-lane singleton entries (finish/gate-only add nothing to
# usd; interactive is UNMEASURED). A lane entry NEVER supersedes a devloop entry —
# the old `map(last)` let a zero/placeholder lane entry appended at completion erase
# a task's real devloop cost (measured: an escalated-then-revalidated task reported
# $0 for the escalated attempt). `usd` is null on unmeasured entries; jq `add` folds
# null to identity, and unmeasured tasks are counted separately so a headline total
# never silently excludes them. WINDOW: all attempts for THIS story, all container
# generations (the tasklog persists under the flat story-keyed dir) — stated on the
# line so the number and its label agree.
if [ -f "$RUN_DIR/cost-ledger.jsonl" ]; then
  # FAIL LOUD, never silent (obs F2): every path prints exactly one STORY_RUN:
  # STORY-COST* line, so "no line" stays diagnosable as a bug rather than a legal
  # outcome. The old `2>/dev/null || true` swallowed two reachable inputs — an EMPTY
  # ledger (`[] | add` -> null, then `* 100` errors) and ONE truncated JSONL line from
  # a killed mid-write (parse error) — and under D5's persistent ledger one bad line
  # would permanently kill the rollup silently. Guard empty in bash AND emit inside jq
  # for the all-blank case; capture the rc and emit STORY-COST-UNAVAILABLE on failure.
  if [ ! -s "$RUN_DIR/cost-ledger.jsonl" ]; then
    slogerr "STORY_RUN: STORY-COST-UNAVAILABLE reason=empty-ledger — the cost ledger exists but is empty; no per-task cost was recorded (every task hit COST-UNAVAILABLE, or the ledger was truncated to zero)."
  else
    __story_cost="$(jq -s -r '
      def num(x): (x // 0);
      if length == 0 then "STORY_RUN: STORY-COST-UNAVAILABLE reason=empty-ledger" else
      ( group_by(.task)
        | map({
            task: .[0].task,
            devloop: ( [ .[] | select((.kind // "devloop") == "devloop") ] | last ),
            unmeasured: ( [ .[] | select(.measured == false) ] | length )
          })
        | { usd: ( map(.devloop | num(.usd)) | add * 100 | round / 100 ),
            output_tokens: ( map(.devloop | num(.output_tokens)) | add ),
            api_minutes: ( map(.devloop | num(.api_minutes)) | add ),
            tasks: length,
            unmeasured_tasks: ( map(select(.unmeasured > 0)) | length ) }
        | "STORY_RUN: STORY-COST window=this-story-all-generations tasks=\(.tasks) usd=\(.usd) output_tokens=\(.output_tokens) api_minutes=\(.api_minutes) unmeasured_tasks=\(.unmeasured_tasks)" )
      end' "$RUN_DIR/cost-ledger.jsonl" 2>/dev/null)"
    if [ -n "$__story_cost" ]; then
      slog "$__story_cost"
    else
      slogerr "STORY_RUN: STORY-COST-UNAVAILABLE reason=rollup-failed — jq could not aggregate ${RUN_DIR}/cost-ledger.jsonl (a malformed/truncated JSONL line from a killed mid-write is the likely cause); the story total is unavailable. The per-task STORY_RUN: COST lines above remain the record."
    fi
  fi
  # Read-only size measurement (Lead item B / D5 measurability): the run dir's size
  # and the largest per-task session log's bytes. No threshold, no deletion — pure
  # measurement, so story 2's data can right-size the deferred retention default.
  __rd_size="$(du -sh "$RUN_DIR" 2>/dev/null | cut -f1 || true)"
  __biggest_log="$(ls -S "$RUN_DIR"/task-*.devloop.log 2>/dev/null | head -n1 || true)"
  __biggest_bytes="$( [ -n "$__biggest_log" ] && wc -c <"$__biggest_log" 2>/dev/null || echo 0)"
  slog "STORY_RUN: RUN-DIR-SIZE dir=${RUN_DIR} total=${__rd_size:-unknown} largest_task_log_bytes=${__biggest_bytes:-0}"
fi

slog "STORY_RUN: ALL TASKS COMPLETE — story-close gate green. Next step: /close-story"
