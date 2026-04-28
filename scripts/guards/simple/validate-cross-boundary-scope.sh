#!/bin/bash
#
# Cross-Boundary Scope-Drift Guard (Layer A) — ADR-0024 §6
#
# Compares the active devloop's plan against the active edit's diff. Purpose:
# prevent a single devloop's edit from going wild beyond its declared scope.
# This guard is NOT an auditor of historical commits — it polices the active
# edit only.
#
# Scope resolution:
#   - If working tree has pending changes (staged, unstaged, or untracked):
#     diff = working tree vs HEAD
#   - Otherwise (clean tree):
#     diff = HEAD vs HEAD^ (most recent commit only)
#
# The active main.md is whichever `docs/devloop-outputs/*/main.md` is added or
# modified in that scoped diff. By convention every devloop step touches its
# main.md (Loop State, verdicts, etc.), so this is a reliable beacon. Edits
# that touch no main.md are mechanical follow-ups, hotfixes, or unrelated to
# any devloop — they are out of scope for this guard and pass silently.
#
# Flags:
#   - Inbound drift: files in the diff but absent from the plan.
#   - Planned-untouched: files listed in the plan but absent from the diff.
#
# Runs at Gate 2 via run-guards.sh, and via pre-commit hook for staged work.
#
# Exit codes:
#   0 - all pass (includes: no main.md in scope — inert)
#   1 - one or more scope-drift violations
#   2 - script error or multi-devloop collision (one edit spans 2+ main.mds)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

# -----------------------------------------------------------------------------
# Resolve the diff scope. Echoes one of:
#   pending  — working tree has uncommitted/staged/untracked changes
#   head-1   — clean tree, compare HEAD vs HEAD^
# Exits 0 silently when there is no scope to check (initial commit, no git).
# -----------------------------------------------------------------------------
resolve_scope() {
    if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree &>/dev/null; then
        # Not a git repo — nothing to compare.
        exit 0
    fi

    if [[ -n "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null)" ]]; then
        echo "pending"
        return 0
    fi

    if git -C "$REPO_ROOT" rev-parse --verify --quiet HEAD^ >/dev/null 2>&1; then
        echo "head-1"
        return 0
    fi

    # Initial commit or detached HEAD with no parent — nothing to scope.
    exit 0
}

# -----------------------------------------------------------------------------
# Get all diff paths in the resolved scope. Includes added, modified, deleted,
# renamed — the plan should declare every touched path.
# -----------------------------------------------------------------------------
get_diff_paths() {
    local scope="$1"
    case "$scope" in
        pending)
            {
                git -C "$REPO_ROOT" diff HEAD --name-only 2>/dev/null || true
                git -C "$REPO_ROOT" ls-files --others --exclude-standard 2>/dev/null || true
            } | sort -u
            ;;
        head-1)
            git -C "$REPO_ROOT" diff HEAD^ HEAD --name-only 2>/dev/null | sort -u || true
            ;;
    esac
}

# -----------------------------------------------------------------------------
# Find the active devloop's main.md from the scoped diff. Looks for main.md
# files under docs/devloop-outputs/ that appear in the diff (added or modified).
# Echoes one path per line; empty if none found.
# -----------------------------------------------------------------------------
find_active_main_md() {
    local diff_paths="$1"
    echo "$diff_paths" | grep -E '^docs/devloop-outputs/[^/]+/main\.md$' || true
}

# -----------------------------------------------------------------------------
# Check the active main.md for scope drift against the scoped diff.
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
    local scope
    scope=$(resolve_scope)

    local diff_paths
    diff_paths=$(get_diff_paths "$scope")

    if [[ -z "$diff_paths" ]]; then
        # Nothing changed in scope — inert.
        exit 0
    fi

    local active_mds
    active_mds=$(find_active_main_md "$diff_paths")

    if [[ -z "$active_mds" ]]; then
        # The active edit doesn't touch a devloop main.md — out of scope for
        # this guard. By convention, every devloop step touches its main.md;
        # commits/edits that don't are mechanical follow-ups, hotfixes, or
        # unrelated to a devloop, and have no plan to enforce against.
        exit 0
    fi

    local count
    count=$(echo "$active_mds" | wc -l)
    if [[ "$count" -gt 1 ]]; then
        echo "ERROR: scope contains $count main.md files — one edit shouldn't span multiple devloops:" >&2
        echo "$active_mds" | sed 's/^/  /' >&2
        exit 2
    fi

    local main_md="$REPO_ROOT/$active_mds"
    if [[ ! -f "$main_md" ]]; then
        # Deleted main.md — nothing to compare against.
        exit 0
    fi

    if ! check_main_md "$main_md" "$diff_paths"; then
        exit 1
    fi

    print_ok "No cross-boundary scope drift"
    exit 0
}

main "$@"
