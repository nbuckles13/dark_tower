#!/usr/bin/env bash
# Proto format: buf format --diff --exit-code (check-only; never reformats).
#
# Naming: this file is fmt.sh (not format.sh) to match the dispatcher verb
# (`scripts/fmt.sh` → `for_each_lang_with_verb "fmt"`) and the lang/rust/fmt.sh
# precedent. ADR-0033 §1 layout listing names it format.sh — that's a doc typo;
# a follow-up commit fixes the ADR §1 listing.
#
# No "$@" pass-through — same lockdown as the other buf wrappers (uniformity).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"

if ! command -v buf >/dev/null 2>&1; then
  emit_status FAIL "buf-binary-missing"
  exit 1
fi

run_and_emit "buf-format" buf format --diff --exit-code proto
