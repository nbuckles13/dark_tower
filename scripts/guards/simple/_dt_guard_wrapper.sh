#!/usr/bin/env bash
# Shared prelude for dt-guard wrappers (ADR-0034 Wave 1 E-DRY-2 fold-in
# 2026-05-19 → resolved 2026-05-20 in-loop).
#
# The 8 dt-guard subcommand wrappers under `scripts/guards/simple/` share
# a byte-identical 5-line prelude (set -euo pipefail; SCRIPT_DIR; REPO_ROOT;
# DT_GUARD; binary-missing check) before their single `exec` line. Per
# ADR-0034 §3 the wrapper layer is intentionally thin; this helper extracts
# the prelude so a future shape change (timeout override, extra env-var,
# structured error envelope) touches one file instead of eight.
#
# Usage from a wrapper:
#
#     #!/usr/bin/env bash
#     # shellcheck source=./_dt_guard_wrapper.sh
#     source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" <subcommand>
#
# Contract:
#   - $1 (required): the dt-guard subcommand to invoke (e.g. `cite-no-line-numbers`).
#   - DT_GUARD env override: if set + executable, used directly; otherwise
#     defaults to `$REPO_ROOT/target/release/dt-guard`.
#   - On missing/non-executable binary: writes `STATUS=FAIL REASON=dt-guard-binary-missing`
#     to stdout and exits 1 BEFORE invoking the subcommand.
#   - On success: `exec`s the binary with `--root "$REPO_ROOT"` (so this
#     script's exit code = the dt-guard binary's exit code; no extra shell
#     frame on the failure path).
#
# NOTE on extra args (Wave-2 amend per @paired-client F4):
#
# `scripts/guards/run-guards.sh` invokes every guard with `"$SEARCH_PATH"` as
# `$1`. The wrapper SHIFTs the subcommand name off `$@` (line below), so any
# remaining args that the consumer wrapper supplies are forwarded to the
# binary AFTER `--root "$REPO_ROOT"`. This supports per-wrapper extra clap
# flags (e.g. `ts/exports-map-closed.sh` builds `extra_args+=(--strict)`
# from `STRICT_EXPORTS_MAP=1`).
#
# Wave-1 consumer wrappers do not pass extra args — `"$@"` is empty after the
# shift — so the forwarding is behavior-preserving for them. The `run-guards.sh`
# positional SEARCH_PATH is still discarded because consumer wrappers do not
# forward it (they source this helper without `"$@"` themselves; the helper
# sees only the subcommand-name arg + whatever extras the wrapper explicitly
# appended).
#
# Non-executable on purpose: `scripts/guards/run-guards.sh` gates execution
# on `[[ -x "$guard" ]]`, so leaving the chmod bit off prevents the runner
# from invoking this helper as a guard. Each consumer wrapper IS executable.

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "STATUS=FAIL REASON=dt-guard-wrapper-missing-subcommand"
    exit 1
fi

_dt_guard_subcommand="$1"
shift

# BASH_SOURCE[1] is the calling wrapper script. Resolve REPO_ROOT by walking
# upward from the wrapper's directory until we find `.git` — this lets the
# same prelude serve wrappers at `scripts/guards/simple/*.sh` (Wave 1) AND
# `scripts/guards/simple/ts/*.sh` (Wave 2 group (a)) without baking a
# fixed-depth `../../..` into the helper.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
while [[ "$REPO_ROOT" != "/" && ! -e "$REPO_ROOT/.git" ]]; do
    REPO_ROOT="$(dirname "$REPO_ROOT")"
done
if [[ ! -e "$REPO_ROOT/.git" ]]; then
    echo "STATUS=FAIL REASON=dt-guard-wrapper-repo-root-not-found"
    exit 1
fi
DT_GUARD="${DT_GUARD:-$REPO_ROOT/target/release/dt-guard}"

[[ -x "$DT_GUARD" ]] || {
    echo "STATUS=FAIL REASON=dt-guard-binary-missing"
    exit 1
}

exec "$DT_GUARD" "$_dt_guard_subcommand" --root "$REPO_ROOT" "$@"
