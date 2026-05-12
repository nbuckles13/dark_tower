#!/bin/bash
#
# Simple Guard: No Dev Trust Path in Prod Bundle (R-14)
#
# Forcing function for the R-14 transition: the prod bundle MUST NOT contain
# the literal `serverCertificateHashes` (dev-only WebTransport fingerprint
# trust path is gated behind `__DEV_TRUST_FINGERPRINT__` build-time literal,
# false in prod, tree-shaken out).
#
# Four states, evaluated in order:
#
#   1. packages/sdk-core/ does not exist
#      → exit 0 (rule scaffolded; no consumer yet).
#
#   2. packages/sdk-core/ exists BUT
#      packages/sdk-core/tests/bundle-content.test.ts does NOT exist
#      → FAIL — forcing function. The Vitest contract test MUST land
#        alongside sdk-core. Message names the canonical path + points at
#        docs/TODO.md §R-14 Transition.
#
#   3. Both sdk-core/ and the canonical test exist
#      → exit 0 (Vitest contract test in Layer 4 carries the real check).
#
#   4. (Belt-and-suspenders, only if state 3 true): if
#      packages/sdk-core/dist/ exists, grep for `serverCertificateHashes` —
#      fail on hit. Cheap Layer-3 catch for stale-build cases in PR-author
#      trees.
#
# See docs/devloop-outputs/2026-05-12-ts-guards-task37/main.md
# §Per-guard design #5; docs/TODO.md §R-14 Transition.
#
# Lifecycle: when task #9 lands packages/sdk-core/ + the canonical test,
# this guard either deletes (Vitest carries the real check) or shrinks to
# state 4 only (stale-dist tripwire). Implementer of #9 decides.
#
# Exit codes:
#   0 - Pass (states 1, 3, or 4-clean)
#   1 - Fail (state 2 forcing function, or state 4 hit)
#   2 - Script error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../common.sh"

SEARCH_PATH="${1:-.}"

# Resolve paths relative to SEARCH_PATH so the guard fires correctly when
# invoked from anywhere (not just the repo root). Per @paired-security F2
# (Gate 2): bare relative paths silently fired state 1 when invoked outside
# the repo root, masking states 2/3/4.
SDK_CORE_DIR="$SEARCH_PATH/packages/sdk-core"
CANONICAL_TEST="$SEARCH_PATH/packages/sdk-core/tests/bundle-content.test.ts"
SDK_CORE_DIST="$SEARCH_PATH/packages/sdk-core/dist"

init_violations
start_timer

print_header "Guard: No Dev Trust Path in Prod Bundle (R-14)
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# State 1 — sdk-core not yet present
# -----------------------------------------------------------------------------
if [[ ! -d "$SDK_CORE_DIR" ]]; then
    echo -e "${GREEN}STATUS: no-dev-trust-path-in-prod-bundle — sdk-core not yet present (rule scaffolded; no consumer yet)${NC}"
    print_elapsed_time
    exit 0
fi

# -----------------------------------------------------------------------------
# State 2 — sdk-core exists BUT canonical contract test missing (FORCING)
# -----------------------------------------------------------------------------
if [[ ! -f "$CANONICAL_TEST" ]]; then
    echo -e "${RED}VIOLATION: no-dev-trust-path-in-prod-bundle — R-14 enforcement gap.${NC}"
    echo ""
    echo "  packages/sdk-core/ exists but packages/sdk-core/tests/bundle-content.test.ts is missing."
    echo ""
    echo "  The canonical contract test MUST grep the prod-mode \`vite build\` output"
    echo "  for the literal string 'serverCertificateHashes' and fail on any hit."
    echo "  R-14 governs prod-bundle exclusion of the dev-only WebTransport"
    echo "  fingerprint trust path; the literal must be tree-shaken out under"
    echo "  __DEV_TRUST_FINGERPRINT__=false."
    echo ""
    echo "  Land the test alongside sdk-core, then this guard becomes a belt-and-"
    echo "  suspenders check against the production bundle."
    echo ""
    echo "  See \"R-14 Transition\" in docs/TODO.md."
    echo ""
    increment_violations
    print_header "Summary"
    print_elapsed_time
    echo ""
    echo -e "${RED}Found $(get_violations) violation(s)${NC}"
    exit 1
fi

# -----------------------------------------------------------------------------
# State 3 — both present
# -----------------------------------------------------------------------------
echo -e "${GREEN}STATUS: no-dev-trust-path-in-prod-bundle — contract test present (Layer 4 carries the real check)${NC}"

# -----------------------------------------------------------------------------
# State 4 — belt-and-suspenders: grep dist if present
# -----------------------------------------------------------------------------
if [[ -d "$SDK_CORE_DIST" ]]; then
    print_section "Check: stale-dist scan for serverCertificateHashes"

    # Grep all built artifacts. The literal must not appear in any prod file.
    hits=$(grep -rn 'serverCertificateHashes' "$SDK_CORE_DIST" 2>/dev/null || true)
    if [[ -n "$hits" ]]; then
        echo -e "${RED}VIOLATION: no-dev-trust-path-in-prod-bundle — serverCertificateHashes literal found in prod bundle:${NC}"
        echo "$hits" | while read -r line; do
            echo "  $line"
            increment_violations
        done
        echo ""
    else
        print_ok "No serverCertificateHashes literal found in dist"
        echo ""
    fi
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
    echo "The dev-only serverCertificateHashes path must be tree-shaken from"
    echo "prod bundles. Ensure __DEV_TRUST_FINGERPRINT__ is false in prod and"
    echo "the surrounding code is gated behind it."
    echo ""
    exit 1
else
    echo -e "${GREEN}Clean${NC}"
    exit 0
fi
