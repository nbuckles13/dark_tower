#!/usr/bin/env bash
# Proto breaking: buf breaking against the resolved base ref.
#
# Always-run per ADR-0033 §3 + §6 — invoked unconditionally from
# `scripts/audit.sh` per ADR-0033 §10:397. This wrapper does NOT self-gate via
# any diff check: the always-run guarantee comes from the caller's
# invocation discipline. If a future cleanup pass adds a skip-if-untouched
# diff check here, the always-run gate silently breaks. Wrapper is
# unconditional; caller decides.
#
# IMPORTANT (security finding 1, mirrored from lang/rust/audit.sh + lang/ts/audit.sh):
# we deliberately do NOT pass "$@" through to `buf breaking`. Suppression via
# --exclude-path, --ignore, or a CLI --against override would silence the
# always-run gate untracked. There is also NO env-var bypass knob (no
# DEVLOOP_PROTO_SKIP_BREAKING etc.). Future override mechanism is deferred to
# ADR-0033 §10 Wave 3 (annotated allowlist file, in-tree, not CLI flags).
#
# Base ref source-of-truth: _get_base_ref.sh. We capture stdout (the sha) and
# conditionally suppress stderr based on DEVLOOP_LAYER — inside a layer, the
# layer script already emitted BASE_REF= once at layer start; standalone
# invocations let BASE_REF= flow as the only anchor.
#
# TODO(Wave 3, security tightening A): consider letting layer scripts export
# DEVLOOP_BASE_SHA once at layer start, so this wrapper reads the env var when
# set instead of re-invoking _get_base_ref.sh. Eliminates the double-resolution
# (one per audit lang vs one per layer) and sidesteps "are the two resolutions
# guaranteed identical?" — they should be (deterministic git ops, same cache),
# but single-source-of-truth is stronger than "should be." If adopted, validate
# DEVLOOP_BASE_SHA shape (`^[0-9a-f]{40}$`) before passing to buf — env vars are
# an attack surface even in trusted CI.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_buf.sh"
install_wrapper_exit_trap  # task #50: BASE_SHA=$(...) can set -e abort before run_and_emit

# COLLAPSE branch (ADR-0037 §D7): buf via `pnpm exec buf`; the preflight replaces the bare-`buf`
# `command -v` guard (four-token taxonomy + version assertion; DiD — breaking is read-only, but the
# uniform check means a stale toolchain reds as version-mismatch, not a spurious wire-compat pass/fail).
proto_buf_preflight || exit 1

if [[ -n "${DEVLOOP_LAYER:-}" ]]; then
  # Inside a layer — suppress the duplicate BASE_REF= stderr emission.
  # Capture exit code so a degraded git state (e.g. origin/main unreachable)
  # emits a precise `base-ref-unresolved` token rather than feeding an empty
  # `--against ".git#ref="` to buf (observability C3 refinement: precise
  # three-way classification — buf-binary-missing / base-ref-unresolved /
  # buf-breaking-{passed,failed}).
  if ! BASE_SHA=$("$(dirname "${BASH_SOURCE[0]}")/../_get_base_ref.sh" 2>/dev/null); then
    emit_status FAIL "base-ref-unresolved"
    exit 1
  fi
else
  # Standalone — let BASE_REF= + any stderr error message flow as the only
  # diagnostic anchor. Same exit-code discipline so an empty BASE_SHA never
  # reaches buf breaking.
  if ! BASE_SHA=$("$(dirname "${BASH_SOURCE[0]}")/../_get_base_ref.sh"); then
    emit_status FAIL "base-ref-unresolved"
    exit 1
  fi
fi

# Surface any in-tree `breaking.ignore` carve-out BEFORE running the gate, using the
# same `SUPPRESSED=` idiom as scripts/lang/rust/audit.sh and scripts/lang/ts/audit.sh
# (the other two audit-family gates that can be silenced by a tracked in-tree list).
#
# This is the loudness control for the ONE suppression channel the comment block above
# does not close. That block is about CLI/env bypass — we don't forward "$@", there is
# no skip env var — and it stays true. But `proto/buf.yaml` can silence this gate via
# `breaking.ignore`, and buf reports a plain green when it does, so without this line a
# reader of Layer-6 output sees `STATUS=OK REASON=buf-breaking-passed` with no
# signal that enforcement is off. Silent green on a disabled gate is the masked failure
# CLAUDE.md forbids; an in-tree, greppable, per-run `SUPPRESSED=` line is the fix, and
# it is also the mechanical reminder that a prose TODO cannot be — it fires on every
# run until the key is deleted.
#
# COVERAGE LIMIT: `.github/workflows/ci-client.yml` does NOT route through this wrapper —
# it hand-rolls `pnpm exec buf breaking` with its own `git merge-base` (self-described
# there as an INTERIM PATCH pending "run CI in the devloop image"). The buf.yaml carve-out
# itself DOES apply there (buf reads proto/buf.yaml regardless of caller — verified on the
# CI-pinned buf 1.72.0), but this SUPPRESSED= line does not, so GitHub CI still shows a
# bare green. Routing that step through this wrapper is the fix and is NOT a drop-in:
# ci-client.yml also runs on `push: [main, develop]`, where its hand-roll bases on
# origin/main while _get_base_ref.sh bases on HEAD~1. Owner: infrastructure.
#
# Deliberately a warning, not a failure: intentional wire breaks are legitimate (ADR-0033
# §13) and this wrapper must not become the thing that blocks one. It makes the carve-out
# impossible to *not notice*, which is the property that was missing.
__buf_yaml="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/proto/buf.yaml"
if [[ -f "$__buf_yaml" ]]; then
  # Paths listed under `breaking:` → `ignore:`. Scoped to that block so a future
  # `lint.ignore` is not misreported as a breaking-gate suppression.
  __ignored="$(awk '
    /^breaking:/            { in_breaking = 1; next }
    /^[^[:space:]#]/        { in_breaking = 0; in_ignore = 0 }
    in_breaking && /^  ignore:[[:space:]]*$/ { in_ignore = 1; next }
    in_breaking && /^  [^[:space:]#]/        { in_ignore = 0 }
    in_ignore && /^    - / { sub(/^    - /, ""); print }
  ' "$__buf_yaml" | paste -sd, -)"
  if [[ -n "$__ignored" ]]; then
    echo "SUPPRESSED=${__ignored}" >&2
    echo "buf breaking: enforcement is DISABLED for the paths above via proto/buf.yaml 'breaking.ignore'. A green result does NOT mean those files are break-free. See the comment block in proto/buf.yaml for the restore condition." >&2
  fi
fi

run_and_emit "buf-breaking" pnpm exec buf breaking proto --against ".git#ref=${BASE_SHA},subdir=proto"
