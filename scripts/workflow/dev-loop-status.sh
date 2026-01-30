#!/usr/bin/env bash
# Dev-loop status scanner
# Used by /dev-loop-status skill (see .claude/skills/dev-loop-status/SKILL.md)
#
# Scans all dev-loop output directories and reports their current state.
# Exit codes:
#   0 - Success (output written)
#   1 - Error
#
# Usage:
#   ./dev-loop-status.sh [OPTIONS]
#
# Options:
#   --format FORMAT   Output format: text (default), json, tsv
#   --active-only     Only show active (non-complete) loops
#   --complete-only   Only show completed loops
#   -h, --help        Show this help message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEV_LOOP_DIR="$REPO_ROOT/docs/dev-loop-outputs"

# Default options
FORMAT="text"
FILTER="all"

usage() {
    head -17 "$0" | tail -14
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --format)
            FORMAT="$2"
            shift 2
            ;;
        --active-only)
            FILTER="active"
            shift
            ;;
        --complete-only)
            FILTER="complete"
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Extract value from a markdown table row
# Usage: extract_field "field_name" "$content"
extract_field() {
    local field="$1"
    local content="$2"
    # Match: | Field Name | `value` | or | Field Name | value |
    # Use || true to prevent grep from failing the script when no match
    local match
    match=$(echo "$content" | grep -E "^\| *$field *\|" || true)
    if [[ -n "$match" ]]; then
        echo "$match" | head -1 | sed 's/.*| *`\?\([^`|]*\)`\? *|.*/\1/' | tr -d '[:space:]'
    fi
}

# Check if directory exists
if [[ ! -d "$DEV_LOOP_DIR" ]]; then
    echo "No dev-loop outputs directory found at: $DEV_LOOP_DIR" >&2
    exit 1
fi

# Collect loop data
declare -a LOOPS=()
declare -a ACTIVE_LOOPS=()
declare -a COMPLETE_LOOPS=()

for dir in "$DEV_LOOP_DIR"/*/; do
    # Skip template directory
    [[ "$dir" == *_template* ]] && continue

    # Skip if no main.md
    main_file="${dir}main.md"
    [[ -f "$main_file" ]] || continue

    # Read the Loop State section (first 50 lines should be enough)
    loop_state=$(head -50 "$main_file")

    # Extract fields
    current_step=$(extract_field "Current Step" "$loop_state")
    specialist=$(extract_field "Implementing Specialist" "$loop_state")
    iteration=$(extract_field "Iteration" "$loop_state")
    agent_id=$(extract_field "Implementing Agent" "$loop_state")

    # Get directory name (strip trailing slash and path)
    dir_name=$(basename "$dir")

    # Extract task from ## Task Overview or **Task**: line
    task=$(grep -E "^\*\*Task\*\*:" "$main_file" | head -1 | sed 's/\*\*Task\*\*: *//' || true)
    if [[ -z "$task" ]]; then
        # Fallback: try to get first line after ## Task Overview
        task=$(grep -A1 "## Task Overview" "$main_file" | tail -1 | sed 's/^### //' || true)
    fi
    # Truncate task to 60 chars for display
    if [[ ${#task} -gt 60 ]]; then
        task="${task:0:57}..."
    fi

    # Build record: dir|step|specialist|iteration|agent|task
    record="$dir_name|${current_step:-unknown}|${specialist:-unknown}|${iteration:-1}|${agent_id:-pending}|${task:-No task description}"

    LOOPS+=("$record")

    if [[ "$current_step" == "complete" ]]; then
        COMPLETE_LOOPS+=("$record")
    else
        ACTIVE_LOOPS+=("$record")
    fi
done

# Apply filter
case "$FILTER" in
    active)
        DISPLAY_LOOPS=("${ACTIVE_LOOPS[@]+"${ACTIVE_LOOPS[@]}"}")
        ;;
    complete)
        DISPLAY_LOOPS=("${COMPLETE_LOOPS[@]+"${COMPLETE_LOOPS[@]}"}")
        ;;
    *)
        DISPLAY_LOOPS=("${LOOPS[@]+"${LOOPS[@]}"}")
        ;;
esac

# Output based on format
case "$FORMAT" in
    json)
        echo "{"
        echo "  \"total\": ${#LOOPS[@]},"
        echo "  \"active\": ${#ACTIVE_LOOPS[@]},"
        echo "  \"complete\": ${#COMPLETE_LOOPS[@]},"
        echo "  \"loops\": ["
        first=true
        for record in "${DISPLAY_LOOPS[@]+"${DISPLAY_LOOPS[@]}"}"; do
            IFS='|' read -r dir step specialist iteration agent task <<< "$record"
            [[ "$first" != "true" ]] && echo ","
            first=false
            # Escape quotes in task
            task="${task//\"/\\\"}"
            echo "    {"
            echo "      \"directory\": \"$dir\","
            echo "      \"current_step\": \"$step\","
            echo "      \"specialist\": \"$specialist\","
            echo "      \"iteration\": \"$iteration\","
            echo "      \"agent_id\": \"$agent\","
            echo "      \"task\": \"$task\""
            echo -n "    }"
        done
        echo ""
        echo "  ]"
        echo "}"
        ;;

    tsv)
        echo -e "directory\tcurrent_step\tspecialist\titeration\tagent_id\ttask"
        for record in "${DISPLAY_LOOPS[@]+"${DISPLAY_LOOPS[@]}"}"; do
            IFS='|' read -r dir step specialist iteration agent task <<< "$record"
            echo -e "$dir\t$step\t$specialist\t$iteration\t$agent\t$task"
        done
        ;;

    text|*)
        echo "Dev-Loop Status"
        echo "==============="
        echo ""
        echo "Total: ${#LOOPS[@]} loops (${#ACTIVE_LOOPS[@]} active, ${#COMPLETE_LOOPS[@]} complete)"
        echo ""

        if [[ ${#ACTIVE_LOOPS[@]} -gt 0 ]]; then
            echo "Active Loops:"
            echo "-------------"
            for record in "${ACTIVE_LOOPS[@]}"; do
                IFS='|' read -r dir step specialist iteration agent task <<< "$record"
                echo "  $dir"
                echo "    Step: $step (iteration $iteration)"
                echo "    Specialist: $specialist"
                echo "    Task: $task"
                echo ""
            done
        fi

        if [[ "$FILTER" != "active" ]] && [[ ${#COMPLETE_LOOPS[@]} -gt 0 ]]; then
            echo "Completed Loops (most recent first):"
            echo "------------------------------------"
            # Reverse order (most recent first - directories are date-prefixed)
            for (( i=${#COMPLETE_LOOPS[@]}-1; i>=0; i-- )); do
                record="${COMPLETE_LOOPS[$i]}"
                IFS='|' read -r dir step specialist iteration agent task <<< "$record"
                echo "  $dir"
            done
            echo ""
        fi

        if [[ ${#ACTIVE_LOOPS[@]} -eq 0 ]]; then
            echo "No active dev-loops."
            echo ""
            echo "To start a new dev-loop, run:"
            echo "  /dev-loop-init \"task description\""
        fi
        ;;
esac
