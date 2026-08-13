#!/usr/bin/env bash
# Guard: every story file containing a dt-story task-metadata block must
# satisfy dt-story's schema (unique ids, valid deps, no cycles, pending tasks
# fully specified). Catches hand-edited manifest drift at Layer 3 before a
# run-story invocation trusts it. DRAFT for review.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

BIN=target/release/dt-story
# Version-agnostic discovery: matching the `v1` suffix here would make a
# future marker bump silently select zero files and exit 0 — a masked gate.
# Any version dt-story cannot parse must reach `validate` and fail loudly.
files="$(grep -l '# task-metadata (dt-story manifest ' docs/user-stories/*.md 2>/dev/null || true)"
[ -z "$files" ] && exit 0   # no manifests in tree yet — nothing to validate

[ -x "$BIN" ] || { echo "dt-story not built but manifests exist (cargo build --release -p dt-story)"; exit 1; }

rc=0
while IFS= read -r f; do
  "$BIN" validate "$f" || { echo "story manifest invalid: ${f}"; rc=1; }
done <<<"$files"
exit "$rc"
