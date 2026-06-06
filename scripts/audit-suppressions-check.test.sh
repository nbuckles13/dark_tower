#!/usr/bin/env bash
# audit-suppressions-check.test.sh — self-test for scripts/audit-suppressions-check.sh
# (task #47). Drives the check's FAIL branches with fixtures — a green guard on the
# real manifest proves nothing about the FAIL paths, and an always-run security check
# whose FAIL path is never tested can't be trusted to fire (security requirement).
#
# Wired into scripts/layer3.sh (there is no *.test.sh auto-runner), so it runs every
# devloop + CI. Consumes scripts/lang/_test_helpers.sh (canonical shape:
# scripts/lang/proto/changed.test.sh) + the assert_status/assert_exit helpers.
#
# Determinism: "now" is injected via AUDIT_SUPPRESSIONS_NOW; fixtures are written to a
# temp dir and pointed at via DEVLOOP_SUPPRESSIONS_MANIFEST / DEVLOOP_CARGO_AUDIT_TOML
# / DEVLOOP_AUDIT_IGNORE_JSON. ALL override envs are honored ONLY because this test
# sets DEVLOOP_TEST=1 (exact-match sentinel); the production guard never sets it.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

# The check under test deliberately exits non-zero on FAIL cases; we capture those
# rc/output and assert them, so `set -e` must NOT abort the harness on a captured
# non-zero. report_results provides the final pass/fail exit code.
set +e

CHECK="${__here}/audit-suppressions-check.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Run the check against a fixture set under the test sentinel. Echoes combined output;
# caller inspects exit code via $? and output via the captured string.
# Args: $1=manifest-file  $2=now (or "")  $@... extra: pass "--fix" as $3 to regenerate.
run_check() {
  local manifest="$1" now="$2" mode="${3:-}"
  local cargo="${WORK}/cargo.audit.toml" ign="${WORK}/pnpm.ignore.json"
  env -i PATH="$PATH" HOME="$HOME" \
      DEVLOOP_TEST=1 \
      DEVLOOP_SUPPRESSIONS_MANIFEST="$manifest" \
      DEVLOOP_CARGO_AUDIT_TOML="$cargo" \
      DEVLOOP_AUDIT_IGNORE_JSON="$ign" \
      ${now:+AUDIT_SUPPRESSIONS_NOW="$now"} \
      "$CHECK" $mode 2>&1
}

# Generate matching derived files for a manifest (so sync-check passes unless we
# deliberately drift them).
fix_check() { run_check "$1" "" "--fix" >/dev/null 2>&1 || true; }

# --- Fixtures -----------------------------------------------------------------
mk_manifest() {
  # $1=path $2=id $3=ecosystem $4=expires $5=ticket $6=reason
  cat > "$1" <<EOF
[[suppression]]
id = "$2"
ecosystem = "$3"
expires = "$4"
ticket = "$5"
reason = "$6"
EOF
}

# === date-check ===============================================================
M="${WORK}/future.toml"; mk_manifest "$M" "RUSTSEC-2099-0001" rust "2099-01-01" "t" "r"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "date-future-ok" 0 "$?"
assert_status "date-future-ok-token" "STATUS=OK" "$out"

M="${WORK}/today.toml"; mk_manifest "$M" "RUSTSEC-2026-0001" rust "2026-06-06" "t" "r"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "date-boundary-today-valid" 0 "$?"

M="${WORK}/y1.toml"; mk_manifest "$M" "RUSTSEC-2026-0002" rust "2026-06-05" "t" "r"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "date-yesterday-fail" 1 "$?"
assert_status "date-yesterday-token" "STATUS=FAIL REASON=suppression-past-due" "$out"
assert_status "date-yesterday-id-days" "RUSTSEC-2026-0002 expired 1 day(s) ago" "$out"
assert_status "date-yesterday-remediation" "re-suppress with a new date or fix the advisory" "$out"
assert_status "date-yesterday-doc-pointer" "docs/contributor/audit-suppressions.md" "$out"

# multiple past-due: each id + its OWN days-past (not just the first).
M="${WORK}/multi.toml"
cat > "$M" <<EOF
[[suppression]]
id = "RUSTSEC-AAA-0001"
ecosystem = "rust"
expires = "2026-06-05"
ticket = "t"
reason = "r"
[[suppression]]
id = "RUSTSEC-BBB-0002"
ecosystem = "rust"
expires = "2026-05-07"
ticket = "t"
reason = "r"
EOF
fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "multi-pastdue-fail" 1 "$?"
assert_status "multi-pastdue-id1-days" "RUSTSEC-AAA-0001 expired 1 day(s) ago" "$out"
assert_status "multi-pastdue-id2-days" "RUSTSEC-BBB-0002 expired 30 day(s) ago" "$out"

# === empty / absent manifest ==================================================
M="${WORK}/empty.toml"; : > "$M"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "empty-manifest-ok" 0 "$?"

# === malformed manifest =======================================================
M="${WORK}/no-expires.toml"
cat > "$M" <<EOF
[[suppression]]
id = "X"
ecosystem = "rust"
ticket = "t"
reason = "r"
EOF
out="$(run_check "$M" "2026-06-06")"; assert_exit "malformed-missing-expires-fail" 1 "$?"
assert_status "malformed-token" "STATUS=FAIL REASON=suppression-malformed" "$out"

M="${WORK}/dup.toml"
cat > "$M" <<EOF
[[suppression]]
id = "RUSTSEC-2026-0009"
ecosystem = "rust"
expires = "2099-01-01"
ticket = "t"
reason = "r"
[[suppression]]
id = "RUSTSEC-2026-0009"
ecosystem = "rust"
expires = "2099-01-01"
ticket = "t"
reason = "r"
EOF
out="$(run_check "$M" "2026-06-06")"; assert_exit "malformed-dup-id-fail" 1 "$?"

# === quality-check ============================================================
M="${WORK}/badeco.toml"; mk_manifest "$M" "X" ruby "2099-01-01" "t" "r"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "quality-bad-ecosystem-fail" 1 "$?"
assert_status "quality-token" "STATUS=FAIL REASON=suppression-quality" "$out"

M="${WORK}/baddate.toml"; mk_manifest "$M" "X" rust "2099-13-99" "t" "r"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "quality-bad-date-fail" 1 "$?"

M="${WORK}/noticket.toml"; mk_manifest "$M" "X" rust "2099-01-01" "" "r"; fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "quality-empty-ticket-fail" 1 "$?"

# === sync-check (both directions, both derived files) =========================
M="${WORK}/sync.toml"; mk_manifest "$M" "RUSTSEC-2026-0011" rust "2099-01-01" "t" "r"; fix_check "$M"
# baseline: in sync -> OK
out="$(run_check "$M" "2026-06-06")"; assert_exit "sync-baseline-ok" 0 "$?"
# drift A: cargo derived drops the id (manifest > derived)
printf '[advisories]\nignore = []\n' > "${WORK}/cargo.audit.toml"
out="$(run_check "$M" "2026-06-06")"; assert_exit "sync-cargo-missing-fail" 1 "$?"
assert_status "sync-drift-token" "STATUS=FAIL REASON=suppression-drift" "$out"
# drift B: cargo derived has an extra id (derived > manifest)
printf '[advisories]\nignore = ["RUSTSEC-2026-0011","RUSTSEC-9999-9999"]\n' > "${WORK}/cargo.audit.toml"
out="$(run_check "$M" "2026-06-06")"; assert_exit "sync-cargo-extra-fail" 1 "$?"
# restore cargo, drift the pnpm json (extra id, derived > manifest for js side)
fix_check "$M"
printf '{ "ignore": ["GHSA-xxxx"] }\n' > "${WORK}/pnpm.ignore.json"
out="$(run_check "$M" "2026-06-06")"; assert_exit "sync-pnpm-extra-fail" 1 "$?"
# malformed pnpm json -> check side fail-LOUD (drift token, fail-closed)
printf '{ not json' > "${WORK}/pnpm.ignore.json"
out="$(run_check "$M" "2026-06-06")"; assert_exit "sync-pnpm-malformed-fail" 1 "$?"

# === --fix idempotence ========================================================
M="${WORK}/idem.toml"; mk_manifest "$M" "RUSTSEC-2026-0012" rust "2099-01-01" "t" "r"; fix_check "$M"
cp "${WORK}/cargo.audit.toml" "${WORK}/c1"; cp "${WORK}/pnpm.ignore.json" "${WORK}/p1"
fix_check "$M"
diff -q "${WORK}/c1" "${WORK}/cargo.audit.toml" >/dev/null; assert_exit "fix-idempotent-cargo" 0 "$?"
diff -q "${WORK}/p1" "${WORK}/pnpm.ignore.json" >/dev/null; assert_exit "fix-idempotent-pnpm" 0 "$?"
# --fix actually re-syncs a dirtied derived file
printf '[advisories]\nignore = []\n' > "${WORK}/cargo.audit.toml"
fix_check "$M"
out="$(run_check "$M" "2026-06-06")"; assert_exit "fix-resyncs-dirtied" 0 "$?"

# === A+B sentinel-gating ======================================================
M="${WORK}/sent.toml"; mk_manifest "$M" "RUSTSEC-2099-0050" rust "2099-01-01" "t" "r"; fix_check "$M"
# A: override + DEVLOOP_TEST=1 -> override honored (fixture used, clean) OK.
# now (2098-01-01) is BEFORE the fixture's expires (2099-01-01), so honoring the
# override yields OK — proving the fixture manifest+now were actually read.
out="$(run_check "$M" "2098-01-01")"; assert_exit "sentinel-A-honored" 0 "$?"
# B: override present + DEVLOOP_TEST ABSENT -> FAIL override-without-sentinel
out="$(env -i PATH="$PATH" HOME="$HOME" DEVLOOP_SUPPRESSIONS_MANIFEST="$M" "$CHECK" 2>&1)"; rc=$?
assert_exit "sentinel-B-absent-fail" 1 "$rc"
assert_status "sentinel-B-absent-token" "REASON=suppression-override-without-test-sentinel" "$out"
# B' (load-bearing): override present + DEVLOOP_TEST=0 (wrong value) -> FAIL (truthy bypass closed)
out="$(env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TEST=0 DEVLOOP_SUPPRESSIONS_MANIFEST="$M" "$CHECK" 2>&1)"; rc=$?
assert_exit "sentinel-Bprime-zero-fail" 1 "$rc"
assert_status "sentinel-Bprime-token" "REASON=suppression-override-without-test-sentinel" "$out"
# NOTE: the former `no-override-repo-root-ok` case was REMOVED (test-rot fix, @test
# Gate-2): it ran the check with no sentinel against the REAL repo-root manifest + REAL
# clock, so it would flip to FAIL once the seed's `expires` (2026-09-01) passes — and it
# was re-testing the guard's own time-bomb, not override-resolution. The sentinel A/B/B'
# cases above already cover override-resolution + the no-sentinel-fallback path.

# === dependabot-ignore guard (security a1, mechanical enforcement) =============
# The check reads REPO_ROOT/.github/dependabot.yml (fixed path, not override-able), so
# this test hermetically backs up the real file, swaps fixtures, asserts, and restores.
#
# DETERMINISM (test-rot fix, @test Gate-2): these cases run UNDER the sentinel with a
# FUTURE-dated fixture manifest + injected AUDIT_SUPPRESSIONS_NOW, so __date_check stays
# green regardless of the real clock and the DEPENDABOT assertion is what's under test.
# Without this they read the real seed against the real clock and short-circuit on
# `suppression-past-due` after 2026-09-01. The dependabot.yml swap is orthogonal (fixed
# path) and stays. dbot_run() = check invocation with sentinel + future fixture + dbot env.
REPO_ROOT_T="$(cd "${__here}/.." && pwd)"
DBOT="${REPO_ROOT_T}/.github/dependabot.yml"
DBOT_BAK="${WORK}/dependabot.yml.bak"
dbot_had_file=0
if [[ -f "$DBOT" ]]; then cp "$DBOT" "$DBOT_BAK"; dbot_had_file=1; fi
restore_dbot() {
  if [[ "$dbot_had_file" -eq 1 ]]; then cp "$DBOT_BAK" "$DBOT"; else rm -f "$DBOT"; fi
}
# Augment the EXIT trap to also restore dependabot.yml (in addition to rm -rf WORK).
trap 'restore_dbot; rm -rf "$WORK"' EXIT

# Future-dated fixture manifest + matching derived files so date/sync/quality all pass,
# isolating the dependabot check as the only thing that can fail.
DBOT_M="${WORK}/dbot-fixture.toml"
mk_manifest "$DBOT_M" "RUSTSEC-2099-0077" rust "2099-01-01" "t" "r"; fix_check "$DBOT_M"
dbot_run() {
  env -i PATH="$PATH" HOME="$HOME" \
      DEVLOOP_TEST=1 \
      DEVLOOP_SUPPRESSIONS_MANIFEST="$DBOT_M" \
      DEVLOOP_CARGO_AUDIT_TOML="${WORK}/cargo.audit.toml" \
      DEVLOOP_AUDIT_IGNORE_JSON="${WORK}/pnpm.ignore.json" \
      AUDIT_SUPPRESSIONS_NOW="2026-06-06" \
      "$CHECK" 2>&1
}

# populated ignore: block -> FAIL dependabot-ignore-present
mkdir -p "${REPO_ROOT_T}/.github"
printf 'version: 2\nupdates:\n  - package-ecosystem: cargo\n    directory: "/"\n    schedule:\n      interval: weekly\n    ignore:\n      - dependency-name: "rsa"\n' > "$DBOT"
out="$(dbot_run)"; rc=$?
assert_exit "dependabot-ignore-populated-fail" 1 "$rc"
assert_status "dependabot-ignore-token" "REASON=dependabot-ignore-present" "$out"
# empty ignore: key followed by another key -> NOT flagged (clean)
printf 'version: 2\nupdates:\n  - package-ecosystem: cargo\n    directory: "/"\n    ignore:\n    schedule:\n      interval: weekly\n' > "$DBOT"
out="$(dbot_run)"; assert_exit "dependabot-empty-ignore-ok" 0 "$?"
# comment-only mention of ignore: -> NOT flagged (the real-file shape)
printf '# Do NOT add an `ignore:` block here to suppress advisories.\nversion: 2\nupdates:\n  - package-ecosystem: cargo\n    directory: "/"\n    schedule:\n      interval: weekly\n' > "$DBOT"
out="$(dbot_run)"; assert_exit "dependabot-comment-ignore-ok" 0 "$?"
restore_dbot

# === GATE-side filter FAIL-SAFE (security §D.1.2 PATH-1, @code-reviewer/@security F4) ===
# _audit_gate.sh::audit_read_pnpm_suppressions on a malformed .pnpm-audit-ignore.json must
# FAIL-SAFE: apply ZERO suppressions (no ids on stdout), emit a WARN on stderr, and RETURN
# 0 (proceed — never abort; a filter-abort = denial-of-coverage). Opposite policy from the
# CHECK side (fail-LOUD). Uses the SENTINEL-GATED DEVLOOP_AUDIT_IGNORE_JSON override
# (DEVLOOP_TEST=1) — the gate path resolver now honors it (mirroring the check), so this
# tests the REAL gate path against a fixture WITHOUT mutating the real repo-root file.
# gate_filter <fixture-json-path> 2>stderr -> echoes stdout ids; rc via $?.
gate_filter() {
  env -i PATH="$PATH" HOME="$HOME" \
      DEVLOOP_TEST=1 DEVLOOP_AUDIT_IGNORE_JSON="$1" \
      bash -c 'source scripts/lang/_audit_gate.sh; audit_read_pnpm_suppressions'
}

# malformed -> rc 0, zero ids, verbatim WARN (no abort)
printf '{ this is not valid json' > "${WORK}/gate-malformed.json"
gate_out="$(gate_filter "${WORK}/gate-malformed.json" 2>"${WORK}/gate.err")"; gate_rc=$?
gate_err="$(cat "${WORK}/gate.err")"
assert_exit "gate-filter-malformed-returns-0" 0 "$gate_rc"
[[ -z "$gate_out" ]] && PASS=$((PASS+1)) || { FAIL=$((FAIL+1)); FAILURES+=("[gate-filter-malformed-zero-ids] expected empty stdout, got '$gate_out'"); }
assert_status "gate-filter-malformed-warns" "WARN: .pnpm-audit-ignore.json unparseable" "$gate_err"
# "ignore" not a list -> also malformed (rc 0 + zero ids + WARN)
printf '{"ignore": "not-a-list"}' > "${WORK}/gate-notlist.json"
gate_out="$(gate_filter "${WORK}/gate-notlist.json" 2>"${WORK}/gate.err")"; gate_rc=$?
assert_exit "gate-filter-ignore-not-list-returns-0" 0 "$gate_rc"
[[ -z "$gate_out" ]] && PASS=$((PASS+1)) || { FAIL=$((FAIL+1)); FAILURES+=("[gate-filter-not-list-zero-ids] got '$gate_out'"); }
# valid -> ids on stdout, rc 0, no WARN
printf '{"ignore":["GHSA-test-1"]}' > "${WORK}/gate-valid.json"
gate_out="$(gate_filter "${WORK}/gate-valid.json" 2>"${WORK}/gate.err")"; gate_rc=$?
gate_err="$(cat "${WORK}/gate.err")"
assert_exit "gate-filter-valid-returns-0" 0 "$gate_rc"
assert_status "gate-filter-valid-emits-id" "GHSA-test-1" "$gate_out"
[[ -z "$gate_err" ]] && PASS=$((PASS+1)) || { FAIL=$((FAIL+1)); FAILURES+=("[gate-filter-valid-no-warn] unexpected stderr '$gate_err'"); }
# ABSENT -> rc 0, zero ids, NO warn (absent != malformed — legitimate zero suppressions;
# pins the distinction so a future change can't collapse them).
gate_out="$(gate_filter "${WORK}/does-not-exist.json" 2>"${WORK}/gate.err")"; gate_rc=$?
gate_err="$(cat "${WORK}/gate.err")"
assert_exit "gate-filter-absent-returns-0" 0 "$gate_rc"
[[ -z "$gate_out" ]] && PASS=$((PASS+1)) || { FAIL=$((FAIL+1)); FAILURES+=("[gate-filter-absent-zero-ids] got '$gate_out'"); }
[[ -z "$gate_err" ]] && PASS=$((PASS+1)) || { FAIL=$((FAIL+1)); FAILURES+=("[gate-filter-absent-no-warn] got '$gate_err'"); }

report_results "scripts/audit-suppressions-check.test.sh"
