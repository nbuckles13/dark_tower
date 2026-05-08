#!/usr/bin/env bash
# Compile dispatcher: invokes lang/<X>/compile.sh per language with skip-if-untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"
for_each_lang_with_verb "compile" "$@"
