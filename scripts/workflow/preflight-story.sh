#!/usr/bin/env bash
# preflight-story.sh — run-story.sh preconditions.
#
# Philosophy: the devloop container is self-defined (image + settings.json
# patched by entrypoint.sh + baked packages), so preflight does NOT re-verify
# what the container definition already pins — env flags live in
# ~/.claude/settings.json (not shell-visible anyway) and jq/claude are baked
# into the image. It checks only what is genuinely outside that control:
#
#   1. Run state that changes between invocations: manifest validity, clean
#      working tree (a prior devloop-no-commit escalation leaves a dirty tree
#      on purpose — human cleans up before resuming), dt-story binary built.
#   2. The experimental substrate: agent teams inside `claude -p` is
#      empirically-working but documentation-silent, AND entrypoint.sh
#      npm-updates the CLI at container start — so the substrate can shift
#      under us without any repo change. The live probe re-runs once per
#      claude version (marker-cached); an auto-update that breaks
#      teams-in-print-mode fails the story loudly here, not mid-devloop.
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

target/release/dt-story validate "$STORY_FILE" || fail "story manifest invalid"

if ! git diff --quiet || ! git diff --cached --quiet; then
  fail "working tree not clean"
fi

# --- Substrate probe, cached per claude version ---
VERSION="$(claude --version 2>/dev/null | head -n 1 | tr -cs 'A-Za-z0-9.' '-')"
RUN_BASE="${DEVLOOP_TMP:-/tmp/devloop}/story-runner"
MARKER="${RUN_BASE}/.substrate-probe-ok-${VERSION%-}"
mkdir -p "$RUN_BASE"

if [ -f "$MARKER" ]; then
  echo "PREFLIGHT: substrate probe cached for claude ${VERSION%-} (rm ${MARKER} to force re-probe)"
else
  probe_log="${RUN_BASE}/substrate-probe-${VERSION%-}.log"
  set +e
  timeout 420 claude -p \
    "Run two tests in order and report honestly; a negative result is a valid outcome. TEST 1 (agent-type registry): use the Agent tool to spawn an agent with subagent_type 'observability' and prompt: reply with the single word ok. If the spawn is rejected because the type is unknown/unregistered, print exactly: AGENT_TYPES: fallback. If it succeeds, print exactly: AGENT_TYPES: registered. TEST 2 (teams in print mode): use the Agent tool to spawn an agent named 'pingpong' (subagent_type 'general-purpose') with the prompt: you are a test teammate; when you receive a SendMessage saying ping, reply pong to the sender via SendMessage, then you are done. SendMessage it 'ping'; if the first ping was queued before it finished starting, send it again. Wait for the reply, then best-effort stop it. Print exactly one line: TEAM_PROBE: spawn=<yes|no> roundtrip=<yes|no> notes=<short>." \
    --allowedTools "Agent,Task,SendMessage,TaskStop,TaskList" >"$probe_log" 2>&1
  probe_rc=$?
  set -e
  [ "$probe_rc" -eq 0 ] || fail "substrate probe session error rc=${probe_rc} (see ${probe_log})"
  grep -q "TEAM_PROBE: spawn=yes roundtrip=yes" "$probe_log" \
    || fail "teams-in-print-mode probe negative (see ${probe_log})"
  if ! grep -q "AGENT_TYPES: registered" "$probe_log"; then
    echo "PREFLIGHT: WARN custom agent types not registered — devloops will use identity-injection fallback (see ${probe_log})" >&2
  fi
  touch "$MARKER"
fi

echo "PREFLIGHT: OK claude=$(claude --version 2>/dev/null | head -n 1)"
