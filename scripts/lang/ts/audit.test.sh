#!/usr/bin/env bash
# audit.test.sh — hermetic self-test for scripts/lang/ts/audit.sh's decision on `pnpm audit --json`.
#
# HERMETIC: `pnpm` is a PATH stub that prints a canned --json body (<case dir>/audit.json) and drops a marker; the
# suppression list comes from a synthetic ignore file via the DEVLOOP_TEST-gated DEVLOOP_AUDIT_IGNORE_JSON
# override; DEVLOOP_AUDIT_FORCE_RUN=1 bypasses the dep-change gate. No network, no real pnpm. Wired into
# scripts/layer3.sh (no *.test.sh auto-runner).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"          # DEVLOOP_TMP, init_devloop_tmp
source "${__here}/../_test_helpers.sh"    # PASS/FAIL, assert_*/report_results

init_devloop_tmp
__work="$(mktemp -d "${DEVLOOP_TMP}/ts-audit-selftest.XXXXXX")"
mkdir -p "${__work}/bin"
trap 'rm -rf "$__work"' EXIT

cat > "${__work}/bin/pnpm" <<'STUB'
#!/usr/bin/env bash
: > "${STUB_DIR}/ran.pnpm"
[[ "$1" == "audit" ]] || exit 0
# The body comes from a file: the large-output case exceeds the 128 KiB cap on an env string.
cat "${STUB_DIR}/audit.json"
exit "${STUB_RC:-0}"
STUB
chmod +x "${__work}/bin/pnpm"

printf '{"_generated":"test","ignore":[]}\n' > "${__work}/ignore-none.json"
printf '{"_generated":"test","ignore":["GHSA-aaaa-bbbb-cccc"]}\n' > "${__work}/ignore-one.json"

HIGH='{"advisories":{"1":{"github_advisory_id":"GHSA-aaaa-bbbb-cccc","severity":"high","module_name":"x"}},"metadata":{}}'

# run <case> <ignore-file> <json> [rc] -> stdout+stderr of the real wrapper against the stub.
run() {
  local dir="${__work}/$1"; mkdir -p "$dir"
  printf '%s\n' "$3" > "${dir}/audit.json"
  env STUB_DIR="$dir" STUB_RC="${4:-0}" PATH="${__work}/bin:$PATH" \
      DEVLOOP_TEST=1 DEVLOOP_AUDIT_IGNORE_JSON="$2" DEVLOOP_AUDIT_FORCE_RUN=1 \
      bash "${__here}/audit.sh" 2>&1
}

# --- unrecognised shape: fails CLOSED with its own token, never pnpm-audit-passed ---
out="$(run shape "${__work}/ignore-none.json" '{"results":[]}')" || true
assert_status "unrecognised shape -> own token"     "STATUS=FAIL REASON=pnpm-audit-unrecognised-output" "$out"
assert_absent "unrecognised shape is not passed"    "pnpm-audit-passed" "$out"
assert_marker "shape case reached pnpm (pos control)" "${__work}/shape" "ran.pnpm"

out="$(run v2 "${__work}/ignore-none.json" '{"vulnerabilities":{"x":{"severity":"high","via":[{"url":"https://github.com/advisories/GHSA-aaaa-bbbb-cccc","severity":"high"}]}}}' 1)" || true
assert_status "npm-v2 vulnerabilities-only -> unrecognised" "STATUS=FAIL REASON=pnpm-audit-unrecognised-output" "$out"

out="$(run nullcves "${__work}/ignore-none.json" '{"advisories":{"7":{"id":7,"cves":null,"severity":"high"}}}' 1)" || true
assert_status "advisory with null cves, no GHSA -> failed (no crash)" "STATUS=FAIL REASON=pnpm-audit-failed" "$out"

out="$(run array "${__work}/ignore-none.json" '[]')" || true
assert_status "non-object JSON -> own token"        "STATUS=FAIL REASON=pnpm-audit-unrecognised-output" "$out"

# --- empty stdout is never a pass and never an advisory finding, whatever pnpm's exit code ---
out="$(run empty0 "${__work}/ignore-none.json" '' 0)" || true
assert_status "empty output rc 0 -> unrecognised"   "STATUS=FAIL REASON=pnpm-audit-unrecognised-output" "$out"
assert_absent "empty output rc 0 is not passed"     "pnpm-audit-passed" "$out"
out="$(run empty1 "${__work}/ignore-none.json" '' 1)" || true
assert_status "empty output rc 1 -> unrecognised"   "STATUS=FAIL REASON=pnpm-audit-unrecognised-output" "$out"
assert_absent "empty output is not an advisory fail" "pnpm-audit-failed" "$out"

# --- positive control: one high advisory -> pnpm-audit-failed, its GHSA in the remaining list ---
out="$(run high "${__work}/ignore-none.json" "$HIGH" 1)" || true
assert_status "one high -> pnpm-audit-failed"       "STATUS=FAIL REASON=pnpm-audit-failed" "$out"
assert_status "one high names its GHSA"             "GHSA-aaaa-bbbb-cccc" "$out"

# --- the same advisory suppressed in the tracked ignore file -> OK, and the suppression is surfaced ---
out="$(run suppressed "${__work}/ignore-one.json" "$HIGH" 1)" || true
assert_status "suppressed high -> passed"           "STATUS=OK REASON=pnpm-audit-passed" "$out"
assert_status "suppression surfaced"                "SUPPRESSED=GHSA-aaaa-bbbb-cccc" "$out"

# --- empty advisories object is a recognised, clean shape ---
out="$(run empty "${__work}/ignore-none.json" '{"advisories":{},"metadata":{}}')" || true
assert_status "empty advisories -> passed"          "STATUS=OK REASON=pnpm-audit-passed" "$out"

# --- below-threshold only (moderate) -> passed ---
out="$(run moderate "${__work}/ignore-none.json" '{"advisories":{"1":{"github_advisory_id":"GHSA-mmmm-mmmm-mmmm","severity":"moderate"}}}')" || true
assert_status "moderate only -> passed"             "STATUS=OK REASON=pnpm-audit-passed" "$out"

# --- LARGE output (> 128 KiB, the per-string argv/env cap): the report must reach the decision intact ---
# 400 moderate filler advisories with ~600-byte overviews + one high; > 200 KiB of valid JSON.
BIG="$(python3 -c '
import json
adv = {str(i): {"github_advisory_id": "GHSA-fill-%04d-xxxx" % i, "severity": "moderate", "overview": "x" * 600} for i in range(400)}
adv["9999"] = {"github_advisory_id": "GHSA-aaaa-bbbb-cccc", "severity": "high"}
print(json.dumps({"advisories": adv, "metadata": {}}))')"
assert_rc "large fixture is > 200 KiB" 0 "$([[ "${#BIG}" -gt 204800 ]] && echo 0 || echo "1 (${#BIG} bytes)")"
out="$(run large "${__work}/ignore-none.json" "$BIG" 1)" || true
assert_status "large output -> pnpm-audit-failed"   "STATUS=FAIL REASON=pnpm-audit-failed" "$out"
assert_status "large output names the high GHSA"    "GHSA-aaaa-bbbb-cccc" "$out"

# --- no decision: a python3 that prints nothing must NOT read as a pass ---
mkdir -p "${__work}/nopy"
printf '#!/usr/bin/env bash\nexit 0\n' > "${__work}/nopy/python3"; chmod +x "${__work}/nopy/python3"
mkdir -p "${__work}/nodecision"; printf '{"advisories":{}}\n' > "${__work}/nodecision/audit.json"
out="$(env STUB_DIR="${__work}/nodecision" PATH="${__work}/nopy:${__work}/bin:$PATH" \
        DEVLOOP_TEST=1 DEVLOOP_AUDIT_IGNORE_JSON="${__work}/ignore-none.json" DEVLOOP_AUDIT_FORCE_RUN=1 \
        bash "${__here}/audit.sh" 2>&1)" || true
assert_status "silent decision -> pnpm-audit-no-decision" "STATUS=FAIL REASON=pnpm-audit-no-decision" "$out"
assert_absent "silent decision is not passed"             "pnpm-audit-passed" "$out"

report_results "scripts/lang/ts/audit.test.sh"
