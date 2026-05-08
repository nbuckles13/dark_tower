#!/usr/bin/env bash
# Rust changed-classifier: exit 0 if Rust touched, 1 if untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_changed_helpers.sh"
diff_touches_path "crates/" || diff_touches_root_files "Cargo.toml" "Cargo.lock" "rust-toolchain.toml"
