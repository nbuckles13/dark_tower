#!/usr/bin/env bash
# Audit dispatcher: invokes lang/<X>/audit.sh per language ALWAYS-RUN
# (no skip-if-untouched per ADR-0033 §3 + §6).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"
DEVLOOP_DISPATCH_ALWAYS_RUN=1 for_each_lang_with_verb "audit" "$@"
