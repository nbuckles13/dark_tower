#!/bin/bash
#
# Simple Guard: Client Metric Name Convention (R-26 / R-24)
#
# ADR-0019 Pattern B: implementation is client's; the rule shape (regex,
# prefix, length cap) is observability's.
#
# Rule (per observability): every metric name passed to an OTel `Meter`
# factory method on client code MUST match:
#
#     ^dt_client_[a-z][a-z0-9_]{0,53}$
#
# Literal prefix dt_client_; lowercase-letter start (no leading digit/
# underscore); snake_case body; no trailing underscore; max total length 64
# (Prometheus/OTel default, encoded via {0,53}).
#
# Scope: packages/sdk-core/src/** + packages/web-app/src/**. packages/
# test-utils/** is exempt by package — InMemoryMetricsSink.ts:1-8 is a
# deliberately passive recorder. packages/sdk-svelte/ is OUT of scope until
# a Meter lands there (main.md §Tech Debt #5).
#
# See docs/devloop-outputs/2026-05-12-ts-guards-task37/main.md
# §Per-guard design #4.
#
# Exit codes:
#   0 - No violations / no in-scope files changed
#   1 - Violations found
#   2 - Script error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../common.sh"

SEARCH_PATH="${1:-.}"

init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: Client Metric Name Convention (dt_client_*)
Path: $SEARCH_PATH
Diff base: $DIFF_BASE"

# Exit cleanly if neither target package exists yet (scaffold-now / fire-later).
if [[ ! -d "packages/sdk-core/src" && ! -d "packages/web-app/src" ]]; then
    echo -e "${GREEN}STATUS: name-guard-dt-client — target packages absent (sdk-core/web-app not yet present)${NC}"
    print_elapsed_time
    exit 0
fi

CHANGED=""
for ext in ".ts" ".tsx" ".svelte"; do
    files=$(get_all_changed_files "$SEARCH_PATH" "$ext")
    [[ -n "$files" ]] && CHANGED="${CHANGED}${files}"$'\n'
done
CHANGED=$(echo "$CHANGED" | grep -v '^$' | sort -u || true)

# Scope gate — INCLUDE-LIST (positive prefix gate), NOT an exclude-list.
# This distinction is load-bearing per @code-reviewer / @dry-reviewer
# review framework: the metric-naming convention applies only to client-
# emitting packages (sdk-core/web-app today; sdk-svelte deferred per
# §Tech Debt #5). All other paths are out of scope by definition, not by
# exemption. Evaluate this BEFORE any subsequent exclude filters so the
# include semantics are obvious to future readers.
FILE_LIST=""
for f in $CHANGED; do
    [[ -f "$f" ]] || continue
    # 1. Positive include-list scope gate.
    [[ "$f" =~ ^packages/(sdk-core|web-app)/src/ ]] || continue
    # 2. Excludes inside the in-scope set (test/build paths within sdk-core/
    #    web-app that are not production metric-emitting code).
    [[ "$f" =~ /node_modules/ ]] && continue
    [[ "$f" =~ /dist/ ]] && continue
    [[ "$f" =~ \.d\.ts$ ]] && continue
    [[ "$f" =~ \.test\.ts$ ]] && continue
    [[ "$f" =~ \.spec\.ts$ ]] && continue
    [[ "$f" =~ \.test\.tsx$ ]] && continue
    [[ "$f" =~ \.spec\.tsx$ ]] && continue
    [[ "$f" =~ /__tests__/ ]] && continue
    FILE_LIST="$FILE_LIST $f"
done

if [[ -z "$FILE_LIST" ]]; then
    echo -e "${GREEN}No in-scope TS files changed${NC}"
    print_elapsed_time
    exit 0
fi

# -----------------------------------------------------------------------------
# Check: Meter factory calls with literal first-arg metric name
# -----------------------------------------------------------------------------
print_section "Check: Meter.create*() literal first-arg names"

# Capture group 3 = metric name. Note: cap groups 1+2 absorb the factory
# method name variants. We extract group 3 with a follow-up sed.
CALL_REGEX='\.(createCounter|createHistogram|createUpDownCounter|createGauge|createObservableCounter|createObservableGauge|createObservableUpDownCounter)\s*\(\s*['"'"'"`]([^'"'"'"`]+)['"'"'"`]'

# Force end on alphanumeric — rejects trailing underscores (e.g. `dt_client_bad_`)
# per @observability F-OBS-1 (Gate 3). Optional middle accepts 0..52 chars
# (snake_case body); when present, must end with [a-z0-9].
NAME_REGEX='^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$'

# grep -oE captures the full call; we then sed out the metric name.
# Walk the files line-by-line so we can report file:line.
violations_found=false
for f in $FILE_LIST; do
    line_num=0
    while IFS= read -r line; do
        line_num=$((line_num + 1))
        # Extract every metric-name string in the line via grep -oE.
        names=$(echo "$line" | grep -oE "$CALL_REGEX" 2>/dev/null \
            | sed -E "s/.*['\"\`]([^'\"\`]+)['\"\`].*/\1/" || true)
        [[ -z "$names" ]] && continue
        while IFS= read -r name; do
            [[ -z "$name" ]] && continue
            if [[ ! "$name" =~ $NAME_REGEX ]]; then
                echo -e "  ${RED}VIOLATION${NC}: $f:$line_num"
                echo "    Metric name '$name' does not match ^dt_client_[a-z][a-z0-9_]{0,53}\$"
                increment_violations
                violations_found=true
            fi
        done <<< "$names"
    done < "$f"
done

if [[ "$violations_found" == false ]]; then
    print_ok "All client metric names match dt_client_* convention"
fi
echo ""

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
    echo "Client metric names MUST match: ^dt_client_[a-z][a-z0-9_]{0,53}\$"
    echo "  - Literal prefix dt_client_"
    echo "  - Lowercase-letter start (no leading digit or underscore)"
    echo "  - snake_case body, no trailing underscore"
    echo "  - Max total length 64 (Prometheus/OTel convention)"
    echo ""
    exit 1
else
    echo -e "${GREEN}All metric names conform${NC}"
    exit 0
fi
