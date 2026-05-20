#!/bin/bash
#
# Guard Runner: Execute all simple (pattern-based) guards
#
# Semantic guards are handled by the semantic-guard agent during devloops,
# not by this script. See .claude/agents/semantic-guard.md.
#
# Exit codes:
#   0 - All guards passed
#   1 - One or more guards failed
#   2 - Script error
#
# Usage:
#   ./run-guards.sh [options] [path]
#
# Options:
#   --verbose         Show detailed output
#   --help            Show this help message
#
# Examples:
#   ./run-guards.sh                              # Run simple guards on entire repo
#   ./run-guards.sh crates/ac-service/src/       # Run on specific directory
#

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common library for helper functions
source "$SCRIPT_DIR/common.sh"

# Colors (already defined in common.sh, but keep for standalone use)
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Default options
VERBOSE=false
SEARCH_PATH=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -25 "$0" | tail -20
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            exit 2
            ;;
        *)
            SEARCH_PATH="$1"
            shift
            ;;
    esac
done

# Default to repository root
if [[ -z "$SEARCH_PATH" ]]; then
    # Find repository root (go up until we find .git directory or file)
    # Note: In worktrees/clones, .git may be a file pointing to the main repo
    REPO_ROOT="$SCRIPT_DIR"
    while [[ ! -d "$REPO_ROOT/.git" ]] && [[ ! -f "$REPO_ROOT/.git" ]] && [[ "$REPO_ROOT" != "/" ]]; do
        REPO_ROOT="$(dirname "$REPO_ROOT")"
    done
    if [[ "$REPO_ROOT" == "/" ]]; then
        echo "Error: Could not find repository root (.git directory or file)"
        exit 2
    fi
    SEARCH_PATH="$REPO_ROOT"
fi

echo -e "${BOLD}=========================================="
echo "Guard Pipeline Runner"
echo "==========================================${NC}"
echo ""
echo "Path: $SEARCH_PATH"
echo ""

# Track results
TOTAL_GUARDS=0
PASSED_GUARDS=0
FAILED_GUARDS=0
declare -a FAILED_GUARD_NAMES

# Timer
START_TIME=$(date +%s.%N)

# -----------------------------------------------------------------------------
# Run Simple Guards
# -----------------------------------------------------------------------------
echo -e "${BOLD}Simple Guards${NC}"
echo "============="
echo ""

# Iterate simple/**/*.sh recursively (per ADR-0033 §1.5). Prune fixtures/ —
# no committed fixtures per docs/TODO.md:341 (2026-05-08 guard-self-test-
# cleanup policy); pruning structurally enforces the policy at the runner
# level so a future accidental fixture addition does not auto-execute.
SIMPLE_GUARDS_DIR="$SCRIPT_DIR/simple"
if [[ -d "$SIMPLE_GUARDS_DIR" ]]; then
    mapfile -d '' -t guards < <(
        find "$SIMPLE_GUARDS_DIR" -name "*.sh" -type f -not -path '*/fixtures/*' -print0 | sort -z
    )
    # Per-guard timeout per ADR-0034 §9 (strategy-independent hardening).
    # `GUARD_TIMEOUT_SECS` defaults to 30s, `GUARD_KILL_AFTER_SECS` to 5s.
    # Exit 124 → STATUS=FAIL REASON=guard-timeout-<name>
    # Exit 137 → STATUS=FAIL REASON=guard-timeout-kill-<name>
    # Capture form `local guard_exit=0 || guard_exit=$?` is load-bearing
    # under `set -euo pipefail` — without the `0` initializer + `||` capture,
    # a non-zero timeout exit aborts the for-loop before the classifier runs.
    GUARD_TIMEOUT_SECS="${GUARD_TIMEOUT_SECS:-30}"
    GUARD_KILL_AFTER_SECS="${GUARD_KILL_AFTER_SECS:-5}"

    # Single classifier per @test F1 fold-in 2026-05-19. Maps the `$1`
    # exit code from the timeout-wrapped guard invocation into one of
    # four classes: 0 (PASS), 124 (timeout), 137 (timeout-kill), or
    # other. Counters / FAILED_GUARD_NAMES are updated as side effects.
    # `$2` is the captured stdout+stderr for non-verbose callers; empty
    # for verbose callers (where output already streamed live). On the
    # generic-failure branch, captured output is greped for VIOLATION /
    # ERROR / WARN markers and the first 5 hits printed for triage.
    classify_guard_exit() {
        local exit_code="$1"
        local captured="$2"
        case "$exit_code" in
            0)
                echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
                ((PASSED_GUARDS++)) || true
                ;;
            124)
                echo "STATUS=FAIL REASON=guard-timeout-${GUARD_NAME}"
                echo -e "${RED}FAILED${NC}: $GUARD_NAME (timed out after ${GUARD_TIMEOUT_SECS}s)"
                ((FAILED_GUARDS++)) || true
                FAILED_GUARD_NAMES+=("$GUARD_NAME")
                ;;
            137)
                echo "STATUS=FAIL REASON=guard-timeout-kill-${GUARD_NAME}"
                echo -e "${RED}FAILED${NC}: $GUARD_NAME (killed after timeout + ${GUARD_KILL_AFTER_SECS}s grace)"
                ((FAILED_GUARDS++)) || true
                FAILED_GUARD_NAMES+=("$GUARD_NAME")
                ;;
            *)
                echo -e "${RED}FAILED${NC}: $GUARD_NAME (exit $exit_code)"
                ((FAILED_GUARDS++)) || true
                FAILED_GUARD_NAMES+=("$GUARD_NAME")
                # Show failure details when caller captured output.
                #
                # `|| true` is load-bearing: under `set -euo pipefail`, if the
                # failed guard's output contains NO `VIOLATION` text (e.g.
                # validate-application-metrics emits `ERROR` instead), grep
                # exits 1, pipefail propagates, and `set -e` aborts the
                # for-loop mid-pipeline. The script then stops running the
                # remaining guards silently — turning a single failure into
                # a CI lie where downstream guard failures go unreported.
                # Keep this sentinel in place even if the pattern changes.
                #
                # `WARN` added per @team-lead F-SG-2 fold-in 2026-05-19: dt-guard
                # subcommands now emit `WARN dt-guard auxiliary skip: <path> (<err>)`
                # to stderr on IO/parse swallow sites (see common/scan.rs).
                # Surfacing them here gives oncall coverage-hole visibility in
                # non-verbose CI logs.
                if [[ -n "$captured" ]]; then
                    { echo "$captured" | grep -E "(VIOLATION|violation|ERROR|error|WARN)" | head -5; } || true
                fi
                ;;
        esac
    }

    for guard in "${guards[@]}"; do
        if [[ -x "$guard" ]]; then
            GUARD_NAME=$(basename "$guard" .sh)
            ((TOTAL_GUARDS++)) || true

            echo -e "${BLUE}Running:${NC} $GUARD_NAME"

            if $VERBOSE; then
                guard_exit=0
                timeout --kill-after="${GUARD_KILL_AFTER_SECS}s" "${GUARD_TIMEOUT_SECS}s" "$guard" "$SEARCH_PATH" || guard_exit=$?
                classify_guard_exit "$guard_exit" ""
            else
                # Capture output for non-verbose mode.
                # `OUTPUT=$(...)` swallows exit code into a separate var so we
                # can still classify timeout vs other failures.
                guard_exit=0
                OUTPUT=$(timeout --kill-after="${GUARD_KILL_AFTER_SECS}s" "${GUARD_TIMEOUT_SECS}s" "$guard" "$SEARCH_PATH" 2>&1) || guard_exit=$?
                classify_guard_exit "$guard_exit" "$OUTPUT"
            fi
            echo ""
        fi
    done
else
    echo "No simple guards found in $SIMPLE_GUARDS_DIR"
fi

# NOTE: Semantic guards are now handled by the semantic-guard agent,
# spawned during devloop validation (see .claude/agents/semantic-guard.md)

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
END_TIME=$(date +%s.%N)
ELAPSED=$(echo "$END_TIME - $START_TIME" | bc)

echo -e "${BOLD}=========================================="
echo "Summary"
echo "==========================================${NC}"
echo ""
echo "Total guards run: $TOTAL_GUARDS"
echo -e "Passed: ${GREEN}$PASSED_GUARDS${NC}"
echo -e "Failed: ${RED}$FAILED_GUARDS${NC}"
printf "Elapsed time: %.2f seconds\n" "$ELAPSED"
echo ""

if [[ $FAILED_GUARDS -gt 0 ]]; then
    echo -e "${RED}Failed guards:${NC}"
    for failed in "${FAILED_GUARD_NAMES[@]}"; do
        echo "  - $failed"
    done
    echo ""
    echo "Run with --verbose for detailed output"
    exit 1
else
    echo -e "${GREEN}All guards passed!${NC}"
    exit 0
fi
