#!/bin/bash
#
# Simple Guard: No Test Removal (TypeScript)
#
# v1 scope per @test rule-owner (ADR-0024 §6.2) and @team-lead ruling:
# file-deletion only. The net (it|test)( block-count heuristic is dropped
# for v1 — block-counting across multi-line `it.each(`, `test.skip(`,
# template literals is brittle and erodes review-trust on false positives.
# Strict-cheap-v1 + defer-heuristic mirrors docs/TODO.md:341 (2026-05-08
# guard-self-test-cleanup) project pattern. Deferred block-count guard is
# captured in main.md §Tech Debt #2.
#
# See docs/devloop-outputs/2026-05-12-ts-guards-task37/main.md
# §Per-guard design #3.
#
# Exit codes:
#   0 - No violations / no TS-test files changed
#   1 - Violations found (test file deleted without replacement)
#   2 - Script error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../common.sh"

SEARCH_PATH="${1:-.}"

init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: No Test Removal (TS)
Path: $SEARCH_PATH
Diff base: $DIFF_BASE"

# Collect all changed and deleted TS-test files. Patterns tracked:
#   packages/**/*.{test,spec}.{ts,tsx}
#   packages/**/__tests__/**/*.{ts,tsx}
#   *.test.svelte (forward-compat; no Svelte tests yet)
collect_test_files() {
    local kind="$1"  # "deleted" or "added" or "modified"
    local getter
    case "$kind" in
        deleted)  getter=get_deleted_files ;;
        added)    getter=get_added_files ;;
        modified) getter=get_modified_files ;;
        *) echo "internal error: bad kind $kind" >&2; exit 2 ;;
    esac

    # Fetch the broad list (no extension filter); we apply our own regex-
    # safe suffix matching below. Rationale: common.sh's `get_*_files`
    # `grep "${ext}$"` interprets `${ext}` as regex, so a pattern like
    # `.test.ts$` falsely matches `featureXtest.ts` (the literal `.` in
    # `.test` matches any character). Per @test Finding 1 (Gate 3) we do
    # explicit suffix matching here rather than relying on the shared
    # helper's interpretation. (Fixing common.sh itself is out of scope
    # for this devloop — it would change behavior of every existing
    # guard.)
    local all_files
    all_files=$("$getter" "$SEARCH_PATH")

    [[ -z "$all_files" ]] && return 0

    # Test-file patterns:
    #   *.test.ts / *.spec.ts / *.test.tsx / *.spec.tsx / *.test.svelte
    #   any *.ts / *.tsx under a __tests__/ directory
    echo "$all_files" | while IFS= read -r f; do
        [[ -z "$f" ]] && continue
        case "$f" in
            *.test.ts|*.spec.ts|*.test.tsx|*.spec.tsx|*.test.svelte)
                echo "$f"
                ;;
            */__tests__/*.ts|*/__tests__/*.tsx)
                echo "$f"
                ;;
            *)
                # also catch deeper nested __tests__/ paths
                if [[ "$f" == *"/__tests__/"* && ( "$f" == *.ts || "$f" == *.tsx ) ]]; then
                    echo "$f"
                fi
                ;;
        esac
    done | sort -u
}

filter_excluded() {
    grep -Ev '/(node_modules|dist|build|\.svelte-kit|coverage|\.nx/cache)/' || true
}

DELETED=$(collect_test_files deleted | filter_excluded)
ADDED=$(collect_test_files added | filter_excluded)
MODIFIED=$(collect_test_files modified | filter_excluded)

# Empty-diff idempotence (ADR-0033 self-classification clause).
if [[ -z "$DELETED" && -z "$ADDED" && -z "$MODIFIED" ]]; then
    echo -e "${GREEN}No TS-test files changed compared to ${DIFF_BASE}${NC}"
    print_elapsed_time
    exit 0
fi

# -----------------------------------------------------------------------------
# Check 1 (only check in v1): deleted test files without a matching addition
# -----------------------------------------------------------------------------
print_section "Check 1: Deleted test files require a matching addition"

if [[ -z "$DELETED" ]]; then
    print_ok "No deleted test files"
    echo ""
else
    # Pre-compute basename set ONCE outside the per-deletion loop. Per @test
    # Finding 1 (Gate 3): the earlier `grep -q "/${local_base}$"` interpreted
    # the dynamic basename as a regex, so `feature.test.ts` would match
    # unrelated `featureXtest.ts` (`.` matches any char). Switching to
    # `grep -Fxq` (fixed-string, whole-line) against the basename set is
    # both correct AND drops the per-deletion cost from O(N×M) to O(N+M).
    ADDED_BASENAMES=$(echo "$ADDED" | awk -F/ '{print $NF}' | sort -u)

    while IFS= read -r deleted_file; do
        [[ -z "$deleted_file" ]] && continue
        local_base=$(basename "$deleted_file")
        # Extract the `packages/<pkg-name>` prefix. If the deleted file is not
        # under packages/* (shouldn't happen given the file-pattern filter
        # upstream), the sed leaves the path unchanged — and the same-package
        # fallback below then matches nothing, so we fall back to basename-
        # only matching. Safe behavior either way.
        local_pkg=$(echo "$deleted_file" | sed -E 's#^(packages/[^/]+)/.*#\1#')

        # Match policy (relaxed per @test): same basename in additions, OR
        # any new test file added in the same package directory.
        #
        # Renamed test files: get_deleted_files uses --diff-filter=D
        # without -M/-C rename detection, so a rename (e.g.
        # __tests__/bar.test.ts -> src/bar.test.ts within the same
        # package) appears as a delete+add pair. The same-basename match
        # below catches that case naturally — a rename to the same
        # filename in the same package counts as a match. If a future
        # change ever switches to -M-aware diff, the `R` filter would
        # also need handling.
        #
        # `grep -Fxq -- ...` = fixed-string + whole-line; `--` guards
        # against basenames that start with `-`. Same for the same-package
        # fallback: `grep -Fq -- "${local_pkg}/"` so a package name
        # containing regex metachars (theoretical; sed extracts a single
        # path segment) is also handled correctly.
        matched=false
        if echo "$ADDED_BASENAMES" | grep -Fxq -- "$local_base"; then
            matched=true
        elif [[ -n "$local_pkg" ]] && echo "$ADDED" | grep -Fq -- "${local_pkg}/"; then
            matched=true
        fi

        if [[ "$matched" == false ]]; then
            echo -e "  ${RED}VIOLATION${NC}: $deleted_file"
            echo "    No matching test addition found (same basename or same package)."
            increment_violations
        fi
    done <<< "$DELETED"
    echo ""
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
print_header "Summary"

TOTAL_VIOLATIONS=$(get_violations)
print_elapsed_time
echo ""

if [[ $TOTAL_VIOLATIONS -gt 0 ]]; then
    echo -e "${RED}Found $TOTAL_VIOLATIONS violation(s)${NC}"
    echo ""
    echo "Test files cannot be silently deleted. Options:"
    echo "  1. Add a replacement test file in the same package"
    echo "  2. If the test is genuinely obsolete, add a commit message"
    echo "     explaining why and seek reviewer ack."
    echo ""
    exit 1
else
    echo -e "${GREEN}No test removals detected${NC}"
    exit 0
fi
