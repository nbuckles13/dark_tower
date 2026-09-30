# shellcheck shell=bash
#
# Shared prelude for the Kind scripts (ADR-0038 step 3). DEFINITIONS ONLY:
# sourcing this file runs nothing, so any script (and scripts/setup.test.sh)
# can source it; each caller invokes what it needs.
#
# THIS FILE IS PART OF THE PROVISION BLUEPRINT (infra/kind/scripts/provision.sh
# hashes it whole, as a PROVISION_INPUT): editing it rebuilds every cluster on
# its next `provision`. So it holds ONLY what provision itself uses. Deploy-only
# helpers live in deploy.sh; the cluster-DB helpers in lib/cluster-db.sh (not
# hashed).

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

_ts() { date '+%H:%M:%S'; }

log_info() {
    echo -e "${GREEN}[$(_ts) INFO]${NC} $1"
}

# Diagnostics go to STDERR, so a function called inside `$(...)` can never
# swallow its own error message (the capture would take stdout only).
log_warn() {
    echo -e "${YELLOW}[$(_ts) WARN]${NC} $1" >&2
}

log_error() {
    echo -e "${RED}[$(_ts) ERROR]${NC} $1" >&2
}

log_step() {
    echo -e "${BLUE}[$(_ts) STEP]${NC} $1"
}

# --- Cluster name validation (#1 name-length, DERIVED from 63) ---
# KIND names the control-plane node `<cluster>-control-plane`, a Kubernetes DNS
# label capped at 63 chars. infra/kind/kind-config.yaml declares exactly one
# control-plane node (see the NOTE above its `nodes:` block), so `-control-plane` is the
# longest node-name suffix and the operative cap is on the CLUSTER NAME:
# 63 - len("-control-plane") = 49. Derive it (one `63` literal here, the suffix by
# length) so the bound can't drift; never type 49. No truncation/hash — that would
# silently collide two clusters onto one name. A pure function (no side effects, no
# cluster calls) so setup.test.sh can source + call it directly, the disk-guard way.
# INVERSE PRECONDITION vs teardown.sh (the (b) Finding-5 pattern): provision MUST cap length
# (it CREATES the cluster and a >49 name can't fit the 63-char DNS-label node), whereas
# teardown deliberately does NOT cap (it must be able to DELETE a pre-existing orphan with
# a too-long name). The CHARSET regex below is kept in sync with teardown.sh's by a check
# in scripts/setup.test.sh; the length-cap divergence is intentional — DO NOT unify them.
validate_cluster_name() {
    local name="$1"
    local node_suffix="-control-plane"   # cross-ref: infra/kind/kind-config.yaml (single control-plane node; see the NOTE above `nodes:`)
    local dns_label_max=63
    local cluster_name_max=$(( dns_label_max - ${#node_suffix} ))   # 49
    if [[ ! "${name}" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]]; then
        echo "ERROR: Invalid cluster name '${name}': must be lowercase alphanumeric/hyphens, start and end with alphanumeric" >&2
        exit 1
    fi
    if (( ${#name} > cluster_name_max )); then
        echo "ERROR: CLUSTER_NAME_TOO_LONG SUBJECT=cluster-name NAME=${name} LEN=${#name} MAX=${cluster_name_max}" >&2
        echo "  Cluster name '${name}' is ${#name} chars; shorten it to <= ${cluster_name_max}." >&2
        echo "  (Its '${node_suffix}' node name must fit the ${dns_label_max}-char DNS label.)" >&2
        exit 1
    fi
}

# Resolve and validate the cluster environment every host-side Kind script
# shares: CLUSTER_NAME (DT_CLUSTER_NAME, default dark-tower; CLUSTER_NAME_EXPLICIT
# records whether the caller named it), the per-slug port map (DT_PORT_MAP), the
# host-gateway IP (DT_HOST_GATEWAY_IP) and the kubectl context.
dt_init_cluster_env() {
    if [[ -n "${DT_CLUSTER_NAME:-}" ]]; then
        CLUSTER_NAME_EXPLICIT=true
    else
        CLUSTER_NAME_EXPLICIT=false
    fi
    CLUSTER_NAME="${DT_CLUSTER_NAME:-dark-tower}"
    validate_cluster_name "${CLUSTER_NAME}"

    # --- DT_PORT_MAP sourcing ---
    if [[ -n "${DT_PORT_MAP:-}" ]]; then
        if [[ ! -f "${DT_PORT_MAP}" ]]; then
            echo "ERROR: DT_PORT_MAP file not found: ${DT_PORT_MAP}" >&2
            exit 1
        fi
        # Validate every line is a safe variable assignment (VAR_NAME=digits)
        local line
        while IFS= read -r line || [[ -n "$line" ]]; do
            # Skip empty lines and comments
            [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
            if [[ ! "$line" =~ ^[A-Z_][A-Z0-9_]*=[0-9]+$ ]]; then
                echo "ERROR: DT_PORT_MAP contains invalid line: ${line}" >&2
                exit 1
            fi
        done < "${DT_PORT_MAP}"
        # Runtime path (the per-slug port map); nothing for shellcheck to follow statically.
        # shellcheck source=/dev/null
        source "${DT_PORT_MAP}"
    fi

    # --- DT_HOST_GATEWAY_IP validation (defense-in-depth, see ADR-0030) ---
    if [[ -n "${DT_HOST_GATEWAY_IP:-}" ]]; then
        if [[ ! "${DT_HOST_GATEWAY_IP}" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "ERROR: DT_HOST_GATEWAY_IP is not a valid IPv4 address: '${DT_HOST_GATEWAY_IP}'" >&2
            exit 1
        fi
        if [[ "${DT_HOST_GATEWAY_IP}" == "0.0.0.0" ]]; then
            echo "ERROR: DT_HOST_GATEWAY_IP must not be 0.0.0.0 (ADR-0030 prohibits binding to all interfaces)" >&2
            exit 1
        fi
    fi

    # --- kubectl with explicit context for multi-cluster support ---
    # KUBE_CONTEXT is the ONE encoding of Kind's `kind-<cluster>` context-naming
    # convention in these scripts. provision_run_org() (setup.sh) asserts the
    # context resolves before it writes, and it must assert the SAME name every
    # ${KUBECTL} call uses — deriving it here rather than re-forming the `kind-`
    # prefix at the check site is what stops the assertion and the action
    # drifting apart (CLAUDE.md single-source-of-truth). Do not inline this back
    # into the KUBECTL string.
    KUBE_CONTEXT="kind-${CLUSTER_NAME}"
    KUBECTL="kubectl --context ${KUBE_CONTEXT}"
}

# True iff a Kind cluster named CLUSTER_NAME exists. `kind get clusters`
# failing is a failure, never "absent" (rc 2 — the caller must not treat it as
# a missing cluster: provision would rebuild on it).
cluster_exists() {
    local out
    out="$(kind get clusters 2>/dev/null)" || return 2
    grep -qx "${CLUSTER_NAME}" <<< "${out}"
}

# A key of a Kubernetes Secret manifest's `stringData` (e.g. POSTGRES_PASSWORD
# from infra/services/postgres/secret.yaml — the ONE owner of the dev Postgres
# credential; every script derives from it rather than retyping it). Prints the
# value; fails naming the KEY (never a value) when it is absent.
# Usage: pg_secret_value <key> <secret.yaml path>
pg_secret_value() {
    local key="$1" file="$2" v
    v="$(awk -v k="${key}" '$1 == k":" { $1 = ""; sub(/^ +/, ""); gsub(/^"|"$/, ""); print; exit }' "${file}")"
    if [[ -z "${v}" ]]; then
        log_error "${file} has no stringData key ${key}"
        return 1
    fi
    printf '%s' "${v}"
}

# The first missing host prerequisite (kind, kubectl, a container runtime),
# printed; rc 1 when one is missing, rc 0 (nothing printed) when all are present.
missing_prerequisite() {
    local tool
    # `timeout` bounds every environment probe below: without it a probe exits 127
    # and would read as the environment being down.
    for tool in kind kubectl timeout; do
        command -v "${tool}" >/dev/null 2>&1 || { echo "${tool}"; return 1; }
    done
    if ! command -v podman >/dev/null 2>&1 && ! command -v docker >/dev/null 2>&1; then
        echo "a container runtime (podman or docker)"
        return 1
    fi
}

# Upper bound, in seconds, on EACH environment probe below (a hung runtime or
# apiserver must not hang the failure path that is trying to name it).
ENV_PROBE_TIMEOUT="${DT_ENV_PROBE_TIMEOUT:-10}"

# The container runtime CLI, from the provider detect_container_runtime()
# exported (docker when unset — kind's own default provider).
container_cmd() {
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        echo podman
    else
        echo docker
    fi
}

# THE environment classifier both EXIT traps (provision.sh, deploy.sh) call when a
# step failed with no specific reason. Prints ONE environment reason, or nothing
# (the failure is then the tree's: `step-failed`). Every probe is keyed on an
# EXIT STATUS, never on a tool's wording, and bounded by `timeout`:
#   prerequisite-missing   `timeout` itself is not on PATH (no probe can run);
#   runtime-unreachable    `<podman|docker> info` fails or times out;
#   apiserver-unreachable  `kubectl get --raw /readyz` fails or times out — only
#                          when $1 is `apiserver` (a step AFTER the cluster exists;
#                          before that there is no apiserver to ask).
# Usage: classify_env_failure <apiserver|no-apiserver>
classify_env_failure() {
    # No `timeout`: every probe would exit 127 and read as "unreachable" — name the
    # missing tool instead of misrouting the tree's failure to the operator lane.
    if ! command -v timeout >/dev/null 2>&1; then
        echo "prerequisite-missing"
        return 0
    fi
    local runtime="${KIND_EXPERIMENTAL_PROVIDER:-}"
    if [[ -z "${runtime}" ]]; then
        if command -v podman >/dev/null 2>&1; then runtime=podman
        elif command -v docker >/dev/null 2>&1; then runtime=docker
        fi
    fi
    if [[ -n "${runtime}" ]] && ! timeout "${ENV_PROBE_TIMEOUT}" "${runtime}" info >/dev/null 2>&1; then
        echo "runtime-unreachable"
        return 0
    fi
    if [[ "${1:-}" == "apiserver" ]] \
        && ! timeout "${ENV_PROBE_TIMEOUT}" ${KUBECTL} get --raw /readyz --request-timeout=5s >/dev/null 2>&1; then
        echo "apiserver-unreachable"
        return 0
    fi
}

# Detect container runtime (prefer Podman)
detect_container_runtime() {
    if command -v podman &> /dev/null; then
        log_info "Using Podman as container runtime"
        export KIND_EXPERIMENTAL_PROVIDER=podman
    elif command -v docker &> /dev/null; then
        log_info "Using Docker as container runtime"
        export KIND_EXPERIMENTAL_PROVIDER=docker
    else
        log_error "Neither Podman nor Docker found. Please install one of them."
        exit 1
    fi
}

# Check prerequisites
check_prerequisites() {
    log_step "Checking prerequisites..."

    if ! command -v kind &> /dev/null; then
        log_error "kind is not installed. Install from: https://kind.sigs.k8s.io/"
        exit 1
    fi

    if ! command -v kubectl &> /dev/null; then
        log_error "kubectl is not installed. Install from: https://kubernetes.io/docs/tasks/tools/"
        exit 1
    fi

    detect_container_runtime

    log_info "All prerequisites satisfied."
}

