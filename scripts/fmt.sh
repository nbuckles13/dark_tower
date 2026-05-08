#!/usr/bin/env bash
# Format dispatcher: invokes lang/<X>/fmt.sh per language with skip-if-untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"
for_each_lang_with_verb "fmt" "$@"
