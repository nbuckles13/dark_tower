#!/usr/bin/env bash
# Audit-Suppressions Hygiene Guard (Layer 3, always-run) — task #47.
#
# This is the ALWAYS-RUN discipline backstop for the dep-change-gated audit
# (ADR-0033 §3 amendment): it runs on EVERY devloop + CI run via run-guards.sh,
# independent of whether any dependency manifest changed, and hard-fails when a
# suppression in audit-suppressions.toml goes past its `expires` date (or drifts
# from the generated derived files, or is malformed). See
# docs/runbooks/devloop-validation.md §6.3.
#
# Delegates to scripts/audit-suppressions-check.sh READ-ONLY (no --fix): the guard
# VERIFIES, it never regenerates derived files. The check's exit code (0 OK, 1 FAIL)
# is the guard's exit code; run-guards.sh classifies it.
#
# run-guards.sh invokes every guard as `guard "$SEARCH_PATH"` (repo root). The check
# operates on fixed repo-root paths and takes no positional path, so the arg is
# ignored here. The guard sets NO test-injection env (DEVLOOP_TEST, manifest/date
# overrides) — the always-run path is inert to those by construction (§J A+B+C).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# $1 ($SEARCH_PATH) intentionally ignored — the check uses fixed repo-root paths.
exec "${__here}/../../audit-suppressions-check.sh"
