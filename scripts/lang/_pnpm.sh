#!/usr/bin/env bash
# _pnpm.sh — sourced helpers for pipeline wrappers that run `pnpm exec`.
#
# pnpm-workspace.yaml sets `verifyDepsBeforeRun: error`: `pnpm exec`/`pnpm run` refuse to
# run against a node_modules that no longer matches the lockfile. Without a preflight that
# refusal surfaces as whatever the wrapped command's REASON is (`nx-lint-failed`,
# `buf-not-installed`, …), pointing a triager at the wrong thing. These helpers give it
# its own token.
#
# Source AFTER _common.sh (uses emit_status).

# The ONE place the verify step is bypassed. Safe only for a read-only probe whose result
# the caller classifies itself AND that is followed by `pnpm_deps_fresh`: _buf.sh uses it
# to read the installed buf version, so a stale buf still reports `buf-version-mismatch`
# (its precedence), and any other staleness is caught by `pnpm_deps_fresh` right after.
# Never use it to run a gate.
pnpm_exec_unverified() {
  pnpm --config.verify-deps-before-run=false exec "$@"
}

# Is node_modules in step with the lockfile? Runs a no-op through `pnpm exec` with the
# verify step ON. The decision is the exit status alone (pnpm's message is shown to the
# operator, never matched on). A missing pnpm is checked first and named as such
# (`pnpm-unavailable`, the token _buf.sh uses), so the failure below is the verify refusal.
# Outputs: on failure, STATUS=FAIL REASON=pnpm-unavailable|pnpm-deps-stale, the remedy, then
# pnpm's stderr.
# Returns: 0 fresh, 1 unavailable or stale.
pnpm_deps_fresh() {
  local err
  if ! command -v pnpm >/dev/null 2>&1; then
    emit_status FAIL "pnpm-unavailable"
    printf 'pnpm is not on PATH — activate the packageManager pin (corepack enable), then `pnpm install --frozen-lockfile`.\n' >&2
    return 1
  fi
  if err="$(pnpm exec true 2>&1 >/dev/null)"; then
    return 0
  fi
  emit_status FAIL "pnpm-deps-stale"
  printf 'node_modules does not match the lockfile (pnpm verifyDepsBeforeRun) — run `pnpm install --frozen-lockfile`.\n' >&2
  [[ -n "$err" ]] && printf '%s\n' "$err" >&2
  return 1
}
