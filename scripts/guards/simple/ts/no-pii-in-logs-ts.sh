#!/bin/bash
#
# Simple Guard: No PII in Logs (TypeScript)
#
# TS port of scripts/guards/simple/no-pii-in-logs.sh. Rule defined by
# observability; ADR-0019 Pattern B. The PII pattern is identical to the
# Rust analog and is CLOSED-LIST: any future expansion MUST be cross-checked
# against the R-26 sanctioned bounded-event field list before merging
# (Owner: observability).
#
# See docs/devloop-outputs/2026-05-12-ts-guards-task37/main.md
# §Per-guard design #2.
#
# Exit codes:
#   0 - No violations / no TS files changed
#   1 - Violations found
#   2 - Script error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../common.sh"

SEARCH_PATH="${1:-.}"

# PII pattern — identical to Rust (no expansion without observability sign-off).
PII_PATTERNS="email|phone|phone_number|ip_address|ip_addr|user_agent|full_name|user_name|first_name|last_name|real_name"

# Unified log-sink regex: native console.*, bounded-event logger.* (lands in
# task #12), and @opentelemetry/api-logs logger.emit() shape.
LOG_SINK_REGEX='\b(console|logger|log)\.(log|info|warn|error|debug|trace|emit)\s*\('

init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: No PII in Logs (TS)
Path: $SEARCH_PATH
Diff base: $DIFF_BASE"

# Collect changed .ts / .svelte files.
CHANGED=""
for ext in ".ts" ".svelte"; do
    files=$(get_all_changed_files "$SEARCH_PATH" "$ext")
    [[ -n "$files" ]] && CHANGED="${CHANGED}${files}"$'\n'
done
CHANGED=$(echo "$CHANGED" | grep -v '^$' | sort -u || true)

if [[ -z "$CHANGED" ]]; then
    echo -e "${GREEN}No TS/Svelte files changed compared to ${DIFF_BASE}${NC}"
    print_elapsed_time
    exit 0
fi

# TEST_PATH_EXCLUDES — inline filter, intentionally NOT extracted to common.sh
# per @dry-reviewer's ADR-0019 threshold judgment (premature extraction at 2-3
# callers). Structure is kept near-identical to no-secrets-in-ts.sh for future
# mechanical extraction when a 4th true-rhyming caller lands.
# Constraint #6 directories (node_modules, dist, build, .svelte-kit, coverage)
# plus type-decl files plus test/fixture paths (*.test.ts, *.spec.ts,
# __tests__/, test-utils/, fixtures/).
FILE_LIST=""
for f in $CHANGED; do
    [[ -f "$f" ]] || continue
    [[ "$f" =~ /node_modules/ ]] && continue
    [[ "$f" =~ /dist/ ]] && continue
    [[ "$f" =~ /build/ ]] && continue
    [[ "$f" =~ /\.svelte-kit/ ]] && continue
    [[ "$f" =~ /coverage/ ]] && continue
    [[ "$f" =~ \.d\.ts$ ]] && continue
    [[ "$f" =~ \.test\.ts$ ]] && continue
    [[ "$f" =~ \.spec\.ts$ ]] && continue
    [[ "$f" =~ \.test\.tsx$ ]] && continue
    [[ "$f" =~ \.spec\.tsx$ ]] && continue
    [[ "$f" =~ /__tests__/ ]] && continue
    [[ "$f" =~ /test-utils/ ]] && continue
    [[ "$f" =~ /fixtures/ ]] && continue
    FILE_LIST="$FILE_LIST $f"
done

if [[ -z "$FILE_LIST" ]]; then
    echo -e "${GREEN}No production TS files to check (all filtered)${NC}"
    print_elapsed_time
    exit 0
fi

# Strip comments (// ...) and same-line `// pii-safe:` opt-out lines before
# pattern matching. We grep, then filter.
filter_allowed() {
    grep -Ev '//\s*pii-safe:|REDACTED|\[REDACTED\]|masked|hashed|_hash\b|Hash\b' || true
}

# -----------------------------------------------------------------------------
# Check 1 (BLOCKING) — PII identifier on the same line as a log-sink call
# -----------------------------------------------------------------------------
print_section "Check 1: PII identifiers in log-sink calls"

check1=$(grep -nE "$LOG_SINK_REGEX" $FILE_LIST 2>/dev/null | \
    grep -E "\b(${PII_PATTERNS})\b" | \
    filter_allowed || true)

if [[ -n "$check1" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$check1" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No PII identifiers in log-sink calls"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2 (BLOCKING) — PII field/property in log call's object argument
# -----------------------------------------------------------------------------
print_section "Check 2: PII fields in structured log objects"

# Same-line greedy match for `{ email: ... }` etc. inside a log call.
check2=$(grep -nE "$LOG_SINK_REGEX[^)]*\{[^}]*\b(${PII_PATTERNS})\s*:" $FILE_LIST 2>/dev/null | \
    filter_allowed || true)

if [[ -n "$check2" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$check2" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No PII fields in structured log objects"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3 (WARNING, non-blocking) — PII in error messages / template literals
# -----------------------------------------------------------------------------
print_section "Check 3: PII in error messages (warning only)"

check3=$(grep -nE '(throw\s+new\s+Error|new\s+Error|Error\s*\()' $FILE_LIST 2>/dev/null | \
    grep -E "\b(${PII_PATTERNS})\b" | \
    filter_allowed || true)

if [[ -n "$check3" ]]; then
    echo -e "${YELLOW}POTENTIAL VIOLATIONS (review manually):${NC}"
    echo "$check3" | while read -r line; do
        echo "  $line"
        # Do not increment — Check 3 is non-blocking per observability MF-OBS-3a.
    done
    echo ""
else
    print_ok "No PII in error messages"
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
    echo "PII must not be logged in plaintext. Options:"
    echo "  1. Remove PII from log statement"
    echo "  2. Use hashed form: meeting_id_hash, user_id_hash, etc."
    echo "  3. Use [REDACTED] / masked literal"
    echo "  4. Same-line escape hatch: // pii-safe: <reason>"
    echo ""
    echo "See R-26 bounded-event field list for sanctioned log fields."
    echo ""
    exit 1
else
    echo -e "${GREEN}No PII violations${NC}"
    exit 0
fi
