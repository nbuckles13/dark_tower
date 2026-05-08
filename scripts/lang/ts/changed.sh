#!/usr/bin/env bash
# TS changed-classifier: exit 0 if TS touched, 1 if untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_changed_helpers.sh"
diff_touches_path "packages/" \
  || diff_touches_root_files \
       "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml" \
       "nx.json" "tsconfig.base.json" ".nvmrc"
