#!/usr/bin/env bash
# _child_group.sh — run a long child in its OWN session and tear it down, bounded.
#
# The ONE home of this mechanism (moved from scripts/workflow/run-story.sh). Consumers:
#   - scripts/workflow/run-story.sh `run_isolated` (devloop session, gate layers, layer-all)
#   - scripts/layer-all.sh (every layer of every mode; its EXIT trap tears down the
#     in-flight layer and reaps a stale one an uncatchably-killed run left behind)
#
# DEPENDENCY-FREE: no shell options, no other source; requires setsid (util-linux) at
# call time — callers precheck it and fail loud (PRECONDITION_FAILURE) when absent.
#
# Why a session: `setsid` makes the child the leader of its own process group, so
#   (1) a terminal Ctrl-C (SIGINT to the foreground group) reaches only the parent, which
#       decides what to do with the work in flight; and
#   (2) the parent can signal the child's WHOLE tree (`kill -- -<pgid>`) — a layer is a tree
#       (layer bash → tee, cargo/nx/kubectl), and `$!` of a pipeline is only its last element.
# Launched with `&` from a non-job-control shell the child is not a group leader, so
# setsid execs in place: `$!` IS the session/group id.

# The teardown grace, in polls of CHILD_GROUP_POLL_SECS (TERM → poll → KILL). The ONE value:
# run-story's outer teardown uses the full grace; layer-all's in-trap teardown uses
# CHILD_GROUP_INNER_POLLS, DERIVED from it to stay strictly inside it — run-story's KILL to
# layer-all's session cannot reach a layer's session, so layer-all must finish its own
# teardown first or the layer is orphaned (scripts/layer-all.test.sh `nested-grace-race`).
CHILD_GROUP_GRACE_POLLS=100
CHILD_GROUP_POLL_SECS=0.05   # fine steps (same 5 s total): a group that exits at once costs ~0.05 s
CHILD_GROUP_INNER_POLLS=$(( CHILD_GROUP_GRACE_POLLS / 2 ))

# The in-flight child's session id ("" = none). Set by child_group_spawn, cleared by
# kill_child_group. A caller's signal handler may set __WAIT_INTERRUPTED=1 to make
# child_group_wait re-wait the same child (run-story's first Ctrl-C).
__CHILD_PID=""
__WAIT_INTERRUPTED=0

# child_group_spawn <cmd...> — start <cmd> in its own session, in the background; sets
# __CHILD_PID. Redirections on the CALL apply to the child (e.g. `>>"$log" 2>&1`,
# `</dev/null`, `{fd}>&-` to keep a lock fd out of the child).
child_group_spawn() {
  setsid "$@" &
  __CHILD_PID=$!
}

# child_group_wait <pid> — wait for <pid> INTERRUPTIBLY: a trapped signal returns `wait`
# early so the handler runs at once; if the handler set __WAIT_INTERRUPTED=1 the same child
# is waited again (bash keeps a reaped child's status for the re-wait). Returns the child's
# exit status. The caller owns errexit (call under `set +e` or in a condition).
child_group_wait() {
  local pid="$1" rc
  while :; do
    __WAIT_INTERRUPTED=0
    wait "$pid"
    rc=$?
    [ "$__WAIT_INTERRUPTED" -eq 1 ] && continue
    break
  done
  return "$rc"
}

# kill_child_group [polls] [pgid] [group-only] — terminate a session/group: TERM the group,
# then wait — polling the GROUP, not its leader — up to <polls> × CHILD_GROUP_POLL_SECS
# (default CHILD_GROUP_GRACE_POLLS) for every member to exit, then KILL whatever remains, then
# reap. Polling the group is what actually GRANTS the grace: a session leader (a wrapper) can
# die on TERM at once while the layer under it is still running its TERM/EXIT cleanup.
# <pgid> defaults to __CHILD_PID; only ever a RECORDED id, never a pattern. Never fails.
#   default    — <pgid> is our UNREAPED child (a live leader, or one that died but has not been
#                waited): falling back to the bare pid is safe (an unreaped pid cannot be reused)
#                and covers the instant before setsid() made it a group leader.
#   group-only — the leader may already be REAPED (layer-all's post-wait reap) or is not our
#                child (a stale session from a dead run): signal ONLY `-<pgid>`, never the bare
#                pid — a reaped pid can be recycled by an unrelated process, while a pgid number
#                stays allocated as long as any member of that group lives.
kill_child_group() {
  local polls="${1:-$CHILD_GROUP_GRACE_POLLS}" pid="${2:-${__CHILD_PID:-}}" mode="${3:-}" _i
  [ -n "$pid" ] || return 0
  if [ "$mode" = "group-only" ]; then
    kill -TERM -- "-${pid}" 2>/dev/null || true
  else
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
  fi
  for (( _i = 0; _i < polls; _i++ )); do
    child_group_has_live_member "$pid" || break
    sleep "$CHILD_GROUP_POLL_SECS"
  done
  if [ "$mode" = "group-only" ]; then
    kill -KILL -- "-${pid}" 2>/dev/null || true
  else
    kill -KILL -- "-${pid}" 2>/dev/null || kill -KILL "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  [ "$pid" = "${__CHILD_PID:-}" ] && __CHILD_PID=""
  return 0
}

# child_group_has_live_member <pgid> — true iff some process in group <pgid> is alive and not
# a zombie (our own exited-but-unreaped leader is a zombie and must not hold the poll open).
# Fork-free /proc scan (builtins only — see proc_starttime_into); without /proc, falls back
# to `kill -0 -- -<pgid>` (zombie-blind, still bounded by the caller's polls).
child_group_has_live_member() {
  local g="$1" f stat rest
  local -a fl=()
  [ -d /proc/self ] || { kill -0 -- "-${g}" 2>/dev/null; return; }
  kill -0 -- "-${g}" 2>/dev/null || return 1
  for f in /proc/[0-9]*/stat; do
    { IFS= read -r stat < "$f"; } 2>/dev/null || continue
    rest="${stat##*) }"
    IFS=' ' read -r -a fl <<< "$rest"
    # fl[0]=state fl[2]=pgrp
    [ "${fl[2]:-}" = "$g" ] && [ "${fl[0]:-}" != "Z" ] && return 0
  done
  return 1
}

# child_group_alive <pid> — true iff <pid> exists and is not a zombie. `kill -0` alone
# succeeds on an exited-but-unreaped child (our own leader after a TERM), which would make
# every teardown sit out its whole grace; /proc's state field tells them apart. Without
# /proc, falls back to `kill -0` (bounded either way).
child_group_alive() {
  local stat
  kill -0 "$1" 2>/dev/null || return 1
  { IFS= read -r stat < "/proc/$1/stat"; } 2>/dev/null || return 0   # builtin: no fork
  stat="${stat##*) }"
  [ "${stat%% *}" != "Z" ]
}

# proc_starttime_into <var> <pid> — set <var> to the process start time (/proc/<pid>/stat
# field 22, clock ticks since boot): with the pid, an identity that survives pid reuse. Empty
# if <pid> is gone. FORK-FREE (builtins only) on purpose: layer-all.sh calls it between
# spawning a layer and waiting on it, where a signal may land — and bash 5.2.x can lose a
# trapped signal that arrives while a command substitution runs ("trap: unexpected EOF while
# looking for matching `)'"), so that window holds no `$(…)`. comm (field 2) may contain
# spaces/parens, so parse after the LAST ')'; explicit IFS (the sourcer may use $'\n\t').
proc_starttime_into() {
  local -n __pst_out="$1"
  local stat rest
  local -a f=()
  __pst_out=""
  { IFS= read -r stat < "/proc/$2/stat"; } 2>/dev/null || return 0
  rest="${stat##*) }"
  IFS=' ' read -r -a f <<< "$rest"
  __pst_out="${f[19]:-}"
}

# proc_starttime <pid> — the same, printed (for callers outside a signal-sensitive window).
proc_starttime() {
  local __pst_v
  proc_starttime_into __pst_v "$1"
  printf '%s\n' "$__pst_v"
}
