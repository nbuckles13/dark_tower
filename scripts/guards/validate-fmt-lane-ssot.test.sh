#!/usr/bin/env bash
#
# Self-test for scripts/guards/simple/validate-fmt-lane-ssot.sh (ADR-0037 §D7).
#
# WHY THIS FILE EXISTS: the guard PASSES on the real tree (that is the point — the
# invariant holds today). A passing guard exercises none of its failure branches, and
# those branches ARE its value: a check that finds the SSoT intact forever, even after
# someone re-scatters an `if $CI` into a wrapper, is the exact "guard lies" defect it
# was written to prevent. So this drives every violation against synthetic files via
# the FMT_SSOT_* seams, PLUS a real-tree positive control, PLUS the load-bearing
# comment-vs-code discriminator (a wrapper that only MENTIONS CI in prose must still
# pass — otherwise the guard would forbid documenting the lane it enforces).
#
# NOT under guards/simple/: run-guards.sh discovers with `find … -name '*.sh'`, which
# matches `*.test.sh`, so a self-test there would be auto-run AS A PRODUCTION GUARD
# (same reasoning as counter-zero-init.test.sh). Hermetic: mktemp + EXIT trap, no
# cluster/network/cargo. A missing guard is a LOUD failure here, never a skip.
set -euo pipefail
IFS=$'\n\t'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

GUARD="${REPO_ROOT}/scripts/guards/simple/validate-fmt-lane-ssot.sh"
[[ -x "$GUARD" ]] || { printf '  - [precondition] guard missing/not executable at %s\n' "$GUARD"; printf '\n%s: 0 passed, 1 failed\n' "$0"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- baseline synthetic fixtures: every check satisfied (the guard must pass) ---
reset_fixtures() {
  COMMON="${WORK}/_common.sh"; RUST="${WORK}/rust-fmt.sh"; PROTO="${WORK}/proto-fmt.sh"; TS="${WORK}/ts-fmt.sh"
  RUNNER="${WORK}/run-story.sh"; PRECOMMIT="${WORK}/pre-commit"; LAYER2="${WORK}/layer2.sh"
  printf '#!/usr/bin/env bash\n# the SSoT home\nfmt_mode() {\n  : lane decision lives here\n}\n' > "$COMMON"
  printf '#!/usr/bin/env bash\n# CHECK in CI, APPLY locally — prose mention of CI is fine\n__v="$(fmt_mode)"\n' > "$RUST"
  printf '#!/usr/bin/env bash\n# proto lane\n__v="$(fmt_mode)"\n' > "$PROTO"
  printf '#!/usr/bin/env bash\n# ts lane\n__v="$(fmt_mode)"\n' > "$TS"
  {
    printf '#!/usr/bin/env bash\n'
    printf 'run_gate() {\n  DEVLOOP_FMT_CHECK_ONLY=1 "scripts/layer${n}.sh"\n}\n'
    printf 'run_full_gate() {\n  DEVLOOP_FAIL_FAST=0 DEVLOOP_FMT_CHECK_ONLY=1 ./scripts/layer-all.sh\n}\n'
  } > "$RUNNER"
  printf '#!/usr/bin/env bash\ncargo fmt --all -- --check\n' > "$PRECOMMIT"
  printf '#!/usr/bin/env bash\n# Layer 2 — Format\n"$(dirname "$0")/fmt.sh" | tee_collect_statuses\n' > "$LAYER2"
}

# run_guard: run the guard against the current synthetic fixtures; sets RC + OUT.
run_guard() {
  OUT="$(FMT_SSOT_COMMON="$COMMON" FMT_SSOT_RUST="$RUST" FMT_SSOT_PROTO="$PROTO" FMT_SSOT_TS="$TS" \
         FMT_SSOT_RUNNER="$RUNNER" FMT_SSOT_PRECOMMIT="$PRECOMMIT" FMT_SSOT_LAYER2="$LAYER2" \
         bash "$GUARD" 2>&1)" && RC=0 || RC=$?
}

# --- P0: REAL-TREE positive control (no seams) — the invariant holds today ---
OUT="$(bash "$GUARD" 2>&1)" && RC=0 || RC=$?
assert_exit "p0-real-tree-passes" 0 "$RC"

# --- P1: synthetic baseline passes (proves the fixtures are well-formed, so a later
#     FAIL is caused by the mutation under test, not a broken baseline) ---
reset_fixtures; run_guard
assert_exit "p1-synthetic-baseline-passes" 0 "$RC"

# --- V1a: fmt_mode() defined ZERO times -> home-count fail ---
reset_fixtures; printf '#!/usr/bin/env bash\n# no fmt_mode here\n' > "$COMMON"; run_guard
assert_exit   "v1a-no-home-fails" 1 "$RC"
assert_status "v1a-no-home-token" "exactly 1 'fmt_mode()'" "$OUT"

# --- V1b: fmt_mode() defined TWICE -> forked SSoT fail ---
reset_fixtures; printf 'fmt_mode() {\n  :\n}\nfmt_mode() {\n  :\n}\n' >> "$COMMON"; run_guard
assert_exit   "v1b-forked-home-fails" 1 "$RC"
assert_status "v1b-forked-home-token" "found 3" "$OUT"   # baseline 1 + 2 appended

# --- V2: a wrapper stops delegating to fmt_mode -> delegation fail ---
reset_fixtures; printf '#!/usr/bin/env bash\n# decides locally now\n' > "$RUST"; run_guard
assert_exit   "v2-no-delegation-fails" 1 "$RC"
assert_status "v2-no-delegation-token" "does not call fmt_mode" "$OUT"

# --- V2-TS (dry F1): the TS wrapper is now in the guard's set — if it stops delegating, the guard catches
#     it. Proves TS is iterated by BOTH loops (a guard that omitted TS, as it did pre-fix, would PASS here). ---
reset_fixtures; printf '#!/usr/bin/env bash\n# ts decides locally now\n' > "$TS"; run_guard
assert_exit   "v2ts-ts-no-delegation-fails" 1 "$RC"
assert_status "v2ts-ts-no-delegation-token" "does not call fmt_mode" "$OUT"

# --- V3-TS (dry F1): a CI branch in the TS wrapper's CODE reds — the exact D9 hole dry-reviewer flagged
#     (pre-fix the guard never inspected ts/fmt.sh, so an `if $CI` there would ship silently). ---
reset_fixtures
printf '#!/usr/bin/env bash\n__v="$(fmt_mode)"\nif [ -n "$GITHUB_ACTIONS" ]; then __v=CHECK; fi\n' > "$TS"
run_guard
assert_exit   "v3ts-ts-ci-branch-fails" 1 "$RC"
assert_status "v3ts-ts-ci-branch-token" "references a CI sentinel" "$OUT"

# --- V3a: a wrapper grows a CI branch in CODE -> SSoT-bypass fail (the load-bearing check) ---
reset_fixtures
printf '#!/usr/bin/env bash\n__v="$(fmt_mode)"\nif [ -n "$GITHUB_ACTIONS" ]; then __v=CHECK; fi\n' > "$RUST"
run_guard
assert_exit   "v3a-ci-branch-in-code-fails" 1 "$RC"
assert_status "v3a-ci-branch-token" "references a CI sentinel" "$OUT"

# --- V3b (NON-VACUITY): a wrapper that only MENTIONS CI in a COMMENT still passes.
#     Without the comment-strip this would false-positive and forbid documenting the
#     very lane the guard enforces — the discriminator that makes V3a meaningful. ---
reset_fixtures
printf '#!/usr/bin/env bash\n# CHECK in CI / at gates; APPLY locally. GITHUB_ACTIONS handled by fmt_mode.\n__v="$(fmt_mode)"\n' > "$RUST"
run_guard
assert_exit "v3b-ci-in-comment-passes" 0 "$RC"

# --- V4a: run_gate loses its check-only prefix -> attesting-gate fail ---
reset_fixtures
{
  printf '#!/usr/bin/env bash\n'
  printf 'run_gate() {\n  "scripts/layer${n}.sh"\n}\n'                              # prefix DROPPED
  printf 'run_full_gate() {\n  DEVLOOP_FAIL_FAST=0 DEVLOOP_FMT_CHECK_ONLY=1 ./scripts/layer-all.sh\n}\n'
} > "$RUNNER"
run_guard
assert_exit   "v4a-run_gate-no-prefix-fails" 1 "$RC"
assert_status "v4a-run_gate-token" "run_gate() in" "$OUT"

# --- V4b: run_full_gate loses its prefix -> attesting-gate fail (the OTHER gate home) ---
reset_fixtures
{
  printf '#!/usr/bin/env bash\n'
  printf 'run_gate() {\n  DEVLOOP_FMT_CHECK_ONLY=1 "scripts/layer${n}.sh"\n}\n'
  printf 'run_full_gate() {\n  DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh\n}\n'    # prefix DROPPED
} > "$RUNNER"
run_guard
assert_exit   "v4b-run_full_gate-no-prefix-fails" 1 "$RC"
assert_status "v4b-run_full_gate-token" "run_full_gate() in" "$OUT"

# --- V5a: pre-commit is not check-only (no --check form) -> hook fail ---
reset_fixtures; printf '#!/usr/bin/env bash\ncargo fmt --all\n' > "$PRECOMMIT"; run_guard
assert_exit   "v5a-precommit-not-check-fails" 1 "$RC"
assert_status "v5a-precommit-check-token" "cargo fmt --all -- --check" "$OUT"

# --- V5b: pre-commit opts into apply -> hook fail (an attesting hook must never apply) ---
reset_fixtures
printf '#!/usr/bin/env bash\nDEVLOOP_FMT_APPLY=1 cargo fmt --all -- --check\n' > "$PRECOMMIT"
run_guard
assert_exit   "v5b-precommit-apply-optin-fails" 1 "$RC"
assert_status "v5b-precommit-apply-token" "must never opt into fmt auto-apply" "$OUT"

# --- V6: a fmt-lane site goes missing -> loud file-not-found fail (not a silent skip) ---
reset_fixtures; rm -f "$PROTO"; run_guard
assert_exit   "v6-missing-site-fails" 1 "$RC"
assert_status "v6-missing-site-token" "not found" "$OUT"

# --- V6-exclude (security F1 / co-sign #7): the fmt layer excludes proto -> fail. This is the SOLE CI
#     proto-format gate; DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto would silence it with no failure status. ---
reset_fixtures
printf '#!/usr/bin/env bash\nDEVLOOP_DISPATCH_EXCLUDE_LANGS=proto "$(dirname "$0")/fmt.sh" | tee_collect_statuses\n' > "$LAYER2"
run_guard
assert_exit   "v6excl-fmt-excludes-proto-fails" 1 "$RC"
assert_status "v6excl-fmt-excludes-proto-token" "excludes proto from the fmt dispatch" "$OUT"

# --- V6-exclude-comment (NON-VACUITY): a triage COMMENT mentioning the excluded pattern still passes
#     (the check strips comment lines) — the discriminator that keeps V6-exclude from banning documentation. ---
reset_fixtures
printf '#!/usr/bin/env bash\n# NB: never DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto here (see security co-sign #7)\n"$(dirname "$0")/fmt.sh" | tee_collect_statuses\n' > "$LAYER2"
run_guard
assert_exit "v6exclcomment-excluded-pattern-in-comment-passes" 0 "$RC"

# --- V6-include (security F1): an INCLUDE allow-list that OMITS proto -> fail (fails-to-include). ---
reset_fixtures
printf '#!/usr/bin/env bash\nDEVLOOP_DISPATCH_INCLUDE_LANGS=rust,ts "$(dirname "$0")/fmt.sh" | tee_collect_statuses\n' > "$LAYER2"
run_guard
assert_exit   "v6incl-include-omits-proto-fails" 1 "$RC"
assert_status "v6incl-include-omits-proto-token" "without naming proto" "$OUT"

# --- V6-include-ok: an INCLUDE list that DOES name proto -> pass (positive control for the include arm). ---
reset_fixtures
printf '#!/usr/bin/env bash\nDEVLOOP_DISPATCH_INCLUDE_LANGS=rust,proto,ts "$(dirname "$0")/fmt.sh" | tee_collect_statuses\n' > "$LAYER2"
run_guard
assert_exit "v6inclok-include-with-proto-passes" 0 "$RC"

report_results "scripts/guards/validate-fmt-lane-ssot.test.sh"
