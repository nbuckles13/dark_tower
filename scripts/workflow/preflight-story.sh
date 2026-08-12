#!/usr/bin/env bash
# preflight-story.sh — run-story.sh preconditions.
#
# Philosophy: the devloop container is self-defined (image + settings.json
# patched by entrypoint.sh + baked packages), so preflight does NOT re-verify
# what the container definition already pins — env flags live in
# ~/.claude/settings.json (not shell-visible anyway) and jq/claude are baked
# into the image. It checks only what is genuinely outside that control:
#
#   1. Run state that changes between invocations: manifest validity and the
#      dt-story binary. (Clean-tree checking lives in run-story.sh per task:
#      fresh starts require it; resumes tolerate the interrupted devloop's
#      uncommitted work.)
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

# --- Container boundary (ADR-0025) ---
# run-story.sh:11 documents "skip-permissions is only safe behind that boundary"
# but nothing enforced it. Run on a host, this script would (a) patch the
# operator's REAL ~/.claude/settings.json — and while the Stop hook self-gates on
# DEVLOOP_HEADLESS, CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS does NOT, so it would
# silently apply to every later interactive session with nothing reverting it —
# and (b) hand an unattended --dangerously-skip-permissions loop (which also runs
# `git stash`/`git clean` on classification decisions) the operator's real tree.
#
# SENTINEL CHOICE (3-way finding: infrastructure/security/operations, 2026-08-10):
# /run/.containerenv (podman) and /.dockerenv (docker) are written by the
# container RUNTIME, so they need no image rebake and cannot be forged by an
# inherited env var. A baked marker file was considered and REJECTED: it exists
# only after a rebake, so landing this guard before the rebake would fail every
# run closed — reintroducing exactly the image-bake coupling that caused the
# 2026-08-06 task #64 idle-death and that preflight ownership exists to remove.
#
# The escape hatch is deliberate (security, 2026-08-10): a hard gate with no
# override gets commented out under pressure, and a deleted gate is worse than a
# named, greppable opt-in. It covers skip-permissions only — see the settings
# note below for the part an operator cannot meaningfully consent to.
#
# NOTE (security, 2026-08-10): a runtime marker proves "in *a* container", not
# "in *the devloop* container". That is the right property: what must be
# established is "not the operator's host $HOME", and the marker establishes
# exactly that. Do NOT "strengthen" this into a baked marker file — see above.
IN_CONTAINER=1
if [ ! -f /run/.containerenv ] && [ ! -f /.dockerenv ]; then
  IN_CONTAINER=0
  [ "${STORY_RUNNER_ALLOW_HOST:-}" = "1" ] || fail "not inside the devloop container (ADR-0025) — launch via infra/devloop/devloop.sh. This script patches \$HOME/.claude/settings.json and the runner drives --dangerously-skip-permissions unattended; neither is safe against a host \$HOME or a host work tree. Set STORY_RUNNER_ALLOW_HOST=1 only if you accept both."
  echo "PREFLIGHT: WARN STORY_RUNNER_ALLOW_HOST=1 — running OUTSIDE the container against ${HOME}." >&2
fi

command -v claude >/dev/null 2>&1 || fail "claude CLI not found"
command -v jq >/dev/null 2>&1 || fail "jq not found"
[ -x target/release/dt-story ] || fail "dt-story not built (cargo build --release -p dt-story)"
[ -f "$STORY_FILE" ] || fail "story file not found: ${STORY_FILE}"

target/release/dt-story validate "$STORY_FILE" || fail "story manifest invalid"

# --- Runner-owned substrate config (idempotent) ---
# The Stop hook and the unlimited print-mode background wait are RUNNER
# dependencies, so the runner installs them — settings.json is patched here,
# in-container, every run. They were previously registered by the container
# entrypoint, which is baked into the image: the 2026-08-06 task #64 idle-death
# happened because the in-tree entrypoint edit never made it into a rebaked
# image, so no container ever had the hook. Preflight ownership removes the
# image-bake coupling entirely.
#
# PERSISTENT-FOOTPRINT SPLIT (security, 2026-08-10). Two settings are written
# here and they are NOT equivalent in blast radius:
#   - the Stop hook self-gates on DEVLOOP_HEADLESS (devloop-stop-hook.sh:12), so
#     it is inert in ordinary sessions. It is also REQUIRED for the runner to
#     work, so it is registered unconditionally.
#   - CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS is NOT gated. Written to a host
#     $HOME it would silently apply to every later interactive session the
#     operator runs, unannounced and never reverted. STORY_RUNNER_ALLOW_HOST
#     reasonably means "I accept skip-permissions for THIS run" — it cannot mean
#     "permanently rewrite my global config".
# So the ceiling is written to settings.json ONLY in-container (ephemeral $HOME).
# On the host-hatch path run-story.sh exports it per-invocation instead, and we
# print exactly what was and was not changed.
SETTINGS="$HOME/.claude/settings.json"
HOOK_CMD="$REPO_ROOT/scripts/workflow/devloop-stop-hook.sh"
[ -x "$HOOK_CMD" ] || fail "stop hook script missing/not executable: ${HOOK_CMD}"
mkdir -p "$HOME/.claude"
[ -f "$SETTINGS" ] || echo '{}' >"$SETTINGS"
jq --arg cmd "$HOOK_CMD" --argjson ceiling "$IN_CONTAINER" '
  (if $ceiling == 1 then .env.CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS = "0" else . end)
  | .hooks.Stop = ((.hooks.Stop // [])
      | if ([.[]?.hooks[]?.command] | index($cmd)) then .
        else . + [{"hooks": [{"type": "command", "command": $cmd}]}] end)
' "$SETTINGS" >"$SETTINGS.tmp" && mv "$SETTINGS.tmp" "$SETTINGS"
jq -e --arg cmd "$HOOK_CMD" --argjson ceiling "$IN_CONTAINER" \
  '([.hooks.Stop[]?.hooks[]?.command] | index($cmd))
   and ($ceiling == 0 or .env.CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS == "0")' \
  "$SETTINGS" >/dev/null || fail "settings.json substrate patch did not verify"
if [ "$IN_CONTAINER" = "1" ]; then
  echo "PREFLIGHT: substrate config ensured (stop hook + bg-wait ceiling)"
else
  echo "PREFLIGHT: WARN registered the Stop hook in ${SETTINGS} (self-gating on DEVLOOP_HEADLESS, inert in normal sessions). To revert: remove the entry naming ${HOOK_CMD} from .hooks.Stop. The bg-wait ceiling was NOT written to your config — run-story exports it per-invocation." >&2
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
