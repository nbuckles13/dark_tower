#!/usr/bin/env bash
# Layer 7 — Env-tests (integration) against the live Kind cluster.
#
# ALWAYS-RUN (ADR-0033 §3; user ruling 2026-06-26, task #56): Layer 7 attempts the
# env-test suite on every devloop regardless of diff surface — business-logic changes
# can break integration even when no infra/proto file is touched. The ONLY clean SKIP is
# CI with no devloop helper (no cluster, none provisionable). A LOCAL devloop with a
# missing/dead helper is NOT a skip — it is a LOUD PRECONDITION_FAILURE (a local run always
# expects a cluster), the four-way gate in __layer7_main below. (Socket-absence ALONE does
# not skip; the GITHUB_ACTIONS discriminator splits CI-skip from local-loud — that split is
# what keeps the 6-week silent-skip from relocating to "helper-not-running".)
#
# FOUR terminal lanes, each a STATUS line through the lifecycle:
#   SKIPPED-NO-CLUSTER (exit 0) — socket ABSENT + `GITHUB_ACTIONS` set (CI, no cluster, not
#                                 provisionable). The ONLY clean-skip case; below OK. A
#                                 PRESENT socket runs even in CI (forward-compatible).
#   OK                 (exit 0) — env-test suite ran green.
#   FAIL               (exit 1) — IMPLEMENTER lane: suite returned non-zero on a
#                                 confirmed-healthy cluster (the diff's problem).
#   PRECONDITION_FAILURE (exit 2) — OPERATOR lane: a Phase-1 pre-suite step (helper
#                                 liveness / cluster bring-up / rebuild / health) failed,
#                                 OR a LOCAL run with no/dead helper (a local devloop
#                                 ALWAYS expects a cluster — never a silent skip).
#
# TWO-PHASE classifier (task #56 ruling #3 — the suite-output log-grep is RETIRED):
#   Phase 1 (pre-suite, deterministic) is the ONLY infra lane. It brings the cluster to
#   a confirmed-healthy state. Phase 2 runs the suite; ANY non-zero is a test FAIL. We do
#   NOT grep the suite output for "connection refused" etc. — that would let a real test
#   failure whose output happens to contain an infra phrase silently escape as infra
#   (the 6-week silent-skip bug, one level down). Load-bearing asymmetry: when uncertain,
#   default to FAIL/loud, never infra/swallow.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.7 (Layer 7 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'

__layer7_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
__repo_root="$(cd "${__layer7_dir}/.." && pwd)"
# shellcheck source=lang/_common.sh
source "${__layer7_dir}/lang/_common.sh"
# shellcheck source=lang/_changed_helpers.sh
source "${__layer7_dir}/lang/_changed_helpers.sh"

# --- Test seams, DEVLOOP_TEST-gated (security; same class as LAYER_SCRIPT_DIR) ---
# These three env overrides REDIRECT or EXECUTE the cluster machinery: the dev-cluster
# binary (DEVLOOP_DEV_CLUSTER_BIN), the helper-socket presence predicate
# (DEVLOOP_HELPER_SOCKET), and the suite's port map (DEVLOOP_PORTS_JSON). Honoring them
# outside the test sentinel would be a silent-validation-disable lever (e.g. point the
# socket at a nonexistent path → mis-gate; point the binary at a stub → fake cluster). So
# they are consulted ONLY under DEVLOOP_TEST=1; in production the paths are the fixed
# ADR-0030 canonical ones (the dev-cluster client itself hardcodes /tmp/devloop/{helper.sock,
# ports.json}). @operations/@security boundary call — mirrors the LAYER_SCRIPT_DIR gate;
# assert_no_ci_sentinel_leak independently reds a DEVLOOP_TEST leak into CI.
if [[ "${DEVLOOP_TEST:-}" == "1" ]]; then
  DEV_CLUSTER="${DEVLOOP_DEV_CLUSTER_BIN:-${__repo_root}/infra/devloop/dev-cluster}"
  DEVLOOP_HELPER_SOCKET="${DEVLOOP_HELPER_SOCKET:-/tmp/devloop/helper.sock}"
  ENV_TEST_PORTS_JSON="${DEVLOOP_PORTS_JSON:-/tmp/devloop/ports.json}"
  # HTTP readiness probe for Phase-1f (command-swap seam — security: gated like the others;
  # the test injects a fake probe keyed on FAKE_{PROM,LOKI}_READY so the obs gate is hermetic).
  HTTP_PROBE="${DEVLOOP_HTTP_PROBE:-curl -fsS -o /dev/null --max-time 5}"
else
  DEV_CLUSTER="${__repo_root}/infra/devloop/dev-cluster"
  DEVLOOP_HELPER_SOCKET="/tmp/devloop/helper.sock"
  ENV_TEST_PORTS_JSON="/tmp/devloop/ports.json"
  HTTP_PROBE="curl -fsS -o /dev/null --max-time 5"
fi

# Path of the suite output log. Named `layer-7-*` so it falls under layer-all.sh's existing
# `rm -f ${DEVLOOP_TMP}/layer-*.log` per-run cleanup (security: a token-bearing suite log —
# RUST_LOG=trace etc. — must not persist across runs; the 700-perm DEVLOOP_TMP bounds it to
# same-user, the naming makes it transient). `tee` (below) truncates it each Phase-2 run.
ENV_TEST_LOG="${DEVLOOP_TMP}/layer-7-env-test.log"

# Wall-clock budget for the suite itself (Phase 2); Phase-1 bring-up is unbounded here
# (the helper enforces its own setup timeout). 600s matches the `all` feature envelope.
ENV_TEST_TIMEOUT="${DEVLOOP_ENV_TEST_TIMEOUT:-600}"

# -----------------------------------------------------------------------------
# Cluster-availability gate (the env-awareness the always-run model requires)
# -----------------------------------------------------------------------------

# True iff the helper socket is present (a manageable cluster exists / can be brought up).
# This is ONLY the socket-presence predicate — it does NOT decide skip-vs-loud. The CALLER
# (__layer7_main) splits its FALSE result on GITHUB_ACTIONS: socket ABSENT + CI ⇒ clean SKIP
# (SKIPPED-NO-CLUSTER, exit 0); socket ABSENT + local ⇒ LOUD PRECONDITION_FAILURE (exit 2,
# never a silent skip). Socket PRESENT ⇒ cluster EXPECTED ⇒ a bring-up failure (incl. a
# present-but-dead socket — the helper was supposed to be running) is the OPERATOR lane.
# Args: (none)  Returns: 0 if the helper socket is present, 1 otherwise.
__env_test_cluster_available() {
  [[ -S "$DEVLOOP_HELPER_SOCKET" ]]
}

# -----------------------------------------------------------------------------
# Operator lane — Phase-1 precondition failure (exit 2 via the lifecycle)
# -----------------------------------------------------------------------------

# Emit the OPERATOR lane: a greppable PRECONDITION_FAILURE banner on stderr (the 3am
# anchor naming the cause + fix) PLUS a STATUS line through tee_collect_statuses so the
# layer aggregate is PRECONDITION_FAILURE and the lifecycle maps it to exit 2. No `trap`
# disarming — the lifecycle's LAYER=/DURATION= anchors still emit on this lane.
# Args: $1=reason-token  $2=human-cause  $3=remediation
# Outputs: stderr banner; stdout STATUS line. Does NOT return (exits via lifecycle).
precondition_fail() {
  local token="$1" cause="$2" fix="$3"
  printf 'PRECONDITION_FAILURE: %s\n  Fix: %s\n' "$cause" "$fix" >&2
  emit_status PRECONDITION_FAILURE "$token" | tee_collect_statuses
  # The layer_lifecycle_begin EXIT trap recomputes the exit code from the aggregated
  # enum (PRECONDITION_FAILURE → 2); the literal arg here is irrelevant.
  exit 0
}

# -----------------------------------------------------------------------------
# dev-cluster status parsing (§C.1 — no --json on the helper; ADR-0030 out of scope)
# -----------------------------------------------------------------------------

# Combined stdout+stderr of `dev-cluster status`. The helper client renders the structured
# health as human-readable lines on STDERR (stdout is empty for status), so we merge with
# 2>&1 and grep (§C.1; the ADR-0030 helper has no --json — out of scope). Its exit code is 0
# whenever the QUERY ran, regardless of health — so the exit code is NOT a readiness signal;
# the grep is.
#
# COUPLED: these greps parse the EXACT field lines `infra/devloop/dev-cluster` prints
# (`Cluster exists:`, `Pods healthy:`, `Setup in progress:` — its status-summary block,
# ~line 454). Keep the two in sync; a matching cross-ref comment lives there. The self-test's
# fake dev-cluster is a frozen copy of this wording and can't catch real-client drift — this
# comment is the insurance (@operations).
__dev_cluster_status_text() {
  "$DEV_CLUSTER" status 2>&1
}

# True iff the cluster is READY to run the suite. FAIL-CLOSED, positive-match (@operations
# condition 2): requires the POSITIVE tokens `Cluster exists: true` AND `Pods healthy: true`
# AND `Setup in progress: false`. Absence of any positive token — pods down OR the client's
# wording drifted — counts as NOT-ready. This is the load-bearing direction: drift/down
# degrades to "not ready → setup → (still not ready) PRECONDITION_FAILURE" (operator lane),
# and structurally CANNOT yield a false-ready that runs the suite against a broken cluster.
# Args: (none)  Returns: 0 if ready, 1 otherwise.
__cluster_ready() {
  local out
  out="$(__dev_cluster_status_text)" || return 1
  grep -qE 'Cluster exists:[[:space:]]+true' <<<"$out" \
    && grep -qE 'Pods healthy:[[:space:]]+true' <<<"$out" \
    && grep -qE 'Setup in progress:[[:space:]]+false' <<<"$out"
}

# True while a cluster setup is in flight (poll target).
# Args: (none)  Returns: 0 if setup in progress, 1 otherwise.
__cluster_setup_in_progress() {
  local out
  out="$(__dev_cluster_status_text)" || return 1
  grep -qE 'Setup in progress:[[:space:]]+true' <<<"$out"
}

# Block until an in-flight setup finishes (or a bounded timeout). Polls every 10s.
# Args: (none)  Returns: 0 when no longer in progress, 1 on timeout.
__poll_setup_idle() {
  local waited=0 budget="${DEVLOOP_SETUP_POLL_BUDGET:-900}"
  while __cluster_setup_in_progress; do
    if (( waited >= budget )); then
      return 1
    fi
    sleep 10
    waited=$(( waited + 10 ))
  done
  return 0
}

# Wait (bounded) for the cluster to become healthy. `dev-cluster rebuild-all` issues
# `kubectl rollout restart` and RETURNS BEFORE the new pods finish rolling out, so a
# one-shot readiness check immediately after rebuild spuriously reports "not ready"
# (observed on the first live run — STEP=rebuild then an instant cluster-unhealthy). Poll
# every 10s up to a budget so the rollout has time to settle; only a genuinely stuck
# cluster trips PRECONDITION_FAILURE.
# Args: $1=budget-seconds (default 300)  Returns: 0 when ready, 1 on timeout.
__wait_cluster_ready() {
  local waited=0 budget="${1:-300}"
  until __cluster_ready; do
    if (( waited >= budget )); then
      return 1
    fi
    sleep 10
    waited=$(( waited + 10 ))
  done
  return 0
}

# Bounded poll of an HTTP readiness endpoint (Phase-1f observability gate — task #56 user
# ruling (a)). Mirrors __wait_cluster_ready's poll shape but for an HTTP /ready. Uses the
# DEVLOOP_TEST-gated $HTTP_PROBE (fixed curl in production; a fake in the self-test). A 2xx
# answer (curl -f exits 0) ends the poll. budget=0 ⇒ one probe then timeout (no sleep).
# Args: $1=url  $2=budget-seconds (default 300)  Returns: 0 ready / 1 timeout.
__wait_http_ready() {
  local url="$1" budget="${2:-300}" waited=0
  until $HTTP_PROBE "$url" >/dev/null 2>&1; do
    if (( waited >= budget )); then
      return 1
    fi
    sleep 5
    waited=$(( waited + 5 ))
  done
  return 0
}

# `dev-cluster setup`, BUSY-TOLERANT (@operations condition 4). The helper serializes
# writes behind a mutex; if another write holds it (typically devloop.sh's eager-setup still
# running when Layer 7 starts) `setup` returns non-zero with a `(busy)` error. That is a
# transient race, NOT a setup failure — short-circuiting it to PRECONDITION_FAILURE would
# spuriously red the operator lane. So on a busy result, wait the in-flight write out
# (poll-setup-idle) and retry ONCE; a non-busy non-zero is a genuine setup failure.
# Args: (none)  Returns: setup's exit code (0 on success).
__dev_cluster_setup() {
  local out rc
  out="$("$DEV_CLUSTER" setup 2>&1)"; rc=$?
  printf '%s\n' "$out" >&2
  if (( rc != 0 )) && grep -qiE '\(busy\)|in.?flight write|another write' <<<"$out"; then
    __poll_setup_idle || return 1
    "$DEV_CLUSTER" setup; return $?
  fi
  return "$rc"
}

# -----------------------------------------------------------------------------
# Env-test URL wiring (Phase 1d)
# -----------------------------------------------------------------------------

# Export ENV_TEST_{AC,GC,PROMETHEUS,GRAFANA,LOKI}_URL from /tmp/devloop/ports.json's
# .container_urls.* (the in-container service URLs the helper writes; ADR-0030). Loki is
# optional — omit the var if empty so env-tests fall back to their auto-skip path.
# Args: (none)  Returns: 0 on success, 1 if ports.json is missing/unreadable.
__env_test_export_urls() {
  local ports="$ENV_TEST_PORTS_JSON"  # DEVLOOP_TEST-gated at the top of this file
  [[ -f "$ports" ]] || return 1

  local ac gc prom graf loki
  ac="$(jq -r '.container_urls.ac // empty' "$ports")" || return 1
  gc="$(jq -r '.container_urls.gc // empty' "$ports")"
  prom="$(jq -r '.container_urls.prometheus // empty' "$ports")"
  graf="$(jq -r '.container_urls.grafana // empty' "$ports")"
  loki="$(jq -r '.container_urls.loki // empty' "$ports")"

  [[ -n "$ac" ]]   && export ENV_TEST_AC_URL="$ac"
  [[ -n "$gc" ]]   && export ENV_TEST_GC_URL="$gc"
  [[ -n "$prom" ]] && export ENV_TEST_PROMETHEUS_URL="$prom"
  [[ -n "$graf" ]] && export ENV_TEST_GRAFANA_URL="$graf"
  [[ -n "$loki" ]] && export ENV_TEST_LOKI_URL="$loki"
  return 0
}

# Per-step timing (observability — `LAYER=7 STEP=<name> DURATION=<s>`) is provided by
# `_common.sh::layer_now` + `emit_step_duration`. Using those helpers (rather than a direct
# timestamp call in this layer body) keeps the `_layer_skeleton.test.sh` "lifecycle owns
# timestamps" invariant intact.

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

__layer7_main() {
  layer_lifecycle_begin 7

  # --- Environment / cluster gate (FAIL-CLOSED) -------------------------------
  # The clean-skip lane (SKIPPED-NO-CLUSTER, exit 0) is reachable ONLY when a cluster is
  # both absent AND not expected (CI). A LOCAL devloop ALWAYS expects a cluster, so a
  # missing/dead helper is LOUD (PRECONDITION_FAILURE, exit 2), never a silent skip — the
  # exact exit-0 silent-skip hole this task closes (task #56; @operations + @security).
  #
  # ORDER MATTERS — socket-presence is checked FIRST (the `[[ -S ]]` is not a dev-cluster
  # call): a PRESENT socket → proceed REGARDLESS of GITHUB_ACTIONS (forward-compatible — if
  # CI ever provisions a helper+cluster, env-tests run with zero code change; team-lead's
  # "GITHUB_ACTIONS + *no usable cluster*" qualifier). GITHUB_ACTIONS only disambiguates the
  # socket-ABSENT case: CI (legit no cluster → skip) vs local (forgot the helper → loud).
  #
  #   socket PRESENT + reachable      → run (Phase 1 below), regardless of CI
  #   socket PRESENT but DEAD          → PRECONDITION_FAILURE helper-unreachable (exit 2)
  #   socket ABSENT + GITHUB_ACTIONS   → SKIPPED-NO-CLUSTER (exit 0) — the ONLY skip
  #   socket ABSENT + local            → PRECONDITION_FAILURE local-helper-not-running (exit 2)
  if __env_test_cluster_available; then
    # Cluster expected. Liveness probe: a stale/unconnectable socket (helper crashed →
    # `dev-cluster status` connection-refused) is an OPERATOR problem, NOT a skip.
    if ! "$DEV_CLUSTER" status >/dev/null 2>&1; then
      precondition_fail helper-unreachable \
        "helper socket is present at ${DEVLOOP_HELPER_SOCKET} but the helper is unreachable (crashed / stale socket → connection refused)" \
        "re-run devloop.sh on the host to restart the helper; check /tmp/devloop/helper.log + helper-stderr.log"
    fi
    # alive → fall through to Phase 1.
  elif [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    # CI, no cluster, not provisionable this round → the ONLY clean skip.
    printf 'Layer7: GITHUB_ACTIONS set + no devloop cluster (provisioning out of scope); skipping env-tests cleanly.\n' >&2
    emit_status SKIPPED-NO-CLUSTER no-cluster-ci | tee_collect_statuses
    exit 0  # lifecycle maps SKIPPED-NO-CLUSTER → exit 0
  else
    # Local, no helper socket → a cluster was expected but the helper isn't running. LOUD
    # operator lane, NEVER a skip. (To run only layers 1-6, invoke the individual
    # scripts/layerN.sh; `layer-all` legitimately demands a cluster for Layer 7.)
    precondition_fail local-helper-not-running \
      "no devloop helper socket at ${DEVLOOP_HELPER_SOCKET} and this is not CI — a local devloop expects a running cluster" \
      "start the devloop cluster (run devloop.sh on the host); to run only layers 1-6 invoke the individual scripts/layerN.sh"
  fi

  # Populate the changed-files cache + emit the BASE_REF= anchor (used by Phase 1b's
  # diff_touches_path). Discard the stdout sha — we only need the side effects.
  "${__layer7_dir}/lang/_get_base_ref.sh" >/dev/null

  # ======================= PHASE 1 — pre-suite (operator lane) ===============
  # Every failure below is PRECONDITION_FAILURE (exit 2). This is the ONLY infra lane.

  # (a) Cluster readiness / setup.
  local t_step
  t_step=$(layer_now)
  if ! __cluster_ready; then
    if __cluster_setup_in_progress; then
      __poll_setup_idle || precondition_fail cluster-setup-failed \
        "cluster setup was in progress but did not complete within the poll budget" \
        "check 'dev-cluster status' and /tmp/devloop/helper.log; re-run devloop.sh on the host if the helper is wedged"
    fi
    if ! __cluster_ready; then
      __dev_cluster_setup || precondition_fail cluster-setup-failed \
        "dev-cluster setup failed — cluster could not be brought to a ready state" \
        "inspect /tmp/devloop/helper.log + 'dev-cluster status'; this is an environment problem, not a code regression"
    fi
  fi
  emit_step_duration cluster-ready "$t_step"

  # (b) Infra-change detection: a touched infra/kind/ skeleton is stale ⇒ rebuild it.
  t_step=$(layer_now)
  if diff_touches_path "infra/kind/"; then
    echo "Layer7: infra/kind/ changed — tearing down + re-creating the cluster skeleton. Triggering files:" >&2
    __changed_files | grep '^infra/kind/' >&2 || true
    "$DEV_CLUSTER" teardown || precondition_fail cluster-rebuild-failed \
      "dev-cluster teardown failed during infra/kind rebuild" \
      "inspect /tmp/devloop/helper.log; the cluster may be in a partially-torn-down state"
    __dev_cluster_setup || precondition_fail cluster-rebuild-failed \
      "dev-cluster setup failed after an infra/kind teardown" \
      "inspect /tmp/devloop/helper.log + 'dev-cluster status'"
  fi
  emit_step_duration infra-change "$t_step"

  # (c) Rebuild service images so the suite runs against the current diff.
  t_step=$(layer_now)
  "$DEV_CLUSTER" rebuild-all || precondition_fail cluster-rebuild-failed \
    "dev-cluster rebuild-all failed — service images could not be rebuilt/redeployed" \
    "inspect /tmp/devloop/helper.log; check image build output and pod status"
  emit_step_duration rebuild "$t_step"

  # (d) Wire the env-test service URLs from the helper's port map.
  t_step=$(layer_now)
  __env_test_export_urls || precondition_fail ports-json-missing \
    "/tmp/devloop/ports.json is missing or unreadable — cannot resolve service URLs" \
    "ensure 'dev-cluster setup' completed; the helper writes ports.json on success"
  emit_step_duration ports-json "$t_step"

  # (e) Wait (bounded) for the cluster to become healthy AFTER rebuild, BEFORE the suite.
  #     This is what makes Phase 2's "any non-zero == the diff's fault" sound — a green
  #     pre-check means a subsequent failure cannot be blamed on cluster bring-up. We POLL
  #     (not one-shot) because rebuild-all returns before the rollout-restarted pods settle.
  t_step=$(layer_now)
  __wait_cluster_ready "${DEVLOOP_HEALTH_BUDGET:-300}" || precondition_fail cluster-unhealthy \
    "cluster did not become healthy within ${DEVLOOP_HEALTH_BUDGET:-300}s after rebuild-all (pods not ready) — refusing to run the suite against a sick cluster" \
    "'dev-cluster status' for the not-ready pods; check pod logs (rollout may be wedged, e.g. ImagePullBackOff/CrashLoopBackOff)"
  emit_step_duration health-confirm "$t_step"

  # (f) Observability-stack HTTP readiness (Phase-1e completion; task #56 user ruling (a)).
  #     Pods-healthy (1e) confirms the containers are UP; THIS confirms the observability HTTP
  #     endpoints the SUITE probes are READY — closing the Phase-1-vs-Phase-2 gap where a cold
  #     observability stack flaked a Phase-2 test (the cold-Loki flake from the first live run).
  #     PER-PROBE disposition (@semantic-guard: NOT a blanket `|| true` that could swallow a hard
  #     fail): Prometheus HARD — `30_observability.rs` one-shot-`.expect()`s an `up{}` query, so a
  #     cold Prometheus flakes it independently of Loki → PRECONDITION_FAILURE; Loki SOFT — the
  #     crate's declared optional-Loki/auto-skip semantics + ~15s cold-start, so warm-or-WARN
  #     (a loud, self-attributing breadcrumb; never a silent swallow). Grafana SKIPPED — no suite
  #     queries its HTTP API; its init TCP check is covered by pods-healthy. Probes the SAME
  #     ENV_TEST_*_URL the suite will use (no gate-vs-suite URL drift). Bounded by DEVLOOP_HEALTH_BUDGET.
  t_step=$(layer_now)
  local obs_budget="${DEVLOOP_HEALTH_BUDGET:-300}"
  if [[ -n "${ENV_TEST_PROMETHEUS_URL:-}" ]]; then
    __wait_http_ready "${ENV_TEST_PROMETHEUS_URL}/-/ready" "$obs_budget" \
      || precondition_fail observability-prometheus-not-ready \
        "Prometheus did not answer ready (${ENV_TEST_PROMETHEUS_URL}/-/ready != 2xx) within ${obs_budget}s — the metrics tests one-shot-query it, so a cold Prometheus would flake them" \
        "check the prometheus pod ('dev-cluster status'; kubectl -n monitoring get pods) + its logs"
  fi
  if [[ -n "${ENV_TEST_LOKI_URL:-}" ]]; then
    __wait_http_ready "${ENV_TEST_LOKI_URL}/ready" "$obs_budget" \
      || printf 'WARN LOKI_NOT_READY_AFTER=%ss — proceeding; test_all_services_have_logs_in_loki WILL fail and that failure is the observability stack, NOT your diff. See docs/TODO.md §Env-Test Resilience (is_loki_available retry/conditional-skip).\n' "$obs_budget" >&2
  fi
  emit_step_duration observability-ready "$t_step"

  # ======================= PHASE 2 — run the suite (no grep) =================
  # Cluster is confirmed healthy. ANY non-zero from the suite is a test FAIL (exit 1,
  # implementer lane).
  #
  # DEVLOOP_ENV_TEST_CMD is a test seam that SWAPS THE EXECUTED COMMAND, so (per security)
  # it is honored ONLY under DEVLOOP_TEST=1 — the same trust boundary as the audit-
  # suppressions overrides. assert_no_ci_sentinel_leak (run at the top of layer-all.sh /
  # layer3.sh) hard-fails the pipeline if DEVLOOP_TEST leaks into CI, so this seam can never
  # redirect the real suite in production. Outside the test sentinel the override is ignored.
  local -a env_test_cmd
  if [[ "${DEVLOOP_TEST:-}" == "1" && -n "${DEVLOOP_ENV_TEST_CMD:-}" ]]; then
    # Split on spaces only (IFS is \n\t here); the seam takes a simple command or a
    # path to a fake suite script.
    IFS=' ' read -r -a env_test_cmd <<<"$DEVLOOP_ENV_TEST_CMD"
  else
    env_test_cmd=(cargo test -p env-tests --features all)
  fi

  local rc
  set +e
  timeout "$ENV_TEST_TIMEOUT" "${env_test_cmd[@]}" 2>&1 | tee "$ENV_TEST_LOG"
  rc=${PIPESTATUS[0]}
  set -e

  if [[ "$rc" -eq 0 ]]; then
    emit_status OK env-tests-passed | tee_collect_statuses
  else
    # No grep: a non-zero suite on a confirmed-healthy cluster is the diff's problem.
    # (timeout's 124 included — a hang on a healthy cluster is a test problem.)
    printf 'Layer7: env-test suite exited %s (full output: %s).\n' "$rc" "$ENV_TEST_LOG" >&2
    emit_status FAIL env-tests-failed | tee_collect_statuses
  fi
}

# Run only when executed, not when sourced by the self-test (which exercises the pure
# helpers directly).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  __layer7_main "$@"
fi
