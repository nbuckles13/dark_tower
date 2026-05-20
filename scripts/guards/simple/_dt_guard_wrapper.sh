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
# NOTE on extra positional args: `scripts/guards/run-guards.sh` invokes
# every guard with `"$SEARCH_PATH"` as `$1`. dt-guard subcommands take
# `--root` (not a positional), so wrapper-side `"$@"` would forward the
# search path as a clap-unrecognized positional and trip exit 2. This
# helper deliberately discards extra wrapper args — the previous inline
# wrappers had the same behavior (they ignored `$@` entirely).
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

# BASH_SOURCE[1] is the calling wrapper script; we resolve REPO_ROOT relative
# to that, NOT to this helper. Both helper + wrapper live in the same dir
# (`scripts/guards/simple/`), so the path math is identical either way.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DT_GUARD="${DT_GUARD:-$REPO_ROOT/target/release/dt-guard}"

[[ -x "$DT_GUARD" ]] || {
    echo "STATUS=FAIL REASON=dt-guard-binary-missing"
    exit 1
}

exec "$DT_GUARD" "$_dt_guard_subcommand" --root "$REPO_ROOT"
