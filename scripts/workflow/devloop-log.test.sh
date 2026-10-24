#!/usr/bin/env bash
# devloop-log.test.sh — self-test for scripts/workflow/devloop-log.sh (+ devloop-log.jq).
#
# A synthetic stream-json log, shaped like a real headless devloop session, pins each
# phase signal and its false-positive twin (a grep of the Phase row is not a transition;
# `grep layer-all.sh` is not a gate run), name attribution for teammates and background
# commands, de-duplication, the view filters, and the follow loop moving to a newer
# task log. Times are rendered with TZ=UTC so expectations are fixed.
#
# Wired into scripts/layer3.sh. Consumes scripts/lang/_test_helpers.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

WORK="$(mktemp -d)"
FOLLOW_PID=""
trap '[ -n "$FOLLOW_PID" ] && kill "$FOLLOW_PID" 2>/dev/null; rm -rf "$WORK"' EXIT
export TZ=UTC NO_COLOR=1
DL="${__here}/devloop-log.sh"
RUNS="${WORK}/story-runs/story-runner/story-x"
mkdir -p "$RUNS"
LOG="${RUNS}/task-1.devloop.log"

ts() { printf '2026-10-24T10:%02d:00.000Z' "$1"; }
lead_tool() { # minute name input-json [id]
  jq -nc --arg t "$(ts "$1")" --arg n "$2" --argjson i "$3" --arg id "${4:-x$1}" \
    '{type:"assistant",parent_tool_use_id:null,timestamp:$t,message:{content:[{type:"tool_use",id:$id,name:$n,input:$i}]}}'
}
lead_text() {
  jq -nc --arg t "$(ts "$1")" --arg s "$2" '{type:"assistant",parent_tool_use_id:null,timestamp:$t,message:{content:[{type:"text",text:$s}]}}'
}
agent_tool() { # minute parent name input-json [id]
  jq -nc --arg t "$(ts "$1")" --arg p "$2" --arg n "$3" --argjson i "$4" --arg id "${5:-y$1}" \
    '{type:"assistant",parent_tool_use_id:$p,subagent_type:"client",timestamp:$t,message:{content:[{type:"tool_use",id:$id,name:$n,input:$i}]}}'
}
result() { # minute parent(or "") tool_use_id text [is_error]
  jq -nc --arg t "$(ts "$1")" --arg p "$2" --arg id "$3" --arg s "$4" --argjson e "${5:-false}" \
    '{type:"user",parent_tool_use_id:(if $p == "" then null else $p end),timestamp:$t,message:{content:[{type:"tool_result",tool_use_id:$id,is_error:$e,content:[{type:"text",text:$s}]}]}}'
}
cmd() { jq -nc --arg c "$1" '{command:$c}'; }

OK6=$'LAYER=1 RESULT=OK DURATION=8\nLAYER=6 RESULT=OK DURATION=6\nTOTAL_DURATION=343 TOTAL_RESULT=OK'
FAIL6=$'LAYER=1 RESULT=OK DURATION=8\nLAYER=4 RESULT=FAIL DURATION=150\nLAYER=6 RESULT=OK DURATION=6\nTOTAL_DURATION=355 TOTAL_RESULT=FAIL'
{
  echo '{"type":"system","subtype":"init","model":"claude-test","claude_code_version":"9.9.9"}'
  lead_tool 1 Agent '{"name":"implementer","subagent_type":"client","description":"C-B implementer"}' toolu_impl
  echo '{"type":"system","subtype":"task_started","task_id":"a1","tool_use_id":"toolu_impl","task_type":"local_agent","subagent_type":"client"}'
  lead_tool 2 Bash "$(cmd 'sed -i "s/| Phase | `setup` |/| Phase | `planning` |/" main.md')"
  lead_tool 3 Bash "$(cmd 'grep -n "| Phase | `review` |" main.md')"
  lead_text 4 $'Plan approved;\nimplementation underway.'
  agent_tool 5 toolu_impl SendMessage '{"to":"security","message":"plan ready"}'
  agent_tool 6 toolu_impl Bash "$(cmd 'cd /work; timeout 590 ./scripts/layer-fast.sh > /tmp/f.log 2>&1')" g1
  result 7 toolu_impl g1 "$FAIL6"
  agent_tool 8 toolu_impl Bash "$(cmd 'grep -n layer-all.sh docs/x.md')"
  result 9 toolu_impl y8 "$OK6"
  result 10 toolu_impl y8 "$OK6"
  echo '{"type":"system","subtype":"task_started","task_id":"b1","parent_task_id":"a1","task_type":"local_bash","description":"deploy"}'
  echo '{"type":"system","subtype":"task_notification","task_id":"b1","status":"completed","summary":"deploy done"}'
  echo '{"type":"system","subtype":"task_notification","task_id":"a1","status":"completed","summary":"waiting for review"}'
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","rateLimitType":"seven_day"}}'
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","rateLimitType":"seven_day"}}'
  echo '{"type":"user","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Stop hook feedback:\nHeadless devloop incomplete"}]}}'
  echo 'this line is torn {"type":'
  lead_tool 11 Edit '{"file_path":"main.md","old_string":"| Phase | `planning` |","new_string":"| Phase | `review` |"}'
  lead_tool 12 Bash "$(cmd 'git add -A && git commit -q -F /tmp/m.txt')" c1
  result 13 "" c1 $'pre-commit refused\nEXIT=1'
  lead_tool 14 Bash "$(cmd 'git commit -F /tmp/m.txt')" c2
  result 15 "" c2 '[feature/x 54d6c0cc] Story 3 C-B: receive path'
  lead_tool 16 Write '{"file_path":"/work/.devloop-escalation.json","content":"{}"}'
  echo '{"type":"result","subtype":"success","is_error":false,"duration_ms":6979819,"num_turns":73,"total_cost_usd":87.7926,"result":"Task 14 committed.\nmore"}'
} > "$LOG"

set +e
phases="$("$DL" -n "$LOG" 2>&1)"; assert_rc "phases-exit-0" 0 $?
assert_status "start" "--:--:--  lead            session start · claude-test · claude 9.9.9" "$phases"
assert_status "spawn" "10:01:00  lead            spawn implementer (client): C-B implementer" "$phases"
assert_status "phase-sed" "10:02:00  lead            phase → planning" "$phases"
assert_absent "phase-grep-is-not-a-transition" "10:03:00" "$phases"
assert_status "phase-edit" "10:11:00  lead            phase planning → review" "$phases"
assert_status "gate-start-after-timeout-prefix" "10:06:00  implementer     gate layer-fast.sh started" "$phases"
assert_status "gate-fail-names-layer" "10:07:00  implementer     gate layers 1-6 FAIL (5m55s): L4 FAIL" "$phases"
assert_absent "grep-is-not-a-gate-run" "10:08:00" "$phases"
assert_status "gate-ok" "10:09:00  implementer     gate layers 1-6 OK (5m43s)" "$phases"
assert_rc "gate-repeat-deduped" 1 "$(grep -c 'gate layers 1-6 OK' <<<"$phases")"
assert_status "stop-hook" "lead            stop hook: Headless devloop incomplete" "$phases"
assert_rc "rate-limit-once" 1 "$(grep -c 'rate limit allowed_warning (seven_day)' <<<"$phases")"
assert_status "commit-refused" "10:13:00  lead            commit FAILED: pre-commit refused" "$phases"
assert_status "commit-sha" "10:15:00  lead            commit 54d6c0cc Story 3 C-B: receive path" "$phases"
assert_status "escalation" "10:16:00  lead            escalation file written" "$phases"
assert_status "end" "lead            end success · 73 turns · 1h56m · \$87.79 · Task 14 committed." "$phases"
assert_absent "phases-hides-text" "Plan approved" "$phases"
assert_absent "phases-hides-messages" "plan ready" "$phases"

lead="$("$DL" -n --lead "$LOG")"
assert_status "lead-text-folded" "10:04:00  lead            Plan approved; ⏎ implementation underway." "$lead"
assert_absent "lead-excludes-teammates" "implementer" "$lead"
assert_absent "lead-excludes-phases" "phase" "$lead"

msgs="$("$DL" -n --messages "$LOG")"
assert_status "message" "10:05:00  implementer     → security: plan ready" "$msgs"

status="$("$DL" -n --status "$LOG")"
assert_status "status-agent" "implementer     [completed] waiting for review" "$status"
assert_status "status-bg-owner" "implementer     [completed] bg: deploy done" "$status"

agent="$("$DL" -n --agent implementer --tools "$LOG")"
assert_status "agent-tools" "10:08:00  implementer     Bash: grep -n layer-all.sh docs/x.md" "$agent"
assert_absent "agent-excludes-lead" "  lead  " "$agent"

full="$("$DL" -n --lead --full "$LOG")"
assert_status "full-keeps-newlines" $'Plan approved;\n                          implementation underway.' "$full"
narrow="$("$DL" -n --lead --width 40 "$LOG")"
assert_status "width-truncates" "10:04:00  lead            Plan approved…" "$narrow"

"$DL" -n --width x "$LOG" >/dev/null 2>&1; assert_rc "bad-width-usage" 2 $?
"$DL" -n --bogus >/dev/null 2>&1; assert_rc "unknown-option-usage" 2 $?
"$DL" -n "${WORK}/missing.log" >/dev/null 2>&1; assert_rc "unreadable-log" 1 $?
out="$("$DL" -n --dir "${WORK}/empty" 2>&1)"; assert_rc "no-dir" 1 $?
assert_status "no-dir-names-path" "${WORK}/empty" "$out"

# --latest (the default): newest task log by mtime under the story-runs dir.
OLD="${RUNS}/task-0.devloop.log"
lead_text 30 "old task" > "$OLD"; touch -d '2026-01-01' "$OLD"
out="$(DEVLOOP_STORY_RUN_BASE="${WORK}/story-runs" "$DL" -n --lead)"
assert_status "latest-banner" "=== ${LOG} ===" "$out"
assert_absent "latest-skips-older" "old task" "$out"

# Follow (the default): picks up appended lines, then moves to the next task's log.
FOLLOW_OUT="${WORK}/follow.out"
DEVLOOP_LOG_POLL_SECS=1 DEVLOOP_STORY_RUN_BASE="${WORK}/story-runs" "$DL" --lead > "$FOLLOW_OUT" 2>&1 &
FOLLOW_PID=$!
wait_for() { local _; for _ in $(seq 1 100); do grep -qF "$1" "$FOLLOW_OUT" && return 0; sleep 0.1; done; return 1; }
wait_for "Plan approved"; assert_rc "follow-initial" 0 $?
lead_text 40 "appended live" >> "$LOG"
wait_for "appended live"; assert_rc "follow-appended" 0 $?
NEXT="${RUNS}/task-2.devloop.log"
lead_text 50 "next task started" > "$NEXT"
wait_for "next task started"; assert_rc "follow-switches-to-newer-log" 0 $?
assert_status "follow-switch-banner" "=== ${NEXT} ===" "$(cat "$FOLLOW_OUT")"
kill "$FOLLOW_PID" 2>/dev/null; wait "$FOLLOW_PID" 2>/dev/null; FOLLOW_PID=""

report_results "scripts/workflow/devloop-log.test.sh"
