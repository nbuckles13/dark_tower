#!/usr/bin/env bash
# Proto breaking: buf breaking against the resolved base ref.
#
# Always-run per ADR-0033 §3 + §6 — invoked unconditionally from
# `scripts/audit.sh` per ADR-0033 §10:397. This wrapper does NOT self-gate via
# its own changed.sh: the always-run guarantee comes from the caller's
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

if ! command -v buf >/dev/null 2>&1; then
  emit_status FAIL "buf-binary-missing"
  exit 1
fi

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

run_and_emit "buf-breaking" buf breaking proto --against ".git#ref=${BASE_SHA},subdir=proto"
