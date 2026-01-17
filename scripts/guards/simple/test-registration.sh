#!/usr/bin/env bash
# Test Registration Guard
#
# Verifies that all test files in subdirectories are registered in their
# corresponding entry point files using #[path = "..."] directives.
#
# This guard catches a common mistake: adding a test file to a subdirectory
# (e.g., tests/integration/my_tests.rs) without registering it in the entry
# point (e.g., tests/integration_tests.rs), which causes the tests to silently
# not run.
#
# Works across all crates in the workspace.

set -e

# Change to repo root
cd "$(dirname "$0")/../../.."

ERRORS=0

# Find all test entry point files (*_tests.rs at crate test root)
for entry_file in crates/*/tests/*_tests.rs; do
    [ -f "$entry_file" ] || continue

    crate_tests_dir=$(dirname "$entry_file")
    entry_name=$(basename "$entry_file" .rs)  # e.g., "integration_tests"
    subdir_name="${entry_name%_tests}"        # e.g., "integration"
    subdir="$crate_tests_dir/$subdir_name"

    # Skip if no matching subdirectory exists
    [ -d "$subdir" ] || continue

    # Check each .rs file in subdirectory (except mod.rs)
    for test_file in "$subdir"/*.rs; do
        [ -f "$test_file" ] || continue
        filename=$(basename "$test_file")
        [ "$filename" = "mod.rs" ] && continue

        # Check if registered with #[path = "..."]
        if ! grep -q "\"$subdir_name/$filename\"" "$entry_file"; then
            echo "ERROR: $test_file not registered in $entry_file"
            echo "  Add: #[path = \"$subdir_name/$filename\"]"
            echo "       mod ${filename%.rs};"
            echo ""
            ERRORS=$((ERRORS + 1))
        fi
    done
done

if [ $ERRORS -gt 0 ]; then
    echo "Found $ERRORS unregistered test file(s)."
    echo "Tests in subdirectories must be registered in the entry point file."
    exit 1
fi

echo "All test files are properly registered."
exit 0
