#!/usr/bin/env bash
# devloop-log.sh — readable view of a headless devloop's stream-json log.
#
# run-story.sh runs each task as `claude -p --output-format stream-json` and keeps the
# session at <story-runs>/story-runner/<story>/task-<id>.devloop.log. That file is
# verbose JSON; this prints one line per event, so a story run can be watched from the
# host or from inside the devloop container. Parsing lives in devloop-log.jq.
#
# Usage:
#   devloop-log.sh [OPTIONS] [LOG | DIR]
#
# With no LOG, the newest task-*.devloop.log under the story-runs directory is used, and
# when following, the view moves to a newer log as soon as the next task starts. A DIR
# means the newest log inside it.
#
# Views (combine freely; default --phases):
#   --phases          phase changes, teammate spawns, gate runs and results, commits,
#                     Stop-hook feedback, rate limiting, escalation, session start/end
#   --lead            the Lead's own text and messages
#   --messages        every SendMessage, as `sender  → recipient: text`
#   --status          teammates' one-line status summaries
#   --agent NAME      everything one teammate does (NAME as spawned: implementer, security…)
#   --tools           add tool calls (to --lead/--agent if given, else for everyone)
#   --all             everything
#
# Output:
#   -n, --no-follow   print what is there and exit (default: follow, like tail -f)
#   --full            keep multi-line text instead of folding each event to one line
#   --width N         truncate lines to N characters (default: terminal width; 0 = none)
#   --no-color        plain output (default: colour when stdout is a terminal)
#   --dir DIR         story-runs directory to search (default below)
#   -h, --help
#
# The default story-runs directory is ${DEVLOOP_STORY_RUN_BASE:-$HOME/.cache/devloop/story-runs},
# the run-dir base resolve_run_base() in run-story.sh composes in production (devloop.sh sets
# DEVLOOP_STORY_RUN_BASE in the container; the host default is the same path under $HOME).
# If that drifts, the search finds nothing and this exits 1 naming where it looked.
#
# Exit: 0 ok, 1 no log found / unreadable, 2 usage error or missing jq.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JQ_PROG="$SCRIPT_DIR/devloop-log.jq"
POLL_SECS="${DEVLOOP_LOG_POLL_SECS:-5}"

usage() { sed -n '2,/^# Exit:/p' "$0" | sed 's/^# \{0,1\}//'; }
die() { echo "devloop-log: $*" >&2; exit "${2:-1}"; }

command -v jq >/dev/null 2>&1 || die "jq is required" 2

FOLLOW=1 FULL=false COLOR="" WIDTH="" DIR="" TARGET=""
WANT_PHASES=0 WANT_LEAD=0 WANT_MESSAGES=0 WANT_STATUS=0 WANT_TOOLS=0 WANT_ALL=0
AGENTS=()

while [ $# -gt 0 ]; do
  case "$1" in
    --phases) WANT_PHASES=1 ;;
    --lead) WANT_LEAD=1 ;;
    --messages) WANT_MESSAGES=1 ;;
    --status) WANT_STATUS=1 ;;
    --agent) [ $# -ge 2 ] || die "--agent needs a name" 2; AGENTS+=("$2"); shift ;;
    --agent=*) AGENTS+=("${1#--agent=}") ;;
    --tools) WANT_TOOLS=1 ;;
    --all) WANT_ALL=1 ;;
    -n|--no-follow) FOLLOW=0 ;;
    -f|--follow) FOLLOW=1 ;;
    --full) FULL=true ;;
    --width) [ $# -ge 2 ] || die "--width needs a number" 2; WIDTH="$2"; shift ;;
    --width=*) WIDTH="${1#--width=}" ;;
    --no-color) COLOR=false ;;
    --dir) [ $# -ge 2 ] || die "--dir needs a path" 2; DIR="$2"; shift ;;
    --dir=*) DIR="${1#--dir=}" ;;
    -h|--help) usage; exit 0 ;;
    -*) die "unknown option: $1 (see --help)" 2 ;;
    *) [ -z "$TARGET" ] || die "only one LOG or DIR may be given" 2; TARGET="$1" ;;
  esac
  shift
done

if [ -n "$WIDTH" ]; then
  case "$WIDTH" in ''|*[!0-9]*) die "--width must be a non-negative integer, got '$WIDTH'" 2 ;; esac
elif [ -t 1 ]; then
  WIDTH="$(tput cols 2>/dev/null || echo "${COLUMNS:-0}")"
else
  WIDTH=0
fi
if [ -z "$COLOR" ]; then
  if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then COLOR=true; else COLOR=false; fi
fi

# --- view rules: an event prints if any {who, kinds} rule matches (see devloop-log.jq) ---
PHASE_KINDS='["start","end","phase","spawn","gate","commit","hook","limit","escalation"]'
ALL_KINDS='["start","end","phase","spawn","gate","commit","hook","limit","escalation","text","msg","status","tool"]'
if [ "$WANT_PHASES$WANT_LEAD$WANT_MESSAGES$WANT_STATUS$WANT_TOOLS$WANT_ALL" = 000000 ] && [ ${#AGENTS[@]} -eq 0 ]; then
  WANT_PHASES=1
fi
rules='[]'
add_rule() { rules="$(jq -c --argjson w "$1" --argjson k "$2" '. + [{who: $w, kinds: $k}]' <<<"$rules")"; }
lead_kinds='["text","msg"]' agent_kinds='["text","msg","status","spawn","gate","commit"]'
if [ "$WANT_TOOLS" = 1 ]; then
  lead_kinds='["text","msg","tool"]' agent_kinds='["text","msg","status","spawn","gate","commit","tool"]'
fi
[ "$WANT_ALL" = 1 ] && add_rule null "$ALL_KINDS"
[ "$WANT_PHASES" = 1 ] && add_rule null "$PHASE_KINDS"
[ "$WANT_LEAD" = 1 ] && add_rule '"lead"' "$lead_kinds"
[ "$WANT_MESSAGES" = 1 ] && add_rule null '["msg"]'
[ "$WANT_STATUS" = 1 ] && add_rule null '["status"]'
for a in ${AGENTS[@]+"${AGENTS[@]}"}; do add_rule "$(jq -cn --arg a "$a" '$a')" "$agent_kinds"; done
if [ "$WANT_TOOLS" = 1 ] && [ "$WANT_LEAD" = 0 ] && [ ${#AGENTS[@]} -eq 0 ]; then
  add_rule null '["tool"]'
fi

render() {
  jq -nrR --unbuffered -f "$JQ_PROG" \
    --argjson rules "$rules" --argjson width "$WIDTH" --argjson full "$FULL" --argjson color "$COLOR"
}

# Newest task-*.devloop.log (by mtime) under $1, or nothing.
newest_log() {
  find "$1" -type f -name 'task-*.devloop.log' -printf '%T@ %p\n' 2>/dev/null \
    | sort -n | tail -n1 | cut -d' ' -f2-
}

banner() {
  local b="=== $1 ==="
  if [ "$COLOR" = true ]; then printf '\033[1;35m%s\033[0m\n' "$b"; else printf '%s\n' "$b"; fi
}

# --- resolve what to read ---
AUTO=0
if [ -z "$TARGET" ]; then
  AUTO=1
  SEARCH="${DIR:-${DEVLOOP_STORY_RUN_BASE:-${HOME}/.cache/devloop/story-runs}}"
elif [ -d "$TARGET" ]; then
  AUTO=1
  SEARCH="$TARGET"
else
  [ -z "$DIR" ] || die "--dir and an explicit LOG are mutually exclusive" 2
  [ -r "$TARGET" ] || die "cannot read $TARGET"
fi

if [ "$AUTO" = 1 ]; then
  [ -d "$SEARCH" ] || die "no story-runs directory at $SEARCH (use --dir or pass a LOG)"
  LOG="$(newest_log "$SEARCH")"
  [ -n "$LOG" ] || die "no task-*.devloop.log under $SEARCH"
else
  LOG="$TARGET"
fi

if [ "$FOLLOW" = 0 ]; then
  [ "$AUTO" = 1 ] && banner "$LOG"
  render < "$LOG"
  exit 0
fi

# --- follow: tail the log; in auto mode, move to a newer log when the next task starts ---
WORK="$(mktemp -d "${TMPDIR:-/tmp}/devloop-log.XXXXXX")"
TAIL_PID="" RENDER_PID=""
stop_pipe() {
  if [ -n "$TAIL_PID" ]; then kill "$TAIL_PID" 2>/dev/null || true; fi
  if [ -n "$RENDER_PID" ]; then wait "$RENDER_PID" 2>/dev/null || true; fi
  TAIL_PID="" RENDER_PID=""
}
cleanup() { stop_pipe; rm -rf "$WORK"; }
trap cleanup EXIT
trap 'exit 130' INT TERM

start_pipe() {
  local fifo="$WORK/pipe.$RANDOM"
  mkfifo "$fifo"
  banner "$1"
  render < "$fifo" &
  RENDER_PID=$!
  tail -n +1 -F "$1" 2>/dev/null > "$fifo" &
  TAIL_PID=$!
}

start_pipe "$LOG"
if [ "$AUTO" = 0 ]; then
  wait "$RENDER_PID"
  exit 0
fi
while kill -0 "$RENDER_PID" 2>/dev/null; do
  sleep "$POLL_SECS"
  next="$(newest_log "$SEARCH")"
  if [ -n "$next" ] && [ "$next" != "$LOG" ]; then
    stop_pipe
    LOG="$next"
    start_pipe "$LOG"
  fi
done
