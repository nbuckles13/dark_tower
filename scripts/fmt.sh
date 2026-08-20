#!/usr/bin/env bash
# Format dispatcher: invokes lang/<X>/fmt.sh for every language (always-run — the
# skip-if-untouched short-circuit was retired 2026-08-20, ADR-0033 §3).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"
for_each_lang_with_verb "fmt" "$@"
