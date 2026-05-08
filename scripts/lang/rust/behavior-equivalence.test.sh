#!/usr/bin/env bash
# behavior-equivalence.test.sh — proves the dispatcher refactor preserves the
# Rust test invocation contract.
#
# Verifies: `./scripts/test.sh <args>` (NEW dispatcher path) and
#           `./scripts/lang/rust/test.sh <args>` (OLD path, body migrated)
# produce IDENTICAL cargo argv on the same args.
#
# Hermetic via PATH-shim (test §3): no real cargo, no DB bring-up, sub-second.
# Tests two arg shapes from existing CI / muscle memory:
#   --workspace
#   -p ac-service --lib

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_ROOT="$(cd "${__here}/../.." && pwd)"

PASS=0
FAIL=0
FAILURES=()

# Set up a tempdir with `cargo` symlink to our shim, plus mock pg_isready/sqlx
# so the DB-bring-up path in lang/rust/test.sh short-circuits without external state.
shim_dir=$(mktemp -d)
trap "rm -rf '$shim_dir'" EXIT

cp "${__here}/fixtures/cargo-shim" "${shim_dir}/cargo"
chmod +x "${shim_dir}/cargo"

# Mock pg_isready so check_external_db decides "external DB reachable" — skips
# all container management (we don't want podman/docker invocation in tests).
cat > "${shim_dir}/pg_isready" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "${shim_dir}/pg_isready"

# Mock sqlx to claim no pending migrations.
cat > "${shim_dir}/sqlx" <<'EOF'
#!/usr/bin/env bash
echo "no pending"
exit 0
EOF
chmod +x "${shim_dir}/sqlx"

# Args: $1=label  $2=args-string (space-separated)
# Returns: 0 if old/new argv match, 1 otherwise; updates PASS/FAIL.
run_equivalence() {
  local label="$1"; shift
  local -a test_args=("$@")

  local old_log="${shim_dir}/argv-old-${label}.log"
  local new_log="${shim_dir}/argv-new-${label}.log"
  : > "$old_log"
  : > "$new_log"

  # OLD path: directly invoke lang/rust/test.sh.
  local old_rc=0
  env -i \
      PATH="${shim_dir}:$PATH" \
      HOME="$HOME" \
      DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test" \
      CARGO_SHIM_ARGV_LOG="$old_log" \
      "${SCRIPTS_ROOT}/lang/rust/test.sh" "${test_args[@]}" >/dev/null 2>&1 || old_rc=$?

  # NEW path: invoke scripts/test.sh dispatcher.
  # Need to also force changed.sh to say "touched" so the dispatcher actually invokes test.sh.
  # Inject a synthetic cache so rust changed.sh returns 0.
  local cache_tmp; cache_tmp=$(mktemp -d)
  printf 'crates/foo/src/lib.rs\n' > "${cache_tmp}/changed-files.layer-shared"

  local new_rc=0
  env -i \
      PATH="${shim_dir}:$PATH" \
      HOME="$HOME" \
      DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test" \
      CARGO_SHIM_ARGV_LOG="$new_log" \
      DEVLOOP_TMP="$cache_tmp" \
      "${SCRIPTS_ROOT}/test.sh" "${test_args[@]}" >/dev/null 2>&1 || new_rc=$?

  rm -rf "$cache_tmp"

  # Assert exit codes match.
  if [[ "$old_rc" == "$new_rc" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] exit-code mismatch: old=${old_rc} new=${new_rc}")
  fi

  # Assert recorded argv matches.
  if diff -q "$old_log" "$new_log" >/dev/null 2>&1; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] argv mismatch:
  old: $(cat "$old_log")
  new: $(cat "$new_log")")
  fi
}

# Test arg shape 1: --workspace
run_equivalence "workspace" --workspace

# Test arg shape 2: -p ac-service --lib
run_equivalence "p-ac-service-lib" -p ac-service --lib

printf '\nbehavior-equivalence.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '%s\n' "$f"
  done
  exit 1
fi
exit 0
