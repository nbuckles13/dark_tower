#!/bin/bash
#
# Cross-Boundary Classification-Sanity Guard (Layer B) — ADR-0024 §6
#
# Checks the plan's ## Cross-Boundary Classification table against the
# ownership manifest. Enforces two narrow mechanical rules:
#
#   (a) GSA path cannot be classified Mechanical.
#   (b) GSA path with a non-Mine classification must have an Owner field
#       set, and that Owner must appear in the manifest's specialist list
#       for the path (union across matching globs).
#
# All semantic judgment — is this really Mechanical? is the intersection
# rule honored? — stays at Gate 1 human review per §6.6 design rationale.
#
# Invocation modes:
#   - Explicit (Gate 1): validate-cross-boundary-classification.sh <main.md>
#     Lead invokes directly before issuing "Plan approved" per
#     .claude/skills/devloop/SKILL.md Step 5.
#   - Default  (Gate 2): validate-cross-boundary-classification.sh [search-path]
#     run-guards.sh invokes with SEARCH_PATH; guard scans for modified
#     docs/devloop-outputs/**/main.md files in the diff.
#
# Exit codes:
#   0 - all pass (includes: no matching main.md files found — inert)
#   1 - one or more violations
#   2 - script error (missing manifest, unreadable main.md, etc.)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

MANIFEST="$SCRIPT_DIR/cross-boundary-ownership.yaml"

# -----------------------------------------------------------------------------
# Manifest parser — reads flat "glob": [specialist, ...] lines.
# Populates MANIFEST_GLOBS (ordered) and MANIFEST_SPECIALISTS[glob] (pipe-joined).
# -----------------------------------------------------------------------------
declare -a MANIFEST_GLOBS=()
declare -A MANIFEST_SPECIALISTS=()

load_manifest() {
    if [[ ! -f "$MANIFEST" ]]; then
        echo "ERROR: manifest not found: $MANIFEST" >&2
        exit 2
    fi

    local line glob specialists_raw
    while IFS= read -r line; do
        # Skip comments + blank lines.
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// }" ]] && continue

        # Match: "glob": [specialist, specialist, ...]
        if [[ "$line" =~ ^\"([^\"]+)\":[[:space:]]*\[([^]]+)\][[:space:]]*$ ]]; then
            glob="${BASH_REMATCH[1]}"
            specialists_raw="${BASH_REMATCH[2]}"
            # Normalize: remove spaces, turn commas into pipes for exact matching.
            specialists_raw="${specialists_raw// /}"
            specialists_raw="${specialists_raw//,/|}"
            MANIFEST_GLOBS+=("$glob")
            MANIFEST_SPECIALISTS["$glob"]="$specialists_raw"
        else
            echo "ERROR: cannot parse manifest line: $line" >&2
            exit 2
        fi
    done < "$MANIFEST"
}

# -----------------------------------------------------------------------------
# Glob match: does $path match $glob per manifest semantics?
# Supports trailing /** for any depth, literal paths, and simple globs.
# -----------------------------------------------------------------------------
path_matches_glob() {
    local path="$1"
    local glob="$2"

    # Literal match.
    [[ "$path" == "$glob" ]] && return 0

    # Trailing /** — match $prefix/ followed by anything.
    if [[ "$glob" == *"/**" ]]; then
        local prefix="${glob%/**}"
        [[ "$path" == "$prefix"/* ]] && return 0
        # Also match the prefix itself as a file (rare but possible).
        [[ "$path" == "$prefix" ]] && return 0
        return 1
    fi

    # Fall back to bash extglob matching.
    shopt -s extglob
    # shellcheck disable=SC2053
    [[ "$path" == $glob ]] && return 0
    return 1
}

# Union of specialist lists across all manifest globs matching $path.
# Output: pipe-joined, deduplicated.
specialists_for_path() {
    local path="$1"
    local union=""
    local glob
    for glob in "${MANIFEST_GLOBS[@]}"; do
        if path_matches_glob "$path" "$glob"; then
            if [[ -z "$union" ]]; then
                union="${MANIFEST_SPECIALISTS[$glob]}"
            else
                union="${union}|${MANIFEST_SPECIALISTS[$glob]}"
            fi
        fi
    done
    # Dedupe by converting pipe-list to sorted unique.
    if [[ -n "$union" ]]; then
        echo "$union" | tr '|' '\n' | sort -u | paste -sd'|'
    fi
}

# Is $path a GSA (matches at least one manifest glob)?
path_is_gsa() {
    local path="$1"
    local specialists
    specialists=$(specialists_for_path "$path")
    [[ -n "$specialists" ]]
}

# Is $specialist in the pipe-joined $list?
specialist_in_list() {
    local specialist="$1"
    local list="$2"
    local item items
    IFS='|' read -ra items <<< "$list"
    for item in "${items[@]}"; do
        [[ "$item" == "$specialist" ]] && return 0
    done
    return 1
}

# -----------------------------------------------------------------------------
# Check one main.md file against the manifest.
# Returns 0 if clean, 1 if violations found. Prints VIOLATION lines to stdout.
# -----------------------------------------------------------------------------
check_main_md() {
    local main_md="$1"
    local rel_path="${main_md#"$REPO_ROOT/"}"
    local violations=0
    local rows
    rows=$(parse_cross_boundary_table "$main_md")

    # Empty table is valid (no classification rows yet).
    [[ -z "$rows" ]] && return 0

    local path classification owner
    while IFS='|' read -r path classification owner; do
        [[ -z "$path" ]] && continue

        # Skip "Mine" rows — no cross-boundary concern.
        if [[ "$classification" == "Mine" ]]; then
            continue
        fi

        local is_mechanical=0
        # Rule (a): "Mechanical" substring — matches "Not mine, Mechanical"
        # and bare "Mechanical" (malformed but possible per @test case 7).
        if [[ "$classification" == *"Mechanical"* ]]; then
            is_mechanical=1
        fi

        local is_gsa=0
        if path_is_gsa "$path"; then
            is_gsa=1
        fi

        # Rule (a): GSA path cannot be Mechanical.
        if [[ "$is_gsa" -eq 1 && "$is_mechanical" -eq 1 ]]; then
            print_violation "$rel_path — GSA-path-cannot-be-Mechanical — path \"$path\" is a Guarded Shared Area per ADR-0024 §6.4; route via owner-implements or Minor-judgment with owner confirmation"
            violations=$((violations + 1))
            continue
        fi

        # Rule (b): GSA path with non-Mine classification needs Owner in manifest.
        if [[ "$is_gsa" -eq 1 ]]; then
            # Owner must be non-empty and not a dash placeholder.
            if [[ -z "$owner" || "$owner" == "—" || "$owner" == "-" ]]; then
                print_violation "$rel_path — GSA-path-missing-owner — path \"$path\" is a GSA per ADR-0024 §6.4; Owner field must name a specialist from the ownership manifest"
                violations=$((violations + 1))
                continue
            fi

            local valid_specialists
            valid_specialists=$(specialists_for_path "$path")
            if ! specialist_in_list "$owner" "$valid_specialists"; then
                print_violation "$rel_path — GSA-owner-not-in-manifest — path \"$path\" lists Owner=\"$owner\" but manifest allows only {${valid_specialists//|/, }}"
                violations=$((violations + 1))
                continue
            fi
        fi
    done <<< "$rows"

    return $((violations > 0 ? 1 : 0))
}

# -----------------------------------------------------------------------------
# Find main.md files to check.
# Explicit mode: single file passed on CLI.
# Default mode: scan for modified docs/devloop-outputs/**/main.md in the diff.
# -----------------------------------------------------------------------------
find_main_md_files() {
    local arg="${1:-}"

    # Explicit mode: arg is an existing .md file.
    if [[ -n "$arg" && -f "$arg" && "$arg" == *.md ]]; then
        echo "$arg"
        return 0
    fi

    # Default mode: arg is search path (or empty → repo root).
    # Use git to find modified main.md files under docs/devloop-outputs/.
    # If not in a git repo or no diff base resolvable, return nothing (exit 0).
    if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree &>/dev/null; then
        return 0
    fi

    local base
    # Prefer explicit $GUARD_DIFF_BASE from common.sh, else merge-base with main.
    base=$(get_diff_base)
    if [[ "$base" == "HEAD" ]]; then
        # Local run; try merge-base with main for devloop-branch semantics.
        local mb
        if mb=$(git -C "$REPO_ROOT" merge-base HEAD main 2>/dev/null); then
            base="$mb"
        fi
        # If merge-base fails (e.g., on main itself), fall back to HEAD —
        # get_modified_files returns working-tree changes since last commit.
    fi

    # Modified + untracked, restricted to main.md under docs/devloop-outputs/.
    local mods untracked
    mods=$(git -C "$REPO_ROOT" diff --name-only "$base" -- 'docs/devloop-outputs/' 2>/dev/null || true)
    untracked=$(git -C "$REPO_ROOT" ls-files --others --exclude-standard -- 'docs/devloop-outputs/' 2>/dev/null || true)
    {
        echo "$mods"
        echo "$untracked"
    } | grep -E '/main\.md$' | sort -u || true
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
main() {
    load_manifest

    local arg="${1:-}"
    local files
    files=$(find_main_md_files "$arg")

    if [[ -z "$files" ]]; then
        # No main.md files matched — inert. Exit 0 silently.
        exit 0
    fi

    local total_violations=0
    local file
    while IFS= read -r file; do
        [[ -z "$file" ]] && continue
        # Absolute path for consistent reporting.
        [[ "$file" != /* ]] && file="$REPO_ROOT/$file"
        if [[ ! -f "$file" ]]; then
            # Deleted main.md — skip silently.
            continue
        fi
        if ! check_main_md "$file"; then
            total_violations=$((total_violations + 1))
        fi
    done <<< "$files"

    if [[ "$total_violations" -gt 0 ]]; then
        exit 1
    fi

    print_ok "No cross-boundary classification violations"
    exit 0
}

main "$@"
