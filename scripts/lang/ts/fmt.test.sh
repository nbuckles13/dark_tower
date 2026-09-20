#!/usr/bin/env bash
# fmt.test.sh — REAL-PATH hermetic self-test for scripts/lang/ts/fmt.sh (ADR-0037 §D7).
#
# NOT STUBBED — a DELIBERATE divergence from rust/fmt.test.sh's `cargo` PATH-stub. This lane's
# correctness rests on nx actually forwarding `-- <flag>` through `run-many` to prettier: bare
# `prettier "<glob>"` with NO mode flag prints to stdout and exits 0 (a vacuous pass), so the ONLY
# thing that proves the flag reaches prettier is running the REAL nx → REAL prettier chain against a
# REAL misformatted fixture. A stub never exercises arg-forwarding, so a stubbed test would silently
# reopen the exact vacuity hole this lane exists to fix.
#
# HERMETIC nonetheless: a synthetic nx workspace under ${DEVLOOP_TMP} (700-perm) — NEVER the repo tree,
# so APPLY-mode `--write` touches fixtures only (a self-test must be incapable of dirtying the tree a
# gate attests). node_modules is a SYMLINK to the repo's (OFFLINE — nx + prettier + prettier-plugin-
# svelte are already installed; no registry fetch). `.prettierrc.json` is COPIED from the repo at
# runtime (SSoT — "misformatted" is defined against the REAL config, with no frozen second copy to
# drift); fixtures are misformatted in GROSS, config-independent ways so they survive a config change.
# NX_DAEMON=false + a per-run NX_CACHE_DIRECTORY: no stale project-graph or cached task result can be
# served (a cached CHECK could go green over a misformat — review-protocol §Assertion-Vacuity #5).
#
# FOREIGN-TOOL RESIDUAL (review-protocol §Assertion-Vacuity #4) — verified on-box, prettier 3.9.6
# (exact-pinned in the root package.json, with prettier-plugin-svelte 4.1.1): `--check` lists offenders
# as `[warn] <repo-relative path>` and exits non-zero; `--write --list-different` prints ONLY the
# changed files (nothing on a clean tree); a no-match glob exits 2 (loud). A prettier/plugin upgrade
# that changes formatting is traceable via this note + the exact pin, not silently re-vacuuming the test.
#
# COST & PLACEMENT (@operations, §6.3): ~10.1–10.3s — the second-largest Layer-3 item after
# validate-frame-vectors.test.sh (23–27s). The cost GROWS with monorepo package count: NX_DAEMON=false
# recomputes the project graph per invocation and the graph scales with the workspace, so whoever adds a
# package next inherits Layer-3 cost. If it grows past ~20s, move it to LAYER 4 (the TS test layer,
# outside the fast tier — cf. release-feature-gate.test.sh living in Layer 1 for its cost profile). To
# shrink it, CONSOLIDATE nx INVOCATIONS while keeping per-case isolation ("group assertions sharing a
# mutation", the §6.3-sanctioned lever) — NEVER stub, which reopens the forwarding vacuity hole above.
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"          # DEVLOOP_TMP, init_devloop_tmp
source "${__here}/../_test_helpers.sh"    # PASS/FAIL, assert_status/_absent, report_results

init_devloop_tmp
__work="$(mktemp -d "${DEVLOOP_TMP}/ts-fmt-selftest.XXXXXX")"
trap 'rm -rf "$__work"' EXIT
__repo="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# Build a synthetic nx workspace. $1 = with-target | no-target.
mk_ws() {
  local mode="$1" ws
  ws="$(mktemp -d "${__work}/ws.XXXXXX")"
  ln -s "${__repo}/node_modules" "${ws}/node_modules"   # offline: reuse installed nx/prettier/plugin
  cp "${__repo}/.prettierrc.json" "${ws}/.prettierrc.json"   # copy-at-runtime SSoT (real config)
  printf '{ "name": "fixws", "version": "0.0.0", "private": true }\n' > "${ws}/package.json"
  printf '{ "$schema": "./node_modules/nx/schemas/nx-schema.json" }\n' > "${ws}/nx.json"
  mkdir -p "${ws}/packages/fixpkg/src"
  if [[ "$mode" == with-target ]]; then
    # Mirrors the real per-package shape: repo-root cwd (omitted), repo-relative glob, cache:false.
    cat > "${ws}/packages/fixpkg/project.json" <<'PJ'
{ "name": "fixpkg", "root": "packages/fixpkg", "projectType": "library",
  "targets": { "format": { "executor": "nx:run-commands", "cache": false,
    "options": { "command": "prettier \"packages/fixpkg/src/**/*.{ts,svelte}\"" } } } }
PJ
  elif [[ "$mode" == prettier-less ]]; then
    # A `format` target that exists (so the zero-target guard passes) but does NOT invoke prettier —
    # models nx output whose shape the extraction can't recognize (obs F2). `true` ignores the forwarded
    # flags and exits 0, so APPLY sees rc=0, an empty changed-file list, and no `prettier --write` echo.
    cat > "${ws}/packages/fixpkg/project.json" <<'PJ'
{ "name": "fixpkg", "root": "packages/fixpkg", "projectType": "library",
  "targets": { "format": { "executor": "nx:run-commands", "cache": false,
    "options": { "command": "true" } } } }
PJ
  else
    # A project with NO format target — the zero-target vacuity case.
    cat > "${ws}/packages/fixpkg/project.json" <<'PJ'
{ "name": "fixpkg", "root": "packages/fixpkg", "projectType": "library",
  "targets": { "build": { "executor": "nx:run-commands", "options": { "command": "true" } } } }
PJ
  fi
  printf '%s\n' "$ws"
}

# Run the REAL wrapper with cwd=workspace, ambient fmt knobs cleared, daemon+cache isolated.
# $1 = workspace; $2.. = extra env assignments (e.g. DEVLOOP_FMT_APPLY=1).
run_fmt() {
  local ws="$1"; shift
  ( cd "$ws" && env -u DEVLOOP_FMT_APPLY -u DEVLOOP_FMT_CHECK_ONLY -u GITHUB_ACTIONS -u CI \
      NX_DAEMON=false NX_CACHE_DIRECTORY="${ws}/.nxcache" "$@" \
      bash "${__here}/fmt.sh" 2>&1 )
}

BAD_TS='const   x=1
export  const  y ="a"'
GOOD_TS='const x = 1;
export const y = '\''a'\'';'   # prettier 3.9.6 + repo .prettierrc (singleQuote:true, semi:true)
BAD_SVELTE='<script lang="ts">
const   a=1
</script>

<p>{a}</p>'

# ENV-SANITY / fixture-collection positive control: prove fixtures exist before asserting on them, so a
# "collected zero fixtures, passed" run is structurally impossible (§Vacuity #5 corollary).
ws="$(mk_ws with-target)"
printf '%s\n' "$BAD_TS" > "${ws}/packages/fixpkg/src/bad.ts"
if [[ -s "${ws}/packages/fixpkg/src/bad.ts" ]]; then PASS=$((PASS + 1));
else FAIL=$((FAIL + 1)); FAILURES+=("[env-sanity] fixture bad.ts was not created"); fi

# Case 1 — CHECK (default) over a misformatted .ts → FAIL, and the fixture is NAMED. The name proves
# prettier EVALUATED and rejected it (discrimination vs an nx-boot/wrong-cwd/missing-target failure,
# which also exit non-zero). Also the glob-match half of the shared-fixture pairing with case 3.
out="$(run_fmt "$ws")" || true
assert_status "check .ts misformat is nx-format-failed"     "STATUS=FAIL REASON=nx-format-failed" "$out"
assert_status "check .ts names the offender (discrimination)" "packages/fixpkg/src/bad.ts"        "$out"
assert_status "check emits FMT_MODE=check + SOURCE=check-default (F5)" "FMT_MODE=check SOURCE=check-default" "$out"

# Case 2 — CHECK over a misformatted .svelte → FAIL naming it (guards the .svelte glob half; a glob that
# dropped .svelte would pass here green).
printf '%s\n' "$BAD_SVELTE" > "${ws}/packages/fixpkg/src/Bad.svelte"
out="$(run_fmt "$ws")" || true
assert_status "check .svelte misformat is FAIL"        "STATUS=FAIL REASON=nx-format-failed" "$out"
assert_status "check names the .svelte offender"       "packages/fixpkg/src/Bad.svelte"     "$out"

# Case 3 — CHECK clean (SAME fixtures, now formatted) → OK. Non-vacuous BECAUSE cases 1/2 proved the
# glob matches these files; formatting them and getting OK is the pairing's positive control (a green
# here over an EMPTY set is impossible — the same files were just shown to be in scope).
printf '%s\n' "$GOOD_TS" > "${ws}/packages/fixpkg/src/bad.ts"
printf '%s\n' '<script lang="ts">
  const a = 1;
</script>

<p>{a}</p>' > "${ws}/packages/fixpkg/src/Bad.svelte"
out="$(run_fmt "$ws")" || true
assert_status "check clean is nx-format-passed" "STATUS=OK REASON=nx-format-passed" "$out"

# Case 4 — APPLY over a misformatted .ts → applied token, NON-EMPTY repo-relative FMT_APPLIED, and the
# file is actually rewritten. FMT_APPLIED non-emptiness is asserted separately from the token so the
# "applied + empty list" contradiction state cannot pass.
printf '%s\n' "$BAD_TS" > "${ws}/packages/fixpkg/src/bad.ts"
rm -f "${ws}/packages/fixpkg/src/Bad.svelte"
out="$(run_fmt "$ws" DEVLOOP_FMT_APPLY=1)" || true
assert_status "apply misformat is nx-format-applied"   "STATUS=OK REASON=nx-format-applied"       "$out"
assert_status "apply FMT_APPLIED names the file"       "FMT_APPLIED=packages/fixpkg/src/bad.ts"   "$out"
assert_absent "FMT_APPLIED has no leading /"           "FMT_APPLIED=/"                            "$out"
assert_absent "FMT_APPLIED has no leading . (./ ../)"  "FMT_APPLIED=."                            "$out"
assert_status "apply emits FMT_MODE=apply + SOURCE=apply-opt-in (F5)" "FMT_MODE=apply SOURCE=apply-opt-in" "$out"
if [[ "$(cat "${ws}/packages/fixpkg/src/bad.ts")" == "$GOOD_TS" ]]; then PASS=$((PASS + 1));
else FAIL=$((FAIL + 1)); FAILURES+=("[apply] bad.ts was not rewritten to formatted bytes"); fi

# Case 5 — APPLY over an ALREADY-CLEAN tree → passed, and NO FMT_APPLIED line at all (distinct from
# applied; "applied + empty" must never read like "clean").
out="$(run_fmt "$ws" DEVLOOP_FMT_APPLY=1)" || true
assert_status "apply clean is nx-format-passed"  "STATUS=OK REASON=nx-format-passed" "$out"
assert_absent "apply clean emits no FMT_APPLIED" "FMT_APPLIED="                      "$out"

# Case 6 — ZERO-TARGET vacuity guard: a workspace whose project has NO format target must FAIL LOUD with
# the DISTINCT token, never green over the empty set (the ORIGINAL defect: nx run-many exits 0 over zero
# matching projects). This is the guard's own positive control.
ws0="$(mk_ws no-target)"
out="$(run_fmt "$ws0")" || true
assert_status "zero targets is no-ts-format-targets"     "STATUS=FAIL REASON=no-ts-format-targets" "$out"
assert_absent "zero-target guard did not dispatch format" "nx-format-failed"                        "$out"

# Case 7 — INVALID knob → loud FAIL, and prettier/nx is NEVER dispatched (positive control on the reject
# path: the wrapper exits before the zero-target guard and before any nx call).
out="$(run_fmt "$ws" DEVLOOP_FMT_APPLY=ture)" || true
assert_status "invalid knob is fmt-mode-invalid-knob" "STATUS=FAIL REASON=fmt-mode-invalid-knob" "$out"
assert_absent "invalid knob never ran prettier"       "Checking formatting"                      "$out"

# Case 8 — APPLY over a target that runs (rc 0) but does NOT invoke prettier: an EMPTY changed-file list
# is then ambiguous (obs F2), and the environment-sanity check must resolve it to LOUD FAIL, NOT a silent
# "clean" — otherwise an nx-output-shape change would report `nx-format-passed` on a run that rewrote
# files, returning the lane to its pre-fix vacuous condition and breaking the FMT_APPLIED contract the
# wireConstants discriminator depends on.
wsx="$(mk_ws prettier-less)"
out="$(run_fmt "$wsx" DEVLOOP_FMT_APPLY=1)" || true
assert_status "apply w/ unrecognized output is FAIL"     "STATUS=FAIL REASON=nx-format-output-unrecognized" "$out"
assert_absent "unrecognized output does NOT report passed" "REASON=nx-format-passed"                        "$out"

# Case 9 — UNKNOWN verdict is FAIL-CLOSED (obs F1): if fmt_mode ever returns a token this wrapper does not
# recognize, it must FAIL LOUD, never fall through to APPLY (a tree write). fmt_mode never returns unknown
# today, so this is the ONE arm the real-path cases above cannot reach; it is driven with a MINIMAL stub
# `_common.sh` whose `fmt_mode` echoes a bogus verdict (the copied wrapper sources it by relative path).
# The stub provides only the four symbols the pre-dispatch path uses — no nx — so the unknown arm exits
# before any format run.
__stub="$(mktemp -d "${__work}/stub.XXXXXX")"
mkdir -p "${__stub}/lang/ts"
cp "${__here}/fmt.sh" "${__stub}/lang/ts/fmt.sh"
cat > "${__stub}/lang/_common.sh" <<'STUB'
set -euo pipefail; IFS=$'\n\t'
install_wrapper_exit_trap() { :; }
emit_status() { printf 'STATUS=%s REASON=%s\n' "$1" "$2"; }
fmt_mode() { printf '%s\n' "${STUB_VERDICT:-CHECK check-default}"; }
fmt_mode_emit() { printf 'FMT_MODE=%s SOURCE=stub\n' "${1%% *}" >&2; }
run_and_emit() { local p="$1"; shift; if "$@"; then emit_status OK "${p}-passed"; else emit_status FAIL "${p}-failed"; fi; }
STUB
out="$(STUB_VERDICT='WEIRD future-token' bash "${__stub}/lang/ts/fmt.sh" 2>&1)" || true
assert_status "unknown verdict is fail-closed"            "STATUS=FAIL REASON=fmt-mode-unknown-verdict" "$out"
assert_absent "unknown verdict never dispatched nx"       "nx run-many"                                 "$out"

report_results "scripts/lang/ts/fmt.test.sh"
