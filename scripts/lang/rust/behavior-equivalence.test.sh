#!/usr/bin/env bash
# behavior-equivalence.test.sh — proves the dispatcher refactor preserves the
# Rust test invocation contract.
#
# Verifies: `./scripts/test.sh <args>` (NEW dispatcher path) and
#           `./scripts/lang/rust/test.sh <args>` (OLD path, body migrated)
# produce IDENTICAL cargo argv on the same args, and that the argv is exactly the
# two lanes test.sh runs: nextest with every arg, then doctests with the
# package-selection subset.
#
# Hermetic via PATH-shim (test §3): no real cargo, no DB bring-up, sub-second.
# Tests three arg shapes (from existing CI / muscle memory, plus `=`/`--` forms):
#   --workspace
#   -p ac-service --lib
#   --package=common -- some_filter
# and the nextest pin: absent / drifted runner fails before anything runs, and a red
# nextest lane is not masked by the doctest lane after it.

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

# `cargo` = the argv-recording fixture, fronted by an answer to test.sh's
# `cargo nextest --version` probe: `cargo-nextest $NEXTEST_SHIM_VERSION` (default: the
# pin test.sh reads, via the same ONE reader), or cargo's own "no such command"
# (exit 101) when NEXTEST_SHIM_ABSENT=1. The probe is NOT logged, so the logged argv is
# exactly the test-lane calls.
# shellcheck source=../../../infra/lib/cargo-tools.sh
source "${SCRIPTS_ROOT}/../infra/lib/cargo-tools.sh"
PIN_NEXTEST="$(cargo_tool_version "${SCRIPTS_ROOT}/../infra/cargo-tools.versions" cargo-nextest)"
cp "${__here}/fixtures/cargo-shim" "${shim_dir}/cargo-record"
chmod +x "${shim_dir}/cargo-record"
# `cargo metadata` (test.sh asks which selected packages have a lib, for the doctest lane)
# is answered from a synthetic workspace, also unlogged: two lib crates the arg shapes
# below select, and a bin-only crate.
cat > "${shim_dir}/metadata.json" <<'EOF'
{"packages":[
 {"name":"ac-service","targets":[{"kind":["lib"]},{"kind":["bin"]}]},
 {"name":"common","targets":[{"kind":["lib"]}]},
 {"name":"bin-only-tool","targets":[{"kind":["bin"]}]}
]}
EOF
cat > "${shim_dir}/cargo" <<EOF
#!/usr/bin/env bash
if [[ "\$1" == "metadata" ]]; then cat "\$(dirname "\$0")/metadata.json"; exit 0; fi
if [[ "\$1" == "nextest" && "\$2" == "--version" ]]; then
  if [[ "\${NEXTEST_SHIM_ABSENT:-}" == 1 ]]; then echo "error: no such command: nextest" >&2; exit 101; fi
  printf 'cargo-nextest %s (shim)\n' "\${NEXTEST_SHIM_VERSION:-${PIN_NEXTEST}}"; exit 0
fi
exec "\$(dirname "\$0")/cargo-record" "\$@"
EOF
chmod +x "${shim_dir}/cargo"

# The two lane commands, read from test.sh itself (not retyped here): the argv each
# run_and_emit lane passes before the caller's args. CARGO_LOCKED is empty here (no
# GITHUB_ACTIONS / DEVLOOP_FMT_CHECK_ONLY under `env -i`).
TEST_SH_SRC="$(cat "${SCRIPTS_ROOT}/lang/rust/test.sh")"
NEXTEST_CMD="$(sed -n 's/.*run_and_emit "cargo-nextest" \(cargo nextest run [^"$]*\)"\${CARGO_LOCKED.*/\1/p' <<< "$TEST_SH_SRC")"
DOCTEST_CMD="$(sed -n 's/.*run_and_emit "cargo-doctest" \(cargo test --doc [^"$]*\)"\${CARGO_LOCKED.*/\1/p' <<< "$TEST_SH_SRC")"

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
cp "${shim_dir}/cargo" "${shim_dir}/cargo-record" "${shim_dir}/metadata.json" "${shim_dir}/pg_isready" "$nosqlx_dir/"

# Args: $1=label  $2=expected doctest-lane args (space-separated: the package-selection
#       subset of the rest)  $3..=the args passed to test.sh
# Updates PASS/FAIL.
run_equivalence() {
  local label="$1" want_doc="$2"; shift 2
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

  # Non-vacuity + exact lanes: two EMPTY logs (both paths aborted before cargo) would
  # "match", so each log must be EXACTLY the two lane calls: nextest with every arg,
  # then doctests with only the package-selection args (never all args, never none).
  local log want joined
  joined="$(IFS=' '; printf '%s' "${test_args[*]}")"
  want="$(printf '%s %s\n%s %s' "${NEXTEST_CMD% }" "$joined" "${DOCTEST_CMD% }" "$want_doc" | sed 's/ *$//')"
  for log in "$old_log" "$new_log"; do
    if [[ -n "$NEXTEST_CMD" && -n "$DOCTEST_CMD" && "$(cat "$log")" == "$want" ]]; then
      PASS=$((PASS + 1))
    else
      FAIL=$((FAIL + 1))
      FAILURES+=("[${label}] ${log##*/} is not exactly the two lanes.
  want: $(tr '\n' '|' <<< "$want")
  got:  $(tr '\n' '|' < "$log")")
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
run_equivalence "workspace" "--workspace" --workspace

# Test arg shape 2: -p ac-service --lib. `--lib` is a target selector `--doc` rejects,
# so the doctest lane keeps only `-p ac-service`.
run_equivalence "p-ac-service-lib" "-p ac-service" -p ac-service --lib

# Test arg shape 3: test-binary args after `--` belong to nextest only.
run_equivalence "package-eq-and-filter" "--package=common" --package=common -- some_filter

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

# === The nextest pin (infra/cargo-tools.versions) ==============================
# test.sh runs the Rust tests with cargo-nextest; a missing or drifted runner must fail
# LOUDLY before any migration or test, never fall back to `cargo test`.
last_status() { grep '^STATUS=' <<< "$1" | tail -n1; }
run_rust_test nextest-absent "${shim_dir}:$PATH" NEXTEST_SHIM_ABSENT=1
check rust-test-nextest-absent-fails "$([[ $RT_RC -ne 0 ]] && echo 0 || echo 1)" \
  "test.sh exited 0 without cargo-nextest"
check rust-test-nextest-absent-status "$([[ "$(last_status "$RT_OUT")" == "STATUS=FAIL REASON=cargo-nextest-missing" ]] && echo 0 || echo 1)" \
  "last STATUS must be the not-found token: ${RT_OUT}"
check rust-test-nextest-absent-hint "$([[ "$RT_OUT" == *"infra/devloop/devloop.sh --rebuild (host)"* && "$RT_OUT" == *"--version =${PIN_NEXTEST}"* ]] && echo 0 || echo 1)" \
  "not-found must carry the rebuild remedy and the pinned install hint: ${RT_OUT}"
check rust-test-nextest-absent-nothing-ran "$([[ ! -s "${shim_dir}/cargo-nextest-absent.log" && ! -s "${shim_dir}/sqlx-nextest-absent.log" ]] && echo 0 || echo 1)" \
  "tests or migrations ran without cargo-nextest"

run_rust_test nextest-drift "${shim_dir}:$PATH" NEXTEST_SHIM_VERSION=0.0.1
check rust-test-nextest-drift-fails "$([[ $RT_RC -ne 0 ]] && echo 0 || echo 1)" \
  "test.sh exited 0 with cargo-nextest 0.0.1 against pin ${PIN_NEXTEST}"
check rust-test-nextest-drift-status "$([[ "$(last_status "$RT_OUT")" == "STATUS=FAIL REASON=cargo-nextest-version-mismatch" ]] && echo 0 || echo 1)" \
  "last STATUS must be the mismatch token: ${RT_OUT}"
check rust-test-nextest-drift-names-both "$([[ "$RT_OUT" == *"CARGO_NEXTEST_VERSION_MISMATCH: cargo-nextest 0.0.1 != pinned ${PIN_NEXTEST}"* ]] && echo 0 || echo 1)" \
  "drift message must carry its token and name both versions: ${RT_OUT}"
check rust-test-nextest-drift-nothing-ran "$([[ ! -s "${shim_dir}/cargo-nextest-drift.log" && ! -s "${shim_dir}/sqlx-nextest-drift.log" ]] && echo 0 || echo 1)" \
  "tests or migrations ran with a drifted cargo-nextest"

# A red nextest lane must not be masked by a green doctest lane after it: the
# dispatcher reads the wrapper's LAST STATUS line.
run_rust_test lane-red "${shim_dir}:$PATH" CARGO_SHIM_FAIL_ON=nextest
check rust-test-red-lane-fails "$([[ $RT_RC -ne 0 ]] && echo 0 || echo 1)" \
  "test.sh exited 0 with a failing nextest lane"
check rust-test-red-lane-last-status "$([[ "$(last_status "$RT_OUT")" == "STATUS=FAIL REASON=rust-test-lanes-failed-nextest" ]] && echo 0 || echo 1)" \
  "last STATUS must name the red lane: ${RT_OUT}"
check rust-test-red-lane-doctests-still-ran "$(grep -q '^cargo test --doc ' "${shim_dir}/cargo-lane-red.log" && echo 0 || echo 1)" \
  "the doctest lane must still run after a red nextest lane"

# A selection with no lib target has no doctests: `cargo test --doc` would exit 101 there, so
# the lane is skipped EXPLICITLY (its own OK token), never silently and never by matching
# cargo's error text. The bin-only crate comes from the shim's metadata, not a literal.
BIN_ONLY="$(jq -r '[.packages[] | select([.targets[].kind[]] | index("lib") | not) | .name][0]' "${shim_dir}/metadata.json")"
: > "${shim_dir}/cargo-bin-only.log"
RT_RC=0
RT_OUT="$(env -i PATH="${shim_dir}:$PATH" HOME="$HOME" \
    DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test" \
    CARGO_SHIM_ARGV_LOG="${shim_dir}/cargo-bin-only.log" \
    "${SCRIPTS_ROOT}/lang/rust/test.sh" -p "$BIN_ONLY" 2>&1)" || RT_RC=$?
check rust-test-no-lib-selection-green "$([[ $RT_RC -eq 0 && -n "$BIN_ONLY" ]] && echo 0 || echo 1)" \
  "a bin-only selection (${BIN_ONLY:-<none found>}) must not red the run: rc=${RT_RC} ${RT_OUT}"
check rust-test-no-lib-selection-token "$([[ "$RT_OUT" == *"STATUS=OK REASON=cargo-doctest-no-lib-targets"* ]] && echo 0 || echo 1)" \
  "the skipped doctest lane must say so: ${RT_OUT}"
check rust-test-no-lib-selection-no-doc-call "$(grep -q 'cargo test --doc' "${shim_dir}/cargo-bin-only.log" && echo 1 || echo 0)" \
  "cargo test --doc ran for a bin-only selection: $(cat "${shim_dir}/cargo-bin-only.log")"
check rust-test-no-lib-selection-nextest-ran "$(grep -q "^cargo nextest run .*-p ${BIN_ONLY}" "${shim_dir}/cargo-bin-only.log" && echo 0 || echo 1)" \
  "the nextest lane must still run: $(cat "${shim_dir}/cargo-bin-only.log")"

printf '\nbehavior-equivalence.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '%s\n' "$f"
  done
  exit 1
fi
exit 0
