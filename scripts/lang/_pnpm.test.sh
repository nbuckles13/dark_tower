#!/usr/bin/env bash
# _pnpm.test.sh — hermetic self-test for scripts/lang/_pnpm.sh (pnpm_deps_fresh, pnpm_exec_unverified),
# the helpers the TS wrappers (compile/lint/test/fmt) and proto/_buf.sh call under
# pnpm-workspace.yaml `verifyDepsBeforeRun: error`.
#
# HERMETIC: `pnpm` is a PATH stub that records its argv and drops a marker when invoked (positive control
# that the helper reached it); no real pnpm, no network. Wired into scripts/layer3.sh (no *.test.sh
# auto-runner — an unrun test is untested).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/_common.sh"          # DEVLOOP_TMP, init_devloop_tmp
source "${__here}/_test_helpers.sh"    # PASS/FAIL, assert_*/report_results

init_devloop_tmp
__work="$(mktemp -d "${DEVLOOP_TMP}/pnpm-helpers-selftest.XXXXXX")"
mkdir -p "${__work}/bin"
trap 'rm -rf "$__work"' EXIT

cat > "${__work}/bin/pnpm" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${STUB_DIR}/argv"
: > "${STUB_DIR}/ran.pnpm"
[[ "${STUB_RC:-0}" -eq 0 ]] || { echo "ERR_PNPM_VERIFY_DEPS_BEFORE_RUN (stub): lockfile changed" >&2; exit "$STUB_RC"; }
exit 0
STUB
chmod +x "${__work}/bin/pnpm"

# run <case-dir> <shell snippet> [env...] -> stdout+stderr of the snippet with _common.sh + _pnpm.sh sourced.
run() {
  local dir="${__work}/$1" snippet="$2"; shift 2
  mkdir -p "$dir"
  env STUB_DIR="$dir" PATH="${__work}/bin:$PATH" "$@" \
    bash -c "source '${__here}/_common.sh'; source '${__here}/_pnpm.sh'; ${snippet}" 2>&1
}

# --- fresh: rc 0, no STATUS line, and the stub ran with verify ON (no bypass flag) ---
rc=0; out="$(run fresh 'pnpm_deps_fresh')" || rc=$?
assert_rc     "fresh -> rc 0"                    0 "$rc"
assert_absent "fresh emits no STATUS"            "STATUS=" "$out"
assert_marker "fresh reached pnpm (pos control)" "${__work}/fresh" "ran.pnpm"
assert_status "fresh runs a no-op through exec"  "exec true" "$(cat "${__work}/fresh/argv")"
assert_absent "fresh does NOT bypass verify"     "verify-deps-before-run" "$(cat "${__work}/fresh/argv")"

# --- stale: rc 1, distinct token, remedy, and pnpm's own message shown (displayed, not matched on) ---
rc=0; out="$(run stale 'pnpm_deps_fresh' STUB_RC=1)" || rc=$?
assert_rc     "stale -> rc 1"                    1 "$rc"
assert_status "stale -> pnpm-deps-stale"         "STATUS=FAIL REASON=pnpm-deps-stale" "$out"
assert_status "stale names the remedy"           "pnpm install --frozen-lockfile" "$out"
assert_status "stale surfaces pnpm's stderr"     "ERR_PNPM_VERIFY_DEPS_BEFORE_RUN (stub)" "$out"
assert_marker "stale reached pnpm (pos control)" "${__work}/stale" "ran.pnpm"

# --- any non-zero rc is stale: the decision is the exit status alone ---
rc=0; out="$(run stale2 'pnpm_deps_fresh' STUB_RC=2)" || rc=$?
assert_status "rc 2 -> pnpm-deps-stale"          "STATUS=FAIL REASON=pnpm-deps-stale" "$out"

# --- no pnpm on PATH: named as unavailable, never as stale (whose remedy cannot work) ---
# This box ships a real pnpm, so build an isolated bin with the real coreutils but no pnpm.
__nopnpm="${__work}/nopnpm"; mkdir -p "$__nopnpm"
for __d in /usr/bin /bin; do for __f in "$__d"/*; do ln -sf "$__f" "$__nopnpm/" 2>/dev/null || true; done; done
rm -f "${__nopnpm}/pnpm"
mkdir -p "${__work}/absent"
rc=0; out="$(env STUB_DIR="${__work}/absent" PATH="$__nopnpm" \
  bash -c "source '${__here}/_common.sh'; source '${__here}/_pnpm.sh'; pnpm_deps_fresh" 2>&1)" || rc=$?
assert_rc     "no pnpm -> rc 1"                   1 "$rc"
assert_status "no pnpm -> pnpm-unavailable"       "STATUS=FAIL REASON=pnpm-unavailable" "$out"
assert_absent "no pnpm is not reported as stale"  "pnpm-deps-stale" "$out"

# --- pnpm_exec_unverified: the ONE bypass, flag before `exec`, args passed through ---
rc=0; out="$(run unverified 'pnpm_exec_unverified buf --version' STUB_RC=0)" || rc=$?
assert_rc     "unverified -> rc 0"               0 "$rc"
assert_status "unverified passes the bypass"     "--config.verify-deps-before-run=false exec buf --version" "$(cat "${__work}/unverified/argv")"

report_results "scripts/lang/_pnpm.test.sh"
