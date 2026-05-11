#!/usr/bin/env bash
# Proto changed-classifier: exit 0 if proto/ touched, 1 if untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_changed_helpers.sh"
diff_touches_path "proto/"
