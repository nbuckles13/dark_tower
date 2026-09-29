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

cp "${__here}/fixtures/cargo-shim" "${shim_dir}/cargo"
chmod +x "${shim_dir}/cargo"

# Mock pg_isready so check_external_db decides "external DB reachable" — skips
# all container management (we don't want podman/docker invocation in tests).
cat > "${shim_dir}/pg_isready" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "${shim_dir}/pg_isready"

# Mock sqlx: records its argv to $SQLX_SHIM_ARGV_LOG (when set) and exits
# $SQLX_SHIM_RC (default 0). test.sh runs `sqlx migrate run` UNCONDITIONALLY —
# there is no `migrate info` gate any more (the old mock printed "no pending",
# which the old gate's `grep pending` happened to match, so it proved nothing).
# `sqlx --version` answers `sqlx-cli $SQLX_SHIM_VERSION` (default: the version test.sh
# derives from Cargo.lock, via the same ONE reader) and is NOT logged, so the logged argv
# is exactly the migration calls.
# shellcheck source=../../../infra/lib/cargo-lock-version.sh
source "${SCRIPTS_ROOT}/../infra/lib/cargo-lock-version.sh"
LOCK_SQLX="$(cargo_lock_version "${SCRIPTS_ROOT}/../Cargo.lock" sqlx)"
cat > "${shim_dir}/sqlx" <<EOF
#!/usr/bin/env bash
if [[ "\$1" == "--version" ]]; then printf 'sqlx-cli %s\n' "\${SQLX_SHIM_VERSION:-${LOCK_SQLX}}"; exit 0; fi
[[ -n "\${SQLX_SHIM_ARGV_LOG:-}" ]] && printf '%s\n' "\$*" >> "\$SQLX_SHIM_ARGV_LOG"
exit "\${SQLX_SHIM_RC:-0}"
EOF
chmod +x "${shim_dir}/sqlx"

# A PATH with the shims but WITHOUT any real sqlx (the devloop image has one in
# /usr/local/cargo/bin): every PATH entry holding an executable `sqlx` is dropped.
path_without_sqlx() {
  local out="" d
  local IFS=':'
  for d in $PATH; do
    [[ -n "$d" && -x "${d}/sqlx" ]] && continue
    out+="${out:+:}${d}"
  done
  printf '%s' "$out"
}
nosqlx_dir=$(mktemp -d)
trap "rm -rf '$shim_dir' '$nosqlx_dir'" EXIT
cp "${shim_dir}/cargo" "${shim_dir}/pg_isready" "$nosqlx_dir/"

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

  # NEW path: invoke scripts/test.sh dispatcher. It always-runs test.sh (ADR-0033 §3 —
  # there is no changed.sh gate to satisfy), so this path needs no diff/cache setup; a
  # throwaway DEVLOOP_TMP just isolates any log writes. The dispatcher is polyglot now
  # and passes the SAME args to every language (vitest rejects `--workspace`), so the
  # equivalence under test — the Rust arm's cargo argv — is scoped to rust. Found when
  # this harness was first wired into layer3.sh (ADR-0038 devloop 2): unwired, it had
  # been red since the TS/proto arms joined the dispatcher.
  local tmp_devloop; tmp_devloop=$(mktemp -d)

  local new_rc=0
  env -i \
      PATH="${shim_dir}:$PATH" \
      HOME="$HOME" \
      DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test" \
      CARGO_SHIM_ARGV_LOG="$new_log" \
      DEVLOOP_TMP="$tmp_devloop" \
      DEVLOOP_DISPATCH_INCLUDE_LANGS=rust \
      "${SCRIPTS_ROOT}/test.sh" "${test_args[@]}" >/dev/null 2>&1 || new_rc=$?

  rm -rf "$tmp_devloop"

  # Assert exit codes match.
  if [[ "$old_rc" == "$new_rc" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] exit-code mismatch: old=${old_rc} new=${new_rc}")
  fi

  # Non-vacuity: two EMPTY logs (both paths aborted before cargo) would "match".
  # Each must hold a real cargo invocation carrying this case's args.
  local log first_arg="${test_args[0]}"
  for log in "$old_log" "$new_log"; do
    if [[ -s "$log" ]] && grep -q -- "cargo test .*${first_arg}" "$log"; then
      PASS=$((PASS + 1))
    else
      FAIL=$((FAIL + 1))
      FAILURES+=("[${label}] vacuous: ${log##*/} holds no 'cargo test … ${first_arg}' (got: $(cat "$log"))")
    fi
  done

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

# === The unit-test DB migration (ADR-0038 devloop 2) =========================
# test.sh is the ONE thing that migrates the unit-test DB, so it must fail LOUDLY
# — before cargo ever runs — when it cannot migrate.

# run_rust_test <label> <path> [VAR=val...]: run lang/rust/test.sh --workspace
# with the given PATH; sets RT_RC and RT_OUT; cargo argv -> ${shim_dir}/cargo-<label>.log,
# sqlx argv -> ${shim_dir}/sqlx-<label>.log.
run_rust_test() {
  local label="$1" path="$2"; shift 2
  : > "${shim_dir}/cargo-${label}.log"
  : > "${shim_dir}/sqlx-${label}.log"
  RT_RC=0
  RT_OUT="$(env -i PATH="$path" HOME="$HOME" \
      DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test" \
      CARGO_SHIM_ARGV_LOG="${shim_dir}/cargo-${label}.log" \
      SQLX_SHIM_ARGV_LOG="${shim_dir}/sqlx-${label}.log" "$@" \
      "${SCRIPTS_ROOT}/lang/rust/test.sh" --workspace 2>&1)" || RT_RC=$?
}
check() {  # check <label> <condition-rc> <message>
  if [[ "$2" == 0 ]]; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); FAILURES+=("[$1] $3"); fi
}

# rust-test-sqlx-absent-fails-before-cargo
run_rust_test sqlx-absent "${nosqlx_dir}:$(path_without_sqlx)"
check rust-test-sqlx-absent-fails-before-cargo "$([[ $RT_RC -ne 0 ]] && echo 0 || echo 1)" \
  "test.sh exited 0 with no sqlx on PATH"
check rust-test-sqlx-absent-names-sqlx "$([[ "$RT_OUT" == *"sqlx-cli not found"* ]] && echo 0 || echo 1)" \
  "no 'sqlx-cli not found' message: ${RT_OUT}"
check rust-test-sqlx-absent-cargo-never-ran "$([[ ! -s "${shim_dir}/cargo-sqlx-absent.log" ]] && echo 0 || echo 1)" \
  "cargo ran against an unmigrated DB: $(cat "${shim_dir}/cargo-sqlx-absent.log")"

# rust-test-migrate-run-failure-fails-before-cargo (e.g. checksum drift)
run_rust_test migrate-fails "${shim_dir}:$(path_without_sqlx)" SQLX_SHIM_RC=1
check rust-test-migrate-run-failure-fails-before-cargo "$([[ $RT_RC -ne 0 ]] && echo 0 || echo 1)" \
  "test.sh exited 0 although 'sqlx migrate run' failed"
check rust-test-migrate-run-failure-attributed "$([[ "$RT_OUT" == *"Failed to run migrations"* ]] && echo 0 || echo 1)" \
  "the failure must be attributed to the migration, not something earlier: ${RT_OUT}"
check rust-test-migrate-run-failure-cargo-never-ran "$([[ ! -s "${shim_dir}/cargo-migrate-fails.log" ]] && echo 0 || echo 1)" \
  "cargo ran after a failed migration"

# rust-test-migrate-run-unconditional-then-cargo (positive control)
run_rust_test migrate-ok "${shim_dir}:$(path_without_sqlx)"
check rust-test-migrate-run-unconditional-rc "$([[ $RT_RC -eq 0 ]] && echo 0 || echo 1)" \
  "test.sh failed with a working sqlx: rc=${RT_RC} ${RT_OUT}"
check rust-test-migrate-run-unconditional-then-cargo "$([[ "$(cat "${shim_dir}/sqlx-migrate-ok.log")" == "migrate run" ]] && echo 0 || echo 1)" \
  "sqlx argv must be exactly 'migrate run' (no 'migrate info' gate); got: $(cat "${shim_dir}/sqlx-migrate-ok.log")"
check rust-test-migrate-run-cargo-ran "$([[ -s "${shim_dir}/cargo-migrate-ok.log" ]] && echo 0 || echo 1)" \
  "cargo did not run after a successful migration"

# rust-test-sqlx-version-drift-fails-before-cargo: an image built before a sqlx bump keeps
# the old CLI; test.sh must refuse it (loudly, naming both versions) before migrating or testing.
run_rust_test version-drift "${shim_dir}:$(path_without_sqlx)" SQLX_SHIM_VERSION=0.0.1
check rust-test-sqlx-version-drift-fails "$([[ $RT_RC -ne 0 ]] && echo 0 || echo 1)" \
  "test.sh exited 0 with sqlx-cli 0.0.1 against Cargo.lock sqlx ${LOCK_SQLX}"
check rust-test-sqlx-version-drift-own-token "$([[ "$RT_OUT" == *"SQLX_CLI_VERSION_MISMATCH:"* && "$RT_OUT" == *"infra/devloop/devloop.sh --rebuild (host)"* ]] && echo 0 || echo 1)" \
  "mismatch must carry its own token and the devloop rebuild remedy: ${RT_OUT}"
check rust-test-sqlx-version-drift-names-both "$([[ "$RT_OUT" == *"sqlx-cli 0.0.1 != Cargo.lock sqlx ${LOCK_SQLX}"* ]] && echo 0 || echo 1)" \
  "drift message must name both versions: ${RT_OUT}"
check rust-test-sqlx-version-drift-no-migrate "$([[ ! -s "${shim_dir}/sqlx-version-drift.log" ]] && echo 0 || echo 1)" \
  "a mismatched CLI still ran: $(cat "${shim_dir}/sqlx-version-drift.log")"
check rust-test-sqlx-version-drift-cargo-never-ran "$([[ ! -s "${shim_dir}/cargo-version-drift.log" ]] && echo 0 || echo 1)" \
  "cargo ran with a mismatched sqlx-cli"
check rust-test-sqlx-absent-hint-is-pinned "$([[ "$(env -i PATH="${nosqlx_dir}:$(path_without_sqlx)" HOME="$HOME" DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test" CARGO_SHIM_ARGV_LOG=/dev/null "${SCRIPTS_ROOT}/lang/rust/test.sh" --workspace 2>&1)" == *"--version =${LOCK_SQLX}"* ]] && echo 0 || echo 1)" \
  "the sqlx-absent host hint must be pinned to Cargo.lock's version"

printf '\nbehavior-equivalence.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '%s\n' "$f"
  done
  exit 1
fi
exit 0
