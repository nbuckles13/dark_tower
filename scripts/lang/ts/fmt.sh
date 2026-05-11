#!/usr/bin/env bash
# TS fmt: nx affected -t format (ADR-0033 §6 + §9).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"

BASE_SHA="$("${__here}/../_get_base_ref.sh")"

run_and_emit "nx-format" pnpm exec nx affected -t format --base="$BASE_SHA" "$@"
