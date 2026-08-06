#!/usr/bin/env bash
# _changed_helpers.sh — declarative diff predicates for per-language changed.sh.
#
# Each lang/<X>/changed.sh sources this file and uses these primitives instead
# of bespoke shell. Keeps every changed.sh to 3-5 lines of intent.
#
# Reads the changed-files cache produced by _get_base_ref.sh. If the cache
# doesn't exist (direct invocation outside a layer), invokes _get_base_ref.sh
# to populate it.

set -euo pipefail
IFS=$'\n\t'

[[ -n "${__DEVLOOP_CHANGED_HELPERS_SH:-}" ]] && return 0
readonly __DEVLOOP_CHANGED_HELPERS_SH=1

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${__here}/_common.sh"

# Path of the per-layer cache file (matches _get_base_ref.sh).
__changed_helpers_cache_file() {
  printf '%s/changed-files.layer-%s\n' "$DEVLOOP_TMP" "${DEVLOOP_LAYER:-shared}"
}

# Ensure the cache file exists; populate by invoking _get_base_ref.sh if not.
__ensure_cache() {
  local cache
  cache=$(__changed_helpers_cache_file)
  if [[ ! -f "$cache" ]]; then
    # Suppress stdout (sha) but let stderr (BASE_REF= line) flow.
    "${__here}/_get_base_ref.sh" >/dev/null
  fi
  printf '%s\n' "$cache"
}

# Print the cached changed-files list (one path per line).
# Outputs: stdout=changed paths
__changed_files() {
  local cache
  cache=$(__ensure_cache)
  cat "$cache"
}

# True if any changed file starts with the given path prefix.
# Args: $1=prefix (e.g. "crates/")
# Returns: 0 if matched, 1 if not
#
# Uses awk + index() for literal-string prefix matching (security finding 2).
# A future "c++" or "c#" lang directory would silently regex-match wrong files
# under grep "^prefix"; awk's index() is fixed-string and signals literal intent.
diff_touches_path() {
  local prefix="$1"
  __changed_files | awk -v p="$prefix" 'index($0, p) == 1 { found = 1; exit } END { exit !found }'
}

# True if any changed file matches one of the listed root-level paths exactly.
# Args: $@=root-level file paths (e.g. "Cargo.toml" "Cargo.lock")
# Returns: 0 if any match, 1 if none
#
# Uses grep -qxF (fixed-string, exact-line) per security finding 2 to keep the
# "literal filename" semantics explicit, not regex-permissive.
diff_touches_root_files() {
  local f
  for f in "$@"; do
    if __changed_files | grep -qxF -- "$f"; then
      return 0
    fi
  done
  return 1
}

# True if any changed file matches one of the listed shell-GLOB patterns.
# Args: $@=glob patterns (e.g. "crates/*/Cargo.toml")
# Returns: 0 if any changed path matches any pattern, 1 if none
#
# This is the ONLY glob matcher among the predicates. It uses bash [[ == ]] pattern
# matching — a GLOB, NOT a regex, and NOT the fixed-string match of diff_touches_path
# (awk index) or diff_touches_root_files (grep -qxF). The RHS is deliberately UNQUOTED
# so [[ ]] treats it as a pattern (a quoted RHS would be a literal-string compare). This
# is the same literal-vs-permissive discipline as security finding 2, made explicit: the
# other predicates stay fixed-string; only THIS one globs, and the SC2053 waiver marks
# the intent so a future reader doesn't "harden" the unquoting into a literal compare.
#
# IMPORTANT — in bash [[ == ]], `*` matches `/` too (UNLIKE pathname/filename globbing).
# So "crates/*/Cargo.toml" matches a Cargo.toml at ANY depth under crates/ — e.g.
# crates/ac-service/Cargo.toml AND the workspace-excluded crates/ac-service/fuzz/Cargo.toml.
# That is CORRECT and intended: any Cargo.toml anywhere under crates/ is a crate manifest,
# and matching = the fail-safe "run the scan" direction. The match is full-line anchored
# (no surrounding wildcards), so a prefix-only share (crates/foo/src/x.rs vs
# crates/*/Cargo.toml) does NOT match, and a suffix-differing sibling (crates/foo/Cargo.toml.bak)
# does NOT match.
#
# Consumes __changed_files — the SAME base-ref-validated cached diff every other predicate
# reads. It never runs its own git diff, so it cannot fail-open to an empty set on an
# enumeration error: a resolver failure surfaces upstream as a loud abort (set -e in
# __ensure_cache) or, in the audit path, as audit_gate's indeterminate->RUN. It thereby
# inherits the cache's clean, unquoted, repo-relative lines (no ./ prefix, no wrapping).
diff_touches_glob() {
  local line pat
  while IFS= read -r line; do
    for pat in "$@"; do
      # shellcheck disable=SC2053  # intentional glob match: RHS pattern MUST stay unquoted
      [[ "$line" == $pat ]] && return 0
    done
  done < <(__changed_files)
  return 1
}
