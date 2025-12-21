#!/bin/bash
#
# Common Guard Library
#
# Shared functions and configuration for all guards.
# Source this file at the top of each guard script:
#   source "$(dirname "$0")/../common.sh"
#

# =============================================================================
# Configuration
# =============================================================================

# Semantic guard model (change this to switch models for all semantic guards)
GUARD_SEMANTIC_MODEL="${GUARD_SEMANTIC_MODEL:-claude-sonnet-4-20250514}"

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# =============================================================================
# Test Code Detection
# =============================================================================

# Cache for test line ranges (avoids re-computing for same file)
declare -A TEST_RANGES_CACHE

# Check if a file is test code (entire file is tests)
# Returns 0 (true) if the file is test code, 1 (false) otherwise
is_test_file() {
    local file="$1"

    # Check path patterns for test files
    if [[ "$file" =~ /tests/ ]] || \
       [[ "$file" =~ _test\.rs$ ]] || \
       [[ "$file" =~ /test_.*\.rs$ ]] || \
       [[ "$file" =~ /mod\.rs$ && "$file" =~ /tests/ ]]; then
        return 0
    fi

    return 1
}

# Check if nightly toolchain is available (called once at startup)
# Exits with error if not installed
check_nightly_required() {
    if ! rustup run nightly rustc --version &>/dev/null 2>&1; then
        echo -e "${RED}ERROR: Rust nightly toolchain is required for guards${NC}" >&2
        echo "" >&2
        echo "Guards use the compiler to reliably detect test code." >&2
        echo "Install nightly with:" >&2
        echo "" >&2
        echo "    rustup toolchain install nightly" >&2
        echo "" >&2
        exit 2
    fi
}

# Get test line ranges for a file using the compiler
# Caches results for efficiency
# Output: space-separated list of "start-end" ranges
get_test_ranges() {
    local file="$1"

    # Check cache
    if [[ -n "${TEST_RANGES_CACHE[$file]:-}" ]]; then
        echo "${TEST_RANGES_CACHE[$file]}"
        return
    fi

    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

    # Use compiler-based detection (requires nightly)
    local ranges=$("$script_dir/strip-test-code.sh" "$file" --ranges 2>/dev/null | tr '\n' ' ')

    # Cache and return
    TEST_RANGES_CACHE[$file]="$ranges"
    echo "$ranges"
}

# Check if a line number is within any test range
# Returns 0 (true) if in test code, 1 (false) otherwise
is_line_in_test_range() {
    local line_num="$1"
    local ranges="$2"

    for range in $ranges; do
        local start=$(echo "$range" | cut -d- -f1)
        local end=$(echo "$range" | cut -d- -f2)

        if [[ $line_num -ge $start ]] && [[ $line_num -le $end ]]; then
            return 0
        fi
    done

    return 1
}

# Filter out test code from grep results
# Input: grep output with format "file:line:content"
# Output: non-test lines only
filter_test_code() {
    local current_file=""
    local current_ranges=""

    while IFS= read -r line; do
        # Extract file and line number
        local file=$(echo "$line" | cut -d: -f1)
        local line_num=$(echo "$line" | cut -d: -f2)

        # Skip if it's a test file (entire file is tests)
        if is_test_file "$file"; then
            continue
        fi

        # Get test ranges for this file (cached)
        if [[ "$file" != "$current_file" ]]; then
            current_file="$file"
            current_ranges=$(get_test_ranges "$file")
        fi

        # Skip if line is in a test range
        if [[ -n "$current_ranges" ]] && [[ "$line_num" =~ ^[0-9]+$ ]]; then
            if is_line_in_test_range "$line_num" "$current_ranges"; then
                continue
            fi
        fi

        echo "$line"
    done
}

# =============================================================================
# Violation Tracking
# =============================================================================

# Initialize violation tracking (call at start of guard)
init_violations() {
    VIOLATIONS_FILE=$(mktemp)
    echo "0" > "$VIOLATIONS_FILE"
    trap "rm -f $VIOLATIONS_FILE" EXIT
}

# Increment violation count
increment_violations() {
    local current=$(cat "$VIOLATIONS_FILE")
    echo $((current + 1)) > "$VIOLATIONS_FILE"
}

# Get current violation count
get_violations() {
    cat "$VIOLATIONS_FILE"
}

# =============================================================================
# Timing
# =============================================================================

# Start timer (call at beginning of guard)
start_timer() {
    GUARD_START_TIME=$(date +%s.%N)
}

# Get elapsed time in seconds
get_elapsed_time() {
    local end_time=$(date +%s.%N)
    echo "$end_time - $GUARD_START_TIME" | bc
}

# Print elapsed time
print_elapsed_time() {
    local elapsed=$(get_elapsed_time)
    printf "Elapsed time: %.3f seconds\n" "$elapsed"
}

# =============================================================================
# Output Helpers
# =============================================================================

print_header() {
    local title="$1"
    echo "=========================================="
    echo "$title"
    echo "=========================================="
    echo ""
}

print_section() {
    local title="$1"
    echo "$title"
    echo "$(echo "$title" | sed 's/./-/g')"
}

print_ok() {
    local message="$1"
    echo -e "${GREEN}OK${NC} - $message"
}

print_violation() {
    local message="$1"
    echo -e "${RED}VIOLATION${NC}: $message"
}

print_warning() {
    local message="$1"
    echo -e "${YELLOW}WARNING${NC}: $message"
}
