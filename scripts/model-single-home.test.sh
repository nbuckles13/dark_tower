#!/usr/bin/env bash
# model-single-home.test.sh — the devloop model has ONE home: "model" in .claude/settings.json.
#
# Every claude session in the repo reads that file (interactive devloop Lead, the story
# runner's headless sessions), and every teammate inherits the Lead's model. Pins used to
# live in 17 places (15 agent definitions, devloop.sh, run-story.sh) plus `model: "opus"`
# in the devloop skill's spawn instructions, and had drifted to three generations in one
# devloop. This test fails if a second home reappears.
#
# Wired into scripts/layer3.sh. Consumes scripts/lang/_test_helpers.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/.." && pwd)"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"
set +e

settings="${REPO_ROOT}/.claude/settings.json"
model="$(jq -r '.model // empty' "$settings" 2>/dev/null)"
assert_rc "settings-json-sets-model" 0 "$([[ -n "$model" ]] && echo 0 || echo "1 (no .model in ${settings#"${REPO_ROOT}"/})")"

# No agent definition pins a model (teammates inherit the Lead's).
pinned_agents="$(grep -lE '^model:' "${REPO_ROOT}"/.claude/agents/*.md 2>/dev/null | sed "s#${REPO_ROOT}/##" | tr '\n' ' ')"
assert_rc "agents-carry-no-model" 0 "$([[ -z "$pinned_agents" ]] && echo 0 || echo "1 (model: in ${pinned_agents})")"

# No skill tells the Lead to pass a model when spawning a teammate.
skill_models="$(grep -rnE '`model: "' "${REPO_ROOT}/.claude/skills" 2>/dev/null | sed "s#${REPO_ROOT}/##" | head -n5 | tr '\n' ' ')"
assert_rc "skills-pass-no-model" 0 "$([[ -z "$skill_models" ]] && echo 0 || echo "1 (${skill_models})")"

# No script or container tooling hardcodes an Opus id or alias as a default.
hard="$(grep -rnE 'claude-opus-[0-9]|(--model|MODEL:-)[[:space:]"]*opus' \
  "${REPO_ROOT}/scripts" "${REPO_ROOT}/infra" 2>/dev/null \
  | grep -v '/model-single-home.test.sh:' | sed "s#${REPO_ROOT}/##" | head -n5 | tr '\n' ' ')"
assert_rc "scripts-hardcode-no-opus" 0 "$([[ -z "$hard" ]] && echo 0 || echo "1 (${hard})")"

report_results "scripts/model-single-home.test.sh"
