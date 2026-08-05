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
  *)           exit 0 ;;
esac
EOF
chmod +x "$FAKE_DC"

# ports.json fixture (all five container_urls, incl. optional Loki).
PORTS="${WORK}/ports.json"
cat > "$PORTS" <<'EOF'
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
  */ready)   [[ "${FAKE_LOKI_READY:-1}" == "1" ]] ;;
  *)         true ;;
esac
EOF
chmod +x "$FAKE_PROBE"

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
      DEVLOOP_PORTS_JSON="${PORTS}" \
      DEVLOOP_ENV_TEST_CMD="${ENVCMD:-}" \
      DEVLOOP_HTTP_PROBE="${FAKE_PROBE}" \
      DEVLOOP_HEALTH_BUDGET=0 \
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
        ENVCMD CI_FLAG BROWSER_TRIGGER BROWSER_CMD FP_JSON PW_DIR
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
resolve_socket_var() {  # $1 = DEVLOOP_TEST value ("" leaves it unset); echoes the resolved path
  ( cd "$REPO_ROOT" && env -i PATH="$PATH" HOME="$HOME" \
      ${1:+DEVLOOP_TEST="$1"} \
      DEVLOOP_TMP="${WORK}/devloop-tmp" \
      DEVLOOP_HELPER_SOCKET="$PRESENT_SOCK" \
      DEVLOOP_DEV_CLUSTER_BIN="${FAKE_DC}" \
      bash -c 'source "'"$LAYER7"'" >/dev/null 2>&1; printf "%s" "${DEVLOOP_HELPER_SOCKET}"' )
}
reset_case
assert_status "socket-seam-inert-without-devloop-test" \
  "/tmp/devloop/helper.sock" "$(resolve_socket_var "")"
assert_status "socket-seam-honored-under-devloop-test" \
  "$PRESENT_SOCK" "$(resolve_socket_var 1)"
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

report_results "scripts/layer7.test.sh"
