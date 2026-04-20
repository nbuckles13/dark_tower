#!/bin/bash
#
# Cross-Boundary Scope-Drift Guard (Layer A) — ADR-0024 §6
#
# Compares the plan's ## Cross-Boundary Classification file list against the
# actual diff against the branch's merge-base with main. Flags:
#   - Inbound drift: files in the diff but absent from the plan.
#   - Planned-untouched: files listed in the plan but absent from the diff.
#
# Both directions surface the same kind of disconnect — the plan and the
# branch disagree on scope — and are flagged for Lead adjudication per
# ADR-0024 §6.3/§6.4 rules.
#
# Runs at Gate 2 via run-guards.sh with SEARCH_PATH.
#
# Exit codes:
#   0 - all pass (includes: no devloop main.md modified — inert)
#   1 - one or more scope-drift violations
#   2 - script error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

# -----------------------------------------------------------------------------
# Resolve the diff base. Returns a commit SHA on stdout and exit 0 on success.
# Exits the script with code 2 on true errors (git broken, detached HEAD with
# no merge-base, etc.); exits the script with code 0 silently when there is
# no devloop context to check (on main branch, no divergence from main).
# -----------------------------------------------------------------------------
resolve_diff_base() {
    if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree &>/dev/null; then
        # Not a git repo — nothing to compare.
        exit 0
    fi

    # Respect $GUARD_DIFF_BASE if set by CI (same contract as common.sh).
    if [[ -n "${GUARD_DIFF_BASE:-}" ]]; then
        echo "$GUARD_DIFF_BASE"
        return 0
    fi

    # Find the base-branch ref. Prefer local 'main'; fall back to 'origin/main'.
    # Both are common in this repo (worktrees sometimes lack a local main).
    local main_ref=""
    local main_name=""
    if git -C "$REPO_ROOT" rev-parse --verify --quiet main >/dev/null 2>&1; then
        main_ref=$(git -C "$REPO_ROOT" rev-parse main 2>/dev/null)
        main_name="main"
    elif git -C "$REPO_ROOT" rev-parse --verify --quiet origin/main >/dev/null 2>&1; then
        main_ref=$(git -C "$REPO_ROOT" rev-parse origin/main 2>/dev/null)
        main_name="origin/main"
    fi

    if [[ -z "$main_ref" ]]; then
        # No main ref available — unusual layout, nothing to compare.
        exit 0
    fi

    # If we're on main (HEAD points at same commit), no devloop to check.
    local head_ref
    head_ref=$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || true)
    if [[ -n "$head_ref" && "$head_ref" == "$main_ref" ]]; then
        exit 0
    fi

    local mb
    if mb=$(git -C "$REPO_ROOT" merge-base HEAD "$main_name" 2>/dev/null); then
        echo "$mb"
        return 0
    fi

    # main_name exists but no common ancestor — genuinely broken.
    echo "ERROR: no merge-base between HEAD and $main_name (detached history?)" >&2
    exit 2
}

# -----------------------------------------------------------------------------
# Find the current devloop's main.md — the one authored on this branch.
#
# A long-lived feature branch may accumulate multiple devloop directories
# over time; historical main.md files from prior devloops are complete
# artifacts and not this guard's concern. Selection strategy:
#   - Take the set of main.md files under docs/devloop-outputs/ that are
#     added-since-base OR untracked.
#   - If more than one, pick the single newest by filesystem mtime as a
#     proxy for "the devloop currently in flight." This matches the ADR-
#     0024 workflow assumption of one devloop per branch in flight at a
#     time, while tolerating legacy plans lingering on long-lived branches.
#
# Output: a single main.md path (repo-relative), or empty if none found.
# -----------------------------------------------------------------------------
find_current_main_md() {
    local base="$1"

    local added untracked candidates
    added=$(git -C "$REPO_ROOT" diff --name-only --diff-filter=A "$base" -- 'docs/devloop-outputs/' 2>/dev/null \
        | grep -E '/main\.md$' || true)
    untracked=$(git -C "$REPO_ROOT" ls-files --others --exclude-standard -- 'docs/devloop-outputs/' 2>/dev/null \
        | grep -E '/main\.md$' || true)

    candidates=$({
        echo "$added"
        echo "$untracked"
    } | grep -v '^$' | sort -u || true)

    [[ -z "$candidates" ]] && return 0

    # Single candidate — return it directly.
    local count
    count=$(echo "$candidates" | wc -l)
    if [[ "$count" -eq 1 ]]; then
        echo "$candidates"
        return 0
    fi

    # Multiple candidates — pick the newest by mtime.
    local newest=""
    local newest_mtime=0
    local file mtime
    while IFS= read -r file; do
        [[ -z "$file" ]] && continue
        [[ ! -f "$REPO_ROOT/$file" ]] && continue
        mtime=$(stat -c %Y "$REPO_ROOT/$file" 2>/dev/null || stat -f %m "$REPO_ROOT/$file" 2>/dev/null || echo 0)
        if [[ "$mtime" -gt "$newest_mtime" ]]; then
            newest_mtime="$mtime"
            newest="$file"
        fi
    done <<< "$candidates"

    echo "$newest"
}

# -----------------------------------------------------------------------------
# Get all diff paths (relative to repo root) since $base.
# -----------------------------------------------------------------------------
get_diff_paths() {
    local base="$1"
    {
        git -C "$REPO_ROOT" diff --name-only "$base" 2>/dev/null || true
        git -C "$REPO_ROOT" ls-files --others --exclude-standard 2>/dev/null || true
    } | sort -u
}

# -----------------------------------------------------------------------------
# Extract the "Start Commit" declared in main.md's Loop Metadata table.
# Format expected: | Start Commit | `<sha>` |
# Returns the sha on stdout, or empty string if absent/unresolvable.
# -----------------------------------------------------------------------------
extract_start_commit() {
    local main_md="$1"
    local sha
    # Grab anything inside backticks on the Start Commit row. Tolerate
    # whitespace variation.
    sha=$(awk -F'`' '
        /^\|[[:space:]]*Start Commit[[:space:]]*\|/ {
            if (NF >= 2) { print $2; exit }
        }
    ' "$main_md" 2>/dev/null)

    # Trim and validate: shas are hex of length >= 7.
    sha="${sha// /}"
    if [[ "$sha" =~ ^[0-9a-f]{7,40}$ ]]; then
        # Confirm the sha resolves in this repo.
        if git -C "$REPO_ROOT" rev-parse --verify --quiet "${sha}^{commit}" >/dev/null 2>&1; then
            echo "$sha"
            return 0
        fi
    fi
    echo ""
}

# -----------------------------------------------------------------------------
# Check one main.md for scope drift.
# Returns 0 if clean, 1 if drift found.
# -----------------------------------------------------------------------------
check_main_md() {
    local main_md="$1"
    local all_diff="$2"
    local rel_path="${main_md#"$REPO_ROOT/"}"
    local violations=0

    # Plan paths: parse the classification table, keep path column only.
    #
    # Auto-exclusions (applied symmetrically to both plan_paths and diff_paths):
    # - main.md itself — self-referential drift is not useful signal.
    # - `docs/specialist-knowledge/**/INDEX.md` — reflection-phase artifacts,
    #   authored by each specialist themselves during Step 8 reflection, after
    #   Gate 2 has already run. Listing them in every plan would be ceremony
    #   for an expected, owner-authored pattern. If a devloop lists them
    #   explicitly anyway (e.g., implementer plans an INDEX update as part of
    #   implementation), the exclusion is harmless — plan and diff both omit
    #   them in Layer A's calculation.
    local exclude_re
    exclude_re="^(${rel_path}|docs/specialist-knowledge/[^/]+/INDEX\\.md)\$"

    local rows plan_paths
    rows=$(parse_cross_boundary_table "$main_md")
    plan_paths=$(echo "$rows" \
        | awk -F'|' 'NF>=1 && $1!="" { print $1 }' \
        | grep -Ev "$exclude_re" \
        | sort -u)

    local diff_paths
    diff_paths=$(echo "$all_diff" | grep -Ev "$exclude_re" | sort -u || true)

    # Inbound drift: in diff, not in plan.
    local inbound
    inbound=$(comm -23 <(echo "$diff_paths") <(echo "$plan_paths") || true)

    # Planned but untouched: in plan, not in diff.
    local untouched
    untouched=$(comm -13 <(echo "$diff_paths") <(echo "$plan_paths") || true)

    if [[ -n "$inbound" ]]; then
        while IFS= read -r path; do
            [[ -z "$path" ]] && continue
            print_violation "$rel_path — scope-drift-inbound — \"$path\" is in the diff but not listed in the Cross-Boundary Classification table"
            violations=$((violations + 1))
        done <<< "$inbound"
    fi

    if [[ -n "$untouched" ]]; then
        while IFS= read -r path; do
            [[ -z "$path" ]] && continue
            print_violation "$rel_path — scope-drift-planned-untouched — \"$path\" is listed in the plan but not in the diff (either not yet implemented, or stale plan entry)"
            violations=$((violations + 1))
        done <<< "$untouched"
    fi

    return $((violations > 0 ? 1 : 0))
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
main() {
    local base
    base=$(resolve_diff_base)

    local modified_main_mds
    modified_main_mds=$(find_current_main_md "$base")
    if [[ -z "$modified_main_mds" ]]; then
        # No new devloop main.md authored on this branch — inert. Exit 0 silently.
        exit 0
    fi

    local total_violations=0
    local file
    while IFS= read -r file; do
        [[ -z "$file" ]] && continue
        local abs="$REPO_ROOT/$file"
        if [[ ! -f "$abs" ]]; then
            # Deleted main.md — nothing to compare against.
            continue
        fi

        # Prefer the plan's declared Start Commit as the diff base — it scopes
        # the comparison to just this devloop's work, not the whole feature
        # branch. Fall back to the merge-base resolved above.
        local per_file_base="$base"
        local start_commit
        start_commit=$(extract_start_commit "$abs")
        if [[ -n "$start_commit" ]]; then
            per_file_base="$start_commit"
        fi

        local all_diff
        all_diff=$(get_diff_paths "$per_file_base")

        if ! check_main_md "$abs" "$all_diff"; then
            total_violations=$((total_violations + 1))
        fi
    done <<< "$modified_main_mds"

    if [[ "$total_violations" -gt 0 ]]; then
        exit 1
    fi

    print_ok "No cross-boundary scope drift"
    exit 0
}

main "$@"
