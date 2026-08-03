#!/usr/bin/env bash
# Stop hook: a headless devloop Lead may not end its turn until the devloop
# has either committed (Step 8) or written .devloop-escalation.json. An idled
# print-mode Lead is never re-invoked (verified 2026-08-02: neither teammate
# messages nor background-task completion start a new turn), so premature
# stops are blocked here mechanically instead of by skill convention.
#
# Registered container-side by infra/devloop/entrypoint.sh; no-op everywhere
# else (gated on run-story's env). Exit 0 with no output = allow the stop.
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
head="$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)"
if [ -z "$head" ] || [ "$head" != "$DEVLOOP_START_HEAD" ]; then
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

cat <<'JSON'
{"decision":"block","reason":"Headless devloop incomplete: no commit and no escalation file. Do not idle-wait — check teammate status (TaskList), hold on pending work with a blocking TaskOutput call (block: true), and drive the next gate. If genuinely blocked, write .devloop-escalation.json per the devloop skill Headless Mode section, then stop."}
JSON
