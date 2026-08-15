#!/usr/bin/env bash
# Layer 7 — Env-tests (integration) against the live Kind cluster.
#
# TWO Phase-2 suites since task #19 (R-48), run SEQUENTIALLY against the same cluster
# under the same single-attempt budget:
#   1. Rust env-tests (`cargo test -p env-tests --features all`) — ALWAYS attempted.
#   2. Browser E2E (`pnpm --filter @darktower/web-app test:e2e`) — DIFF-TRIGGERED (see
#      __BROWSER_E2E_TRIGGER_PATHS below), run only AFTER the Rust suite and only when it
#      passed (an env-test FAIL skips the browser suite with a loud stderr note — fixing
#      the Rust failures comes first; the browser suite runs on the retry).
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
# FOUR terminal lanes, each a STATUS line through the lifecycle (the browser suite adds
# REASON tokens — browser-e2e-passed / browser-e2e-failed / browser-e2e-no-diff /
# dev-certs-missing / playwright-browser-missing — but NO new lanes):
#   SKIPPED-NO-CLUSTER (exit 0) — socket ABSENT + `GITHUB_ACTIONS` set (CI, no cluster, not
#                                 provisionable). The ONLY clean-skip case; below OK. A
#                                 PRESENT socket runs even in CI (forward-compatible).
#   OK                 (exit 0) — every suite that ran was green (a diff-untouched browser
#                                 suite reports SKIPPED-NO-DIFF, which ranks below OK).
#   FAIL               (exit 1) — IMPLEMENTER lane: a suite returned non-zero on a
#                                 confirmed-healthy cluster (the diff's problem).
#   PRECONDITION_FAILURE (exit 2) — OPERATOR lane: a Phase-1 pre-suite step (helper
#                                 liveness / cluster bring-up / rebuild / health /
#                                 browser-suite preconditions) failed, OR a LOCAL run
#                                 with no/dead helper (a local devloop ALWAYS expects a
#                                 cluster — never a silent skip).
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
  # Browser-E2E seams (task #19, same trust boundary): the executed suite command
  # (DEVLOOP_BROWSER_E2E_CMD), the fingerprints-file path the Phase-1g gate reads
  # (DEVLOOP_FINGERPRINTS_JSON — pointing it elsewhere could fake the cert precondition),
  # and a force-run/force-skip trigger override (DEVLOOP_BROWSER_E2E_TRIGGER=1|0) so the
  # self-test pins both Phase-2 browser branches without depending on the repo's live diff.
  BROWSER_E2E_FINGERPRINTS="${DEVLOOP_FINGERPRINTS_JSON:-${__repo_root}/infra/docker/certs/fingerprints.json}"
  BROWSER_E2E_CMD_OVERRIDE="${DEVLOOP_BROWSER_E2E_CMD:-}"
  BROWSER_E2E_TRIGGER_OVERRIDE="${DEVLOOP_BROWSER_E2E_TRIGGER:-}"
  # Phase-1h org provisioning (R-7): the EXECUTED provisioning script. Same trust boundary as
  # DEVLOOP_DEV_CLUSTER_BIN — pointing it at a stub would fake the per-run organization, so the
  # suites would silently run against whatever org they last used and the cross-run meeting-cap
  # exhaustion R-7 removes would come straight back with every gate still green.
  # DELIBERATELY NOT A SEAM: the generated subdomain itself. An override there would let a
  # PINNED subdomain reach production, which is cross-run org REUSE — the exact defect — wearing
  # a config knob. The self-test observes the generated value through the stub instead.
  SETUP_SH="${DEVLOOP_SETUP_SH:-${__repo_root}/infra/kind/scripts/setup.sh}"
  # AC org-resolution probe (Phase-1h verification). Command-swap seam, gated exactly like
  # HTTP_PROBE: it must return an HTTP STATUS CODE on stdout, because the check discriminates
  # 401 (org resolved, credentials rejected) from 404 (org_extraction failed closed). A
  # pass/fail probe cannot express that distinction.
  #
  # DEVLOOP_ORG_PROBE_TIMEOUT is a PLAIN BUDGET KNOB, not a seam — same class as
  # DEVLOOP_ORG_PROVISION_TIMEOUT, so it is deliberately NOT DEVLOOP_TEST-gated: it redirects
  # nothing and executes nothing, it only moves a deadline. It exists because this cap was the
  # one hardcoded budget on the Phase-1h path while every sibling (DEVLOOP_HEALTH_BUDGET,
  # DEVLOOP_ORG_PROVISION_TIMEOUT, ENV_TEST_TIMEOUT, BROWSER_E2E_TIMEOUT) is tunable — and it
  # is the budget MOST likely to bite. The probe's latency is dominated by AC running bcrypt
  # UNCONDITIONALLY at cost 12 against a dummy hash for non-existent users (a deliberate
  # timing-attack mitigation: ac-service/src/services/token_service.rs, "Always run bcrypt to
  # prevent timing attacks"), so the cost is CPU-bound on a possibly-throttled Kind node, NOT
  # the two indexed lookups. Blowing this budget lands PRECONDITION_FAILURE / ac-unreachable,
  # which under scripts/workflow/run-story.sh is PIPELINE-PRECONDITION -> exit 2 and HALTS THE
  # WHOLE STORY for operator intervention. An operator who has diagnosed a slow node needs a
  # deadline they can move; without the knob the only remedy the runbook could offer was
  # "re-run".
  ORG_PROBE="${DEVLOOP_ORG_PROBE:-curl -s -o /dev/null -w %{http_code} --max-time ${DEVLOOP_ORG_PROBE_TIMEOUT:-10}}"
else
  DEV_CLUSTER="${__repo_root}/infra/devloop/dev-cluster"
  DEVLOOP_HELPER_SOCKET="/tmp/devloop/helper.sock"
  ENV_TEST_PORTS_JSON="/tmp/devloop/ports.json"
  HTTP_PROBE="curl -fsS -o /dev/null --max-time 5"
  BROWSER_E2E_FINGERPRINTS="${__repo_root}/infra/docker/certs/fingerprints.json"
  BROWSER_E2E_CMD_OVERRIDE=""
  BROWSER_E2E_TRIGGER_OVERRIDE=""
  SETUP_SH="${__repo_root}/infra/kind/scripts/setup.sh"
  ORG_PROBE="curl -s -o /dev/null -w %{http_code} --max-time ${DEVLOOP_ORG_PROBE_TIMEOUT:-10}"
fi

# NOT a DEVLOOP_TEST-gated seam, deliberately: PLAYWRIGHT_BROWSERS_PATH is Playwright's OWN
# runtime env var — the Phase-1g presence check MUST read the same location the runner will,
# or the gate and the runtime drift. Redirecting it cannot silently disable validation: an
# empty dir fails the gate loudly, and a dir without a real Chromium fails the suite loudly.
PLAYWRIGHT_BROWSERS_DIR="${PLAYWRIGHT_BROWSERS_PATH:-/opt/ms-playwright}"

# Paths of the suite output logs. Named `layer-7-*` so both fall under layer-all.sh's
# existing `rm -f ${DEVLOOP_TMP}/layer-*.log` per-run cleanup (security: token-bearing suite
# logs — RUST_LOG=trace, Playwright request lines etc. — must not persist across runs; the
# 700-perm DEVLOOP_TMP bounds them to same-user, the naming makes them transient). `tee`
# (below) truncates each on its Phase-2 run. NB: Playwright's OWN failure artifacts
# (traces/screenshots under packages/web-app/test-results/) live OUTSIDE DEVLOOP_TMP and are
# retained across runs by design — gitignored, local-only, and they CAN contain live tokens;
# see packages/web-app/e2e/README.md §Artifacts and runbook §6.7.
ENV_TEST_LOG="${DEVLOOP_TMP}/layer-7-env-test.log"
BROWSER_E2E_LOG="${DEVLOOP_TMP}/layer-7-browser-e2e.log"

# Wall-clock budgets, one PER SUITE (the browser suite must never eat the Rust suite's
# budget, or vice versa); Phase-1 bring-up is unbounded here (the helper enforces its own
# setup timeout). 600s matches the `all` feature envelope; the browser default covers the
# Vite dev-server start (60s webServer ceiling) + the specs' own 120s/test ceilings.
ENV_TEST_TIMEOUT="${DEVLOOP_ENV_TEST_TIMEOUT:-600}"
BROWSER_E2E_TIMEOUT="${DEVLOOP_BROWSER_E2E_TIMEOUT:-600}"

# -----------------------------------------------------------------------------
# Browser-E2E trigger (task #19, R-48 — Gate-1 Q4 ruling)
# -----------------------------------------------------------------------------

# Backend contract surfaces the BROWSER CLIENT observes. Mechanism (fail-toward-RUN): run
# the browser suite whenever the diff can change behavior a real browser client sees —
# over-triggering costs minutes, under-triggering silently un-gates the browser tier.
# Wider than the story text's literal "MC/AC/GC" on purpose (Gate-1 Q4, reviewer-ruled):
#   crates/ac-service/     — auth exchange the demo drives (sign-up/sign-in wire shapes)
#   crates/gc-service/     — meeting create/join HTTP contract + error envelopes
#   crates/mc-service/     — WebTransport signaling contract (JoinResponse/ErrorMessage)
#   crates/mh-service/     — MH WebTransport handshake the happy-path spec asserts for
#                            every media_servers URL (assertion (b))
#   crates/common/         — shared JWT/meeting-token types all four services serve from
#   crates/media-protocol/ — 42-byte frame format the browser SDK framing mirrors
# packages/** + the TS root manifests are covered by lang/ts/changed.sh (the TS-diff SSoT
# — not restated here), and proto/ is the wire contract itself.
# THE ONE ENCODING: __browser_e2e_triggered() AND the Phase-2 skip note both iterate this
# array — never restate the list in a hand-written string (@code-reviewer, Gate 1).
__BROWSER_E2E_TRIGGER_PATHS=(
  "proto/"
  "crates/ac-service/"
  "crates/gc-service/"
  "crates/mc-service/"
  "crates/mh-service/"
  "crates/common/"
  "crates/media-protocol/"
)

# True iff the browser E2E suite should run for this diff. Reads the SAME changed-files
# cache the rest of the layer uses (populated by _get_base_ref.sh in __layer7_main — no
# second git-diff derivation): first via lang/ts/changed.sh (packages/ + TS root files),
# then via diff_touches_path over __BROWSER_E2E_TRIGGER_PATHS.
# Args: (none)  Returns: 0 if triggered, 1 otherwise.
__browser_e2e_triggered() {
  # Self-test seam (DEVLOOP_TEST-gated at the top of this file): force-run/force-skip so
  # the flow tests don't depend on the repo's live diff. Empty = real predicate.
  if [[ -n "$BROWSER_E2E_TRIGGER_OVERRIDE" ]]; then
    [[ "$BROWSER_E2E_TRIGGER_OVERRIDE" == "1" ]]
    return
  fi
  if "${__layer7_dir}/lang/ts/changed.sh"; then
    return 0
  fi
  local p
  for p in "${__BROWSER_E2E_TRIGGER_PATHS[@]}"; do
    if diff_touches_path "$p"; then
      return 0
    fi
  done
  return 1
}

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
  # $HTTP_PROBE is a multi-word command string ("curl -fsS -o /dev/null --max-time 5").
  # This script runs under IFS=$'\n\t' (no space), so an UNQUOTED $HTTP_PROBE would NOT
  # word-split into argv — the whole string would be treated as a single command name and
  # fail with exit 127 (command-not-found) on every probe, manifesting as a phantom
  # observability-*-not-ready against a perfectly healthy endpoint. Split on space into an
  # array so the probe runs regardless of the script's IFS (mirrors the DEVLOOP_ENV_TEST_CMD
  # `IFS=' ' read -r -a` handling in __layer7_main).
  local -a probe; IFS=' ' read -r -a probe <<<"$HTTP_PROBE"
  until "${probe[@]}" "$url" >/dev/null 2>&1; do
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
# Per-run organization (Phase 1h, R-7)
# -----------------------------------------------------------------------------

# Generate the per-run organization subdomain: `e2e-<16 lowercase hex>`, 20 chars, no inputs.
#
# VALID BY CONSTRUCTION — no sanitizer, no validation, nothing a future input can break. The
# fixed `e2e-` prefix and a lowercase-hex suffix mean the result cannot begin or end with a
# hyphen, cannot contain uppercase, and is 20 chars against a 63-char limit. This function
# therefore carries NO copy of the schema's subdomain CHECK regex (@code-reviewer: generation
# over validation). migrations/20250118000001_initial_schema.sql:16 is the format SSoT;
# setup.sh's own check is defense-in-depth at the point the value meets SQL and must NOT be
# collapsed on DRY grounds (@dry-reviewer, @security) — provenance ("it happens to be locally
# generated") does not survive future edits; construction does.
#
# NO CLUSTER/STORY SLUG IN THE SUBDOMAIN, deliberately (@client). An earlier draft used
# `e2e-<sanitized-slug>-<hex>`, where the middle was valid by *sanitization* — a function of
# inputs this script does not control (story slug, task name, branch), and story titles in this
# repo already contain em-dashes and other non-ASCII. That gap lands on the WRONG LANE: a
# malformed subdomain is exported, reaches packages/web-app/e2e/env.ts's required-var check, and
# throws at Playwright CONFIG-LOAD time — so the browser suite exits non-zero and Phase 2 records
# `FAIL browser-e2e-failed`, exit 1, IMPLEMENTER lane. A provisioning-input defect billed to the
# diff is the exact misattribution R-7 forbids and R-4 exists to prevent. Human-readable
# identification lives in the org's `display_name`, which has no DNS-label constraint.
#
# RANDOMNESS — `od -An -tx1 -N8 /dev/urandom` reads EXACTLY 8 bytes and exits. The idiomatic
# `tr -dc 'a-z0-9' </dev/urandom | head -c N` is a landmine under this script's `set -euo
# pipefail`: head closes the pipe, tr takes SIGPIPE (141), pipefail propagates, and set -e kills
# layer7.sh with NO STATUS line and NO lane — a bare non-zero exit that never reaches
# precondition_fail. Same defect class this story's Deferred section records for `find` in
# run-story.sh (@security). `od` also emits lowercase hex by construction, so there is nothing to
# normalize between generation and INSERT (@security S12: no `${x,,}`, no trim, no truncate — a
# value that changed shape after validation would land under a different subdomain than the one
# exported to the suites, and both suites would then fail at AC token acquisition).
#
# 64 bits ⇒ P(collision) over k=1000 orgs accumulated on one cluster ≈ k²/2^65 ≈ 2.7e-14. A
# collision is loud regardless: the INSERT carries no ON CONFLICT, so it is a unique violation,
# never a silent reuse of an at-cap org.
# Args: (none)  Outputs: the subdomain on stdout.
__generate_org_subdomain() {
  # LC_ALL=C pins the bracket ranges in the postcondition below to ASCII. Bash applies a
  # `local LC_ALL=` assignment through setlocale and restores it on unwind, so the scope is
  # exactly this function. Same control, same reason, as infra/kind/scripts/setup.sh's
  # `local LC_ALL=C` above `subdomain_re` — under a UTF-8 locale `[0-9a-f]` is
  # collation-dependent and does not reliably mean ASCII, so the pattern LOOKS structural
  # without being so. Not exploitable today (the input is `od -tx1` output, ASCII by
  # construction) — but that is PROVENANCE, and this postcondition exists precisely because
  # "valid by construction" can silently stop holding. A check that catches provenance
  # breaking must not itself depend on the environment. (@security)
  local LC_ALL=C
  local suffix
  suffix="$(od -An -tx1 -N8 /dev/urandom | LC_ALL=C tr -d ' \n')"
  # POSTCONDITION ON THE ENTROPY READ. `set -e` does NOT catch a failed command substitution in
  # argument position — measured: `printf 'e2e-%s' "$(od -An -tx1 -N8 /nonexistent 2>/dev/null
  # | tr -d ' \n')"` yields the string `e2e-` with rc 0. That ends in a hyphen, so it fails the
  # schema CHECK, and "valid by construction" would have silently stopped holding while the
  # function still reported success. setup.sh's boundary check does catch it — which is that
  # layered check earning its keep — but it names the wrong problem at 3am ("invalid subdomain
  # at the SQL boundary" when the fault is "the entropy read failed").
  # NOT a copy of the schema regex, and deliberately so (@code-reviewer, @dry-reviewer ruled
  # against that): this asserts `[0-9a-f]{16}` — that the generator produced the entropy it
  # claims — and says nothing about DNS-label format. Generation-over-validation is preserved;
  # this is a postcondition on generation itself. (@operations)
  [[ "$suffix" =~ ^[0-9a-f]{16}$ ]] || return 1
  printf 'e2e-%s' "$suffix"
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

  # (h) Per-run organization (R-7). No production code ever marks a meeting ended, so an org's
  #     count of live meetings only CLIMBS toward max_concurrent_meetings: the browser suite
  #     creates ~7 meetings/run against demo's cap of 10, so a second run against the same
  #     cluster failed partway with a 403 the pipeline attributed to the code under test.
  #     Provisioning a FRESH org per run makes run N's verdict independent of runs 1..N-1 for
  #     everything keyed by org_id. (NOT for anything keyed above it — AC's registration limiter
  #     counts auth_events by IP, org-independent (user_service.rs:203-229). Not a regression,
  #     but the same mechanism one level up; see the story's Deferred section.)
  #
  #     ORDERING IS LOAD-BEARING: this runs AFTER (e) pods-healthy and (f) observability-HTTP,
  #     so a failure here can be attributed narrowly — the cluster and the NodePort path are
  #     already confirmed, which is what lets `ac-unreachable` and `org-provision-unverified` be
  #     different lanes instead of one ambiguous cell. Do not reorder without re-reading this.
  #
  #     OPERATOR LANE THROUGHOUT: every failure below is precondition_fail (exit 2). There is
  #     deliberately NO fallback to the static devtest/demo orgs — a fallback would report R-7
  #     fixed while the cluster kept accumulating meetings, i.e. a false green.
  t_step=$(layer_now)
  local run_org cluster_name provision_out provision_rc
  # THE ONE `|| true` IN THIS FILE, AND IT IS LOAD-BEARING — do not delete it to satisfy the
  # "no `|| true`" rule declared above (@operations F5). Without it, a missing or unreadable
  # ports.json makes jq exit non-zero and `set -e` kills layer7.sh on THIS line: no STATUS
  # line, no REASON token, no lane — the same shape as the stray-`fi` parse failure. With it,
  # the emptiness is caught by the `-z` check two lines down and routed to
  # org-provision-context-unresolved, which is a named operator lane. The `|| true` converts a
  # lane-less death into a diagnosed one; it swallows nothing, because the very next statement
  # inspects the value it produced.
  cluster_name="$(jq -r '.cluster_name // empty' "$ENV_TEST_PORTS_JSON" 2>/dev/null || true)"
  if [[ -z "$cluster_name" ]] || ! command -v kubectl >/dev/null 2>&1; then
    precondition_fail org-provision-context-unresolved \
      "cannot resolve a kubectl context for the devloop cluster (ports.json '.cluster_name'='${cluster_name:-<empty>}', kubectl $(command -v kubectl >/dev/null 2>&1 && echo present || echo MISSING)) — refusing to provision the per-run organization against an unknown database" \
      "this is a cluster-addressing problem, NOT a database problem: ensure 'dev-cluster setup' completed and wrote /tmp/devloop/ports.json, and that the container kubeconfig is mounted (re-run devloop.sh on the host)"
  fi
  # Folded into org-provision-failed with an explicit cause rather than a sixth token: the
  # runbook is scoped at five and this cause is too rare to earn a dedicated row (@operations).
  run_org="$(__generate_org_subdomain)" || precondition_fail org-provision-failed \
    "could not generate a per-run organization subdomain — the read from /dev/urandom did not yield 16 hex characters, so the generator's postcondition failed (this is an ENTROPY/environment fault, not a SQL or subdomain-format problem)" \
    "check /dev/urandom is readable in this container and that od/tr are present on PATH"
  # Emitted BEFORE the call, unconditionally: with no audit log on this route this is the only
  # record of which org the run used, and printing it first means it precedes every later
  # failure note in this same stream (incl. env-tests-failed / browser-e2e-failed).
  printf 'Layer7: provisioning per-run organization subdomain=%s cluster=%s\n' \
    "$run_org" "$cluster_name" >&2
  # DELIBERATE, NARROW DEPARTURE from this file's own "no post-hoc $? under set -e" rule, and
  # signed off as correct (@code-reviewer): `timeout`'s rc 124 must stay distinguishable from
  # generic non-zero, and the `if ! cmd` form collapses every non-zero into one branch. The
  # window is exactly one statement wide and the rc is branched three ways (124, then non-zero,
  # then the fail-closed PROVISIONED_ORG positive match). Do not "fix" this back to `if !`.
  #
  # SINGLE ATTEMPT, AND ANY FUTURE RETRY MUST REGENERATE `run_org` FIRST (@database). The INSERT
  # in provision_run_org() carries NO `ON CONFLICT`, deliberately — a duplicate subdomain must be
  # a detectable collision, not a silently reused at-cap org. So a retry loop wrapped around this
  # call that re-sends the SAME subdomain would hit the `organizations_subdomain_key` unique
  # violation on attempt 2 and surface as `org-provision-failed` — reading as a provisioning
  # DEFECT when the real fault was whatever made attempt 1 transient. Regenerate per attempt, or
  # do not retry. (This is the opposite direction from the "never regenerated" note at the export
  # below, which forbids re-deriving the subdomain AFTER a successful provision.)
  set +e
  provision_out="$(DT_CLUSTER_NAME="$cluster_name" \
    timeout "${DEVLOOP_ORG_PROVISION_TIMEOUT:-120}" "$SETUP_SH" --provision-org "$run_org" 2>&1)"
  provision_rc=$?
  set -e
  # Relay to STDERR: setup.sh's log_* write to STDOUT, and THIS script's stdout is the STATUS=
  # channel tee_collect_statuses/parse_status_line consume. Mirrors __dev_cluster_setup:311-312.
  printf '%s\n' "$provision_out" >&2
  # 124 branched FIRST — a timeout is a distinct operator action from a rejected INSERT.
  if (( provision_rc == 124 )); then
    precondition_fail org-provision-timeout \
      "provisioning the per-run organization did not complete within ${DEVLOOP_ORG_PROVISION_TIMEOUT:-120}s — the cluster passed pods-healthy and observability-HTTP, so a hang here points at the K8s API server or the postgres pod, not at the diff" \
      "kubectl -n dark-tower get pods; check the postgres-0 pod and its logs; raise DEVLOOP_ORG_PROVISION_TIMEOUT only after ruling out a wedged pod"
  fi
  # Positive-match, fail-closed (same discipline as __cluster_ready): absence of the token counts
  # as failure. setup.sh interleaves log_step/log_info on stdout, so there is no "last line".
  if (( provision_rc != 0 )) || ! grep -qF "PROVISIONED_ORG " <<<"$provision_out"; then
    precondition_fail org-provision-failed \
      "could not provision the per-run organization '${run_org}' — setup.sh --provision-org exited ${provision_rc}$( (( provision_rc == 0 )) && printf ' but emitted no PROVISIONED_ORG line, so the statement never confirmably ran (a success-looking no-op)') — the env-test and browser suites have no organization to run against" \
      "the relayed setup.sh output above carries the psql diagnostic; verify the postgres pod is up and the migrations ran ('kubectl -n dark-tower exec postgres-0 -c postgres -- psql -U darktower -d dark_tower -c \\\\dt')"
  fi
  # TWO TIMERS, NOT ONE (@observability F2). `provision-org` is closed HERE, before verification,
  # and `org-verify` is opened for the AC probes. Verification waits up to $obs_budget (300s) on
  # AC /health, so a single timer spanning both would bill a cold AC to PROVISIONING: an operator
  # reading `STEP=provision-org DURATION=200` goes looking at postgres-0, which is exactly where
  # the org-provision-timeout row also sends them. That is the timing surface committing the same
  # attribution error the lane tokens go to real lengths to avoid. The failure lanes already
  # discriminate the two halves; the timing line now does too, for the same reason.
  emit_step_duration provision-org "$t_step"
  t_step=$(layer_now)

  # Verify DIRECTLY (story R-7 / Task-3 notes). A provisioning failure surfaces at AC TOKEN
  # ACQUISITION, not at GC meeting creation: AC's org_extraction middleware resolves the Host
  # subdomain via organizations::get_by_subdomain (WHERE subdomain = $1 AND is_active = true) and
  # FAILS CLOSED with AcError::NotFound, so no token is minted and the request never reaches
  # POST /api/v1/meetings. A detector keyed on a GC meeting-creation status would be permanently
  # dead code.
  #
  # AC's own health is probed FIRST so "AC is down" and "the org is not there" get DIFFERENT
  # lanes rather than one ambiguous cell. Built on __wait_http_ready (not a raw curl) so it
  # inherits the DEVLOOP_TEST-gated $HTTP_PROBE seam — which makes this lane hermetically
  # testable — and the IFS=$'\n\t' array-splitting handling documented at :284-291, whose absence
  # would produce exit 127 on every probe and surface as a phantom ac-unreachable against a
  # perfectly healthy AC.
  # ABSENT AC URL IS A FAILURE, NOT A SKIP (@operations). This deliberately INVERTS the `-n`
  # shape used for Prometheus/Loki at 1f: those are genuinely optional (documented auto-skip
  # semantics, `--skip-observability` is a supported mode), so guarding them on presence is
  # correct. AC is NOT optional — both suites acquire their tokens through it — so the same
  # shape here would mean the opposite thing. Reachable, not theoretical: Phase 1d's
  # __env_test_export_urls returns 0 when ports.json EXISTS but lacks `.container_urls.ac`
  # (stale file, partial write, helper crashed mid-write), so `ports-json-missing` never trips
  # and ENV_TEST_AC_URL stays unset. Guarding on presence would then skip verification silently
  # and run the suites against an unverified org — both fail at AC token acquisition, and the
  # verdict is FAIL env-tests-failed, exit 1, IMPLEMENTER lane, consuming an attempt. A
  # provisioning-verification gap billed to the diff is the precise misattribution this task
  # exists to prevent.
  if [[ -z "${ENV_TEST_AC_URL:-}" ]]; then
    precondition_fail ac-unreachable \
      "ports.json has no '.container_urls.ac', so AC has no address and the per-run organization '${run_org}' cannot be verified — refusing to run the suites against an unverified org" \
      "re-run 'dev-cluster setup' so the helper rewrites /tmp/devloop/ports.json with a complete container_urls block; inspect it with: jq .container_urls /tmp/devloop/ports.json"
  fi
  # Verification runs UNCONDITIONALLY from here — there is no path to Phase 2 that skips it.
  __wait_http_ready "${ENV_TEST_AC_URL}/health" "$obs_budget" \
      || precondition_fail ac-unreachable \
        "AC did not answer ready (${ENV_TEST_AC_URL}/health != 2xx) within ${obs_budget}s — the per-run organization was provisioned, but AC cannot be asked whether it resolves, so a failure here is NOT attributable to provisioning" \
        "check the ac-service pod ('dev-cluster status'; kubectl -n dark-tower get pods) + its logs"
  # 401 vs 404 is the whole check. A deliberately NON-EXISTENT email means: org missing or
  # inactive -> 404 (org_extraction fails closed); org present and active -> user lookup misses
  # -> 401. Consumes NO rate-limit budget: token_service.rs:222-235 only rate-limits by_user
  # when the user exists, and the IP-keyed registration counter (user_service.rs:203-229) counts
  # only success=true events. A registering probe would instead spend from the very budget the
  # two suites share -- causing the flake it exists to prevent.
  #
  # NOT WRAPPED IN AN `if`, deliberately — see the "runs UNCONDITIONALLY" note above. An earlier
  # revision of this hunk guarded the probe on `[[ -n "$ENV_TEST_AC_URL" ]]`; inverting that to
  # the fail-closed check at :639 left its closing `fi` behind, which made this whole FILE
  # unparseable (`bash -n` → "syntax error near unexpected token `fi'"). That is worse than any
  # lane bug: bash rejects the script before line 1 runs, so layer7 exits 2 from the shell itself
  # with NO STATUS line, NO REASON token and NO lane — layer-all records UNKNOWN. layer7.test.sh
  # runs the real script for every flow case, so the suite catches this class immediately; keep
  # it wired (scripts/layer3.sh) and keep new branches balanced.
  local -a org_probe; IFS=' ' read -r -a org_probe <<<"$ORG_PROBE"
  local ac_authority probe_code
  ac_authority="${ENV_TEST_AC_URL#*://}"
  probe_code="$("${org_probe[@]}" -X POST \
    -H "Host: ${run_org}.${ac_authority}" \
    -H 'Content-Type: application/json' \
    --data '{"email":"layer7-provision-probe@invalid.test","password":"not-a-real-password"}' \
    "${ENV_TEST_AC_URL}/api/v1/auth/user/token" 2>/dev/null)" || probe_code="000"
  # DISCRIMINATE BY WHAT THE CODE ACTUALLY EVIDENCES (@operations F1+F2, @observability F1,
  # @semantic-guard, @code-reviewer, @security). Each token fires ONLY on the condition it names:
  #
  #   401  -> PASS. AC resolved the org and reached credential checking.
  #   404  -> org-provision-unverified. THE ONLY code that evidences a provisioning fault:
  #           org_extraction fell through get_by_subdomain's
  #           `WHERE subdomain = $1 AND is_active = true` and failed closed.
  #   2xx  -> ac-auth-bypass-signature. AC ANSWERED, and authenticated an account that cannot
  #           exist. Neither a provisioning fault nor an availability one. Own token (@security
  #           F4) — see the arm below for why the catch-all is the wrong home for it.
  #   *    -> ac-unreachable. The shared cause is "AC could not answer the question", which is
  #           that token's meaning. The observed code goes in the cause line.
  #
  # WHY THE CATCH-ALL IS ON ac-unreachable AND NOT ON unverified. An earlier revision had
  # `!= 401 -> org-provision-unverified`, whose remediation asserts with maximum confidence
  # "this is a PROVISIONING fault, not a flake — re-running will not change it". That sentence is
  # FALSE for a transport failure (`000`) and for a 5xx, and it sends the operator to inspect an
  # `organizations` row that is fine — the exact misattribution R-7 exists to remove, produced by
  # the check built to prevent it. Defaulting the unknown case to the SPECIFIC diagnosis was the
  # bug; defaulting it to "AC could not answer" is the honest one.
  #
  # NO DEDICATED 429 LANE, deliberately — and this is a CORRECTION, recorded rather than silently
  # applied. An earlier revision had one, justified by "repeated failed auth is exactly what AC's
  # 60s sliding-window limiter counts". That is FALSE for this probe, as the comment above already
  # implies: `issue_user_token`'s limiter is inside `if let Some(ref u) = user`
  # (token_service.rs:226-246) and returns AcError::TooManyRequests, so a deliberately
  # NON-EXISTENT email never reaches it. The other 429 sources are off this path entirely —
  # token_service.rs:61-68 is `issue_service_token` (client_credentials) and returns
  # AcError::RateLimitExceeded; user_service.rs:86 is registration, IP-keyed. AC has no
  # rate-limit middleware. So 429 CANNOT occur here today, and a dedicated LANE would have been a
  # TOKEN that cannot fire — the same dead-code defect this story refused to commit for
  # `helper-verb-unsupported` / `org-provision-helper-busy`. The catch-all still routes a future
  # 429 to ac-unreachable, so @security's forward-compatible fail-safe is preserved WITHOUT a
  # dead token: if AC ever gains an IP or endpoint limiter, it lands on the operator lane
  # naming the code, never as a phantom provisioning fault.
  #
  # PRECISION ON WHAT SURVIVED, because the sentence above used to overclaim (@code-reviewer N4).
  # What was deleted is the dedicated 429 LANE — its own REASON token, its own runbook row, its
  # own operator action. What REMAINS is a `429)` arm in the sub-cause `case` below, and that arm
  # genuinely cannot fire today. It is deliberate and it is NOT the dead-branch class, because it
  # claims no diagnosis: it selects explanatory TEXT inside a lane (`ac-unreachable`) that has
  # already been chosen by the catch-all, and the text it selects says exactly "if you are
  # reading this, AC has GAINED a limiter this probe does not model — update both." A dead LANE
  # asserts a wrong cause and sends an operator somewhere; a forward-compatible sub-cause string
  # costs one `case` arm and turns a future silent misfile into a self-describing one. Those are
  # different things and the earlier wording conflated them — worth correcting in place, since a
  # comment naming a property the code does not have is the exact failure mode this file is
  # otherwise careful about.
  #
  # The false justification is corrected rather than deleted because it was actively dangerous:
  # a reader citing it as evidence that this probe spends failed-auth budget would "fix" the
  # probe by registering a user — spending the IP-KEYED REGISTRATION budget the two suites
  # genuinely DO share, and causing the very flake the 401 probe was chosen to avoid.
  if [[ "$probe_code" == "404" ]]; then
    precondition_fail org-provision-unverified \
      "AC is healthy and ANSWERED, but will not resolve the organization just provisioned: a token request with Host '${run_org}.${ac_authority}' returned 404. AC's org_extraction fell through organizations::get_by_subdomain's 'WHERE subdomain = \$1 AND is_active = true' — the row is missing or inactive despite provisioning reporting success" \
      "this is a PROVISIONING fault, not a flake — re-running will not change it. Check the row: kubectl -n dark-tower exec postgres-0 -c postgres -- psql -U darktower -d dark_tower -c \"SELECT subdomain, is_active FROM organizations WHERE subdomain = '${run_org}'\""
  fi
  # 2xx BEFORE the catch-all, on its OWN token (@security F4). This is the one non-401 status
  # that is neither "AC could not answer" nor "the org is missing": AC answered, and the answer
  # is that it AUTHENTICATED an account that cannot exist. Filing it under ac-unreachable would
  # repeat, one arm over, the precise defect F1 fixed — that token's cause line closes with "AC
  # simply could not be asked", which is FALSE here in the most load-bearing way available.
  # `ac-unreachable` is NOT a neutral bucket: it makes a positive claim about what did not
  # happen, so an unknown-but-answering code must not default there any more than it may default
  # to org-provision-unverified. A security signature filed under a name that says "the service
  # was unreachable" is a misattribution an operator acts on at 3am — and this one is acted on
  # by re-running, which is the single worst response available.
  #
  # NOT the dead-token class this file refuses elsewhere (see the 429 note above): unlike 429,
  # which is unreachable by inspection, a 2xx here is not provably impossible. It is the
  # signature of a regression in AC's user-token path, which is exactly a mode worth naming.
  if [[ "$probe_code" == 2?? ]]; then
    precondition_fail ac-auth-bypass-signature \
      "the organization-resolution probe for Host '${run_org}.${ac_authority}' got ${probe_code} — a SUCCESS status for an account that CANNOT EXIST. The probe posts 'layer7-provision-probe@invalid.test', an address nothing registers, so any 2xx means AC accepted credentials for a non-existent user. This is NOT a provisioning fault and NOT an availability problem: AC answered, promptly, with the wrong answer. It is an authentication-bypass signature in AC's user-token path" \
      "TREAT THIS AS A SECURITY FINDING. Do NOT re-run to see if it clears, and do NOT raise any timeout — neither changes what AC just did, and a second green run would bury it. Note the org-resolution question this probe exists to ask went UNANSWERED, so the per-run organization is also unverified. Capture the full response before anything is redeployed or torn down: curl -i -X POST -H \"Host: ${run_org}.${ac_authority}\" -H 'Content-Type: application/json' --data '{\"email\":\"layer7-provision-probe@invalid.test\",\"password\":\"not-a-real-password\"}' \"${ENV_TEST_AC_URL}/api/v1/auth/user/token\" ; then grab AC's logs (kubectl -n dark-tower logs -l app=ac-service --tail=200) and escalate to the auth-controller and security owners"
  fi
  if [[ "$probe_code" != "401" ]]; then
    # Sub-cause detail only — ONE token, because the operator action is the same in kind ("find
    # out why AC could not answer"), while the specific hint differs. No 2xx arm here: 2xx is
    # claimed by its own lane above and can never reach this case.
    local probe_detail
    case "$probe_code" in
      000) probe_detail="got NO HTTP RESPONSE at all (transport failure, or the probe's own ${DEVLOOP_ORG_PROBE_TIMEOUT:-10}s cap elapsed). AC answered /health moments ago, so a cold or CPU-starved AC exceeding the probe budget on its FIRST token request is the likely cause: /health is a trivial handler, whereas this endpoint does two indexed lookups AND — dominating both — an UNCONDITIONAL bcrypt verify at cost 12 against a dummy hash, which AC runs even for a non-existent account to keep the path constant-time. That is CPU-bound, so a throttled or contended node stretches it. Raise DEVLOOP_ORG_PROBE_TIMEOUT (default 10) rather than re-running blind if this recurs" ;;
      5??) probe_detail="got ${probe_code}: AC answered but failed INTERNALLY. Since org_extraction returns 404 when an organization does not resolve, a 5xx is not evidence about the per-run organization — look at AC's own dependencies (database, JWKS)" ;;
      429) probe_detail="got 429. NOTE: no AC limiter counts this request today (a non-existent email never reaches the by-user window), so this means AC has GAINED a limiter that layer7.sh's probe does not model — both the probe and this token's guidance need updating" ;;
      *)   probe_detail="got the unexpected status ${probe_code}, which is neither 401 (org resolved) nor 404 (org not resolved). AC did not answer the question this probe asks" ;;
    esac
    precondition_fail ac-unreachable \
      "the organization-resolution probe for Host '${run_org}.${ac_authority}' ${probe_detail}. This is NOT a provisioning fault — the per-run organization may well be fine; AC simply could not be asked" \
      "check the ac-service pod ('dev-cluster status'; kubectl -n dark-tower get pods) and its logs ('kubectl -n dark-tower logs -l app=ac-service --tail=100'). Unlike org-provision-unverified, re-running Layer 7 is a reasonable action here"
  fi

  # ONE generation site, ONE exported value — the subdomain actually provisioned and verified,
  # never regenerated. The browser half is exported with its E2E_* siblings below (Phase 2).
  export ENV_TEST_ORG_SUBDOMAIN="$run_org"
  emit_step_duration org-verify "$t_step"

  # (g) Browser-E2E trigger + preconditions (task #19, R-48). Trigger-gated: these checks
  #     run ONLY when the browser suite will actually run — an untriggered diff must not
  #     be able to red on a workstation without Playwright installed. Both checks are
  #     deterministic pre-suite facts, so a failure is the OPERATOR lane (exit 2) — a
  #     missing cert/browser can only produce cryptic spec timeouts if allowed through,
  #     which would masquerade as a test FAIL (the exact misattribution the hard
  #     requirement forbids). AC/GC/Prometheus reachability is deliberately NOT re-checked
  #     here: steps (e)/(f) above already confirmed the very endpoints the browser suite's
  #     global-setup probes (same ports.json-derived URLs — see the Phase-2 export).
  t_step=$(layer_now)
  local browser_e2e=skip
  if __browser_e2e_triggered; then
    browser_e2e=run
    # WebTransport cert fingerprints: without BOTH pinned hashes the browser refuses the
    # MC/MH self-signed certs (serverCertificateHashes) and every join spec can only time
    # out at the handshake. Writer: scripts/generate-dev-certs.sh; same canonical keys the
    # Vite loader (packages/web-app/vite/fingerprints.ts) and the suite's global-setup read.
    if ! jq -e '(.MC_CERT_SHA256 // "" | length > 0) and (.MH_CERT_SHA256 // "" | length > 0)' \
        "$BROWSER_E2E_FINGERPRINTS" >/dev/null 2>&1; then
      precondition_fail dev-certs-missing \
        "WebTransport dev cert fingerprints missing/incomplete at ${BROWSER_E2E_FINGERPRINTS} (need MC_CERT_SHA256 + MH_CERT_SHA256) — the browser cannot pin the MC/MH certs, so the browser E2E suite would only time out" \
        "run scripts/generate-dev-certs.sh on the host, then restart the Vite dev server if one is running (fingerprints are read at Vite config time)"
    fi
    # Playwright Chromium: same location the runner itself resolves (PLAYWRIGHT_BROWSERS_PATH).
    if ! compgen -G "${PLAYWRIGHT_BROWSERS_DIR}/chromium*" >/dev/null; then
      precondition_fail playwright-browser-missing \
        "no Playwright Chromium installation under ${PLAYWRIGHT_BROWSERS_DIR} — the browser E2E suite cannot launch a browser" \
        "run 'pnpm exec playwright install chromium' (the devloop image is expected to bake it; see infra/devloop/Dockerfile)"
    fi
  fi
  emit_step_duration browser-e2e-preconditions "$t_step"

  # ======================= PHASE 2 — run the suites (no grep) ================
  # Cluster is confirmed healthy. ANY non-zero from a suite is a test FAIL (exit 1,
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

  # --- Browser E2E (task #19, R-48): sequentially AFTER the Rust suite, same cluster ---
  if [[ "$browser_e2e" == "skip" ]]; then
    # Explicit, never an invisible if-branch: the child STATUS line records WHY the
    # browser suite did not run (SKIPPED-NO-DIFF ranks below OK — a green Rust suite
    # still aggregates to OK). The note iterates the ONE trigger-path encoding (space-
    # joined in a subshell: this script's IFS=$'\n\t' would make ${arr[*]} newline-join).
    local trigger_list
    trigger_list="$(IFS=' '; printf '%s' "${__BROWSER_E2E_TRIGGER_PATHS[*]}")"
    printf 'Layer7: browser E2E skipped — no diff under its trigger surfaces (packages/** + TS root manifests via lang/ts/changed.sh, plus: %s).\n' \
      "$trigger_list" >&2
    emit_status SKIPPED-NO-DIFF browser-e2e-no-diff | tee_collect_statuses
  elif [[ "$rc" -ne 0 ]]; then
    # Shared single-attempt budget (Gate-1 Q1 ruling): with the Rust suite already FAIL,
    # running the browser suite against the same possibly-diff-broken backend adds
    # attribution noise, not signal. Loud + greppable (stable `browser-e2e-not-run:`
    # token — runbook §6.7/§8); deliberately NO STATUS line — the layer is already FAIL
    # and a SKIPPED-* enum would misstate the cause as a diff/cluster condition.
    printf 'Layer7: browser-e2e-not-run: env-tests failed first (shared single-attempt budget) — fix the env-test failures; the browser suite runs on the retry.\n' >&2
  else
    # Suite command seam: same DEVLOOP_TEST trust boundary as DEVLOOP_ENV_TEST_CMD above.
    local -a browser_cmd
    if [[ "${DEVLOOP_TEST:-}" == "1" && -n "$BROWSER_E2E_CMD_OVERRIDE" ]]; then
      IFS=' ' read -r -a browser_cmd <<<"$BROWSER_E2E_CMD_OVERRIDE"
    else
      browser_cmd=(pnpm --filter @darktower/web-app test:e2e)
    fi

    # Suite URLs: the SAME ports.json-derived values the Rust suite just used (exported
    # once in Phase 1d) — one read, zero gate-vs-suite drift. E2E_* feeds the Playwright
    # harness (packages/web-app/e2e/env.ts); VITE_* points the dev proxy the browser
    # traffic flows through at the same AC/GC endpoints.
    export E2E_AC_URL="$ENV_TEST_AC_URL" E2E_GC_URL="$ENV_TEST_GC_URL"
    # The per-run org (Phase 1h), DERIVED from the one generation site — not regenerated.
    # E2E_BASE_URL is deliberately NOT exported: packages/web-app/e2e/env.ts derives it from
    # this value and hard-throws if both are set and the host label disagrees, so exporting it
    # here would trip that check on the first mismatch. One knob, one encoding.
    export E2E_ORG_SUBDOMAIN="$ENV_TEST_ORG_SUBDOMAIN"
    export VITE_AC_PROXY_TARGET="$ENV_TEST_AC_URL" VITE_GC_PROXY_TARGET="$ENV_TEST_GC_URL"
    if [[ -n "${ENV_TEST_PROMETHEUS_URL:-}" ]]; then
      export E2E_PROMETHEUS_URL="$ENV_TEST_PROMETHEUS_URL"
    fi

    local browser_rc
    t_step=$(layer_now)
    set +e
    timeout "$BROWSER_E2E_TIMEOUT" "${browser_cmd[@]}" 2>&1 | tee "$BROWSER_E2E_LOG"
    browser_rc=${PIPESTATUS[0]}
    set -e
    emit_step_duration browser-e2e "$t_step"

    if [[ "$browser_rc" -eq 0 ]]; then
      emit_status OK browser-e2e-passed | tee_collect_statuses
    else
      # No grep (same rule as the Rust suite). Playwright's retained failure artifacts
      # live OUTSIDE DEVLOOP_TMP and can contain live tokens — named here so triage finds
      # them, flagged so nobody promotes them off the workstation.
      printf 'Layer7: browser E2E suite exited %s (full output: %s; Playwright traces/screenshots: packages/web-app/test-results/ — retained on failure, may contain live tokens, local-only).\n' \
        "$browser_rc" "$BROWSER_E2E_LOG" >&2
      emit_status FAIL browser-e2e-failed | tee_collect_statuses
    fi
  fi
}

# Run only when executed, not when sourced by the self-test (which exercises the pure
# helpers directly).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  __layer7_main "$@"
fi
