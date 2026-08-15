#!/usr/bin/env bash
# layer7.test.sh — self-test for scripts/layer7.sh (task #56).
#
# Layer 7 runs the env-test suite against a live Kind cluster, so its happy path needs a
# cluster — but its LANE LOGIC (the four terminal STATUS lines + exit codes) is pure
# control flow we can pin hermetically with a fake `dev-cluster` on PATH + a minted unix
# socket, WITHOUT a cluster. This drives every branch a healthy Gate-2 run won't reach:
#   - env gate, absent socket + CI (GITHUB_ACTIONS)    → SKIPPED-NO-CLUSTER, exit 0 (the ONLY skip)
#   - env gate, absent socket + LOCAL                  → PRECONDITION_FAILURE, exit 2 (regression-closer)
#   - env gate, dead socket (present, unconnectable)   → PRECONDITION_FAILURE helper-unreachable, exit 2
#   - Phase-1 precondition failure (setup fails)      → PRECONDITION_FAILURE, exit 2 (operator)
#   - Phase-1f Prometheus HARD not-ready              → PRECONDITION_FAILURE observability-prometheus-not-ready, exit 2
#   - Phase-1f Loki SOFT not-ready                    → loud WARN + PROCEED → OK, exit 0 (never blocks)
#   - Phase-2 green                                   → OK, exit 0
#   - Phase-2 non-zero                                → FAIL, exit 1 (implementer)
#   - Phase-2 non-zero whose output CONTAINS infra words → STILL FAIL (the retired-grep proof)
#   - Browser E2E (task #19): trigger-off → SKIPPED-NO-DIFF child line, layer still OK;
#     trigger-on both-green → OK with BOTH -passed REASONs; browser suite non-zero → FAIL
#     browser-e2e-failed (exit 1); fingerprints missing / Chromium missing → the two
#     Phase-1g PRECONDITION_FAILURE tokens (exit 2, BEFORE any suite runs); env-tests red
#     + trigger-on → browser suite NOT run (greppable `browser-e2e-not-run:` stderr note,
#     NO browser STATUS line); plus direct-call cases pinning the REAL
#     __browser_e2e_triggered() predicate against a hand-written changed-files cache.
#   - PARSE-PATH INTEGRITY: the operator lane's emitted STATUS survives the SHARED parse/
#     aggregate helpers layer-all.sh will later run on it (parse_status_line /
#     aggregate_worst_status / status_to_exit_code) → PRECONDITION_FAILURE, not UNKNOWN/FAIL.
#     This is the layer7-DIRECT seam check; it does NOT run layer-all.sh. The true
#     orchestrator-integrity test (final_exit aggregation / exit-2-not-collapsed) is
#     scripts/layer-all.test.sh (Test B, @test-owned).
#
# Wired into scripts/layer3.sh (there is no *.test.sh auto-runner), so it runs every
# devloop + CI. Consumes scripts/lang/_test_helpers.sh (assert_status/assert_exit/
# report_results) + sources _common.sh for the parse-path seam assertions.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/.." && pwd)"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"
# shellcheck source=lang/_common.sh
source "${__here}/lang/_common.sh"  # parse_status_line, aggregate_worst_status, status_to_exit_code

# The check deliberately exits non-zero on the FAIL/PRECONDITION lanes; capture those
# rc/output, so set -e must NOT abort the harness. report_results sets the final code.
set +e

LAYER7="${REPO_ROOT}/scripts/layer7.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- Fixtures -----------------------------------------------------------------

# Fake dev-cluster: behavior controlled by FAKE_* env. `status` prints the health lines
# the real client emits on stderr; write verbs honor injected exit codes.
FAKE_DC="${WORK}/dev-cluster"
cat > "$FAKE_DC" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  status)
    # FAKE_STATUS_RC != 0 simulates a DEAD helper (connection refused on a stale socket):
    # the real client prints an error to stderr and exits non-zero WITHOUT health lines.
    if [[ "${FAKE_STATUS_RC:-0}" != "0" ]]; then
      echo "ERROR: Cannot connect to helper socket (simulated dead helper)" >&2
      exit "${FAKE_STATUS_RC}"
    fi
    echo "  Cluster exists:     ${FAKE_CLUSTER_EXISTS:-true}" >&2
    echo "  Pods healthy:       ${FAKE_PODS_HEALTHY:-true}" >&2
    echo "  Setup in progress:  ${FAKE_SETUP_IN_PROGRESS:-false}" >&2
    exit 0 ;;
  setup)       exit "${FAKE_SETUP_RC:-0}" ;;
  teardown)    exit "${FAKE_TEARDOWN_RC:-0}" ;;
  rebuild-all) exit "${FAKE_REBUILD_RC:-0}" ;;
  # LOUD on any unrecognized verb — NOT `exit 0`. A permissive catch-all here is a
  # vacuous-pass generator, and it was a real one: with `*) exit 0`, a MISSPELLED or
  # SILENTLY DROPPED dev-cluster verb in layer7.sh still produced a green suite, because
  # the fake succeeded at whatever it was asked. Every case asserting STATUS=OK would keep
  # passing while the cluster call it depends on had stopped happening. Naming the verb on
  # stderr means the diagnostic points at the drift instead of at the assertion that
  # noticed it. (Lead-confirmed in-scope hazard, story R-7 / task #3.)
  *)
    echo "FAKE dev-cluster: unrecognized verb '$1' — layer7.sh called a verb this fixture does not model. Add a case here (or fix the caller); do NOT restore a permissive catch-all." >&2
    exit 97 ;;
esac
EOF
chmod +x "$FAKE_DC"

# ports.json fixture (all five container_urls, incl. optional Loki, plus the cluster_name
# Phase-1h reads to address the provisioning target).
PORTS="${WORK}/ports.json"
cat > "$PORTS" <<'EOF'
{"cluster_name":"devloop-fixture","container_urls":{"ac":"http://ac:8082","gc":"http://gc:8080","prometheus":"http://prom:9090","grafana":"http://graf:3000","loki":"http://loki:3100"}}
EOF

# Same file with `.cluster_name` ABSENT — drives Phase-1h's fail-closed context check.
# A separate fixture rather than a FAKE_* flag: the condition IS the file's content.
PORTS_NO_CLUSTER="${WORK}/ports-no-cluster.json"
cat > "$PORTS_NO_CLUSTER" <<'EOF'
{"container_urls":{"ac":"http://ac:8082","gc":"http://gc:8080","prometheus":"http://prom:9090","grafana":"http://graf:3000","loki":"http://loki:3100"}}
EOF

# Fake HTTP readiness probe for Phase-1f (injected via DEVLOOP_HTTP_PROBE so the obs gate is
# hermetic — no real curl/cluster). Last arg is the URL; Prometheus uses `/-/ready`, Loki uses
# `/ready`. Honors FAKE_PROM_READY / FAKE_LOKI_READY (default 1=ready). Exit 0=2xx-ready, 1=not.
FAKE_PROBE="${WORK}/http-probe"
cat > "$FAKE_PROBE" <<'EOF'
#!/usr/bin/env bash
url="${@: -1}"
case "$url" in
  */-/ready) [[ "${FAKE_PROM_READY:-1}" == "1" ]] ;;
  # AC's /health, probed in Phase 1h BEFORE the org-resolution probe so "AC is down" and
  # "the org is not there" land on different tokens. Must be matched before */ready.
  */health)  [[ "${FAKE_AC_READY:-1}" == "1" ]] ;;
  */ready)   [[ "${FAKE_LOKI_READY:-1}" == "1" ]] ;;
  *)         true ;;
esac
EOF
chmod +x "$FAKE_PROBE"

# --- Phase-1h (per-run organization, R-7) fixtures ----------------------------
# Markers dir: stubs touch a file when they run, so a case can prove the path it names
# ACTUALLY executed — and, for the precondition lanes, that the SUITES did not. An
# exit-code-only assertion cannot tell "the branch I meant to test ran" from "some other
# branch produced the same status", and every broken harness also exits non-zero.
MARKERS="${WORK}/markers"
# One line per --provision-org invocation, holding the subdomain ACTUALLY passed. The
# Nth-run case reads this file: "an organization got created" does not pin R-7; "run 2
# asked for a DIFFERENT subdomain than run 1" does.
PROVISIONED="${WORK}/provisioned"
# The Host header the AC org-resolution probe was given, one per line.
PROBED_HOSTS="${WORK}/probed-hosts"

# Fake setup.sh, installed through the DEVLOOP_SETUP_SH seam. Stands in for
# `infra/kind/scripts/setup.sh --provision-org <sub>`; the REAL script is exercised
# against PATH-stubbed kubectl/psql in scripts/setup.test.sh, which is the tier that can
# reach it (setup.sh is by construction stubbed out here).
FAKE_SETUP="${WORK}/setup.sh"
cat > "$FAKE_SETUP" <<EOF
#!/usr/bin/env bash
touch "${MARKERS}/ran.setup-sh"
sub=""
while [[ \$# -gt 0 ]]; do
  case "\$1" in
    --provision-org) sub="\${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done
printf '%s\n' "\$sub" >> "${PROVISIONED}"
# Recorded BEFORE the optional sleep so the timeout case still yields its argument.
if [[ "\${FAKE_PROVISION_SLEEP:-0}" != "0" ]]; then sleep "\${FAKE_PROVISION_SLEEP}"; fi
# The stable machine-readable line layer7.sh POSITIVE-matches (fail-closed: absence is a
# failure, since setup.sh interleaves log_step/log_info on stdout and there is no
# "last line" to trust). FAKE_PROVISION_TOKEN=0 omits it while still exiting 0 — the
# success-looking no-op that a pure exit-code check would wave through.
if [[ "\${FAKE_PROVISION_TOKEN:-1}" == "1" ]]; then
  printf 'PROVISIONED_ORG org_id=11111111-2222-3333-4444-555555555555 subdomain=%s\n' "\$sub"
fi
exit "\${FAKE_PROVISION_RC:-0}"
EOF
chmod +x "$FAKE_SETUP"

# Fake AC org-resolution probe (DEVLOOP_ORG_PROBE). Must print an HTTP STATUS CODE on
# stdout: Phase 1h discriminates 401 (org resolved, credentials rejected — PASS) from 404
# (org_extraction failed closed — unverified) from 429 (AC cannot answer). A pass/fail
# probe could not express that, which is why this seam is separate from HTTP_PROBE.
FAKE_ORG_PROBE_BIN="${WORK}/org-probe"
cat > "$FAKE_ORG_PROBE_BIN" <<EOF
#!/usr/bin/env bash
touch "${MARKERS}/ran.org-probe"
prev=""
for a in "\$@"; do
  if [[ "\$prev" == "-H" && "\$a" == Host:* ]]; then printf '%s\n' "\${a#Host: }" >> "${PROBED_HOSTS}"; fi
  prev="\$a"
done
printf '%s' "\${FAKE_PROBE_CODE:-401}"
EOF
chmod +x "$FAKE_ORG_PROBE_BIN"

# Marker-dropping suite stubs. The Rust one also records the ENV_TEST_ORG_SUBDOMAIN it was
# handed and the browser one records E2E_ORG_SUBDOMAIN — end-to-end consumption is the
# whole point of the feature, and one export reaching only the Rust suite would leave the
# BROWSER suite (the one that actually exhausts the cap at ~7 meetings/run) on a stale org.
SUITE_STUB="${WORK}/suite-env.sh"
cat > "$SUITE_STUB" <<EOF
#!/usr/bin/env bash
touch "${MARKERS}/ran.env-suite"
printf '%s\n' "\${ENV_TEST_ORG_SUBDOMAIN-<unset>}" > "${WORK}/seen-env-test-subdomain"
exit "\${FAKE_SUITE_RC:-0}"
EOF
chmod +x "$SUITE_STUB"

BROWSER_STUB="${WORK}/suite-browser.sh"
cat > "$BROWSER_STUB" <<EOF
#!/usr/bin/env bash
touch "${MARKERS}/ran.browser-suite"
printf '%s\n' "\${E2E_ORG_SUBDOMAIN-<unset>}" > "${WORK}/seen-e2e-subdomain"
exit 0
EOF
chmod +x "$BROWSER_STUB"

# Mint a real unix socket so `[[ -S ]]` is true without a running helper.
PRESENT_SOCK="${WORK}/helper.sock"
python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.close()' "$PRESENT_SOCK"
ABSENT_SOCK="${WORK}/does-not-exist.sock"

# Browser-E2E fixtures (task #19): a complete fingerprints file + a fake Playwright
# browsers dir containing a chromium-* entry — the two Phase-1g preconditions. Cases flip
# them to the "missing" variants to drive the PRECONDITION_FAILURE tokens.
GOOD_FP="${WORK}/fingerprints.json"
printf '{"MC_CERT_SHA256":"fake-mc-hash","MH_CERT_SHA256":"fake-mh-hash"}\n' > "$GOOD_FP"
INCOMPLETE_FP="${WORK}/fingerprints-incomplete.json"
printf '{"MC_CERT_SHA256":"fake-mc-hash"}\n' > "$INCOMPLETE_FP"
GOOD_PW_DIR="${WORK}/ms-playwright"
mkdir -p "${GOOD_PW_DIR}/chromium-1234"
EMPTY_PW_DIR="${WORK}/ms-playwright-empty"
mkdir -p "$EMPTY_PW_DIR"

# Run layer7.sh under a scrubbed env (env -i so a CI GITHUB_ACTIONS doesn't flip
# _get_base_ref into PR mode) with the given overrides. Writes stdout→$1, stderr→$2.
# Caller exports SOCK + optional ENVCMD + FAKE_* before calling. Sets global RC.
run_layer7() {
  local out_f="$1" err_f="$2"
  ( cd "$REPO_ROOT" && env -i \
      PATH="$PATH" HOME="$HOME" \
      ${CI_FLAG:+GITHUB_ACTIONS="$CI_FLAG"} \
      DEVLOOP_TEST=1 \
      DEVLOOP_TMP="${WORK}/devloop-tmp" \
      DEVLOOP_HELPER_SOCKET="${SOCK}" \
      DEVLOOP_DEV_CLUSTER_BIN="${FAKE_DC}" \
      DEVLOOP_PORTS_JSON="${PORTS_JSON_OVERRIDE:-$PORTS}" \
      DEVLOOP_ENV_TEST_CMD="${ENVCMD:-}" \
      DEVLOOP_HTTP_PROBE="${FAKE_PROBE}" \
      DEVLOOP_HEALTH_BUDGET=0 \
      DEVLOOP_SETUP_SH="${SETUP_SH_OVERRIDE:-$FAKE_SETUP}" \
      DEVLOOP_ORG_PROBE="${ORG_PROBE_OVERRIDE:-$FAKE_ORG_PROBE_BIN}" \
      DEVLOOP_ORG_PROVISION_TIMEOUT="${PROVISION_TIMEOUT:-120}" \
      FAKE_PROVISION_RC="${FAKE_PROVISION_RC:-0}" \
      FAKE_PROVISION_TOKEN="${FAKE_PROVISION_TOKEN:-1}" \
      FAKE_PROVISION_SLEEP="${FAKE_PROVISION_SLEEP:-0}" \
      FAKE_PROBE_CODE="${FAKE_PROBE_CODE:-401}" \
      FAKE_AC_READY="${FAKE_AC_READY:-1}" \
      FAKE_SUITE_RC="${FAKE_SUITE_RC:-0}" \
      DEVLOOP_BROWSER_E2E_TRIGGER="${BROWSER_TRIGGER:-0}" \
      DEVLOOP_BROWSER_E2E_CMD="${BROWSER_CMD:-}" \
      DEVLOOP_FINGERPRINTS_JSON="${FP_JSON:-$GOOD_FP}" \
      PLAYWRIGHT_BROWSERS_PATH="${PW_DIR:-$GOOD_PW_DIR}" \
      FAKE_PROM_READY="${FAKE_PROM_READY:-1}" \
      FAKE_LOKI_READY="${FAKE_LOKI_READY:-1}" \
      FAKE_CLUSTER_EXISTS="${FAKE_CLUSTER_EXISTS:-true}" \
      FAKE_PODS_HEALTHY="${FAKE_PODS_HEALTHY:-true}" \
      FAKE_SETUP_IN_PROGRESS="${FAKE_SETUP_IN_PROGRESS:-false}" \
      FAKE_STATUS_RC="${FAKE_STATUS_RC:-0}" \
      FAKE_SETUP_RC="${FAKE_SETUP_RC:-0}" \
      FAKE_TEARDOWN_RC="${FAKE_TEARDOWN_RC:-0}" \
      FAKE_REBUILD_RC="${FAKE_REBUILD_RC:-0}" \
      bash "$LAYER7" ) >"$out_f" 2>"$err_f"
  RC=$?
}

# Reset per-case FAKE_* / ENVCMD / browser-E2E overrides to defaults.
# NB: BROWSER_TRIGGER defaults to 0 (force-skip) in run_layer7 — every pre-existing flow
# case stays hermetic (no dependence on the repo's live diff); browser cases opt in with
# BROWSER_TRIGGER=1.
reset_case() {
  unset FAKE_CLUSTER_EXISTS FAKE_PODS_HEALTHY FAKE_SETUP_IN_PROGRESS FAKE_STATUS_RC \
        FAKE_SETUP_RC FAKE_TEARDOWN_RC FAKE_REBUILD_RC FAKE_PROM_READY FAKE_LOKI_READY \
        ENVCMD CI_FLAG BROWSER_TRIGGER BROWSER_CMD FP_JSON PW_DIR \
        FAKE_PROVISION_RC FAKE_PROVISION_TOKEN FAKE_PROVISION_SLEEP FAKE_PROBE_CODE \
        FAKE_AC_READY FAKE_SUITE_RC PORTS_JSON_OVERRIDE SETUP_SH_OVERRIDE \
        ORG_PROBE_OVERRIDE PROVISION_TIMEOUT
  # Markers are per-case evidence; a stale one from the previous case would make
  # assert_no_marker report a stub that never ran (and assert_marker pass vacuously).
  rm -rf "$MARKERS"; mkdir -p "$MARKERS"
  rm -f "$PROVISIONED" "$PROBED_HOSTS"
}

OUT="${WORK}/out"; ERR="${WORK}/err"

# === environment discriminator (fail-closed; socket-present-FIRST; silent-skip closer) ==
# The skip lane (exit 0) is reachable ONLY when the socket is ABSENT *and* GITHUB_ACTIONS is
# set (CI). Socket PRESENT → proceed regardless of CI; local absent → loud PRECONDITION.

# (1) absent socket + CI (GITHUB_ACTIONS) → SKIPPED-NO-CLUSTER, exit 0 (the ONLY skip case).
reset_case
SOCK="$ABSENT_SOCK"; export CI_FLAG=true
run_layer7 "$OUT" "$ERR"
assert_exit   "ci-no-cluster-exit0"   0 "$RC"
assert_status "ci-no-cluster-status"  "STATUS=SKIPPED-NO-CLUSTER" "$(cat "$OUT")"
assert_status "ci-no-cluster-reason"  "REASON=no-cluster-ci" "$(cat "$OUT")"
reset_case

# (1b) FORWARD-COMPAT: present socket + CI → RUNS (does NOT skip). Socket-present is checked
#      before GITHUB_ACTIONS, so a future CI that provisions a helper+cluster runs env-tests
#      with zero code change (team-lead's "+ no usable cluster" qualifier).
reset_case
SOCK="$PRESENT_SOCK"; export CI_FLAG=true ENVCMD="true"
run_layer7 "$OUT" "$ERR"
assert_exit   "ci-with-cluster-runs-exit0" 0 "$RC"
assert_status "ci-with-cluster-runs-ok"    "STATUS=OK REASON=env-tests-passed" "$(cat "$OUT")"
assert_status "ci-with-cluster-not-skip"   "STATUS=OK" "$(cat "$OUT")"  # NOT SKIPPED-NO-CLUSTER
reset_case

# (2) absent socket + LOCAL (no GITHUB_ACTIONS) → PRECONDITION_FAILURE, exit 2 (LOUD).
#     THE REGRESSION-CLOSER: a local devloop that forgot to start the helper must NOT
#     silently skip env-tests (the exact exit-0 hole this task closes).
reset_case
SOCK="$ABSENT_SOCK"
run_layer7 "$OUT" "$ERR"
assert_exit   "local-no-helper-exit2"  2 "$RC"
assert_status "local-no-helper-status" "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
assert_status "local-no-helper-token"  "REASON=local-helper-not-running" "$(cat "$OUT")"
reset_case

# (3) DEAD socket: present but unconnectable → PRECONDITION_FAILURE, NOT skip ====
# (Locks @team-lead/@operations' safety case: a crashed helper leaving a stale socket
#  must surface loud as the operator lane — it must NOT silently skip like absent-socket.)
reset_case
SOCK="$PRESENT_SOCK"; export FAKE_STATUS_RC=1
run_layer7 "$OUT" "$ERR"
assert_exit   "dead-socket-exit2"   2 "$RC"
assert_status "dead-socket-status"  "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
assert_status "dead-socket-token"   "REASON=helper-unreachable" "$(cat "$OUT")"
assert_status "dead-socket-not-skip" "PRECONDITION_FAILURE:" "$(cat "$ERR")"
reset_case

# === seam gate-inertness: the DEVLOOP_HELPER_SOCKET override is INERT without DEVLOOP_TEST =====
# @security-required pin (task #56). The three cluster-machinery seams (socket / dev-cluster
# binary / ports.json) are silent-validation-disable levers if honored in production: a forged
# DEVLOOP_HELPER_SOCKET pointing at an attacker-minted LIVE socket would let a CI/devloop run
# pass the gate against a fake cluster. layer7.sh consults them ONLY under DEVLOOP_TEST=1;
# otherwise it pins the ADR-0030 canonical /tmp/devloop/helper.sock. We assert that gating at
# the VARIABLE level rather than behaviorally on purpose: the canonical socket is PRESENT in a
# dev container, so a full behavioral run with DEVLOOP_TEST unset would proceed to the REAL
# cluster — the exact production mutation the gate exists to prevent. Sourcing layer7.sh (its
# `BASH_SOURCE==$0` guard suppresses __layer7_main; the git work is inside diff_touches_path,
# not at source time) lets us read the RESOLVED path without running a single cluster verb.
# The behavioral consequence (canonical-absent → PRECONDITION / SKIPPED) is already pinned by
# the four-way discriminator cases above.
#
# GENERALIZED from the socket-only version (@test-reviewer F1). `DEVLOOP_SETUP_SH` and
# `DEVLOOP_ORG_PROBE` are Phase-1h's two seams and BOTH EXECUTE A COMMAND, which is a strictly
# larger lever than the socket path — layer7.sh's own comment says pointing SETUP_SH at a stub
# "would fake the per-run organization … with every gate still green". Their inertness was NOT
# pinned, and the gap was measured: moving the `${DEVLOOP_SETUP_SH:-…}` / `${DEVLOOP_ORG_PROBE:-…}`
# expansions into the PRODUCTION half left the suite at 141/0. The seam-symmetry case at the end
# of this file cannot catch that — it compares variable NAMES across the two branches, and the
# name is present either way; it catches unbound-variable-in-production, a different bug. Both
# cases are needed and neither subsumes the other.
resolve_seam_var() {  # $1 = variable name  $2 = DEVLOOP_TEST value ("" leaves it unset)
  ( cd "$REPO_ROOT" && env -i PATH="$PATH" HOME="$HOME" \
      ${2:+DEVLOOP_TEST="$2"} \
      DEVLOOP_TMP="${WORK}/devloop-tmp" \
      DEVLOOP_HELPER_SOCKET="$PRESENT_SOCK" \
      DEVLOOP_DEV_CLUSTER_BIN="${FAKE_DC}" \
      DEVLOOP_SETUP_SH="${FAKE_SETUP}" \
      DEVLOOP_ORG_PROBE="${FAKE_ORG_PROBE_BIN}" \
      bash -c 'source "'"$LAYER7"'" >/dev/null 2>&1; printf "%s" "${!1}"' _ "$1" )
}
reset_case
assert_status "socket-seam-inert-without-devloop-test" \
  "/tmp/devloop/helper.sock" "$(resolve_seam_var DEVLOOP_HELPER_SOCKET "")"
assert_status "socket-seam-honored-under-devloop-test" \
  "$PRESENT_SOCK" "$(resolve_seam_var DEVLOOP_HELPER_SOCKET 1)"
# SETUP_SH — the executed provisioning script. Inert ⇒ the canonical in-repo path.
assert_status "setup-sh-seam-inert-without-devloop-test" \
  "${REPO_ROOT}/infra/kind/scripts/setup.sh" "$(resolve_seam_var SETUP_SH "")"
assert_status "setup-sh-seam-honored-under-devloop-test" \
  "$FAKE_SETUP" "$(resolve_seam_var SETUP_SH 1)"
# ORG_PROBE — the executed AC probe. Inert ⇒ the canonical curl invocation.
assert_status "org-probe-seam-inert-without-devloop-test" \
  "curl -s -o /dev/null -w %{http_code} --max-time 10" "$(resolve_seam_var ORG_PROBE "")"
assert_status "org-probe-seam-honored-under-devloop-test" \
  "$FAKE_ORG_PROBE_BIN" "$(resolve_seam_var ORG_PROBE 1)"
reset_case

# === Phase-1 precondition failure (cluster not ready + setup fails) ============
# This doubles as the PARSE-PATH case: the operator lane survives the shared parse/aggregate
# helpers (not the orchestrator itself — that's layer-all.test.sh / Test B, @test-owned).
reset_case
SOCK="$PRESENT_SOCK"; export FAKE_CLUSTER_EXISTS=false FAKE_SETUP_RC=1
run_layer7 "$OUT" "$ERR"
assert_exit   "setup-fail-exit2"          2 "$RC"
assert_status "setup-fail-status"         "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
assert_status "setup-fail-token"          "REASON=cluster-setup-failed" "$(cat "$OUT")"
assert_status "setup-fail-banner"         "PRECONDITION_FAILURE:" "$(cat "$ERR")"
# The exact seams layer-all.sh uses to render the summary + derive the exit code:
ps="$(parse_status_line "$OUT")"
assert_status "orch-parse_status_line"    "PRECONDITION_FAILURE" "$ps"
assert_status "orch-aggregate-survives"   "PRECONDITION_FAILURE" "$(aggregate_worst_status OK "$ps")"
assert_exit   "orch-status_to_exit_code"  2 "$(status_to_exit_code "$ps")"
reset_case

# === Phase-1 precondition: rebuild-all fails → PRECONDITION_FAILURE ============
reset_case
SOCK="$PRESENT_SOCK"; export FAKE_REBUILD_RC=1
run_layer7 "$OUT" "$ERR"
assert_exit   "rebuild-fail-exit2"  2 "$RC"
assert_status "rebuild-fail-token"  "REASON=cluster-rebuild-failed" "$(cat "$OUT")"
reset_case

# === Phase-1 precondition: ports.json missing → PRECONDITION_FAILURE ==========
reset_case
SOCK="$PRESENT_SOCK"
# NB: DEVLOOP_TEST=1 is MANDATORY — the layer7 path/socket/binary seams are now gated behind
# it (security), so without the sentinel the fixture overrides are IGNORED and layer7 would
# hit the REAL dev-cluster/socket/ports.json (a real-cluster mutation + hang). `env -i` keeps
# GITHUB_ACTIONS unset (so it's the local, not CI, branch).
( cd "$REPO_ROOT" && env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TEST=1 \
    DEVLOOP_TMP="${WORK}/devloop-tmp" DEVLOOP_HELPER_SOCKET="$SOCK" \
    DEVLOOP_DEV_CLUSTER_BIN="$FAKE_DC" DEVLOOP_PORTS_JSON="${WORK}/nope.json" \
    bash "$LAYER7" ) >"$OUT" 2>"$ERR"; RC=$?
assert_exit   "ports-missing-exit2" 2 "$RC"
assert_status "ports-missing-token" "REASON=ports-json-missing" "$(cat "$OUT")"
reset_case

# === Phase 2 green → OK =======================================================
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true"
run_layer7 "$OUT" "$ERR"
assert_exit   "phase2-ok-exit0"  0 "$RC"
assert_status "phase2-ok-status" "STATUS=OK REASON=env-tests-passed" "$(cat "$OUT")"
reset_case

# === Phase-1f observability readiness (user ruling (a) — per-probe hard/soft) =========
# Prometheus HARD: not-ready → PRECONDITION_FAILURE (exit 2, operator lane), BEFORE the suite.
# The metrics tests one-shot-query Prometheus, so a cold Prometheus must block, not flake.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" FAKE_PROM_READY=0
run_layer7 "$OUT" "$ERR"
assert_exit   "prom-not-ready-exit2"  2 "$RC"
assert_status "prom-not-ready-status" "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
assert_status "prom-not-ready-token"  "REASON=observability-prometheus-not-ready" "$(cat "$OUT")"
reset_case

# Loki SOFT: not-ready → loud WARN + PROCEED to the suite (→ OK). Never blocks (optional-Loki
# semantics); the WARN pre-attributes the eventual Phase-2 Loki test failure to the obs stack.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" FAKE_LOKI_READY=0
run_layer7 "$OUT" "$ERR"
assert_exit   "loki-not-ready-exit0"  0 "$RC"
assert_status "loki-not-ready-ok"     "STATUS=OK REASON=env-tests-passed" "$(cat "$OUT")"
assert_status "loki-not-ready-warn"   "WARN LOKI_NOT_READY_AFTER" "$(cat "$ERR")"
assert_status "loki-not-ready-attrib" "NOT your diff" "$(cat "$ERR")"
reset_case

# === Phase 2 non-zero → FAIL (implementer lane) ===============================
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="false"
run_layer7 "$OUT" "$ERR"
assert_exit   "phase2-fail-exit1"  1 "$RC"
assert_status "phase2-fail-status" "STATUS=FAIL REASON=env-tests-failed" "$(cat "$OUT")"
reset_case

# === LOAD-BEARING: a test FAIL whose output contains infra words STILL FAILs ===
# (Proves the retired log-grep no longer swallows a real failure as infra — the
#  reverse-masking bug ruling #3 closes.)
reset_case
FAKE_SUITE="${WORK}/fake-suite.sh"
cat > "$FAKE_SUITE" <<'EOF'
#!/usr/bin/env bash
echo "test gc_telemetry_proxy ... FAILED: connection refused (and: connection reset, broken pipe, timed out)"
exit 1
EOF
chmod +x "$FAKE_SUITE"
SOCK="$PRESENT_SOCK"; export ENVCMD="$FAKE_SUITE"
run_layer7 "$OUT" "$ERR"
assert_exit   "infra-words-still-fail-exit1"  1 "$RC"
assert_status "infra-words-still-fail-status" "STATUS=FAIL REASON=env-tests-failed" "$(cat "$OUT")"
reset_case

# === Browser E2E lanes (task #19, R-48) ========================================

# (B1) trigger-off → explicit SKIPPED-NO-DIFF child line; layer aggregate stays OK, exit 0.
#      Pins the "never an invisible if-branch" contract AND the no-ladder-edits claim
#      (SKIPPED-NO-DIFF must rank below the Rust suite's OK).
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" BROWSER_TRIGGER=0
run_layer7 "$OUT" "$ERR"
assert_exit   "browser-skip-exit0"     0 "$RC"
assert_status "browser-skip-child"     "STATUS=SKIPPED-NO-DIFF REASON=browser-e2e-no-diff" "$(cat "$OUT")"
assert_status "browser-skip-note"      "browser E2E skipped" "$(cat "$ERR")"
assert_status "browser-skip-env-ok"    "STATUS=OK REASON=env-tests-passed" "$(cat "$OUT")"
reset_case

# (B2) trigger-on, both suites green → OK with BOTH -passed REASONs, exit 0.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" BROWSER_TRIGGER=1 BROWSER_CMD="true"
run_layer7 "$OUT" "$ERR"
assert_exit   "browser-ok-exit0"       0 "$RC"
assert_status "browser-ok-env"         "STATUS=OK REASON=env-tests-passed" "$(cat "$OUT")"
assert_status "browser-ok-browser"     "STATUS=OK REASON=browser-e2e-passed" "$(cat "$OUT")"
reset_case

# (B3) trigger-on, browser suite non-zero → FAIL browser-e2e-failed (implementer lane,
#      exit 1) — the Rust suite's OK must not mask the browser failure.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" BROWSER_TRIGGER=1 BROWSER_CMD="false"
run_layer7 "$OUT" "$ERR"
assert_exit   "browser-fail-exit1"     1 "$RC"
assert_status "browser-fail-env-ok"    "STATUS=OK REASON=env-tests-passed" "$(cat "$OUT")"
assert_status "browser-fail-status"    "STATUS=FAIL REASON=browser-e2e-failed" "$(cat "$OUT")"
assert_status "browser-fail-artifacts" "test-results" "$(cat "$ERR")"
reset_case

# (B4) trigger-on, fingerprints incomplete → PRECONDITION_FAILURE dev-certs-missing
#      (exit 2, operator lane) — surfaced in PHASE 1, i.e. BEFORE either suite runs, so a
#      missing cert can never masquerade as a spec-timeout test FAIL.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" BROWSER_TRIGGER=1 BROWSER_CMD="true" FP_JSON="$INCOMPLETE_FP"
run_layer7 "$OUT" "$ERR"
assert_exit   "certs-missing-exit2"    2 "$RC"
assert_status "certs-missing-status"   "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
assert_status "certs-missing-token"    "REASON=dev-certs-missing" "$(cat "$OUT")"
if ! grep -q 'REASON=env-tests' "$OUT"; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[certs-missing-before-suites] env-test suite ran despite a Phase-1g precondition failure"); fi
reset_case

# (B5) trigger-on, no chromium under the browsers dir → PRECONDITION_FAILURE
#      playwright-browser-missing (exit 2, operator lane).
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="true" BROWSER_TRIGGER=1 BROWSER_CMD="true" PW_DIR="$EMPTY_PW_DIR"
run_layer7 "$OUT" "$ERR"
assert_exit   "pw-missing-exit2"       2 "$RC"
assert_status "pw-missing-status"      "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
assert_status "pw-missing-token"       "REASON=playwright-browser-missing" "$(cat "$OUT")"
reset_case

# (B6) trigger-on + env-tests red → browser suite NOT run: greppable stderr token, NO
#      browser STATUS line of any kind (Gate-1 Q1 — the layer is already FAIL; a browser
#      enum would misattribute the cause). Exit stays 1 (implementer lane).
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="false" BROWSER_TRIGGER=1 BROWSER_CMD="true"
run_layer7 "$OUT" "$ERR"
assert_exit   "env-red-exit1"          1 "$RC"
assert_status "env-red-env-fail"       "STATUS=FAIL REASON=env-tests-failed" "$(cat "$OUT")"
assert_status "env-red-not-run-note"   "browser-e2e-not-run:" "$(cat "$ERR")"
if ! grep -q 'REASON=browser-e2e' "$OUT"; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[env-red-no-browser-status] browser STATUS line emitted despite env-tests FAIL"); fi
reset_case

# (B7) REAL trigger predicate (direct call, no flow): __browser_e2e_triggered() against a
#      hand-written changed-files cache. The flow cases above use the force override for
#      hermeticity; THIS pins the actual array/ts-changed logic. _changed_helpers reads
#      ${DEVLOOP_TMP}/changed-files.layer-shared when DEVLOOP_LAYER is unset, and only
#      populates it when ABSENT — pre-writing it short-circuits any git derivation.
BTRIG_TMP="${WORK}/btrig-tmp"; mkdir -p "$BTRIG_TMP"
btrig() {  # $1=cache content (newline-separated changed paths); echoes predicate rc
  printf '%s\n' "$1" > "${BTRIG_TMP}/changed-files.layer-shared"
  ( cd "$REPO_ROOT" && env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TEST=1 \
      DEVLOOP_TMP="$BTRIG_TMP" \
      bash -c 'source scripts/layer7.sh >/dev/null 2>&1; set +e; __browser_e2e_triggered >/dev/null 2>&1; echo $?' )
}
assert_exit "btrig-mc-service-triggers"      0 "$(btrig 'crates/mc-service/src/actors/meeting.rs')"
assert_exit "btrig-mh-service-triggers"      0 "$(btrig 'crates/mh-service/src/webtransport/server.rs')"
assert_exit "btrig-proto-triggers"           0 "$(btrig 'proto/dark_tower/signaling/v1/signaling.proto')"
assert_exit "btrig-packages-triggers-via-ts" 0 "$(btrig 'packages/web-app/src/App.svelte')"
assert_exit "btrig-docs-only-no-trigger"     1 "$(btrig 'docs/runbooks/devloop-validation.md')"
assert_exit "btrig-unrelated-crate-no-trigger" 1 "$(btrig 'crates/dt-guard/src/main.rs')"

# === busy-tolerant setup (@operations condition 4) ============================
# `__dev_cluster_setup` must RETRY a `(busy)` result (another write holds the helper mutex —
# e.g. devloop.sh eager-setup), not short-circuit it to PRECONDITION_FAILURE. Exercise the
# function directly (sourced; the BASH_SOURCE guard keeps __layer7_main from running). Status
# reports "Setup in progress: false" so __poll_setup_idle returns immediately.
BUSY_CNT="${WORK}/setup-count"
mk_busy_dc() { # $1=path  $2=always-busy(1) | busy-then-ok(0)
  cat > "$1" <<EOF
#!/usr/bin/env bash
case "\$1" in
  status) echo "  Setup in progress:  false" >&2; exit 0 ;;
  setup)
    n=\$(( \$(cat "$BUSY_CNT" 2>/dev/null || echo 0) + 1 )); echo "\$n" > "$BUSY_CNT"
    if [[ "$2" -eq 1 || "\$n" -eq 1 ]]; then echo "ERROR: cluster busy: in-flight write (busy)" >&2; exit 1; fi
    exit 0 ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$1"
}
# busy-then-success → retry yields rc 0 (NOT precondition).
# NB: `set +e` after sourcing — layer7.sh enables set -e; in the real flow __dev_cluster_setup
# is called as `… || precondition_fail` (set -e suppressed in the function body), so we mirror
# that here to exercise its rc contract without an internal command-sub abort.
mk_busy_dc "${WORK}/dc-busy-ok" 0; : > "$BUSY_CNT"
rc=$( source "$LAYER7" >/dev/null 2>&1; set +e; DEV_CLUSTER="${WORK}/dc-busy-ok" DEVLOOP_SETUP_POLL_BUDGET=0 __dev_cluster_setup >/dev/null 2>&1; echo $? )
assert_exit "busy-setup-retries-to-success" 0 "$rc"
# persistent busy → non-zero (so the caller preconditions — never a silent pass).
mk_busy_dc "${WORK}/dc-busy-always" 1; : > "$BUSY_CNT"
rc=$( source "$LAYER7" >/dev/null 2>&1; set +e; DEV_CLUSTER="${WORK}/dc-busy-always" DEVLOOP_SETUP_POLL_BUDGET=0 __dev_cluster_setup >/dev/null 2>&1; echo $? )
[[ "$rc" -ne 0 ]] && PASS=$((PASS+1)) || { FAIL=$((FAIL+1)); FAILURES+=("[busy-setup-persistent-nonzero] persistent (busy) returned 0"); }

# === Phase-1f probe is IFS-immune (regression for the 2026-06-30 root-cause) ==========
# layer7.sh runs under `IFS=$'\n\t'` (no space). __wait_http_ready must split the multi-word
# $HTTP_PROBE ("curl -fsS …") into an ARRAY — an unquoted `$HTTP_PROBE` would NOT word-split
# under that IFS, so the whole string becomes one command name → exit 127 → a phantom
# observability-*-not-ready against a perfectly HEALTHY endpoint. (The fake probe used by the
# flow tests above is single-token, which never exercised this — the gap that hid the bug.)
# A multi-word probe that returns 0 ONLY if argv-split correctly: `true` ignores its args and
# exits 0; unsplit, "true --max-time 5" is command-not-found (127). budget=0 ⇒ one probe only.
rc=$( source "$LAYER7" >/dev/null 2>&1; set +e; HTTP_PROBE="true --max-time 5"; __wait_http_ready "http://endpoint/-/ready" 0 >/dev/null 2>&1; echo $? )
assert_exit "wait-http-ready-splits-multiword-probe-under-strict-IFS" 0 "$rc"

# NOTE: orchestrator integrity (does PRECONDITION_FAILURE/SKIPPED-NO-CLUSTER survive
# layer-all.sh's loop → LAYER_ALL_EXIT + summary cell + gate2 verdict + the rc FLOOR) is
# covered comprehensively by scripts/layer-all.test.sh (@test-owned, also wired into
# layer3.sh) — not duplicated here. This file stays focused on layer7.sh's own behavior.

# =============================================================================================
# === Phase 1h — per-run organization provisioning (R-7, story task #3) ========================
# =============================================================================================
# Everything below is hermetic: the provisioning transport is `setup.sh --provision-org <sub>`
# reached through the DEVLOOP_SETUP_SH seam, and the AC org-resolution probe through
# DEVLOOP_ORG_PROBE. No case needs a cluster, a database or a network.
#
# CASE COMMENTS DESCRIBE THE SHIPPED FORM: setup.sh passes values to psql as SQL on STDIN with
# `--set`, via `kubectl exec -i`. It is NOT `psql -c` with `:'var'` interpolation — verified on
# the live pod that `psql -v sub=abc -tAc "SELECT :'sub'"` fails with `syntax error at or near
# ":"`, i.e. `-c` performs no variable interpolation at all. A comment claiming :'sub' quoting
# as the control under a `-c` invocation would be naming a control that is not there.

# Shared lane assertions for a Phase-1h operator-lane failure. Every token gets all six:
# the exit code, the STATUS line, its own REASON, the stderr banner, and BOTH halves of the
# not-run proof.
#
# The not-run proof is the load-bearing half. R-7's hard requirement is that a provisioning
# failure reports on the OPERATOR lane and NEVER as a suite failure; asserting exit 2 alone
# does not show that, because a run which provisioned badly, ran the suites anyway, and then
# preconditioned would produce the same code. The STATUS-line grep proves no suite verdict was
# recorded; the markers prove neither suite BINARY was executed. Neither implies the other:
# a suite could run and have its status swallowed, or a status could be emitted without a run.
assert_org_precondition_lane() {  # $1=label  $2=expected REASON token
  local label="$1" token="$2"
  assert_exit   "${label}-exit2"   2 "$RC"
  assert_status "${label}-status"  "STATUS=PRECONDITION_FAILURE" "$(cat "$OUT")"
  assert_status "${label}-token"   "REASON=${token}" "$(cat "$OUT")"
  assert_status "${label}-banner"  "PRECONDITION_FAILURE:" "$(cat "$ERR")"
  if ! grep -q 'REASON=env-tests' "$OUT"; then PASS=$((PASS+1)); else
    FAIL=$((FAIL+1))
    FAILURES+=("[${label}-no-suite-status] an env-tests STATUS line was emitted despite a Phase-1h PRECONDITION_FAILURE — the operator lane must terminate BEFORE any suite verdict exists")
  fi
  assert_no_marker "${label}-env-suite-not-run"     "$MARKERS" 'ran.env-suite'
  assert_no_marker "${label}-browser-suite-not-run" "$MARKERS" 'ran.browser-suite'
}

# (P1) THE CASE THAT PINS R-7 — the Nth run does not reuse the (N-1)th run's organization.
#      layer7.sh is invoked TWICE in this one test process; the setup.sh stub appends the
#      subdomain it was actually passed to $PROVISIONED. Two non-empty lines, and they DIFFER.
#
#      A case asserting merely "an organization was provisioned" would pass on the pre-fix
#      code path too, since the defect was never "no org exists" — it was "the SAME org is
#      reused until its concurrent-meeting cap fills and run 2 gets a 403 attributed to the
#      diff". Distinctness across runs is the property, so distinctness is what is asserted.
#      Deliberately NOT reset_case'd between the two runs: the accumulation IS the fixture.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB"
run_layer7 "$OUT" "$ERR"
assert_exit "org-nth-run-first-exit0"  0 "$RC"
run_layer7 "$OUT" "$ERR"
assert_exit "org-nth-run-second-exit0" 0 "$RC"
mapfile -t PROVISIONED_SUBS < "$PROVISIONED"
assert_exit "org-nth-run-two-invocations" 2 "${#PROVISIONED_SUBS[@]}"
if [[ -n "${PROVISIONED_SUBS[0]:-}" && -n "${PROVISIONED_SUBS[1]:-}" ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-nth-run-non-empty] setup.sh --provision-org was invoked with an EMPTY subdomain (got: '${PROVISIONED_SUBS[0]:-}' / '${PROVISIONED_SUBS[1]:-}')")
fi
if [[ "${PROVISIONED_SUBS[0]:-x}" != "${PROVISIONED_SUBS[1]:-x}" ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-nth-run-distinct] run 2 provisioned the SAME organization as run 1 ('${PROVISIONED_SUBS[0]:-}') — R-7 is un-fixed: the per-run org would accumulate meetings toward its cap exactly as the shared 'demo' org did")
fi

# (P2) DNS-LABEL VALIDITY of the subdomain that was actually handed to setup.sh.
#      With `e2e-<16 hex>` (20 chars) this is a CONSTRUCTION check, not a sanitizer check —
#      there is no slug and no sanitizer to test. It still earns its place: the value crosses
#      into a SQL boundary and into packages/web-app/e2e/env.ts's required-var check, which
#      throws at Playwright CONFIG-LOAD time. A malformed subdomain would therefore surface as
#      `FAIL browser-e2e-failed`, exit 1, IMPLEMENTER lane — a provisioning-input defect billed
#      to the diff, the exact misattribution R-7 forbids.
#      Pattern copied VERBATIM from migrations/20250118000001_initial_schema.sql:16, and
#      byte-identity across all six encodings is enforced by
#      scripts/guards/simple/validate-subdomain-regex-sync.sh.
sub1="${PROVISIONED_SUBS[0]:-}"
if [[ "$sub1" =~ ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$ ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-subdomain-dns-label] provisioned subdomain '${sub1}' does not match the schema's subdomain_format CHECK — the INSERT would be rejected and env.ts would throw at Playwright config-load")
fi
if (( ${#sub1} <= 63 )); then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-subdomain-varchar63] provisioned subdomain is ${#sub1} chars, over the schema's VARCHAR(63)")
fi
# CHARSET, pinned under LC_ALL=C and therefore INDEPENDENT of the `[[ =~ ]]` check above:
# bracket ranges in a bash regex are collation-dependent, so under a UTF-8 locale `[a-z]`
# does not reliably mean ASCII a-z and the pattern LOOKS structural without being so. Deleting
# every ASCII-legal character must leave nothing — uppercase, whitespace and non-ASCII
# (e.g. Cyrillic `а` U+0430) all survive that deletion and fail here.
sub1_residue="$(printf '%s' "$sub1" | LC_ALL=C tr -d 'a-z0-9-')"
if [[ -z "$sub1_residue" ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-subdomain-charset-ascii] provisioned subdomain '${sub1}' contains characters outside ASCII [a-z0-9-] (residue: '${sub1_residue}') — AC lowercases nothing and the schema CHECK would reject the row")
fi

# (P3) STUB-CONSULTED on the SUCCESS path. Pairs with the loud `*)` catch-all in the fake
#      dev-cluster above: together they close the "green because nothing ran" hole from both
#      ends. Without this marker, DELETING Phase 1h entirely leaves every OK case green.
assert_marker "org-provision-setup-sh-consulted" "$MARKERS" 'ran.setup-sh'
assert_marker "org-provision-ac-probe-consulted" "$MARKERS" 'ran.org-probe'
# The probe asked about the org that was actually provisioned — not a regenerated one.
assert_status "org-probe-host-carries-run-org" "${sub1}." "$(cat "$PROBED_HOSTS")"
reset_case

# (P4) END-TO-END CONSUMPTION — the change is INERT unless BOTH suites receive the value.
#      Same-value-in-both is the assertion: an export reaching only the Rust suite would leave
#      the BROWSER suite on the stale org, and the browser suite is the one that actually
#      exhausts the cap (~7 meetings/run against 10).
reset_case
SOCK="$PRESENT_SOCK"
export ENVCMD="$SUITE_STUB" BROWSER_TRIGGER=1 BROWSER_CMD="$BROWSER_STUB"
run_layer7 "$OUT" "$ERR"
assert_exit "org-consumption-exit0" 0 "$RC"
mapfile -t CONSUMED_SUBS < "$PROVISIONED" 2>/dev/null || true
consumed="${CONSUMED_SUBS[0]:-}"
# NON-EMPTY GUARD — without it this whole case SELF-NEUTRALIZES (@test-reviewer F3, measured).
# Deleting Phase 1h entirely made 44 assertions red and NONE of them were this case's: the
# `mapfile` fails on the now-absent $PROVISIONED under the harness's `set +e`, `consumed`
# becomes "", `assert_status` is a SUBSTRING check so the empty needle matches anything, and
# both stubs write "<unset>" so the equality check compares two equal strings. The case whose
# entire job is "both suites received the value" reported success when neither did.
# Same shape as P1's org-nth-run-non-empty guard, and required for the same reason.
if [[ -n "$consumed" ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-consumption-subdomain-captured] no subdomain was recorded by the setup.sh stub, so every assertion below compares against an EMPTY needle and passes vacuously — Phase 1h did not run at all")
fi
# Independent of the file read: prove the provisioning seam was consulted in this very case.
assert_marker "org-consumption-setup-sh-ran"      "$MARKERS" 'ran.setup-sh'
assert_marker "org-consumption-env-suite-ran"     "$MARKERS" 'ran.env-suite'
assert_marker "org-consumption-browser-suite-ran" "$MARKERS" 'ran.browser-suite'
assert_status "org-consumption-rust-var" "$consumed" "$(cat "${WORK}/seen-env-test-subdomain")"
assert_status "org-consumption-browser-var" "$consumed" "$(cat "${WORK}/seen-e2e-subdomain")"
# Both halves of one value, never two knobs that happen to agree.
if [[ "$(cat "${WORK}/seen-env-test-subdomain")" == "$(cat "${WORK}/seen-e2e-subdomain")" ]]; then
  PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-consumption-both-equal] ENV_TEST_ORG_SUBDOMAIN and E2E_ORG_SUBDOMAIN disagree — the two suites would run against DIFFERENT organizations")
fi
reset_case

# (P5) LANE: `.cluster_name` absent from ports.json → org-provision-context-unresolved.
#      Fail-closed and PROBED, not assumed: the socket⇒kubeconfig coupling at
#      devloop.sh:519-520 is emergent, not guaranteed. Refusing to provision against an
#      unresolved context is what stops a silent write to the WRONG database, which would
#      surface much later as an unattributable auth failure in Phase 2.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" PORTS_JSON_OVERRIDE="$PORTS_NO_CLUSTER"
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-context-unresolved" "org-provision-context-unresolved"
# Nothing was provisioned: the check runs BEFORE the call, not after it.
assert_no_marker "org-context-unresolved-no-provision" "$MARKERS" 'ran.setup-sh'
reset_case

# (P6) LANE: the provisioning call exceeds its budget → org-provision-timeout.
#      A REAL `timeout` expiry (budget 1s, stub sleeps 3s), not a stub returning 124 directly:
#      that also pins that the bounded-call wrapper is actually present. Deterministic — the
#      stub cannot finish early. Before this budget existed a wedged `kubectl exec … psql`
#      hung the devloop with no lane at all, and under run-story hung the whole story.
reset_case
SOCK="$PRESENT_SOCK"
export ENVCMD="$SUITE_STUB" PROVISION_TIMEOUT=1 FAKE_PROVISION_SLEEP=3
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-timeout" "org-provision-timeout"
reset_case

# (P7) LANE: setup.sh exits non-zero → org-provision-failed.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROVISION_RC=1
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-failed" "org-provision-failed"
reset_case

# (P8) LANE: setup.sh exits ZERO but emits no `PROVISIONED_ORG ` line → still
#      org-provision-failed. The positive-match, fail-closed half of the same token, and the
#      one an exit-code-only check would wave straight through. setup.sh interleaves
#      log_step/log_info on stdout, so there is no "last line" to trust and absence of the
#      token must count as failure.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROVISION_TOKEN=0
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-no-token-line" "org-provision-failed"
# The distinguishing evidence: setup.sh DID run and DID exit 0 — this is not the P7 path.
assert_marker "org-no-token-line-setup-ran" "$MARKERS" 'ran.setup-sh'
reset_case

# (P9) LANE: AC not answering /health → ac-unreachable, ordered BEFORE the org probe.
#      The separation is the point: "AC is down" and "the org is not there" are different
#      operator actions, and collapsing them into one token would send someone hunting a
#      provisioning fault that does not exist.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_AC_READY=0
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-ac-unreachable" "ac-unreachable"
# Ordering proof: the org-resolution probe was never reached, so its result cannot be the
# thing being reported.
assert_no_marker "org-ac-unreachable-probe-not-reached" "$MARKERS" 'ran.org-probe'
reset_case

# (P10) LANE: AC healthy but returns 404 for the new subdomain's Host →
#       org-provision-unverified. 404 means AC's org_extraction fell through
#       organizations::get_by_subdomain's `WHERE subdomain = $1 AND is_active = true`, i.e.
#       provisioning reported success and the row is not usable. A provisioning fault, not a
#       flake — re-running cannot change it, which is why it is its own token.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=404
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-unverified" "org-provision-unverified"
assert_marker "org-unverified-probe-ran" "$MARKERS" 'ran.org-probe'
reset_case

# (P10b-d) LANE: every probe outcome that is NEITHER 401 nor 404 → ac-unreachable, NOT
#          org-provision-unverified. These three pin the discrimination P10 depends on: without
#          them a future `!= 401 → org-provision-unverified` simplification restores the bug
#          with the suite still green (measured — the branches had zero coverage).
#
#          Each of these once told the operator "this is a PROVISIONING fault, not a flake —
#          re-running will not change it" and sent them to inspect a row that was fine.

# 000 — curl transport failure or the probe's own --max-time 10 expiry. THE reachable one: the
# probe is a POST with its own budget, and /health passing milliseconds earlier does not bound
# the token path's latency (/health does not touch the database; this endpoint does).
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=000
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-probe-no-response" "ac-unreachable"
assert_status "org-probe-no-response-cause" "NO HTTP RESPONSE" "$(cat "$ERR")"
# The claim that must NOT appear: this is precisely the wrong instruction for a timeout.
assert_absent "org-probe-no-response-not-blamed-on-provisioning" \
  "re-running will not change it" "$(cat "$ERR")"
reset_case

# 503 — AC answered but failed internally. org_extraction returns 404 when an org does not
# resolve, so a 5xx says nothing about the row.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=503
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-probe-5xx" "ac-unreachable"
assert_status "org-probe-5xx-cause" "failed INTERNALLY" "$(cat "$ERR")"
reset_case

# 429 — NOT reachable from this probe today (issue_user_token's limiter is inside
# `if let Some(ref u) = user`, token_service.rs:226-246, and a non-existent email never reaches
# it; the other 429 sources are the service-credential and registration paths). There is
# deliberately no dedicated 429 ARM in layer7.sh — a branch that cannot fire is the dead-code
# defect this story refused to commit for the two dead REASON tokens. This case pins that the
# CATCH-ALL covers it anyway, which is what preserves the forward-compatible fail-safe: if AC
# ever gains an IP or endpoint limiter, it lands on the operator lane naming the code rather
# than as a phantom provisioning fault.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=429
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-probe-rate-limited" "ac-unreachable"
assert_marker "org-probe-rate-limited-probe-ran" "$MARKERS" 'ran.org-probe'
assert_status "org-probe-rate-limited-cause" "no AC limiter counts this request today" "$(cat "$ERR")"
reset_case

# 200 — the "should be impossible" code, and the one non-401 status that gets its OWN lane
# (@security F4, @operations Gate 3). AC ANSWERED and authenticated an account that cannot
# exist, so `ac-unreachable` — whose cause line closes "AC simply could not be asked" — would be
# false in the most load-bearing way available. This case therefore pins the SPLIT, not the
# catch-all: 2xx must NOT land on ac-unreachable.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=200
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-probe-unexpected-200" "ac-auth-bypass-signature"
assert_status "org-probe-unexpected-200-cause" "authentication-bypass signature" "$(cat "$ERR")"
# The two claims that must NOT appear: re-running is never the response to a bypass signature,
# and this lane must not inherit ac-unreachable's "could not be asked" framing. Without these
# the case passes on a message that says the opposite of what it should.
assert_absent "org-probe-unexpected-200-not-framed-as-unreachable" \
  "AC simply could not be asked" "$(cat "$ERR")"
assert_absent "org-probe-unexpected-200-not-told-to-rerun" \
  "re-running Layer 7 is a reasonable action" "$(cat "$ERR")"
reset_case

# 204 — the arm is the GLOB `2??`, NOT an equality on 200. A single 200 case cannot tell those
# apart, and an `== "200"` regression would silently reroute every OTHER 2xx into
# `ac-unreachable` — filing an authentication-bypass signature under a token that says the
# service could not be reached and tells the operator to re-run. That is the exact
# misattribution @security F4 split this lane to prevent, surviving in the one status nobody
# thought to try.
# (Driven by the 201 case below — @test and @security wrote this case concurrently at Gate 3
# and the duplicate 204 was collapsed into it rather than left as two spellings of one
# assertion. The reasoning is kept here because it is the longer form.)

# 302 — THE CATCH-ALL IS A GENUINE DEFAULT rather than an enumeration of the codes someone
# happened to think of. The 200 case carried this property until the 2xx lane split took it
# away, and nothing else replaced it: 000, 5xx and 429 all hit NAMED `case` arms, so with 200
# gone every remaining probe case matched an arm by name and deleting the `*)` default would
# have red'd nothing. Restored here on a code that matches no arm.
#
# Recorded rather than quietly re-pointed, because the mechanism generalises: a lane split can
# silently drop a property the split case was pinning WITHOUT any test being deleted or any
# assertion turning red. The suite stays green and the coverage is simply gone — the same
# vacuous-pass class as the `*) exit 0` catch-all this file already fixed, one level up.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=302
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-probe-unexpected-302" "ac-unreachable"
assert_status "org-probe-unexpected-302-cause" "the unexpected status 302" "$(cat "$ERR")"
reset_case

# 201 — the SECOND 2xx, pinning that the lane is a PATTERN (2??) and not an equality test on
# 200. A bypass that returns 201 is the same finding; an `== "200"` arm would file it under
# ac-unreachable and tell the operator to re-run.
reset_case
SOCK="$PRESENT_SOCK"; export ENVCMD="$SUITE_STUB" FAKE_PROBE_CODE=201
run_layer7 "$OUT" "$ERR"
assert_org_precondition_lane "org-probe-unexpected-201" "ac-auth-bypass-signature"
reset_case

# NO CASES for `helper-verb-unsupported` or `org-provision-helper-busy`, deliberately. Those
# were Design-A (dev-cluster helper verb) tokens for stale-helper / write-mutex conditions that
# CANNOT occur here: Phase 1h makes no socket call at all. A test for an unreachable failure
# mode is drift in the same way a missing test is, and worse — it documents a lane that does
# not exist and will be cited as evidence the lane was considered.

# (P11) GENERATOR — called directly on the sourced helper, the way :446-451/:461 already call
#       __dev_cluster_setup / __wait_http_ready. Structural assertions only.
#
#       COLLISION ARITHMETIC (why there is no 1000-draw statistical case): with a k-draw
#       birthday bound P ≈ k²/2^(2n) for an n-bit suffix, an 8-hex (32-bit) suffix gives
#       P ≈ 1000²/2^33 ≈ 1.2e-4 — roughly one unattributable red per 8500 runs, in a file that
#       runs on EVERY devloop and CI run, under ADR-0028's zero-retry policy. That is a test
#       that manufactures the flake class it exists to prevent. Resolved twice over: the suffix
#       is 16 hex (64 bits, `od -N8`), giving ≈ 1000²/2^65 ≈ 2.7e-14, AND the statistical
#       assertion is replaced by the structural ones below. Two-draws-distinct costs nothing
#       and cannot flake at that bound; the end-to-end distinctness property is pinned by P1.
gen1=$( source "$LAYER7" >/dev/null 2>&1; set +e; __generate_org_subdomain )
gen2=$( source "$LAYER7" >/dev/null 2>&1; set +e; __generate_org_subdomain )
# Shape: the `e2e-` prefix (which is ALSO what makes a collision with `devtest`/`demo`
# impossible by construction) plus exactly 16 lowercase hex characters. Anchored: an
# unanchored match would accept a longer value with a valid prefix.
if [[ "$gen1" =~ ^e2e-[0-9a-f]{16}$ ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-generator-shape] __generate_org_subdomain produced '${gen1}', expected e2e- followed by exactly 16 chars from [0-9a-f]")
fi
assert_exit "org-generator-length-20" 20 "${#gen1}"
if [[ "$gen1" != "$gen2" ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-generator-two-draws-distinct] two consecutive draws returned the same value ('${gen1}') — the generator is not drawing fresh entropy")
fi
# ENTROPY SOURCE, asserted structurally on the source text. A behavioural test cannot tell
# /dev/urandom from a seeded PRNG, but the distinction is exactly what matters: a predictable
# source (PID, $RANDOM, second-resolution time) collides with a concurrent devloop or a fast
# re-run, which reintroduces the cross-run org reuse R-7 removes.
if grep -q '/dev/urandom' "$LAYER7"; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-generator-urandom-source] layer7.sh no longer reads /dev/urandom — the subdomain suffix must not come from \$RANDOM, the PID or the clock")
fi
# `od -An -tx1 -N8` reads EXACTLY 8 bytes and exits. The idiomatic
# `tr -dc 'a-z0-9' </dev/urandom | head -c N` is a landmine under this script's
# `set -euo pipefail`: head closes the pipe, tr takes SIGPIPE (measured rc 141), pipefail
# propagates and set -e kills layer7.sh with NO STATUS line and NO lane.
if grep -q 'od -An -tx1 -N8 /dev/urandom' "$LAYER7"; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-generator-od-form] the generator no longer uses 'od -An -tx1 -N8 /dev/urandom'; a 'tr -dc … | head -c N' pipeline takes SIGPIPE under pipefail and exits layer7.sh with no lane")
fi

# (P11b) STATIC: the generator's locale pin is present. Mirrors setup.test.sh's
#        `provision-locale-pin` case and exists for the same reason — a harness cannot prove a
#        script pins its own locale, because the harness's own locale is whatever it inherits.
#        Bracket ranges in a bash regex are collation-dependent, so an unpinned `[0-9a-f]{16}`
#        LOOKS structural without being so. Not exploitable today (the input is `od -tx1`
#        output, ASCII by construction) — but that is PROVENANCE, and this postcondition exists
#        precisely because "valid by construction" can silently stop holding. A check that
#        catches provenance breaking must not itself be environment-dependent. Asserted
#        textually so it cannot be deleted as apparently-redundant. (@security F1)
#        EXTRACTOR SCOPED TO THE FUNCTION BODY AND TO STATEMENT LINES ONLY. The first version
#        of this case was `grep -A6 -F '__generate_org_subdomain() {' | grep -c 'local LC_ALL=C'`
#        and it asserted on a COMMENT: the pin sits ~9 lines below the opener behind its own
#        docstring, outside the -A6 window, while the docstring's own prose cites
#        "`local LC_ALL=C` above `subdomain_re`" and satisfied the count. Measured — deleting
#        the real statement left the suite at 182/0. A fixed -A<n> window is a line-count
#        coupling to a docstring, and the `^[[:space:]]*local LC_ALL=C$` anchor is what makes a
#        prose mention of the control stop counting as the control.
gen_locale_pin="$(awk '
  /^__generate_org_subdomain\(\) \{/            { f=1 }
  f && /^[[:space:]]*local LC_ALL=C$/           { c++ }
  f && /^\}$/                                   { exit }
  END                                           { print c+0 }
' "$LAYER7")"
if [[ "$gen_locale_pin" -ge 1 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-generator-locale-pin] __generate_org_subdomain no longer pins 'local LC_ALL=C', so its [0-9a-f] postcondition becomes collation-dependent and stops meaning ASCII")
fi

# (P12) NON-REGRESSION: max_participants_per_meeting stays the schema default of 100.
#       Grounded SOLELY on setup.sh's INSERT column list not naming the column — that is the
#       whole mechanism. GC computes LEAST($6, o.max_participants_per_meeting)
#       (gc-service/src/repositories/meetings.rs), so an INSERT that set the column to
#       anything lower would SILENTLY cap the response and red
#       crates/env-tests/tests/23_meeting_creation.rs:116 with a message pointing nowhere near
#       the cause. Static, textual and cheap; the live assertion stays where it already is.
#       EXTRACTOR SCOPED TO provision_run_org AND MATCHED ON A PREFIX, NOT ON THE FULL COLUMN
#       LIST. The first version of this case matched
#       `^INSERT INTO organizations \(subdomain, display_name, plan_tier, max_concurrent_meetings\)$`
#       and then asserted the forbidden column was absent from what it matched — a TAUTOLOGY.
#       Adding the column removes the line from the match set instead of showing up in it, so
#       the regression this case exists to catch was invisible BY CONSTRUCTION. It was doubly
#       blind: the pattern also matched seed_test_data's byte-identical column list at
#       setup.sh:709, so whichever site was mutated, the other stayed behind as a clean witness
#       and even the zero-match extractor guard never fired. Measured: mutating
#       provision_run_org's INSERT alone left the suite at 182/0; mutating seed_test_data's
#       alone, 182/0; only mutating BOTH red anything, and then it red the extractor guard
#       rather than the column assertion. The prefix match is what keeps a widened column list
#       INSIDE the haystack, and the function scope is what makes the case about the right
#       INSERT. setup.test.sh's `provision-happy-omits-max-participants` had it right — it reads
#       the SQL the psql stub actually received — which is why this slip was survivable.
SETUP_REAL="${REPO_ROOT}/infra/kind/scripts/setup.sh"
provision_insert="$(awk '
  /^provision_run_org\(\) \{/          { f=1 }
  f && /^INSERT INTO organizations /   { print }
  f && /^\}$/                          { exit }
' "$SETUP_REAL")"
if [[ -n "$provision_insert" ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[org-insert-column-list-extractor] could not find the organizations INSERT column list in setup.sh — this case is VACUOUS until the extractor is repaired (a zero-match grep that then finds no forbidden column would pass trivially)")
fi
assert_absent "org-insert-omits-max-participants" "max_participants_per_meeting" "$provision_insert"

# === SEAM SYMMETRY: both branches of the DEVLOOP_TEST block define the SAME variable set ======
# STATIC analysis of layer7.sh, not an execution case — which is the whole point. layer7.test.sh
# runs exclusively under DEVLOOP_TEST=1, so every flow case above takes the branch where the seam
# variables ARE bound. The file is therefore structurally INCAPABLE of catching a variable that
# was added to the test half and forgotten in the production `else` half: the suite goes green
# while every production run dies with `<VAR>: unbound variable` under `set -euo pipefail` — a
# bare non-zero exit with NO STATUS line and NO lane. That happened for real in this diff
# (SETUP_SH + ORG_PROBE, caught in review by @security, not by this suite).
#
# Same vacuous-pass class as the `*) exit 0` catch-all above, but living in the seam MECHANISM
# rather than in a case — so no gated case could ever fix it. A set-equality assertion covers
# every seam at once, including the next one someone adds, and subsumes the per-seam inertness
# pattern at :222-234 (which pins DEVLOOP_HELPER_SOCKET alone).
#
# SCOPE (@test ruling): this fixes the site THIS diff widened. The `DEVLOOP_TEST` sentinel idiom
# is NOT layer7-specific — scripts/workflow/run-story.sh, scripts/layer-all.sh,
# scripts/lang/_audit_gate.sh and scripts/audit-suppressions-check.sh all carry one (three with
# an `else`). A guard parsing bash branch structure across all four is a new guard with real
# surface, owned by whoever owns the seam convention; those four paths are named in docs/TODO.md
# so the class is picked up rather than re-discovered.
seam_vars_in_branch() {  # $1 = 'test' | 'prod'; echoes the sorted variable names
  awk -v want="$1" '
    /^if \[\[ "\$\{DEVLOOP_TEST:-\}" == "1" \]\]; then$/ { br="test"; next }
    br=="test" && /^else$/                               { br="prod"; next }
    (br=="test" || br=="prod") && /^fi$/                 { br=""; exit }
    br==want && /^[[:space:]]+[A-Z_][A-Z0-9_]*=/ {
      line=$0; sub(/^[[:space:]]+/,"",line); sub(/=.*/,"",line); print line
    }
  ' "$LAYER7" | sort -u
}
seam_test="$(seam_vars_in_branch test)"
seam_prod="$(seam_vars_in_branch prod)"
# Guard against the extractor itself silently matching nothing (the bug this case exists to stop,
# reappearing inside the case). Both branches must be non-empty before the comparison means anything.
if [[ -z "$seam_test" || -z "$seam_prod" ]]; then
  FAIL=$((FAIL+1)); FAILURES+=("[seam-symmetry-extractor] extracted no variables from one or both DEVLOOP_TEST branches of layer7.sh — the awk branch matcher has drifted from the file's structure; this case is vacuous until fixed")
elif [[ "$seam_test" == "$seam_prod" ]]; then
  PASS=$((PASS+1))
else
  FAIL=$((FAIL+1))
  FAILURES+=("[seam-symmetry] layer7.sh's DEVLOOP_TEST branches define DIFFERENT variable sets — a variable bound only in the test half aborts every PRODUCTION run with 'unbound variable' under set -u, with no STATUS line and no lane. Only in test half: [$(comm -23 <(printf '%s\n' "$seam_test") <(printf '%s\n' "$seam_prod") | tr '\n' ' ')] Only in prod half: [$(comm -13 <(printf '%s\n' "$seam_test") <(printf '%s\n' "$seam_prod") | tr '\n' ' ')]")
fi

report_results "scripts/layer7.test.sh"
