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
# Usage: scripts/workflow/run-story.sh <story-file.md | story-slug> [--stop-after=N]
#
#   --stop-after=N  exit 0 after task N completes (skips the story-close gate).
#                   For staged runs where a human step belongs between tasks.
#
# Exit: 0  all tasks complete, story-close gate green (or --stop-after reached)
#       1  task escalated (escalation.json path printed) or blocked
#       2  precondition / infra / manifest failure
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

ARG="${1:?usage: run-story.sh <story-file.md | story-slug> [--stop-after=N]}"
STOP_AFTER="${2:-}"
if [ -n "$STOP_AFTER" ]; then
  case "$STOP_AFTER" in
    --stop-after=*) STOP_AFTER="${STOP_AFTER#--stop-after=}" ;;
    *) echo "STORY_RUN: unknown argument '$STOP_AFTER'" >&2; exit 2 ;;
  esac
  [[ "$STOP_AFTER" =~ ^[0-9]+$ ]] || { echo "STORY_RUN: --stop-after needs a task id" >&2; exit 2; }
fi
if [ -f "$ARG" ]; then
  STORY_FILE="$ARG"
else
  # Slug form: suffix match against docs/user-stories/, mirroring /close-story.
  shopt -s nullglob
  matches=(docs/user-stories/*"${ARG}".md)
  shopt -u nullglob
  case "${#matches[@]}" in
    0) echo "STORY_RUN: no story file matches '${ARG}'" >&2; exit 2 ;;
    1) STORY_FILE="${matches[0]}" ;;
    *) echo "STORY_RUN: ambiguous slug '${ARG}' matches: ${matches[*]}" >&2; exit 2 ;;
  esac
fi
DT_STORY="target/release/dt-story"
RUN_DIR="${DEVLOOP_TMP:-/tmp/devloop}/story-runner/$(basename "${STORY_FILE%.md}")"
mkdir -p "$RUN_DIR"

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
# Judgment-heavy work (planning, escalation triage) stays on the strongest
# model in interactive sessions.
STORY_MODEL="${STORY_MODEL:-claude-opus-4-8}"

scripts/workflow/preflight-story.sh "$STORY_FILE"

# Best-effort cost telemetry: sum every result event in the task's log (all
# attempts, including cross-invocation resumes, append to the same file).
# Telemetry only — never routes control flow; classification uses the canary.
report_task_cost() {
  local id="$1" summary
  summary="$( (grep -h '"type":"result"' "$RUN_DIR/task-${id}.devloop.log" 2>/dev/null || true) \
    | jq -s --argjson task "$id" 'select(length > 0) | {
        task: $task, attempts: length,
        usd: (map(.total_cost_usd // 0) | add * 100 | round / 100),
        output_tokens: (map(.usage.output_tokens // 0) | add),
        cache_read_tokens: (map(.usage.cache_read_input_tokens // 0) | add),
        turns: (map(.num_turns // 0) | add),
        api_minutes: ((map(.duration_api_ms // 0) | add) / 60000 | round)
      }' 2>/dev/null)" || true
  [ -n "$summary" ] || return 0
  echo "$summary" >>"$RUN_DIR/cost-ledger.jsonl"
  echo "STORY_RUN: COST $(jq -r '"task=\(.task) attempts=\(.attempts) usd=\(.usd) output_tokens=\(.output_tokens) cache_read_tokens=\(.cache_read_tokens) turns=\(.turns) api_minutes=\(.api_minutes)"' <<<"$summary")"
}

escalate() {
  local id="$1" reason="$2" log="$3"
  report_task_cost "$id"
  "$DT_STORY" escalate "$STORY_FILE" "$id" --reason "$reason" --log "$log" \
    --out "$RUN_DIR/escalation.json"
  echo "STORY_RUN: ESCALATED task=${id} reason=${reason} log=${log}" >&2
  echo "STORY_RUN: escalation record: ${RUN_DIR}/escalation.json" >&2
  exit 1
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
  timeout 120 claude -p "Reply with exactly: ok" --model haiku \
    --output-format json --dangerously-skip-permissions >"$out" 2>&1
}

# Classify a canary output file: healthy | session-limit | auth-expired | infra
canary_classify() {
  local out="$1" is_err status text
  is_err="$(jq -r '.is_error // false' "$out" 2>/dev/null)" || { echo infra; return 0; }
  status="$(jq -r '.api_error_status // empty' "$out" 2>/dev/null)" || true
  text="$(jq -r '.result // empty' "$out" 2>/dev/null)" || true
  if [ "$is_err" = "false" ] && [ -n "$text" ]; then echo healthy; return 0; fi
  if [ "$status" = "429" ] || printf '%s' "$text" | grep -q 'session limit'; then
    echo session-limit; return 0
  fi
  if printf '%s' "$text" | grep -qi 'authenticat\|oauth'; then
    echo auth-expired; return 0
  fi
  echo infra
}

# Persist the --continue pointer for the current task if its devloop created
# an output dir and nothing was committed. Uses the per-task vars set in the
# main loop. Safe to call on any exit path; no-op if already persisted.
persist_resume_pointer() {
  [ -s "$slug_file" ] && return 0
  [ "$(git rev-parse HEAD)" != "$head_before" ] && return 0
  local d
  d="$(find docs/devloop-outputs -mindepth 1 -maxdepth 1 -type d \
    -newer "$start_marker" -printf '%T@ %f\n' 2>/dev/null | sort -rn | head -n 1 | cut -d' ' -f2-)"
  if [ -n "$d" ] && [ -f "docs/devloop-outputs/${d}/main.md" ]; then
    echo "$d" >"$slug_file"
  fi
  return 0
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
  stop_count_file="$RUN_DIR/task-${id}.stopblocks"
  slug_file="$RUN_DIR/task-${id}.slug"
  start_marker="$RUN_DIR/task-${id}.start"
  rm -f .devloop-escalation.json "$stop_count_file"

  # Resume detection: a persisted slug (written on any uncommitted exit) means
  # this task has an interrupted devloop to continue instead of a fresh start.
  continue_slug=""
  if [ -s "$slug_file" ]; then
    s="$(cat "$slug_file")"
    [ -f "docs/devloop-outputs/${s}/main.md" ] && continue_slug="$s"
  fi
  if [ -z "$continue_slug" ]; then
    # Fresh starts require a clean tree; resumes tolerate (expect) the
    # interrupted devloop's uncommitted work.
    if ! git diff --quiet || ! git diff --cached --quiet; then
      echo "STORY_RUN: dirty tree and no resumable devloop for task ${id} — clean up, or point ${slug_file} at the interrupted devloop dir" >&2
      exit 2
    fi
  fi

  echo "STORY_RUN: START task=${id} specialist=${specialist} resume=${continue_slug:-no} log=${tasklog}"
  touch "$start_marker"
  limit_waits=0
  while :; do
    if [ -n "$continue_slug" ]; then
      task_prompt="$(printf 'HEADLESS RUN (run-story task #%s, resumed): follow the devloop skill including its Headless Mode section.\n/devloop "This devloop was interrupted before completion. Resume from main.md state: finish incomplete phases, then gates and commit as normal." --continue=%s' \
          "$id" "$continue_slug")"
    else
      task_prompt="$(printf 'HEADLESS RUN (run-story task #%s): follow the devloop skill including its Headless Mode section.\n/devloop "%s" --specialist=%s' \
          "$id" "$(cat "$prompt_file")" "$specialist")"
    fi

    # stream-json + verbose: default text mode prints only the final result at
    # session end, leaving the log empty for the whole run. JSONL events make
    # `tail -f` useful; filter with e.g.
    #   jq -r 'select(.type=="assistant") | .message.content[]? | .text? // empty'
    set +e
    DEVLOOP_HEADLESS=1 DEVLOOP_START_HEAD="$head_before" \
      DEVLOOP_STOP_COUNT_FILE="$stop_count_file" \
      timeout "$TASK_TIMEOUT" claude -p "$task_prompt" \
      --model "$STORY_MODEL" \
      --output-format stream-json --verbose \
      --dangerously-skip-permissions >>"$tasklog" 2>&1
    claude_rc=$?
    set -e

    if [ "$claude_rc" -ne 0 ]; then
      cfile="$RUN_DIR/task-${id}.canary.json"
      canary_probe "$cfile" || true
      case "$(canary_classify "$cfile")" in
        auth-expired)
          # Pure infra, needs a human (host /login + container restart to
          # re-copy credentials). No manifest edit — the task stays pending
          # and the resume pointer survives, so a rerun picks up cleanly.
          persist_resume_pointer
          echo "STORY_RUN: AUTH-EXPIRED task=${id} — run /login on the host, restart the container (entrypoint re-copies credentials), then rerun to resume" >&2
          exit 2
          ;;
        infra)
          persist_resume_pointer
          echo "STORY_RUN: INFRA task=${id} — API unreachable or canary unclassifiable (${cfile}); manifest untouched, rerun to resume" >&2
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
              continue_slug="$(find docs/devloop-outputs -mindepth 1 -maxdepth 1 -type d \
                -newer "$start_marker" -printf '%T@ %f\n' 2>/dev/null | sort -rn | head -n 1 | cut -d' ' -f2-)"
              if [ -n "$continue_slug" ] && [ -f "docs/devloop-outputs/${continue_slug}/main.md" ]; then
                echo "$continue_slug" >"$slug_file"
              else
                continue_slug=""   # nothing to resume — clean up and retry fresh
                git reset --hard -q "$head_before"
                git clean -fdq
              fi
            fi
            echo "STORY_RUN: SESSION-LIMIT task=${id} wait=${sleep_secs}s resume=${continue_slug:-fresh} (${limit_waits}/${SESSION_LIMIT_RETRIES})"
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
  [ "$claude_rc" -eq 124 ] && escalate "$id" devloop-timeout "$tasklog"
  [ "$claude_rc" -ne 0 ] && escalate "$id" devloop-session-error "$tasklog"
  [ "$(git rev-parse HEAD)" = "$head_before" ] && escalate "$id" devloop-no-commit "$tasklog"

  # Gate: fast floor (layers 1-6) every task; layer 7 only when the task is
  # tagged env_tests. Full pipeline incl. layer 7 runs once at story close.
  gate_layers="1-6"
  [ "$env_tests" = "true" ] && gate_layers="1-6+7"
  echo "STORY_RUN: GATE task=${id} layers=${gate_layers} running (log=${gatelog})"
  gate_start="$(date +%s)"
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
  echo "STORY_RUN: GATE task=${id} layers=${gate_layers} rc=${gate_rc} elapsed=$(( $(date +%s) - gate_start ))s"
  if [ "$gate_rc" -ne 0 ]; then
    tail -n 50 "$gatelog"
    escalate "$id" pipeline-red "$gatelog"
  fi

  # Fold the manifest bump into the devloop's own commit. No --commit sha in
  # the manifest: amending changes the sha, so it can't be recorded inside the
  # commit it refers to. Fall back to a separate chore commit if the amend is
  # rejected (e.g. a hook that pins devloop-commit trees).
  "$DT_STORY" complete "$STORY_FILE" "$id"
  git add "$STORY_FILE"
  if ! git commit --quiet --amend --no-edit; then
    git commit --quiet -m "chore(story): task #${id} complete (run-story manifest bump)"
  fi
  rm -f "$slug_file" "$start_marker" "$stop_count_file"
  echo "STORY_RUN: COMPLETE task=${id} commit=$(git rev-parse --short HEAD)"
  report_task_cost "$id"

  if [ -n "$STOP_AFTER" ] && [ "$id" = "$STOP_AFTER" ]; then
    echo "STORY_RUN: STOPPED after task ${id} (--stop-after) — story-close gate NOT run; rerun without the flag to continue"
    exit 0
  fi
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

# Story-level cost rollup (last ledger entry per task — later entries for a
# resumed task are supersets of earlier partials).
if [ -f "$RUN_DIR/cost-ledger.jsonl" ]; then
  jq -s -r 'group_by(.task) | map(last)
    | "STORY_RUN: STORY-COST tasks=\(length) usd=\(map(.usd) | add * 100 | round / 100) output_tokens=\(map(.output_tokens) | add) api_minutes=\(map(.api_minutes) | add)"' \
    "$RUN_DIR/cost-ledger.jsonl" 2>/dev/null || true
fi

echo "STORY_RUN: ALL TASKS COMPLETE — story-close gate green. Next step: /close-story"
