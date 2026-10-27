#!/usr/bin/env bash
# coverage-exempt: exercises tool-bins.sh path/build/exec logic with a stubbed cargo; drives no real dt-guard
# tool-bins.test.sh — self-test for scripts/lib/tool-bins.sh, the one home for the
# dev-tool (dt-guard, dt-story) cargo profile, binary paths, and build helper.
#
# Hermetic: a mktemp fake repo root holding a COPY of tool-bins.sh (its root is
# derived from its own location), and a PATH-stubbed `cargo` that records its argv
# and fakes the build output. Nothing is really built and the real tree's target/
# is never touched.
#
# What is pinned, and why each matters:
#   - source-safety: sourcing changes no shell option / IFS / CWD and never exits the
#     caller (the pre-commit hook sources it under bash-3.2-tolerant `set -e`);
#   - every assignment is unconditional: an ambient DT_TOOLS_PROFILE / *_BIN /
#     DT_TOOLS_CRATES / DT_TOOLS_ROOT is overwritten, never honoured — the property
#     run-story.sh's sentinel-gated DT_STORY seam depends on;
#   - the crate allow-list refuses anything else, before cargo runs;
#   - the build always pins --target-dir (an ambient CARGO_TARGET_DIR must not move
#     the producer away from where consumers read);
#   - the stale release-profile binary is removed only after a SUCCESSFUL build, only
#     when the profile is not `release`, and a failed removal fails the build;
#   - exec mode fails loudly with <crate>-binary-missing (never falls through to a
#     stale release-profile copy, never honours an ambient DT_STORY);
#   - the CI coverage selector does not pick up a suite that names only the
#     *_BIN default (this file is the live example).
#
# Literal tool-binary paths are deliberately never written here: `dt-guard
# tool-bin-paths` scans *.test.sh too. Paths are built from the sourced variables.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_REPO_ROOT="$(cd "${__here}/../.." && pwd -P)"
# shellcheck source=../lang/_test_helpers.sh
source "${REAL_REPO_ROOT}/scripts/lang/_test_helpers.sh"
set +e

WORK="$(mktemp -d "${TMPDIR:-/tmp}/tool-bins-test.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# Where the tools were built before the devtools profile (the stale-copy location).
OLD="target/release"

STUB="${WORK}/stub"
mkdir -p "$STUB"
cat >"${STUB}/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CARGO_LOG"
if [ "${STUB_CARGO_RC:-0}" -ne 0 ]; then exit "$STUB_CARGO_RC"; fi
td="" p="" prof=""
while [ $# -gt 0 ]; do
  case "$1" in
    --target-dir) td="$2"; shift ;;
    -p) p="$2"; shift ;;
    --profile) prof="$2"; shift ;;
  esac
  shift
done
mkdir -p "${td}/${prof}"
printf '#!/bin/sh\necho "ran %s $*"\n' "$p" >"${td}/${prof}/${p}"
chmod +x "${td}/${prof}/${p}"
EOF
chmod +x "${STUB}/cargo"

# new_root [sed-expr] — a fresh fake repo root with (optionally edited) tool-bins.sh.
new_root() {
  local r
  r="$(mktemp -d "${WORK}/root.XXXXXX")"
  r="$(cd "$r" && pwd -P)"
  mkdir -p "${r}/scripts/lib"
  if [ -n "${1:-}" ]; then
    sed "$1" "${REAL_REPO_ROOT}/scripts/lib/tool-bins.sh" >"${r}/scripts/lib/tool-bins.sh"
  else
    cp "${REAL_REPO_ROOT}/scripts/lib/tool-bins.sh" "${r}/scripts/lib/tool-bins.sh"
  fi
  chmod +x "${r}/scripts/lib/tool-bins.sh"
  printf '%s' "$r"
}

# in_root <root> <bash-snippet> — run the snippet in a clean bash with the stub cargo.
in_root() {
  local r="$1"; shift
  env -i PATH="${STUB}:/usr/bin:/bin" HOME="$WORK" CARGO_LOG="${r}/cargo.log" \
    STUB_CARGO_RC="${STUB_CARGO_RC:-0}" ROOT="$r" bash -c "$1"
}

# --- (1) source-safety -------------------------------------------------------------
R="$(new_root)"
out="$(in_root "$R" '
  set +e +u; before="$(set -o; printf "IFS=%q" "$IFS"; pwd)"
  source "$ROOT/scripts/lib/tool-bins.sh"; rc=$?
  after="$(set -o; printf "IFS=%q" "$IFS"; pwd)"
  [ "$before" = "$after" ] && echo SAME-LOOSE rc=$rc
  set -euo pipefail; IFS=$'"'"'\n\t'"'"'
  before="$(set -o; printf "IFS=%q" "$IFS"; pwd)"
  source "$ROOT/scripts/lib/tool-bins.sh"
  source "$ROOT/scripts/lib/tool-bins.sh"
  after="$(set -o; printf "IFS=%q" "$IFS"; pwd)"
  [ "$before" = "$after" ] && echo SAME-STRICT-TWICE
  echo STILL-RUNNING')"
assert_status "source-loose-no-side-effects" "SAME-LOOSE rc=0" "$out"
assert_status "source-strict-twice-no-side-effects" "SAME-STRICT-TWICE" "$out"
assert_status "source-does-not-exit-caller" "STILL-RUNNING" "$out"

# --- (2) derived paths -------------------------------------------------------------
out="$(in_root "$R" '
  source "$ROOT/scripts/lib/tool-bins.sh"
  [ "$DT_TOOLS_ROOT" = "$ROOT" ] && echo ROOT-OK
  [ -n "$DT_TOOLS_PROFILE" ] && [ "$DT_TOOLS_PROFILE" != release ] && echo PROFILE-CUSTOM
  [ "$DT_TOOLS_DIR" = "$ROOT/target/$DT_TOOLS_PROFILE" ] && echo DIR-DERIVED
  [ "$DT_GUARD_BIN" = "$DT_TOOLS_DIR/dt-guard" ] && [ "$DT_STORY_BIN" = "$DT_TOOLS_DIR/dt-story" ] && echo BINS-DERIVED
  [ "$DT_TOOLS_CRATES" = "dt-guard dt-story" ] && echo CRATES-OK
  cd /; source "$ROOT/scripts/lib/tool-bins.sh"; [ "$DT_TOOLS_ROOT" = "$ROOT" ] && echo CWD-INDEPENDENT')"
for tok in ROOT-OK PROFILE-CUSTOM DIR-DERIVED BINS-DERIVED CRATES-OK CWD-INDEPENDENT; do
  assert_status "paths-${tok}" "$tok" "$out"
done

# --- (3) ambient values are overwritten, never honoured ---------------------------
out="$(in_root "$R" '
  DT_TOOLS_PROFILE=release DT_STORY_BIN=/bogus/s DT_GUARD_BIN=/bogus/g DT_TOOLS_CRATES=evil DT_TOOLS_ROOT=/bogus DT_TOOLS_DIR=/bogus/d DT_TOOLS_BUILD_CMD=evil
  export DT_TOOLS_PROFILE DT_STORY_BIN DT_GUARD_BIN DT_TOOLS_CRATES DT_TOOLS_ROOT DT_TOOLS_DIR DT_TOOLS_BUILD_CMD
  source "$ROOT/scripts/lib/tool-bins.sh"
  case "$DT_TOOLS_PROFILE$DT_STORY_BIN$DT_GUARD_BIN$DT_TOOLS_CRATES$DT_TOOLS_ROOT$DT_TOOLS_DIR$DT_TOOLS_BUILD_CMD" in
    *bogus*|*evil*|release*) echo HONOURED ;;
    *) echo OVERWRITTEN ;;
  esac')"
assert_status "ambient-overwritten" "OVERWRITTEN" "$out"
# Static backstop for the same property: no `:-`/`:=` default and no export in the file.
if grep -nE '^[^#]*(\$\{[A-Z_]+:[-=]|\bexport\b|\breadonly\b|declare -g)' "${REAL_REPO_ROOT}/scripts/lib/tool-bins.sh" \
   | grep -vE '\$\{(1|BASH_SOURCE)' >/dev/null; then
  FAIL=$((FAIL + 1)); FAILURES+=("[static-no-env-defaults] tool-bins.sh contains an env-defaulted/exported/readonly assignment")
else
  PASS=$((PASS + 1))
fi

# --- (4) crate allow-list ------------------------------------------------------------
R="$(new_root)"
out="$(in_root "$R" 'source "$ROOT/scripts/lib/tool-bins.sh"; dt_build_tool not-a-tool 2>&1; echo "rc=$?"')"
assert_status "unknown-crate-rc2" "rc=2" "$out"
assert_status "unknown-crate-message" "is not a dev-tool crate" "$out"
assert_absent_file() { if [ -e "$2" ]; then FAIL=$((FAIL + 1)); FAILURES+=("[$1] $2 exists but should not"); else PASS=$((PASS + 1)); fi; }
assert_present_file() { if [ -e "$2" ]; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); FAILURES+=("[$1] $2 missing"); fi; }
assert_absent_file "unknown-crate-cargo-not-run" "${R}/cargo.log"

# --- (5) successful build: pinned profile + target-dir, stale removed, CWD kept -------
R="$(new_root)"
mkdir -p "${R}/${OLD}" "${R}/elsewhere"
printf 'old\n' >"${R}/${OLD}/dt-guard"
printf 'keep\n' >"${R}/target/release/some-service"
out="$(in_root "$R" '
  export CARGO_TARGET_DIR=/bogus/ambient-target
  source "$ROOT/scripts/lib/tool-bins.sh"; cd "$ROOT/elsewhere"
  dt_build_tool dt-guard --locked --quiet 2>&1; echo "rc=$?"; echo "cwd=$(pwd)"')"
assert_status "build-ok-rc0" "rc=0" "$out"
assert_status "build-keeps-caller-cwd" "cwd=${R}/elsewhere" "$out"
log="$(cat "${R}/cargo.log" 2>/dev/null)"
assert_status "build-pins-profile" "--profile devtools" "$log"
assert_status "build-pins-target-dir" "--target-dir ${R}/target" "$log"
assert_status "build-forwards-args" "-p dt-guard --locked --quiet" "$log"
assert_absent_file "build-ok-removes-stale" "${R}/${OLD}/dt-guard"
assert_present_file "build-ok-leaves-other-release-files" "${R}/target/release/some-service"
assert_status "build-ok-announces-removal" "removed stale ${R}/${OLD}/dt-guard" "$out"

# --- (6) failed build: rc propagated, stale NOT removed --------------------------------
R="$(new_root)"
mkdir -p "${R}/${OLD}"; printf 'old\n' >"${R}/${OLD}/dt-story"
out="$(STUB_CARGO_RC=101 in_root "$R" 'source "$ROOT/scripts/lib/tool-bins.sh"; dt_build_tool dt-story 2>&1; echo "rc=$?"')"
assert_status "build-fail-rc-propagated" "rc=101" "$out"
assert_present_file "build-fail-keeps-stale" "${R}/${OLD}/dt-story"

# --- (7) profile=release: nothing removed --------------------------------------------
R="$(new_root 's/^DT_TOOLS_PROFILE=.*/DT_TOOLS_PROFILE=release/')"
mkdir -p "${R}/${OLD}"
out="$(in_root "$R" 'source "$ROOT/scripts/lib/tool-bins.sh"; dt_build_tool dt-guard 2>&1; echo "rc=$?"')"
assert_status "release-profile-build-rc0" "rc=0" "$out"
assert_present_file "release-profile-keeps-fresh-binary" "${R}/${OLD}/dt-guard"

# --- (8) removal failure fails the build (a directory: rm -f fails even as root) ------
R="$(new_root)"
mkdir -p "${R}/${OLD}/dt-guard/inner"
out="$(in_root "$R" 'source "$ROOT/scripts/lib/tool-bins.sh"; dt_build_tool dt-guard 2>&1; echo "rc=$?"')"
assert_status "rm-failure-fails-build" "rc=1" "$out"
assert_status "rm-failure-message" "could not remove stale" "$out"

# --- (9) dt_build_tools under a newline/tab IFS builds every crate ---------------------
R="$(new_root)"
out="$(in_root "$R" 'set -euo pipefail; IFS=$'"'"'\n\t'"'"'; source "$ROOT/scripts/lib/tool-bins.sh"; dt_build_tools -q; echo "rc=$?"')"
assert_status "build-all-rc0" "rc=0" "$out"
log="$(cat "${R}/cargo.log" 2>/dev/null)"
assert_status "build-all-dt-guard" "-p dt-guard -q" "$log"
assert_status "build-all-dt-story" "-p dt-story -q" "$log"

# --- (10) executable mode -----------------------------------------------------------
R="$(new_root)"
mkdir -p "${R}/${OLD}"
printf '#!/bin/sh\necho STALE-RAN\n' >"${R}/${OLD}/dt-story"; chmod +x "${R}/${OLD}/dt-story"
mkdir -p "${R}/bogus"; printf '#!/bin/sh\necho AMBIENT-RAN\n' >"${R}/bogus/dt-story"; chmod +x "${R}/bogus/dt-story"
out="$(in_root "$R" 'DT_STORY="$ROOT/bogus/dt-story" "$ROOT/scripts/lib/tool-bins.sh" dt-story list-tasks x 2>&1; echo "rc=$?"')"
assert_status "exec-missing-rc1" "rc=1" "$out"
assert_status "exec-missing-token" "STATUS=FAIL REASON=dt-story-binary-missing" "$out"
assert_status "exec-missing-names-build" "run: scripts/lib/tool-bins.sh build" "$out"
if [[ "$out" == *STALE-RAN* || "$out" == *AMBIENT-RAN* ]]; then
  FAIL=$((FAIL + 1)); FAILURES+=("[exec-missing-no-fallthrough] a stale or ambient binary ran: ${out}")
else
  PASS=$((PASS + 1))
fi
out="$(in_root "$R" '"$ROOT/scripts/lib/tool-bins.sh" build -q 2>&1; echo "rc=$?"')"
assert_status "exec-build-rc0" "rc=0" "$out"
out="$(in_root "$R" 'DT_STORY="$ROOT/bogus/dt-story" "$ROOT/scripts/lib/tool-bins.sh" dt-story list-tasks x 2>&1; echo "rc=$?"')"
assert_status "exec-runs-built-binary" "ran dt-story list-tasks x" "$out"
assert_status "exec-rc0" "rc=0" "$out"
out="$(in_root "$R" '"$ROOT/scripts/lib/tool-bins.sh" frobnicate 2>&1; echo "rc=$?"')"
assert_status "exec-unknown-verb-rc2" "rc=2" "$out"
assert_status "exec-unknown-verb-usage" "usage:" "$out"

# --- (11) CI coverage selection ignores a suite naming only the *_BIN default -------------
# Fixture precondition: the negative below proves the whole-word selector only
# while THIS file names the *_BIN default on a non-comment line and never the bare
# override name. Without that line the negative would pass for the wrong reason.
__bin_line="$(awk '!/^[[:space:]]*#/ && /DT_GUARD_BIN/ { print; exit }' "${BASH_SOURCE[0]}")"
# The override name is assembled so this check does not itself contain it.
__ovr="DT_""GUARD"
__bare_line="$(awk -v n="$__ovr" '!/^[[:space:]]*#/ && $0 ~ ("(^|[^A-Za-z0-9_])" n "([^A-Za-z0-9_]|$)") { print; exit }' "${BASH_SOURCE[0]}")"
if [ -n "$__bin_line" ] && [ -z "$__bare_line" ]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1)); FAILURES+=("[coverage-selector-fixture-gone] this file must name DT_GUARD_BIN on a non-comment line and never the bare override (bin='${__bin_line}' bare='${__bare_line}')")
fi
sel="$("${REAL_REPO_ROOT}/scripts/guards/list-coverage-suites.sh" 2>&1)"; sel_rc=$?
assert_rc "coverage-selector-rc0" 0 "$sel_rc"
if [[ "$sel" == *"scripts/lib/tool-bins.test.sh"* ]]; then
  FAIL=$((FAIL + 1)); FAILURES+=("[coverage-selector-excludes-bin-only] tool-bins.test.sh was selected: ${sel}")
else
  PASS=$((PASS + 1))
fi
assert_status "coverage-selector-keeps-counter-zero-init" "scripts/guards/counter-zero-init.test.sh" "$sel"
assert_status "coverage-selector-keeps-media-telemetry-deny" "scripts/guards/media-telemetry-deny.test.sh" "$sel"

report_results "$0"
