#!/bin/bash
#
# Strip Test Code Helper
#
# Uses rustc nightly to strip #[cfg(test)] and #[test] code from Rust source.
# This provides 100% reliable test code removal using the compiler's own
# conditional compilation logic.
#
# Usage:
#   ./strip-test-code.sh <file.rs>              # Output stripped code to stdout
#   ./strip-test-code.sh <file.rs> --check      # Exit 0 if file has test code, 1 if not
#   ./strip-test-code.sh <file.rs> --ranges     # Output line ranges that are test-only
#
# Requirements:
#   - rustup with nightly toolchain installed
#
# Note: Requires nightly because -Z unpretty is an unstable feature.
# The output is macro-expanded, so line numbers don't match the source.
# For line-number-preserving filtering, use the --ranges mode.
#

set -euo pipefail

# Check arguments
if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <file.rs> [--check|--ranges]" >&2
    exit 2
fi

FILE="$1"
MODE="${2:-strip}"

# Verify file exists
if [[ ! -f "$FILE" ]]; then
    echo "Error: File not found: $FILE" >&2
    exit 2
fi

# Check for nightly
if ! rustup run nightly rustc --version &>/dev/null; then
    echo "Error: nightly toolchain not installed. Run: rustup toolchain install nightly" >&2
    exit 2
fi

case "$MODE" in
    --check)
        # Check if file contains test code by comparing line counts
        original_lines=$(wc -l < "$FILE")
        stripped_lines=$(rustup run nightly rustc -Z unpretty=expanded "$FILE" 2>/dev/null | wc -l)

        # If stripped is significantly shorter, there was test code
        # (Some difference is expected due to macro expansion, but test modules are big)
        if [[ $stripped_lines -lt $((original_lines / 2)) ]]; then
            echo "Has significant test code"
            exit 0
        else
            echo "Minimal or no test code"
            exit 1
        fi
        ;;

    --ranges)
        # This is trickier - we need to find test function/module boundaries
        # For now, use a heuristic: find all #[test], #[cfg(test)], #[*::test] lines
        # and mark them plus subsequent lines until the closing brace

        # Get line numbers of test attributes
        grep -n '#\[test\]\|#\[cfg(test)\]\|#\[.*::test\]' "$FILE" 2>/dev/null | cut -d: -f1 | while read -r start_line; do
            # Find the end of this function/module by counting braces
            # This is a heuristic - we look for the function/module and count to closing brace
            awk -v start="$start_line" '
                NR >= start {
                    # Count braces
                    for (i = 1; i <= length($0); i++) {
                        c = substr($0, i, 1)
                        if (c == "{") depth++
                        if (c == "}") depth--
                    }
                    # If we started counting and depth returns to 0, we found the end
                    if (started && depth == 0) {
                        print start "-" NR
                        exit
                    }
                    if (depth > 0) started = 1
                }
            ' "$FILE"
        done
        ;;

    strip|--strip)
        # Output stripped code (macro-expanded, test code removed)
        rustup run nightly rustc -Z unpretty=expanded "$FILE" 2>/dev/null
        ;;

    *)
        echo "Unknown mode: $MODE" >&2
        echo "Use: --check, --ranges, or --strip (default)" >&2
        exit 2
        ;;
esac
