#!/usr/bin/env bash
# parse-whole-script.test.sh — the long-running entry scripts are parsed whole before
# they run.
#
# Bash reads a script from disk as it executes. devloop.sh blocks for hours inside the
# Claude session and run-story.sh for the length of a story, both from trees that get
# edited meanwhile. Git checkouts replace the file (new inode; the running bash keeps
# the old bytes), but an in-place write shifts the bytes under bash's read offset, and
# when control returns it parses mid-statement (seen 2026-09-27: devloop.sh "syntax
# error near unexpected token `)'" on session exit, Phase 3 lost). Each listed script wraps
# its body in `{ ... exit; }`: the first command is `{`, the last two lines are
# `exit` and `}`.
#
# Adding a script: put it in LONG_RUNNING below. Wired into scripts/layer3.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/.." && pwd)"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

LONG_RUNNING=(
  infra/devloop/devloop.sh
  scripts/workflow/run-story.sh
)

for rel in "${LONG_RUNNING[@]}"; do
  f="${REPO_ROOT}/${rel}"
  # First line that is neither blank, a comment, nor the shebang.
  first="$(awk '!/^[[:space:]]*(#|$)/ { print; exit }' "$f")"
  tail2="$(grep -vE '^[[:space:]]*$' "$f" | tail -n2 | tr '\n' ' ')"
  assert_rc "${rel}-opens-with-brace" 0 "$([[ "$first" == "{" ]] && echo 0 || echo "1 (first command: '${first}')")"
  assert_rc "${rel}-ends-exit-brace" 0 "$([[ "$tail2" == "exit } " ]] && echo 0 || echo "1 (last two lines: '${tail2}')")"
  parse_rc=0; bash -n "$f" || parse_rc=$?
  assert_rc "${rel}-parses" 0 "$parse_rc"
done

report_results "scripts/parse-whole-script.test.sh"
