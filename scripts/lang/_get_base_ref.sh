#!/usr/bin/env bash
# _get_base_ref.sh — env-aware diff base resolver per ADR-0033 §7.
#
# Detects local devloop vs CI (PR or push), resolves the base ref, and emits
# the canonical normative stderr line:
#
#   BASE_REF=<sha> BASE_SOURCE=<source> DIFF_MODE=<mode> FILES_CHANGED=<count>
#
# Outputs:
#   stdout: <BASE_REF sha>             (for callers using "$(_get_base_ref.sh)")
#   stderr: BASE_REF=... line          (always, on every invocation — runbook anchor)
#   side effect: writes file list to ${DEVLOOP_TMP}/changed-files.layer-${DEVLOOP_LAYER:-shared}
#
# Exit codes: 0 success; non-zero on unreachable PR base ref or git failure.
#
# DO NOT enable trace mode (set -x); this script reads GITHUB_BASE_REF / GITHUB_TOKEN env in CI.

set -euo pipefail
IFS=$'\n\t'

# Source common (DEVLOOP_TMP, etc.) — but defer to defaults if not yet sourced.
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${__here}/_common.sh"

# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------

# Validate ref name shape (security §1 — defense in depth against env injection).
# Args: $1=ref-name
# Returns: 0 on valid, exits 2 on invalid (with stderr message)
__validate_ref_name() {
  local ref="$1"
  if [[ ! "$ref" =~ ^[A-Za-z0-9._/-]+$ ]]; then
    echo "ERROR: ref name contains unexpected characters: ${ref}" >&2
    exit 2
  fi
}

# Resolve a ref to a full 40-char sha via git rev-parse (observability §2).
# Args: $1=ref
# Outputs: stdout=full sha, stderr=git error if ref unresolvable
# Returns: git's exit code
__resolve_full_sha() {
  git rev-parse --verify "$1^{commit}" 2>/dev/null
}

# Emit the normative BASE_REF= stderr line.
# Args: $1=sha $2=source $3=mode $4=files-changed-count
__emit_base_ref_line() {
  printf 'BASE_REF=%s BASE_SOURCE=%s DIFF_MODE=%s FILES_CHANGED=%s\n' \
    "$1" "$2" "$3" "$4" >&2
}

# Path of the per-layer cache file (paired-operations Round 3 nit).
# Layer scripts set DEVLOOP_LAYER="<n>" before calling. Default "shared" for direct callers.
__cache_file_path() {
  printf '%s/changed-files.layer-%s\n' "$DEVLOOP_TMP" "${DEVLOOP_LAYER:-shared}"
}

# -----------------------------------------------------------------------------
# Main resolution
# -----------------------------------------------------------------------------

main() {
  init_devloop_tmp

  local base source mode sha cache_file
  cache_file=$(__cache_file_path)

  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    # CI mode
    if [[ "${GITHUB_EVENT_NAME:-}" == "pull_request" ]]; then
      # CI on PR: 3-dot diff vs origin/$GITHUB_BASE_REF
      __validate_ref_name "${GITHUB_BASE_REF:?GITHUB_BASE_REF required for pull_request event}"
      # Defensive fetch — sparse-checkouts and worktrees may not have base ref locally.
      # Suppress fetch stderr (could include token-bearing URLs per security O5).
      if ! git fetch --no-tags origin "$GITHUB_BASE_REF" 2>/dev/null; then
        echo "ERROR: PR base ref unreachable: GITHUB_BASE_REF=${GITHUB_BASE_REF}" >&2
        exit 2
      fi
      base="origin/${GITHUB_BASE_REF}"
      source="ci-pr"
      mode="three-dot"
      # TODO: emit `git merge-base origin/$GITHUB_BASE_REF HEAD` SHA on stdout
      # instead of tip-SHA so downstream consumers (e.g. `nx affected --base=$SHA`)
      # get three-dot-equivalent semantics. Tracked in docs/TODO.md under
      # "## Polyglot Pipeline Follow-ups (ADR-0033 Wave 1 #1)" — entry
      # "_get_base_ref.sh CI-PR tip-SHA vs merge-base-SHA divergence". Surfaced
      # by #36 (TS wrappers). Changed-files cache (line 121) already uses three-dot.
    else
      # CI on push: HEAD~1, with HEAD fallback for first-commit edge case.
      if git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
        base="HEAD~1"
        source="ci-push-main"
      else
        base="HEAD"
        source="ci-push-first-commit"
      fi
      mode="two-dot"
    fi
  else
    # Local mode
    # Validation is intentionally invoked even on the literal "origin/main" so all
    # call paths route through the same defense (security finding 3); if a future
    # refactor parameterizes the local-mode base ref, the validation guard is
    # already in place.
    __validate_ref_name "origin/main"
    if base_resolved=$(git merge-base origin/main HEAD 2>/dev/null); then
      base="$base_resolved"
      source="local-mergebase"
    else
      base="HEAD"
      source="local-no-mergebase"
    fi
    mode="two-dot"
  fi

  # Resolve to full sha for unambiguous output.
  if ! sha=$(__resolve_full_sha "$base"); then
    echo "ERROR: could not resolve base ref to sha: ${base}" >&2
    exit 2
  fi

  # Compute changed-files list.
  if [[ "$mode" == "three-dot" ]]; then
    git diff --name-only "${base}...HEAD" > "$cache_file"
  else
    {
      git diff --name-only "$base"
      # Local mode: union in untracked files.
      if [[ -z "${GITHUB_ACTIONS:-}" ]]; then
        git ls-files --others --exclude-standard
      fi
    } | sort -u > "$cache_file"
  fi

  local files_changed
  files_changed=$(wc -l < "$cache_file" | tr -d '[:space:]')

  __emit_base_ref_line "$sha" "$source" "$mode" "$files_changed"
  printf '%s\n' "$sha"
}

main "$@"
