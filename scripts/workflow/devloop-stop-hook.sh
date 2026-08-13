#!/usr/bin/env bash
# Stop hook: a headless devloop Lead may not end its turn until the devloop
# has either committed (Step 8) or written .devloop-escalation.json. An idled
# print-mode Lead is never re-invoked (verified 2026-08-02: neither teammate
# messages nor background-task completion start a new turn), so premature
# stops are blocked here mechanically instead of by skill convention.
#
# Registered in-container by scripts/workflow/preflight-story.sh on every run
# (NOT by the image entrypoint — an unbaked entrypoint edit caused the 2026-08-06
# task #64 idle-death); no-op everywhere else (gated on run-story's env).
# Exit 0 with no output = allow the stop.
set -uo pipefail

if [ "${DEVLOOP_HEADLESS:-}" != "1" ] || [ -z "${DEVLOOP_START_HEAD:-}" ]; then
  exit 0
fi

dir="${CLAUDE_PROJECT_DIR:-$PWD}"

# Escalated: the Lead followed the contract — allow.
if [ -f "${dir}/.devloop-escalation.json" ]; then
  exit 0
fi

# Committed: devloop finished its work — allow.
#
# FAIL-CLOSED on a git error (R-2 defect 4 / ADR-0035 Follow-on Group 1 item 1).
# This used to read `[ -z "$head" ] || [ "$head" != "$START" ]` -> exit 0, so a
# `git rev-parse` failure ALLOWED the stop: the headless Lead ended its turn
# with no commit and no escalation file, and an idled print-mode Lead is never
# re-invoked. "git could not tell me" is not "the devloop committed", so the
# empty case now falls through to the block below rather than joining the
# allow branch.
git_ok=1
head="$(git -C "$dir" rev-parse --verify HEAD 2>/dev/null)" || git_ok=0
if [ "$git_ok" = "1" ] && [ "$head" != "$DEVLOOP_START_HEAD" ]; then
  exit 0
fi

# Safety valve: after too many blocks, allow the stop and let the runner's
# devloop-no-commit check escalate — never loop a wedged Lead forever.
count_file="${DEVLOOP_STOP_COUNT_FILE:-/tmp/devloop-stop-blocks}"
count=$(( $(cat "$count_file" 2>/dev/null || echo 0) + 1 ))
echo "$count" > "$count_file"
if [ "$count" -gt "${DEVLOOP_STOP_BLOCK_MAX:-120}" ]; then
  exit 0
fi

# A git error gets its OWN reason. Reusing the "you have not committed" text
# would tell a Lead facing an unreadable .git, a full disk or a held index.lock
# to do the one thing it cannot — and it would be told 120 times before the
# safety valve opens. Naming the escalation file as the achievable recovery
# turns that into one or two turns.
if [ "$git_ok" = "0" ]; then
  cat <<'JSON'
{"decision":"block","reason":"Cannot verify devloop completion: `git rev-parse --verify HEAD` failed in the project directory, so whether you committed is unknown — and this hook fails closed rather than assuming you did. Do NOT retry the commit blindly. Check the repository first (is .git readable, is the disk full, is another process holding .git/index.lock?). If you cannot resolve it, write .devloop-escalation.json per the devloop skill Headless Mode section with reason 'precondition-failure' and detail the git error, then stop — that is the recovery available to you here, not committing."}
JSON
  exit 0
fi

cat <<'JSON'
{"decision":"block","reason":"Headless devloop incomplete: no commit and no escalation file. Do not idle-wait — check teammate status (TaskList), hold on pending work with a blocking TaskOutput call (block: true), and drive the next gate. If genuinely blocked, write .devloop-escalation.json per the devloop skill Headless Mode section, then stop."}
JSON
