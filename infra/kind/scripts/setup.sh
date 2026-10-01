#!/usr/bin/env bash
#
# setup.sh — bring the local Dark Tower Kind environment up to date, the host
# one-stop entry (ADR-0038 step 3):
#
#   1. infra/kind/scripts/provision.sh  — the PLATFORM (kind cluster, Calico,
#      namespaces, Secret/TLS material), rebuilt only when its blueprint changed
#   2. infra/kind/scripts/deploy.sh     — the APPLICATION: content-tagged images,
#      the migration Job, the ONE environment root
#      (infra/kubernetes/overlays/kind/), rolling only what changed
#   3. host conveniences: Telepresence, port-forwards, access info
#
# It is NOT what a devloop gate runs: Layer 7 runs the helper's `provision` and
# `deploy` verbs (ADR-0030), which call provision.sh and deploy.sh directly.
#
# Prerequisites:
#   - kind: https://kind.sigs.k8s.io/docs/user/quick-start/#installation
#   - kubectl: https://kubernetes.io/docs/tasks/tools/
#   - podman (preferred) or docker
#
# EXECUTION CONTEXT — read before adding a mode:
#   Every mode in this script is HOST-ONLY and may use kind/podman/docker, EXCEPT
#   --provision-org, which is the one mode invoked from INSIDE the devloop
#   container (by scripts/layer7.sh Phase 1h). That container has kubectl and a
#   cluster kubeconfig but has NO kind, podman or docker — see ADR-0030. So the
#   --provision-org path must touch nothing but ${KUBECTL}, and must return from
#   main() BEFORE provision.sh/deploy.sh, which require kind and a container
#   runtime.
#
# Environment variables:
#   DT_CLUSTER_NAME    Cluster name (default: dark-tower)
#   DT_PORT_MAP        Path to shell-sourceable port variable file
#   DT_HOST_GATEWAY_IP Host-gateway IP for devloop advertise addresses (optional)
#
# Usage:
#   ./infra/kind/scripts/setup.sh [OPTIONS]
#
# Options:
#   --yes                    Auto-answer yes to all interactive prompts
#                            (provision's rebuild prompt)
#   --provision-org <sub>    Create one fresh organization and exit. Requires
#                            DT_CLUSTER_NAME. Container-runnable (see EXECUTION
#                            CONTEXT above). Prints one line:
#                              PROVISIONED_ORG org_id=<uuid> subdomain=<sub>
#   --help                   Show this help message
#
# To redeploy after a code/config change: ./infra/kind/scripts/deploy.sh
# See ADR-0013 for the single-tier development environment strategy.
# See ADR-0030 for multi-cluster parameterization, ADR-0038 for provision/deploy.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
# shellcheck source=lib/common.sh
source "${SCRIPT_DIR}/lib/common.sh"
# shellcheck source=lib/cluster-db.sh
source "${SCRIPT_DIR}/lib/cluster-db.sh"
dt_init_cluster_env

# --- Argument parsing ---
AUTO_YES=false
PROVISION_ORG_SUB=""

# Auto-yes when stdin is not a TTY (automated callers)
if [[ ! -t 0 ]]; then
    AUTO_YES=true
fi

print_usage() {
    sed -n '2,/^$/{ s/^# \?//; p }' "${BASH_SOURCE[0]}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --yes)
            AUTO_YES=true
            shift
            ;;
        --provision-org)
            # The anchored regex in provision_run_org() is what rejects a
            # LEADING-HYPHEN value (`--provision-org --yes` would otherwise
            # consume the next flag as the subdomain), which is one more reason
            # that validation must not be collapsed as redundant.
            if [[ -z "${2:-}" ]]; then
                echo "ERROR: --provision-org requires a subdomain argument" >&2
                exit 1
            fi
            PROVISION_ORG_SUB="$2"
            shift 2
            ;;
        --help)
            print_usage
            exit 0
            ;;
        *)
            echo "ERROR: Unknown option '$1'" >&2
            print_usage >&2
            exit 1
            ;;
    esac
done

# --- Per-run organization provisioning (R-7) ---------------------------------
# Creates ONE freshly generated organization per layer-7 run, so the Nth
# consecutive run against the same dev cluster reaches the same verdict as the
# first. Without it every run's meetings accumulate against a single org's
# max_concurrent_meetings — nothing in production code marks a meeting ROW
# ended (MC's meeting-end notify, story 2 task 12, ends only GC's MC-assignment
# row, never `meetings.status`, which is what the cap counts) — and the browser
# suite's ~7 meetings/run exhaust the seeded cap on the second run, reported as
# a failure of the code under test.
#
# THE CONNECTION ROUTE IS LOAD-BEARING. This goes through the cluster's Postgres
# (dt_psql), never the container's $DATABASE_URL, which points at the devloop's
# unit-test DB (dark_tower_test) which AC and GC never read. That database has
# its own organizations table, so the wrong-target write SUCCEEDS silently rather
# than erroring. An org inserted there leaves R-7 looking fixed in Phase 1 and
# broken in Phase 2, surfacing as GC's org_not_provisioned discriminant — a
# message that points at provisioning which demonstrably ran.
#
# ROW SHAPE (schema: migrations/20250118000001_initial_schema.sql:5-21). The
# column list is exhaustive by design; each omission is a decision:
#   max_concurrent_meetings       explicit. The column default of 10 IS the R-7
#                                 bug. Value from ORG_MAX_CONCURRENT_MEETINGS.
#   max_participants_per_meeting  OMITTED, taking its default of 100.
#                                 crates/env-tests/tests/23_meeting_creation.rs:116
#                                 asserts max_participants == 100, and GC computes
#                                 it as LEAST($6, o.max_participants_per_meeting)
#                                 (gc-service/src/repositories/meetings.rs:246).
#                                 Any lower value silently caps the response and
#                                 reds that assert with a misleading message.
#   is_active                     OMITTED, defaults true. GC gates on
#                                 WHERE o.is_active; an inactive org now yields
#                                 the correctly-typed but bewildering org_inactive.
#   plan_tier                     'enterprise', mirroring the seeded devtest org.
#                                 Byte-for-byte the profile the suites are green
#                                 against today, which keeps "same verdict" a
#                                 one-line argument rather than an investigation.
#   org_id                        OMITTED, gen_random_uuid().
#
# NO ON CONFLICT, deliberately — unlike seed_test_data (DO UPDATE) and
# seed_demo_org (DO NOTHING), which are right for a FIXED subdomain seeded
# repeatedly. This row is fresh per run, so the property wanted is collision
# DETECTION, not reconciliation: DO NOTHING would hand the suites a stale org
# already at its cap (R-7 wearing a new hat), and DO UPDATE would resurrect one.
# A unique violation on subdomain must raise, exit non-zero, and reach the
# operator lane as PRECONDITION_FAILURE exit 2. ON_ERROR_STOP=1 is the MECHANISM
# that makes that sentence true, not a hardening extra: on the stdin transport
# psql reads to EOF and exits 0 after a failed statement (measured: rc=0 without
# it, rc=3 with it), so without it the unique violation, the subdomain_format
# CHECK and the valid_plan_tier CHECK would every one of them report success.
# Empty stdin exits 0 for the same reason — a statement that never arrives is
# indistinguishable from one that succeeded — which is why the SQL is a quoted
# heredoc (no producer that can fail mid-stream, no shell expansion inside the
# statement) and why the caller asserts RETURNING org_id produced exactly one
# UUID-shaped line.
#
# WHY stdin + --set AND NOT psql -c: `-c` hands its string to the SERVER
# unprocessed, so psql's own :'var' interpolation never runs. Measured against
# the live pod (psql 16.14): `psql -v sub=abc -tAc "SELECT :'sub'"` →
# `ERROR: syntax error at or near ":"`. A -c form would therefore be no
# parameterization at all while READING as parameterized. On the stdin transport
# the substitution does happen, and it quotes: --set="sub=a'; DROP TABLE
# organizations; --" comes back as data, not SQL. Do not "simplify" this to -c.
#
# NO REAPER, and this is a decision rather than an oversight — do not "fix" it
# later by adding a cascade DELETE:
#   1. Per run this adds 1 org, a handful of users, ~15 meetings, ~30
#      participants, and some audit_logs.
#   2. ~1,000 runs is tens of thousands of rows, and this Postgres dies with the
#      Kind cluster at devloop teardown.
#   3. Every hot path is org-scoped and index-backed (idx_organizations_subdomain
#      partial on is_active, idx_meetings_org_id), so cost is O(rows-per-org) —
#      and this change BOUNDS rows-per-org to a single run instead of letting it
#      climb forever. That bounding is the R-7 fix itself.
#   4. Reaping means a cascade DELETE reaching users, meetings, participants and
#      audit_logs (all ON DELETE CASCADE from organizations); a mis-scoped one
#      deletes a live run's org. That risk exceeds the storage it would save.
# If a bound is ever genuinely needed, the only acceptable form is age-scoped
# with a fixed literal prefix and a cutoff far beyond any live run:
#   DELETE FROM organizations
#    WHERE subdomain LIKE 'e2e-%' AND created_at < NOW() - INTERVAL '7 days';
# (index-backed by idx_organizations_created_at).
#
# NO MIGRATION REQUIRED: every column, default, CHECK and index relied on here
# already exists.
#
# EXIT DISCIPLINE — deliberately breaks the surrounding style. seed_test_data()
# and seed_demo_org() log and continue on some failures; this function returns
# NON-ZERO on any failure, because layer7.sh maps a non-zero here to
# PRECONDITION_FAILURE exit 2 (the operator lane). If it returned 0 on failure,
# Phase 1 would read success and R-7's "operator lane, never a suite failure"
# would invert exactly backwards. Do not harmonize this with its neighbours.
#
# TODO: Replace with AC org provisioning API (see docs/TODO.md) — SAME marker as
# the seed_test_data() call site below, and for the same reason. This function is
# a STOPGAP, not the intended design.
#
# Writing organizations by SQL from a test fixture inverts the dependency: the
# suite reaches around the product to arrange state the product should own, so it
# exercises a path no real client can take, and every consumer of that state
# (AC's subdomain rules, GC's cap semantics) is duplicated here rather than
# enforced once by the service. The direction of travel is already tracked for
# the identical pattern with service credentials — admin API, then registration
# Jobs, then "Remove setup.sh seed_test_data" — and org provisioning belongs in
# that sequence rather than beside it. Note the ordering constraint: calling an
# AC org endpoint needs an admin credential, and credentials are themselves
# SQL-seeded today, so neither escapes seeding until both admin APIs exist.
#
# Do not extend this function's reach. Adding more product state here (users,
# roles, credentials) widens the workaround and the eventual removal.
#
# Args: $1 = subdomain (lowercase, DNS-label shaped)
# Env:  DT_CLUSTER_NAME (required, no fallback)
# Stdout: one line `PROVISIONED_ORG org_id=<uuid> subdomain=<sub>`
provision_run_org() {
    local sub="${1:-}"
    local display_name="Layer-7 run ${sub}"

    # (1) Cluster identity must be EXPLICIT on this path — no fallback.
    # The global default (lib/common.sh:dt_init_cluster_env, ${DT_CLUSTER_NAME:-dark-tower})
    # stays, because a bare host-side run legitimately means the manually created
    # `dark-tower` cluster and the helper always passes the variable explicitly
    # (crates/devloop-helper/src/commands.rs). But on THIS path a fallback is a
    # hazard rather than a convenience: on a workstation that also has a manual
    # `dark-tower` cluster, `kind-dark-tower` RESOLVES, and we would provision
    # into the wrong database while the suite runs against the devloop one — a
    # silent cross-cluster write presenting later as an unexplained auth failure.
    # Note this tests the ENV VAR, not $CLUSTER_NAME, which is never empty and
    # would make the check vacuous.
    if [[ -z "${DT_CLUSTER_NAME:-}" ]]; then
        log_error "--provision-org requires DT_CLUSTER_NAME to be set explicitly (no fallback on this path)."
        log_error "  Callers derive it from the helper's port map: jq -r '.cluster_name' /tmp/devloop/ports.json"
        return 1
    fi

    # (2) Validate the subdomain independently of whoever generated it. The
    # caller generates it over a closed alphabet, which is the primary defense;
    # this is defense-in-depth at a trust boundary, mirroring the client-side
    # service-name check in infra/devloop/dev-cluster even though the helper
    # validates server-side. It is ALSO the only thing that rejects a
    # leading-hyphen value smuggled in by `--provision-org --yes`.
    #
    # The pattern is copied VERBATIM from the migration's subdomain_format CHECK
    # so the two can be diffed; the database remains the SSoT for the format and
    # this is the fail-fast. Do not replace it with a different-but-equivalent
    # rule — a third invented pattern is uncheckable against anything.
    #
    # LC_ALL=C pins bracket-range collation to ASCII: under a UTF-8 locale
    # `[a-z]` is collation-dependent and does not reliably mean ASCII a-z, so an
    # unpinned pattern LOOKS structural without being so.
    local LC_ALL=C
    local subdomain_re='^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'
    if [[ ! "$sub" =~ $subdomain_re ]] || (( ${#sub} > 63 )); then
        log_error "Invalid subdomain for --provision-org: '${sub}'"
        log_error "  Must match ${subdomain_re} and be at most 63 characters (schema: VARCHAR(63))."
        return 1
    fi

    # (3) Confirm the kube context resolves BEFORE any write, so a mismatch reads
    # as "wrong cluster" rather than as a provisioning bug. Pure kubeconfig read.
    # The name checked is KUBE_CONTEXT — the same string ${KUBECTL} will use — so
    # this asserts the context the writes actually go through, not a second
    # re-derivation of Kind's naming convention that could drift away from it.
    #
    # READ INTO A VARIABLE, NOT PIPED INTO `grep -q`. Under this script's
    # `set -o pipefail`, `grep -q` exits at the FIRST match and closes the pipe;
    # kubectl then takes SIGPIPE (141) on its next write and pipefail makes the
    # whole pipeline non-zero EVEN THOUGH THE CONTEXT WAS FOUND.
    #
    # IT IS A RACE, NOT A SIZE THRESHOLD — an earlier draft of this comment said
    # ">=200 contexts fails, <=100 passes", which was one sample per point and is
    # WRONG. What decides it is whether kubectl is still writing when `grep -q`
    # exits, which is scheduler-dependent. Re-measured on this image, 30 runs per
    # point, needle on the FIRST line: 100 contexts 2/30 false negatives, 200
    # 18/30, 300 29/30, 3000 30/30. So a "small" kubeconfig does not make the
    # check safe, it makes it INTERMITTENT — the failure mode that gets diagnosed
    # as flake and re-run rather than fixed. Corrected in place rather than
    # silently, because a threshold reads as "under N we are fine" and would be
    # cited to justify reintroducing the pipe somewhere else. The
    # devloop container cannot hit it — its kubeconfig comes from
    # `kind get kubeconfig` and holds exactly one context — but a host-side
    # operator run against a busy workstation kubeconfig can, and the symptom
    # would be a PRECONDITION_FAILURE blaming cluster addressing on a cluster
    # whose context is right there. That is the misattribution class this whole
    # path exists to remove, so the check must not be able to produce it.
    # Assignment is split from `local` so the command's status is not masked.
    local expected_context="${KUBE_CONTEXT}"
    local available_contexts=""
    available_contexts="$(kubectl config get-contexts -o name 2>/dev/null)" || available_contexts=""
    if ! grep -qxF "${expected_context}" <<<"${available_contexts}"; then
        log_error "Kube context '${expected_context}' not found in the active kubeconfig (KUBECONFIG=${KUBECONFIG:-<unset>})."
        log_error "  Available contexts: $(tr '\n' ' ' <<<"${available_contexts}")"
        log_error "  DT_CLUSTER_NAME=${DT_CLUSTER_NAME} — check it matches the cluster this container targets."
        return 1
    fi

    log_step "Provisioning per-run organization '${sub}'..."

    # (4) The INSERT. Quoted heredoc delimiter (<<'SQL') is load-bearing: with an
    # unquoted delimiter the shell would expand ${...} and backticks INTO the SQL
    # text before psql saw it, reconstituting the string-concatenation hazard
    # this design exists to remove — and it would do so in a statement that still
    # reads as parameterized. Values reach SQL only via --set/:'name'.
    # Stdout is captured; stderr deliberately flows through to the caller's log
    # so psql's diagnostics stay visible (never 2>&1 into the capture).
    local org_id
    # max_meetings is passed via --set (never expanded into the heredoc, which is
    # quoted) and referenced as (:'max_meetings')::int — QUOTED then cast, NOT
    # bare :max_meetings. psql applies literal quoting ONLY to the :'var' form;
    # bare :var is raw TEXT SUBSTITUTION into the statement before the server
    # sees it, i.e. the string concatenation this whole design forbids. Measured
    # against the live pod with --set=max_meetings="1000 OR 1=1": bare :var
    # returned count=2 (the OR executed); (:'max_meetings')::int failed with
    # `invalid input syntax for type integer`. Not attacker-reachable today —
    # the value is a config constant — but the property must not rest on the
    # value's provenance staying benign, and a reader seeing :'sub' quoted above
    # would reasonably infer bare is fine below. It is not.
    if ! org_id="$(dt_psql --stdin -X -tAq -v ON_ERROR_STOP=1 \
            --set=sub="${sub}" --set=name="${display_name}" \
            --set=max_meetings="${ORG_MAX_CONCURRENT_MEETINGS}" <<'SQL'
INSERT INTO organizations (subdomain, display_name, plan_tier, max_concurrent_meetings)
VALUES (:'sub', :'name', 'enterprise', (:'max_meetings')::int)
RETURNING org_id;
SQL
    )"; then
        log_error "Failed to create organization '${sub}' (psql returned non-zero)."
        return 1
    fi

    # (5) Assert exactly one UUID-shaped line. LOAD-BEARING, not belt-and-braces:
    # empty stdin also exits 0, so this is the only thing distinguishing "the
    # statement never arrived" from "the statement succeeded".
    local uuid_re='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    if [[ "$org_id" == *$'\n'* ]] || [[ ! "$org_id" =~ $uuid_re ]]; then
        log_error "Organization insert did not return exactly one org_id for '${sub}'."
        log_error "  Got: '${org_id}'"
        return 1
    fi

    # (6) Readback against AC's EXACT predicate (org_extraction resolves the Host
    # subdomain through organizations::get_by_subdomain, which filters
    # `WHERE subdomain = $1 AND is_active = true` and fails closed). This does not
    # rely on psql's exit semantics at all — here the exit code is `kubectl exec`
    # propagating a REMOTE code, which no hermetic stub can pin. A SELECT matching
    # zero rows exits 0, so this asserts on OUTPUT: the count must be exactly 1.
    # Scoped to named columns on organizations only — nothing from the
    # service_credentials neighbourhood may reach the layer-7 log.
    # The predicate asserts the ROW SHAPE, not merely that a row exists —
    # everything R-7 depends on lives in the column values, so an existence-only
    # check would pass on a row that cannot do the job.
    #
    # `max_participants_per_meeting = 100` is the conjunct that earns this. That
    # column is a schema default this function deliberately does NOT set, making
    # it the one input nothing here pins. If the default ever drifts, GC's
    # LEAST($6, o.max_participants_per_meeting)
    # (gc-service/src/repositories/meetings.rs:246) silently caps the response
    # and crates/env-tests/tests/23_meeting_creation.rs:116 reds with a message
    # about max_participants that points nowhere near the cause. Pinned here, the
    # same drift fails in Phase 1 on the operator lane, naming this row. The
    # literal is an INDEPENDENT pin of the schema default, not a duplicate of a
    # value this function chose — which is exactly why it must stay a literal.
    # The cap comes from (:'max_meetings')::int so ORG_MAX_CONCURRENT_MEETINGS
    # stays SSoT; see the INSERT above for why it is quoted-then-cast and never
    # bare :max_meetings. The apparent inconsistency between the two is the
    # point: one value we own and pin from our own source, one value we do not
    # own and assert against a hardcoded expectation.
    local found
    if ! found="$(dt_psql --stdin -X -tAq -v ON_ERROR_STOP=1 --set=sub="${sub}" \
            --set=max_meetings="${ORG_MAX_CONCURRENT_MEETINGS}" <<'SQL'
SELECT count(*) FROM organizations
 WHERE subdomain = :'sub'
   AND plan_tier = 'enterprise'
   AND max_concurrent_meetings = (:'max_meetings')::int
   AND max_participants_per_meeting = 100
   AND is_active;
SQL
    )"; then
        log_error "Readback query failed for organization '${sub}'."
        return 1
    fi
    if [[ "$found" != "1" ]]; then
        log_error "Readback did not find exactly one organization '${sub}' with the expected shape (count=${found})."
        log_error "  Expected: plan_tier=enterprise, max_concurrent_meetings=${ORG_MAX_CONCURRENT_MEETINGS}, max_participants_per_meeting=100, is_active=true."
        log_error "  AC resolves the Host subdomain with WHERE subdomain=\$1 AND is_active=true and fails closed,"
        log_error "  so token acquisition would fail with no usable diagnostic. Refusing to report success."
        return 1
    fi

    log_info "Provisioned organization '${sub}' (org_id=${org_id}, max_concurrent_meetings=${ORG_MAX_CONCURRENT_MEETINGS})."
    printf 'PROVISIONED_ORG org_id=%s subdomain=%s\n' "$org_id" "$sub"
}

# Install Telepresence traffic-manager (optional)
install_telepresence() {
    log_step "Checking Telepresence..."

    if ! command -v telepresence &> /dev/null; then
        log_warn "Telepresence CLI not installed. Skipping traffic-manager installation."
        log_info "Install from: https://www.telepresence.io/docs/latest/install/"
        log_info "Then use: ./scripts/dev/iterate.sh ac"
        return 0
    fi

    # Install traffic-manager if not present
    if ! ${KUBECTL} get deployment traffic-manager -n ambassador &> /dev/null 2>&1; then
        log_info "Installing Telepresence traffic-manager..."
        telepresence helm install || log_warn "Failed to install traffic-manager (may already exist)"
    else
        log_info "Telepresence traffic-manager already installed."
    fi
}

# Setup port-forwards
setup_port_forwards() {
    log_step "Setting up port-forwards (running in background)..."

    # Kill any existing port-forwards for this cluster
    pkill -f "kubectl --context kind-${CLUSTER_NAME} port-forward" 2>/dev/null || true

    # Port variables can be overridden via DT_PORT_MAP
    local PF_POSTGRES="${POSTGRES_PORT:-5432}"
    local PF_AC="${AC_HTTP_PORT:-8082}"
    local PF_GC="${GC_HTTP_PORT:-8080}"
    local PF_MH="${MH_HEALTH_PORT:-8083}"
    local PF_PROMETHEUS="${PROMETHEUS_PORT:-9090}"
    local PF_GRAFANA="${GRAFANA_PORT:-3000}"
    local PF_LOKI="${LOKI_PORT:-3100}"

    # Start port-forwards in background
    ${KUBECTL} port-forward -n dark-tower svc/postgres "${PF_POSTGRES}:5432" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower svc/ac-service "${PF_AC}:8082" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower svc/gc-service "${PF_GC}:8080" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower svc/mh-service "${PF_MH}:8083" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower-observability svc/prometheus "${PF_PROMETHEUS}:9090" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower-observability svc/grafana "${PF_GRAFANA}:3000" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower-observability svc/loki "${PF_LOKI}:3100" &>/dev/null &

    sleep 2
    log_info "Port-forwards established."
}

# Print access information
print_access_info() {
    local p_ac="${AC_HTTP_PORT:-8082}"
    local p_gc="${GC_HTTP_PORT:-8080}"
    local p_mh="${MH_HEALTH_PORT:-8083}"
    local p_grafana="${GRAFANA_PORT:-3000}"
    local p_prometheus="${PROMETHEUS_PORT:-9090}"
    local p_loki="${LOKI_PORT:-3100}"
    local p_postgres="${POSTGRES_PORT:-5432}"
    local ctx="--context kind-${CLUSTER_NAME}"

    echo ""
    log_info "=========================================="
    log_info "Dark Tower kind cluster '${CLUSTER_NAME}' is ready!"
    log_info "=========================================="
    echo ""
    echo "Services Running in Cluster:"
    echo ""
    echo "  AC Service (Auth Controller):"
    echo "    URL: http://localhost:${p_ac}"
    echo "    Host-side E2E/browser (kind NodePort passthrough, loopback): http://127.0.0.1:8443"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  GC Service (Global Controller):"
    echo "    HTTP API: http://localhost:${p_gc}"
    echo "    Host-side E2E/browser (kind NodePort passthrough, loopback): http://127.0.0.1:8444"
    echo "    gRPC: localhost:50051 (cluster-internal)"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  MC Service (Meeting Controller) — 2 per-instance Deployments:"
    echo "    WebTransport (per-pod, QUIC/UDP via NodePort):"
    echo "      mc-service-0: https://localhost:4433"
    echo "      mc-service-1: https://localhost:4435"
    echo "    gRPC: localhost:50052 (cluster-internal)"
    echo "    Health: localhost:8081 (cluster-internal)"
    echo "    TLS: Self-signed (CA at infra/docker/certs/ca.crt)"
    echo ""
    echo "  MH Service (Media Handler) — 2 per-instance Deployments:"
    echo "    WebTransport (per-pod, QUIC/UDP via NodePort):"
    echo "      mh-service-0: https://localhost:4434"
    echo "      mh-service-1: https://localhost:4436"
    echo "    gRPC: localhost:50053 (cluster-internal)"
    echo "    Health: http://localhost:${p_mh}"
    echo "    TLS: Self-signed (CA at infra/docker/certs/ca.crt)"
    echo ""
    echo "  Grafana:"
    echo "    URL: http://localhost:${p_grafana}"
    echo "    Credentials: admin/admin"
    echo "    Datasources: Prometheus and Loki (pre-configured)"
    echo "    Dashboards: AC Service dashboard (pre-loaded)"
    echo ""
    echo "  Prometheus:"
    echo "    URL: http://localhost:${p_prometheus}"
    echo ""
    echo "  Loki:"
    echo "    URL: http://localhost:${p_loki}"
    echo "    (Access via Grafana Explore)"
    echo ""
    echo "  PostgreSQL:"
    echo "    Connection: localhost:${p_postgres}"
    local pg_secret="${PROJECT_ROOT}/infra/services/postgres/secret.yaml"
    echo "    DATABASE_URL: postgresql://$(pg_secret_value POSTGRES_USER "${pg_secret}"):$(pg_secret_value POSTGRES_PASSWORD "${pg_secret}")@localhost:${p_postgres}/$(pg_secret_value POSTGRES_DB "${pg_secret}")"
    echo ""
    echo "OAuth 2.0 Service Credentials (pre-seeded, per ADR-0010):"
    echo ""
    echo "  global-controller / global-controller-secret-dev-001  (used by GC)"
    echo "  meeting-controller / meeting-controller-secret-dev-002  (used by MC)"
    echo "  media-handler / media-handler-secret-dev-003  (used by MH)"
    echo "  test-client / test-client-secret-dev-999  (for testing)"
    echo ""
    echo "  GC, MC, and MH use these credentials to obtain OAuth tokens from AC."
    echo "  Tokens are acquired automatically via TokenManager (client credentials flow)."
    echo ""
    echo "Quick Test (AC service is already running):"
    echo ""
    echo "  curl -X POST http://localhost:${p_ac}/api/v1/auth/service/token \\"
    echo "    -H 'Content-Type: application/x-www-form-urlencoded' \\"
    echo "    -d 'grant_type=client_credentials' \\"
    echo "    -d 'client_id=test-client' \\"
    echo "    -d 'client_secret=test-client-secret-dev-999'"
    echo ""
    echo "Local Development (Telepresence):"
    echo ""
    echo "  To iterate on AC service locally while connected to the cluster:"
    echo "    ./scripts/dev/iterate.sh ac"
    echo ""
    echo "  This will:"
    echo "    - Scale down in-cluster AC pods"
    echo "    - Route cluster traffic to your local cargo run"
    echo "    - Auto-restore cluster state on Ctrl+C"
    echo ""
    echo "View Logs & Metrics:"
    echo ""
    echo "  Open http://localhost:${p_grafana} (Grafana)"
    echo "  - Navigate to Dashboards > AC Service"
    echo "  - Or Explore > Loki for logs"
    echo ""
    echo "Deploy Code/Config Changes (rolls only what changed):"
    echo ""
    echo "  ./infra/kind/scripts/deploy.sh"
    echo ""
    echo "Reset Runtime State Only (does NOT pick up new code — pods pin a content-tagged image):"
    echo ""
    echo "  kubectl ${ctx} rollout restart statefulset/ac-service -n dark-tower"
    echo "  kubectl ${ctx} rollout restart deployment/mc-0 deployment/mc-1 -n dark-tower"
    echo "  kubectl ${ctx} rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower"
    echo ""
    echo "To tear down:"
    echo "  ./infra/kind/scripts/teardown.sh"
    echo ""
    log_info "Happy coding!"
    echo ""
}

# Main
main() {
    # --provision-org: terminal, CONTAINER-RUNNABLE mode. This branch is FIRST
    # and returns immediately — see EXECUTION CONTEXT in the file header. It must
    # not fall through to provision.sh/deploy.sh, which require kind + podman or
    # docker (absent in the devloop container, so the run would die on "Neither
    # Podman nor Docker found" — the wrong problem, and unfixable from there),
    # nor reach the seeds or secret creation, which put credential material into
    # psql/kubectl arguments that would land in the layer-7 log. The early
    # return is therefore a containment control as well as a correctness one.
    if [[ -n "${PROVISION_ORG_SUB}" ]]; then
        provision_run_org "${PROVISION_ORG_SUB}" || return 1
        return 0
    fi

    log_info "Setting up Dark Tower local development environment..."
    echo ""

    local yes=()
    [[ "${AUTO_YES}" != "true" ]] || yes=(--yes)
    DT_CALLER=setup.sh "${SCRIPT_DIR}/provision.sh" "${yes[@]}"
    DT_CALLER=setup.sh "${SCRIPT_DIR}/deploy.sh"
    install_telepresence
    setup_port_forwards
    print_access_info
}

# Run main only when executed, not when sourced (the self-test sources this script to
# exercise provision_run_org directly). Mirrors scripts/layer7.sh's BASH_SOURCE guard.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
