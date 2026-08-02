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
# Usage: scripts/workflow/run-story.sh docs/user-stories/YYYY-MM-DD-slug.md
#
# Exit: 0  all tasks complete, story-close gate green
#       1  task escalated (escalation.json path printed) or blocked
#       2  precondition / infra / manifest failure
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

STORY_FILE="${1:?usage: run-story.sh <story-file.md>}"
DT_STORY="target/release/dt-story"
RUN_DIR="${DEVLOOP_TMP:-/tmp/devloop}/story-runner/$(basename "${STORY_FILE%.md}")"
mkdir -p "$RUN_DIR"

# Per-devloop wall-clock ceiling. A wedged headless session must not hold the
# story forever; timeout is an infra escalation, not a task verdict.
TASK_TIMEOUT="${STORY_TASK_TIMEOUT:-14400}"

scripts/workflow/preflight-story.sh "$STORY_FILE"

escalate() {
  local id="$1" reason="$2" log="$3"
  "$DT_STORY" escalate "$STORY_FILE" "$id" --reason "$reason" --log "$log" \
    --out "$RUN_DIR/escalation.json"
  echo "STORY_RUN: ESCALATED task=${id} reason=${reason} log=${log}" >&2
  echo "STORY_RUN: escalation record: ${RUN_DIR}/escalation.json" >&2
  exit 1
}

while :; do
  set +e
  task_json="$("$DT_STORY" next "$STORY_FILE")"
  next_rc=$?
  set -e
  case "$next_rc" in
    0) ;;
    3) break ;;
    4) echo "STORY_RUN: BLOCKED — pending tasks with unsatisfied deps (see dt-story stderr)" >&2; exit 1 ;;
    *) echo "STORY_RUN: manifest error (dt-story next rc=${next_rc})" >&2; exit 2 ;;
  esac

  id="$(jq -r .id <<<"$task_json")"
  specialist="$(jq -r .specialist <<<"$task_json")"
  env_tests="$(jq -r .env_tests <<<"$task_json")"
  prompt_file="$RUN_DIR/task-${id}.prompt"
  jq -r .prompt <<<"$task_json" >"$prompt_file"

  tasklog="$RUN_DIR/task-${id}.devloop.log"
  gatelog="$RUN_DIR/task-${id}.gate.log"
  head_before="$(git rev-parse HEAD)"
  rm -f .devloop-escalation.json

  echo "STORY_RUN: START task=${id} specialist=${specialist} log=${tasklog}"
  set +e
  DEVLOOP_HEADLESS=1 timeout "$TASK_TIMEOUT" claude -p \
    "$(printf 'HEADLESS RUN (run-story task #%s): follow the devloop skill including its Headless Mode section.\n/devloop "%s" --specialist=%s' \
        "$id" "$(cat "$prompt_file")" "$specialist")" \
    --dangerously-skip-permissions >"$tasklog" 2>&1
  claude_rc=$?
  set -e

  # Order matters: an explicit escalation file is the most informative signal;
  # timeout and session-error are infra-lane; no-commit catches a devloop that
  # ended "cleanly" without doing anything (a false-green would otherwise pass
  # the pipeline on an unchanged tree).
  if [ -f .devloop-escalation.json ]; then
    mv .devloop-escalation.json "$RUN_DIR/task-${id}.escalation.json"
    escalate "$id" devloop-escalated "$RUN_DIR/task-${id}.escalation.json"
  fi
  [ "$claude_rc" -eq 124 ] && escalate "$id" devloop-timeout "$tasklog"
  [ "$claude_rc" -ne 0 ] && escalate "$id" devloop-session-error "$tasklog"
  [ "$(git rev-parse HEAD)" = "$head_before" ] && escalate "$id" devloop-no-commit "$tasklog"

  # Gate: fast floor (layers 1-6) every task; layer 7 only when the task is
  # tagged env_tests. Full pipeline incl. layer 7 runs once at story close.
  gate_rc=0
  for n in 1 2 3 4 5 6; do
    set +e
    "scripts/layer${n}.sh" >>"$gatelog" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then gate_rc=$rc; break; fi
  done
  if [ "$gate_rc" -eq 0 ] && [ "$env_tests" = "true" ]; then
    set +e
    scripts/layer7.sh >>"$gatelog" 2>&1
    gate_rc=$?
    set -e
  fi
  if [ "$gate_rc" -ne 0 ]; then
    tail -n 50 "$gatelog"
    escalate "$id" pipeline-red "$gatelog"
  fi

  "$DT_STORY" complete "$STORY_FILE" "$id" --commit "$(git rev-parse HEAD)"
  echo "STORY_RUN: COMPLETE task=${id} commit=$(git rev-parse --short HEAD)"
done

# Story-close gate: the full pipeline, layer 7 included, on the final tree.
closelog="$RUN_DIR/story-close.gate.log"
set +e
./scripts/layer-all.sh >"$closelog" 2>&1
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
  tail -n 50 "$closelog"
  echo "STORY_RUN: story-close gate red (log=${closelog})" >&2
  exit "$rc"
fi

echo "STORY_RUN: ALL TASKS COMPLETE — story-close gate green. Next step: /close-story"
