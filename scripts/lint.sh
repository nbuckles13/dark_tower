#!/usr/bin/env bash
# Lint dispatcher: invokes lang/<X>/lint.sh per language with skip-if-untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"
for_each_lang_with_verb "lint" "$@"
