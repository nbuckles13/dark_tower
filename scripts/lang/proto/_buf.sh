#!/usr/bin/env bash
# Shared buf preflight for the proto pipeline lane — COLLAPSE branch (ADR-0037 §D7; @team-lead 2026-09-20):
# every proto wrapper (compile/lint/breaking/fmt) invokes buf via `pnpm exec buf`, NOT a bare PATH `buf`.
# This preflight is the PRECONDITION each wrapper runs BEFORE its buf call, so a stale/absent toolchain
# reds with a specific, actionable token INSTEAD of surfacing as a downstream failure.
#
# FOUR DISTINCT failure states, FOUR DISTINCT REASON tokens — no two share one (@operations/@security):
# each sends a triager somewhere different, so conflating them is the exact defect this loop caught 3×
# (fmt pre-pass rc=1; the old `command -v buf`; here).
#
# The version check is EXACT string equality against the LOCKFILE-declared `@bufbuild/buf`, with the
# EXPECTED value FAIL-CLOSED-derived (jq -er + non-empty). It is evaluated BEFORE any buf call that can
# report drift — @security's precedence: a stale writer must red as `buf-version-mismatch` (remedy: `pnpm
# install --frozen-lockfile`), NOT as `buf-format-drift` (whose remedy — reformat — drives the operator
# into the unwinnable loop). NEVER relax this to a substring/grep match: "2.0" is a substring of "1.72.0".
#
# WHY the version check matters here even though CI runs `--frozen-lockfile`: the devloop container does
# NOT bake node_modules and its entrypoint's `pnpm install` is presence-gated (`[ ! -x …/.bin/nx ]`), so a
# container whose node_modules predates a lockfile bump is NOT reinstalled — a stale LOCAL writer while CI
# checks fresh. This assertion is the only thing that closes that gap.
#
# `proto_buf_preflight` emit_status FAILs + returns 1 on any failure (caller: `|| exit 1`); returns 0 clean.

__buf_repo_root() {
  # Prefer git; fall back to the fixed script-relative repo root (scripts/lang/proto/ -> ../../..).
  git rev-parse --show-toplevel 2>/dev/null || (cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
}

proto_buf_preflight() {
  # (1) pnpm / node_modules toolchain present at all?
  if ! command -v pnpm >/dev/null 2>&1; then
    emit_status FAIL "pnpm-unavailable"
    printf 'proto lane runs `pnpm exec buf`; pnpm is unavailable — run `pnpm install --frozen-lockfile`.\n' >&2
    return 1
  fi
  # (2) @bufbuild/buf actually installed (does `pnpm exec buf` resolve a binary)?
  local actual
  if ! actual="$(pnpm exec buf --version 2>/dev/null)"; then
    emit_status FAIL "buf-not-installed"
    printf '`pnpm exec buf` did not resolve — @bufbuild/buf is not installed; run `pnpm install --frozen-lockfile`.\n' >&2
    return 1
  fi
  actual="$(printf '%s' "$actual" | tr -d '[:space:]')"
  # (3) EXPECTED value: fail-closed derivation from the lockfile-declared pin (the jq lesson — a wrong
  #     path returns null rc=0 and would make the comparison vacuously pass).
  local pkg expected
  pkg="$(__buf_repo_root)/package.json"
  if ! expected="$(jq -er '.devDependencies["@bufbuild/buf"] // empty' "$pkg" 2>/dev/null)" || [[ -z "$expected" ]]; then
    emit_status FAIL "buf-version-underivable"
    printf 'could not derive the pinned @bufbuild/buf version from %s (fail-closed).\n' "$pkg" >&2
    return 1
  fi
  # (4) EXACT equality — never substring. Runs BEFORE any format/lint/breaking call (precedence).
  if [[ "$actual" != "$expected" ]]; then
    emit_status FAIL "buf-version-mismatch"
    printf 'buf %s != pinned @bufbuild/buf %s — run `pnpm install --frozen-lockfile` (do NOT reformat; a stale writer would rewrite into the old style).\n' "$actual" "$expected" >&2
    return 1
  fi
  return 0
}
