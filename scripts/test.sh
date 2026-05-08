#!/usr/bin/env bash
# Test dispatcher (per-verb, ADR-0033 §1).
#
# Refactored from the original Rust-only test.sh:
#   - Body migrated to scripts/lang/rust/test.sh (DB bring-up + cargo test).
#   - This file is now a thin dispatcher; args flow through to lang/rust/test.sh.
#
# Backward compat preserved:
#   ./scripts/test.sh --workspace
#   ./scripts/test.sh -p ac-service --lib
#   ./scripts/test.sh --workspace -- --test-threads=1
#
# Behavior-equivalence test (scripts/lang/rust/behavior-equivalence.test.sh)
# verifies same exit code + same cargo argv on a known Rust-only fixture
# diff before and after this refactor.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"
for_each_lang_with_verb "test" "$@"
