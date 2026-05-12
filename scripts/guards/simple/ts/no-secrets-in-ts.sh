#!/bin/bash
#
# Simple Guard: No Hardcoded Secrets (TypeScript)
#
# TS port of scripts/guards/simple/no-hardcoded-secrets.sh. Detects hardcoded
# secrets in changed .ts/.tsx/.svelte source. Mechanical port of the Rust
# guard with one TS-specific tightening: bare `token` is dropped from the
# Check 1 identifier list because `const token = "..."` is routine runtime
# code in browser/SDK fixtures. Real token leaks are still caught by Check 2
# (prefix patterns) and Check 5 (JWT shape).
#
# Rule shape co-signed by paired-security; see docs/devloop-outputs/
# 2026-05-12-ts-guards-task37/main.md §Per-guard design #1.
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

init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: No Hardcoded Secrets (TS)
Path: $SEARCH_PATH
Diff base: $DIFF_BASE (only changed TS files are checked)"

# Collect changed .ts / .tsx / .svelte files.
CHANGED=""
for ext in ".ts" ".tsx" ".svelte"; do
    files=$(get_all_changed_files "$SEARCH_PATH" "$ext")
    [[ -n "$files" ]] && CHANGED="${CHANGED}${files}"$'\n'
done
CHANGED=$(echo "$CHANGED" | grep -v '^$' | sort -u || true)

if [[ -z "$CHANGED" ]]; then
    echo -e "${GREEN}No TS files changed compared to ${DIFF_BASE}${NC}"
    print_elapsed_time
    exit 0
fi

# TEST_PATH_EXCLUDES — inline filter, intentionally NOT extracted to common.sh
# per @dry-reviewer's ADR-0019 threshold judgment (premature extraction at 2-3
# callers). Structure is kept near-identical to no-pii-in-logs-ts.sh for future
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

# -----------------------------------------------------------------------------
# Check 1: secret variable assignments with literal RHS
# -----------------------------------------------------------------------------
# `token` intentionally dropped from the identifier list per paired-security
# S1(a) — browser/SDK code legitimately writes `const token = await ...`.
print_section "Check 1: Secret variable assignments with literals"

# Skip blank `password=""` / `secret=""` (common in scaffolding) by requiring
# at least one char between the quotes. Env-lookups and build-time defines
# are excluded.
secret_violations=$(grep -nEi \
    '(password|secret|api_key|credential|master_key|private_key|client_secret)\s*[:=]\s*["'"'"'`][^"'"'"'`]+' \
    $FILE_LIST 2>/dev/null | \
    grep -Ev 'process\.env\.|import\.meta\.env\.|Deno\.env\.get\(|Bun\.env\.|globalThis\.__VITE_DEFINE__|\b__[A-Z_]+__\b|^\s*//|/\*' || true)

if [[ -n "$secret_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$secret_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No secret variable assignments with literals"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: API key prefixes
# -----------------------------------------------------------------------------
print_section "Check 2: API key prefixes"

api_key_violations=$(grep -nE \
    '"(sk-[a-zA-Z0-9]{20,}|pk-[a-zA-Z0-9]{20,}|AKIA[A-Z0-9]{16}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xox[baprs]-[a-zA-Z0-9-]+)"' \
    $FILE_LIST 2>/dev/null || true)

if [[ -n "$api_key_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$api_key_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No API key patterns found"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3: Connection strings with credentials (per paired-security S2)
# -----------------------------------------------------------------------------
print_section "Check 3: Connection strings with credentials"

conn_violations=$(grep -nE \
    '["'"'"'`](postgresql|mysql|redis|mongodb|amqp)://[^:]+:[^@{$]+@' \
    $FILE_LIST 2>/dev/null || true)

if [[ -n "$conn_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$conn_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No connection strings with embedded credentials"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 4: Authorization headers with tokens (per paired-security S3)
# -----------------------------------------------------------------------------
print_section "Check 4: Authorization headers with tokens"

auth_violations=$(grep -nEi \
    '"(Authorization:\s*(Bearer|Basic)\s+[A-Za-z0-9+/=_.~-]{20,})"' \
    $FILE_LIST 2>/dev/null || true)

if [[ -n "$auth_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$auth_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No hardcoded authorization headers"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 5: JWT-like patterns (header.payload.signature)
# -----------------------------------------------------------------------------
print_section "Check 5: JWT-like patterns"

jwt_violations=$(grep -nE \
    '["'"'"'`]eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}' \
    $FILE_LIST 2>/dev/null || true)

if [[ -n "$jwt_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$jwt_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No JWT-like string literals"
    echo ""
fi

# Long-base64 heuristic (Rust Check 5) DEFERRED — see docs/devloop-outputs/
# 2026-05-12-ts-guards-task37/main.md §Tech Debt #6.

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
    echo "Move secrets to environment variables (process.env.* / import.meta.env.*)"
    echo "or build-time defines (Vite \`define\` / __VITE_DEFINE__)."
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found${NC}"
    exit 0
fi
