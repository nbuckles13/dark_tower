#!/usr/bin/env bash
# preflight-story.sh — run-story.sh preconditions. DRAFT for review.
#
# The story runner rides an experimental, documentation-silent combination
# (agent teams inside `claude -p`). This preflight is the compatibility test:
# it re-proves the substrate with a live teammate spawn + SendMessage
# round-trip before task 1, so a CLI upgrade that breaks it fails the story
# loudly here instead of wedging a devloop mid-run.
#
# Usage: scripts/workflow/preflight-story.sh <story-file.md>
# Exit: 0 ok / 2 precondition failure
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

STORY_FILE="${1:?usage: preflight-story.sh <story-file.md>}"
fail() { echo "PREFLIGHT: FAIL $*" >&2; exit 2; }

command -v claude >/dev/null 2>&1 || fail "claude CLI not found"
command -v jq >/dev/null 2>&1 || fail "jq not found"
[ -x target/release/dt-story ] || fail "dt-story not built (cargo build --release -p dt-story)"
[ -f "$STORY_FILE" ] || fail "story file not found: ${STORY_FILE}"

[ "${CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS:-}" = "1" ] \
  || fail "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS must be 1 (devloop teams)"
[ "${CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS:-}" = "0" ] \
  || fail "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS must be 0 — print mode caps background waits at 10min by default, which kills long devloops"

target/release/dt-story validate "$STORY_FILE" || fail "story manifest invalid"

if ! git diff --quiet || ! git diff --cached --quiet; then
  fail "working tree not clean"
fi

probe_log="${DEVLOOP_TMP:-/tmp/devloop}/story-runner/preflight-team-probe.log"
mkdir -p "$(dirname "$probe_log")"
set +e
timeout 420 claude -p \
  "You are testing teammate orchestration in headless print mode. (1) Use the Agent tool to spawn an agent named 'pingpong' (subagent_type 'general-purpose') with the prompt: you are a test teammate; when you receive a SendMessage saying ping, reply pong to the sender via SendMessage, then you are done. (2) SendMessage it 'ping'. (3) Wait for the reply; if the first ping was queued before the teammate finished starting, send it again. (4) Best-effort stop it. (5) Print EXACTLY one final line: TEAM_PROBE: spawn=<yes|no> roundtrip=<yes|no> notes=<short>. Report honestly; a negative result is a valid outcome." \
  --allowedTools "Agent,Task,SendMessage,TaskStop,TaskList" >"$probe_log" 2>&1
probe_rc=$?
set -e
[ "$probe_rc" -eq 0 ] || fail "team probe session error rc=${probe_rc} (see ${probe_log})"
grep -q "TEAM_PROBE: spawn=yes roundtrip=yes" "$probe_log" \
  || fail "teams-in-print-mode probe negative (see ${probe_log})"

echo "PREFLIGHT: OK claude=$(claude --version 2>/dev/null | head -n 1)"
