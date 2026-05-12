#!/bin/bash
#
# Simple Guard: Exports Map Closed-World (ADR-0028 §5 supply chain)
#
# Rule shape co-signed by paired-security. For every changed
# packages/*/package.json where the package is NOT exempt (private:true
# OR name prefix @darktower/test-), enforce three checks:
#
#   Check A (HARD): forbid `exports` KEYS matching regex
#     (^|/)(test|tests|testing|test-only|__tests__|internal|private)(/|$)
#     plus wildcard-only "./*" — closed-world bypass.
#
#   Check B (HARD): forbid `exports` VALUES (resolved file paths in any of
#     string-form, import/require/types conditional sub-fields) pointing
#     into ./src/test/, ./src/tests/, ./src/internal/, ./src/private/,
#     ./src/__tests__/, ./test-only/, ./test/.
#
#   Check C (SOFT, promotable): missing `exports` emits WARN by default.
#     Set STRICT_EXPORTS_MAP=1 to promote to HARD VIOLATION. Transition:
#     CI flips strict once all non-private packages have exports
#     (target: end of ADR-0033 Wave 2). See main.md §Tech Debt #7.
#
# Allowlist: `"private": true` OR name starts with `@darktower/test-`.
# No third mechanism per security S6.
#
# See docs/devloop-outputs/2026-05-12-ts-guards-task37/main.md
# §Per-guard design #6.
#
# Exit codes:
#   0 - No violations
#   1 - Violations found
#   2 - Script error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../common.sh"

SEARCH_PATH="${1:-.}"
STRICT="${STRICT_EXPORTS_MAP:-0}"

init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: Exports Map Closed-World
Path: $SEARCH_PATH
Diff base: $DIFF_BASE
Strict mode: $([ "$STRICT" = "1" ] && echo ON || echo off)"

# Collect changed packages/*/package.json files.
CHANGED=$(get_all_changed_files "$SEARCH_PATH" "package.json")

PKG_LIST=""
for f in $CHANGED; do
    # Match packages/<name>/package.json (depth-2 only — root package.json
    # and nested package.json files are out of scope).
    if [[ "$f" =~ ^packages/[^/]+/package\.json$ ]] && [[ -f "$f" ]]; then
        PKG_LIST="$PKG_LIST $f"
    fi
done

if [[ -z "$PKG_LIST" ]]; then
    echo -e "${GREEN}No packages/*/package.json files changed${NC}"
    print_elapsed_time
    exit 0
fi

FORBIDDEN_KEY_REGEX='(^|/)(test|tests|testing|test-only|__tests__|internal|private)(/|$)'
# Wildcard-only public key: forbid bare ./*. (./*.{ext} is fine; we match ^\./\*$)
WILDCARD_KEY_REGEX='^\./\*$'

# Segment regex mirrors Check A's word-boundary semantics. Catches built-
# artifact paths (./dist/test-only/, ./lib/__tests__/, ./build/internal/, ...)
# in addition to source-tree paths (./src/test/, ./src/internal/, ...).
# Per @paired-security F1 (Gate 2): literal-prefix array left ./dist/test-only/
# paths as a public-named-key + test-source-target bypass.
FORBIDDEN_VALUE_REGEX='/(test|tests|testing|test-only|__tests__|internal|private)(/|$)'

is_exempt() {
    local pkg="$1"
    local private name
    private=$(jq -r '.private // false' "$pkg" 2>/dev/null || echo "false")
    name=$(jq -r '.name // ""' "$pkg" 2>/dev/null || echo "")
    if [[ "$private" == "true" ]]; then return 0; fi
    if [[ "$name" =~ ^@darktower/test- ]]; then return 0; fi
    return 1
}

# Walk exports keys + values. jq emits one line per key|value pair (with
# conditional sub-fields flattened to their resolved path strings).
walk_exports() {
    local pkg="$1"
    # If exports is a string (single-export form), treat as key="." value=string.
    # Emits one "key|value" line per pair. `|` is the delimiter; it is safe
    # because npm package.json exports keys/values must be JSON-valid paths
    # (relative `./...` strings or condition tokens like `import`/`require`),
    # none of which can contain `|` literally. Caller splits on first `|`.
    jq -r '
        .exports as $e |
        if $e == null then
            empty
        elif ($e | type) == "string" then
            ".|" + $e
        elif ($e | type) == "object" then
            $e | to_entries[] |
            .key as $k |
            (.value |
                if type == "string" then [.]
                elif type == "object" then [.. | strings]
                else []
                end) as $vals |
            $vals[] | ($k + "|" + .)
        else
            empty
        end
    ' "$pkg" 2>/dev/null || true
}

for pkg in $PKG_LIST; do
    print_section "Package: $pkg"

    if is_exempt "$pkg"; then
        echo -e "  ${GREEN}EXEMPT${NC} (private:true or @darktower/test-* name)"
        echo ""
        continue
    fi

    # Check C — missing exports
    has_exports=$(jq 'has("exports")' "$pkg" 2>/dev/null || echo "false")
    if [[ "$has_exports" != "true" ]]; then
        if [[ "$STRICT" == "1" ]]; then
            echo -e "  ${RED}VIOLATION${NC}: missing 'exports' field (STRICT_EXPORTS_MAP=1)"
            increment_violations
        else
            echo -e "  ${YELLOW}WARN${NC}: package missing 'exports' (closed-world surface not enforceable)"
            echo "    Set STRICT_EXPORTS_MAP=1 to promote to hard violation."
        fi
        echo ""
        continue
    fi

    pkg_has_violation=false

    # Check A — forbidden keys
    keys=$(jq -r '.exports | to_entries[] | .key' "$pkg" 2>/dev/null || true)
    if [[ -n "$keys" ]]; then
        while IFS= read -r key; do
            [[ -z "$key" ]] && continue
            if [[ "$key" =~ $FORBIDDEN_KEY_REGEX ]]; then
                echo -e "  ${RED}VIOLATION${NC}: forbidden exports key '$key' (matches test/internal/private)"
                increment_violations
                pkg_has_violation=true
            fi
            if [[ "$key" =~ $WILDCARD_KEY_REGEX ]]; then
                echo -e "  ${RED}VIOLATION${NC}: wildcard-only key '$key' (closed-world bypass)"
                increment_violations
                pkg_has_violation=true
            fi
        done <<< "$keys"
    fi

    # Check B — forbidden values
    while IFS= read -r kv; do
        [[ -z "$kv" ]] && continue
        local_key="${kv%%|*}"
        local_val="${kv#*|}"
        # Only string values that look like ./paths are checked.
        [[ "$local_val" =~ ^\./ ]] || continue
        if [[ "$local_val" =~ $FORBIDDEN_VALUE_REGEX ]]; then
            echo -e "  ${RED}VIOLATION${NC}: key '$local_key' resolves to test/internal path '$local_val'"
            increment_violations
            pkg_has_violation=true
        fi
    done < <(walk_exports "$pkg")

    if [[ "$pkg_has_violation" == false ]]; then
        echo -e "  ${GREEN}OK${NC}"
    fi
    echo ""
done

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
    echo "Public packages must have a closed-world 'exports' map:"
    echo "  - No test/internal/private subpath keys"
    echo "  - No wildcard './*' key"
    echo "  - No values pointing into ./src/test/ etc."
    echo "  - Exempt: 'private: true' or '@darktower/test-*' name."
    echo ""
    exit 1
else
    echo -e "${GREEN}All packages comply${NC}"
    exit 0
fi
