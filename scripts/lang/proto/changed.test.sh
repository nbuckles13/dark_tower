#!/usr/bin/env bash
# changed.test.sh — locality self-test for lang/proto/changed.sh.
#
# Fires the proto predicate against a small set of representative paths.
# Consumes scripts/lang/_test_helpers.sh (dry-reviewer D1): the
# PASS/FAIL/assert_rc/run_with_cache scaffolding lives there. Future devloop
# migrates lang/rust/ and lang/ts/ to the same helper.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../_test_helpers.sh
source "${__here}/../_test_helpers.sh"

# Touched cases (exit 0). Path strings matched via `diff_touches_path "proto/"`
# — prefix-only, file existence is not checked. Inputs are deliberately a
# mix of synthetic paths and real config files; both exercise the same
# prefix pattern any regression would break.
assert_rc "proto/foo.proto"        0 "$(run_with_cache "proto/foo.proto")"
assert_rc "proto/internal.proto"   0 "$(run_with_cache "proto/internal.proto")"
assert_rc "proto/signaling.proto"  0 "$(run_with_cache "proto/signaling.proto")"
assert_rc "proto/buf.yaml"         0 "$(run_with_cache "proto/buf.yaml")"
assert_rc "proto/buf.gen.yaml"     0 "$(run_with_cache "proto/buf.gen.yaml")"

# Untouched cases (exit 1).
assert_rc "docs/x.md"              1 "$(run_with_cache "docs/x.md")"
assert_rc "crates/foo/src/lib.rs"  1 "$(run_with_cache "crates/foo/src/lib.rs")"
assert_rc "packages/foo/src/index.ts" 1 "$(run_with_cache "packages/foo/src/index.ts")"
assert_rc "scripts/test.sh"        1 "$(run_with_cache "scripts/test.sh")"

report_results "lang/proto/changed.test.sh"
