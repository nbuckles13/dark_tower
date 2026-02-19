#!/bin/bash
#
# Simple Guard: Test Rigidity (env-tests)
#
# Detects patterns in env-tests where tests accept failures, unavailability,
# or error conditions as "passing" — creating false confidence that features work.
#
# What it checks:
#   1. Early return when infrastructure/services unavailable
#   2. Warning used instead of assertion
#   3. Aspirational non-enforcement (documented intentional skips)
#   4. Multi-status acceptance (contradictory HTTP status codes accepted)
#   5. Assertion-free match arms (HTTP errors or Ok paths silently accepted)
#   6. Placeholder test stubs (unimplemented!() bodies)
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./test-rigidity.sh [path]    # path defaults to repo root
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Resolve env-tests directory
SEARCH_PATH="${1:-.}"

# Find repository root
REPO_ROOT="$SEARCH_PATH"
while [[ ! -d "$REPO_ROOT/.git" ]] && [[ ! -f "$REPO_ROOT/.git" ]] && [[ "$REPO_ROOT" != "/" ]]; do
    REPO_ROOT="$(dirname "$REPO_ROOT")"
done

if [[ "$REPO_ROOT" == "/" ]]; then
    echo "Error: Could not find repository root"
    exit 2
fi

ENV_TESTS_DIR="$REPO_ROOT/crates/env-tests"

if [[ ! -d "$ENV_TESTS_DIR/tests" ]]; then
    echo "No env-tests/tests directory found - nothing to check"
    exit 0
fi

# Verify test files exist
shopt -s nullglob
test_files=("$ENV_TESTS_DIR"/tests/*.rs)
shopt -u nullglob

if [[ ${#test_files[@]} -eq 0 ]]; then
    echo "No .rs test files found in $ENV_TESTS_DIR/tests/"
    exit 0
fi

init_violations
start_timer

print_header "Guard: Test Rigidity (env-tests)
Path: $ENV_TESTS_DIR/tests/

Detects escape clauses that let env-tests pass without actually testing."

# Temp file for intermediate results
RESULTS_FILE=$(mktemp)
trap "rm -f $RESULTS_FILE" EXIT

# =============================================================================
# Check 1: Early return on service/tool unavailability
# =============================================================================
print_section "Check 1: Early return on service/tool unavailability"

# Find return; preceded (within 4 lines) by availability checks or skip messages.
# These cause the test to silently pass when infrastructure is missing.
awk '
/is_.*_available/ { avail_line = NR }
/println!\("SKIPPED/ { skip_line = NR }
/eprintln!\("Warning/ { warn_line = NR }
/eprintln!\("Skipping/ { skip2_line = NR }
/return;/ {
    triggered = ""
    if (avail_line > 0 && NR - avail_line <= 4) triggered = "service unavailable check"
    if (skip_line > 0 && NR - skip_line <= 3) triggered = "SKIPPED message"
    if (warn_line > 0 && NR - warn_line <= 3) triggered = "Warning message"
    if (skip2_line > 0 && NR - skip2_line <= 3) triggered = "Skipping message"

    if (triggered != "") {
        printf "  %s:%d: return; after %s\n", FILENAME, NR, triggered
    }
}
' "${test_files[@]}" > "$RESULTS_FILE" 2>/dev/null || true

if [[ -s "$RESULTS_FILE" ]]; then
    echo -e "${RED}VIOLATIONS:${NC}"
    cat "$RESULTS_FILE"
    count=$(wc -l < "$RESULTS_FILE")
    for ((i=0; i<count; i++)); do increment_violations; done
    echo ""
else
    print_ok "No early-return escape clauses found"
    echo ""
fi

# =============================================================================
# Check 2: Warning used instead of assertion
# =============================================================================
print_section "Check 2: Warning used instead of assertion"

# Find Warning/WARNING strings in test code that are NOT followed by return;
# (those are Check 1). These are standalone warnings that paper over failures.
> "$RESULTS_FILE"

grep -rn -i '"Warning' "${test_files[@]}" 2>/dev/null | while IFS=: read -r file line_num rest; do
    [[ -z "$file" ]] && continue

    # Check if return; appears within 5 lines after this warning
    # (those are Check 1 — early-return escapes — not standalone warnings)
    end_line=$((line_num + 5))
    nearby_return=$(sed -n "${line_num},${end_line}p" "$file" 2>/dev/null | grep -c 'return;' || true)
    if [[ "$nearby_return" -gt 0 ]]; then
        continue
    fi

    # Check if an assert! or panic! appears within 15 lines BEFORE this warning.
    # If so, this is a secondary concern (e.g., cleanup warning after main assertion),
    # not a warning being used instead of an assertion.
    start_line=$((line_num > 15 ? line_num - 15 : 1))
    preceding_assert=$(sed -n "${start_line},${line_num}p" "$file" 2>/dev/null | grep -cE 'assert|panic!' || true)
    if [[ "$preceding_assert" -gt 0 ]]; then
        continue
    fi

    text=$(sed -n "${line_num}p" "$file" | sed 's/^[[:space:]]*//')
    # Truncate long lines
    if [[ ${#text} -gt 120 ]]; then
        text="${text:0:117}..."
    fi
    echo "  ${file}:${line_num}: ${text}" >> "$RESULTS_FILE"
done || true

if [[ -s "$RESULTS_FILE" ]]; then
    echo -e "${RED}VIOLATIONS:${NC}"
    cat "$RESULTS_FILE"
    count=$(wc -l < "$RESULTS_FILE")
    for ((i=0; i<count; i++)); do increment_violations; done
    echo ""
else
    print_ok "No warnings-as-assertions found"
    echo ""
fi

# =============================================================================
# Check 3: Aspirational non-enforcement
# =============================================================================
print_section "Check 3: Aspirational non-enforcement"

# Find strings in executable code that document intentional non-enforcement.
# Excludes comment lines (// and /// doc comments) — comments explaining why
# a test was removed are informational, not escape clauses.
grep -rn -iE \
    "(Don.t fail|aspirational|future enhancement|not a hard failure)" \
    "${test_files[@]}" 2>/dev/null | \
    grep -vE '^\s*[^:]+:[0-9]+:\s*//' | \
    sed 's/^/  /' > "$RESULTS_FILE" || true

if [[ -s "$RESULTS_FILE" ]]; then
    echo -e "${RED}VIOLATIONS:${NC}"
    cat "$RESULTS_FILE"
    count=$(wc -l < "$RESULTS_FILE")
    for ((i=0; i<count; i++)); do increment_violations; done
    echo ""
else
    print_ok "No aspirational non-enforcement found"
    echo ""
fi

# =============================================================================
# Check 4: Multi-status acceptance in assertions
# =============================================================================
print_section "Check 4: Multi-status acceptance in assertions"

# Find assertions that accept multiple HTTP status codes with ||.
# e.g., status == 404 || status == 401 or contains("401") || contains("403")
# These make it impossible to verify specific behavior.
grep -rn -E \
    '==[[:space:]]*[0-9]{3}.*\|\|' \
    "${test_files[@]}" 2>/dev/null | \
    sed 's/^/  /' > "$RESULTS_FILE" || true

if [[ -s "$RESULTS_FILE" ]]; then
    echo -e "${RED}VIOLATIONS:${NC}"
    cat "$RESULTS_FILE"
    count=$(wc -l < "$RESULTS_FILE")
    for ((i=0; i<count; i++)); do increment_violations; done
    echo ""
else
    print_ok "No multi-status acceptance found"
    echo ""
fi

# =============================================================================
# Check 5: Assertion-free match arms (errors accepted silently)
# =============================================================================
print_section "Check 5: Assertion-free match arms"

# For each HTTP status code match arm (NNN => {), Ok(...) => {, or
# Err(...status: NNN...) => {, check whether the arm body contains any
# assert! or panic!. Arms without enforcement silently accept failures.
#
# Uses brace-depth tracking to find exact arm boundaries, avoiding
# false negatives from assertions in adjacent arms.

> "$RESULTS_FILE"

check_arm_enforcement() {
    local file="$1"
    local start_line="$2"
    local depth=0
    local has_enforcement=false
    local line_count=0

    while IFS= read -r line; do
        # Count braces using parameter expansion (no subshells)
        local stripped_open="${line//[^\{]/}"
        local opens=${#stripped_open}
        local stripped_close="${line//[^\}]/}"
        local closes=${#stripped_close}
        depth=$((depth + opens - closes))

        if [[ "$line" =~ assert|panic! ]]; then
            has_enforcement=true
        fi

        ((line_count++)) || true

        # Arm closed
        if [[ $depth -le 0 ]]; then
            break
        fi

        # Safety: don't scan more than 30 lines per arm
        if [[ $line_count -gt 30 ]]; then
            break
        fi
    done < <(sed -n "${start_line},\$p" "$file")

    $has_enforcement
}

# 5a: HTTP status code arms — NNN => {
while IFS=: read -r file line_num _rest; do
    [[ -z "$file" ]] && continue
    if ! check_arm_enforcement "$file" "$line_num"; then
        arm_text=$(sed -n "${line_num}p" "$file" | sed 's/^[[:space:]]*//')
        echo "  ${file}:${line_num}: ${arm_text}" >> "$RESULTS_FILE"
    fi
done < <(grep -rn -E '^[[:space:]]*[0-9]{3}[[:space:]]*=>[[:space:]]*\{' \
    "${test_files[@]}" 2>/dev/null || true)

# 5b: Ok(...) => {
while IFS=: read -r file line_num _rest; do
    [[ -z "$file" ]] && continue
    if ! check_arm_enforcement "$file" "$line_num"; then
        arm_text=$(sed -n "${line_num}p" "$file" | sed 's/^[[:space:]]*//')
        echo "  ${file}:${line_num}: ${arm_text}" >> "$RESULTS_FILE"
    fi
done < <(grep -rn -E '^[[:space:]]*Ok\(.*\)[[:space:]]*=>[[:space:]]*\{' \
    "${test_files[@]}" 2>/dev/null || true)

# 5c: Err(...status: NNN...) => {
while IFS=: read -r file line_num _rest; do
    [[ -z "$file" ]] && continue
    if ! check_arm_enforcement "$file" "$line_num"; then
        arm_text=$(sed -n "${line_num}p" "$file" | sed 's/^[[:space:]]*//')
        echo "  ${file}:${line_num}: ${arm_text}" >> "$RESULTS_FILE"
    fi
done < <(grep -rn -E '^[[:space:]]*Err\(.*status:[[:space:]]*[0-9]{3}.*\)[[:space:]]*=>[[:space:]]*\{' \
    "${test_files[@]}" 2>/dev/null || true)

if [[ -s "$RESULTS_FILE" ]]; then
    echo -e "${RED}VIOLATIONS:${NC}"
    cat "$RESULTS_FILE"
    count=$(wc -l < "$RESULTS_FILE")
    for ((i=0; i<count; i++)); do increment_violations; done
    echo ""
else
    print_ok "No assertion-free match arms found"
    echo ""
fi

# =============================================================================
# Check 6: Placeholder test stubs
# =============================================================================
print_section "Check 6: Placeholder test stubs"

# Find unimplemented!() in test bodies, but skip tests marked #[ignore].
# Ignored tests don't run by default, so their stubs aren't escape clauses.
> "$RESULTS_FILE"

for file in "${test_files[@]}"; do
    while IFS=: read -r line_num _rest; do
        [[ -z "$line_num" ]] && continue

        # Check if any of the 15 lines before contain #[ignore
        # (stubs may have comment blocks between #[ignore] and unimplemented!())
        start_line=$((line_num > 15 ? line_num - 15 : 1))
        preceding_ignore=$(sed -n "${start_line},${line_num}p" "$file" 2>/dev/null | grep -c '#\[ignore' || true)
        if [[ "$preceding_ignore" -gt 0 ]]; then
            continue
        fi

        text=$(sed -n "${line_num}p" "$file" | sed 's/^[[:space:]]*//')
        echo "  ${file}:${line_num}: ${text}" >> "$RESULTS_FILE"
    done < <(grep -n 'unimplemented!' "$file" 2>/dev/null || true)
done

if [[ -s "$RESULTS_FILE" ]]; then
    echo -e "${RED}VIOLATIONS:${NC}"
    cat "$RESULTS_FILE"
    count=$(wc -l < "$RESULTS_FILE")
    for ((i=0; i<count; i++)); do increment_violations; done
    echo ""
else
    print_ok "No placeholder stubs found"
    echo ""
fi

# =============================================================================
# Summary
# =============================================================================
print_header "Summary"

TOTAL_VIOLATIONS=$(get_violations)
print_elapsed_time
echo ""

if [[ $TOTAL_VIOLATIONS -gt 0 ]]; then
    echo -e "${RED}Found $TOTAL_VIOLATIONS violation(s)${NC}"
    echo ""
    echo "Env-tests with escape clauses create false confidence — they pass"
    echo "even when the features they claim to test are broken."
    echo ""
    echo "To fix: Replace escape clauses with proper assertions, or convert"
    echo "to explicit #[ignore] with a reason so the test framework tracks them."
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
