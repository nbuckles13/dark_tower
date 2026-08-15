#!/usr/bin/env bash
#
# Setup script for Dark Tower local development environment
#
# This script creates a kind cluster with full production parity:
# - Calico CNI for NetworkPolicy enforcement
# - PostgreSQL and Redis
# - Full observability stack (Prometheus, Grafana, Loki)
# - Database migrations
# - Port-forwarding
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
#   main() BEFORE check_prerequisites(), which calls detect_container_runtime()
#   and exits 1 when no container runtime is found. Note --only is NOT a
#   precedent to copy: it routes through deploy_only_service(), which calls both
#   `kind get clusters` and check_prerequisites().
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
#   --only <svc>             Only rebuild+redeploy one service (ac, gc, mc, mh)
#   --skip-build             Skip image builds, only apply manifests
#   --provision-org <sub>    Create one fresh organization and exit. Requires
#                            DT_CLUSTER_NAME. Container-runnable (see EXECUTION
#                            CONTEXT above); rejected in combination with --only
#                            or --skip-build. (NOT with --yes, which this script
#                            sets itself whenever stdin is not a TTY, so every
#                            automated caller of this mode already has it on.)
#                            Prints one line:
#                              PROVISIONED_ORG org_id=<uuid> subdomain=<sub>
#   --help                   Show this help message
#
# See ADR-0013 for the single-tier development environment strategy.
# See ADR-0030 for multi-cluster parameterization.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
CLUSTER_NAME="${DT_CLUSTER_NAME:-dark-tower}"
KIND_CONFIG="${PROJECT_ROOT}/infra/kind/kind-config.yaml"
CALICO_VERSION="v3.27.0"

# --- Cluster name validation ---
if [[ ! "${CLUSTER_NAME}" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]] || [[ ${#CLUSTER_NAME} -gt 63 ]]; then
    echo "ERROR: Invalid cluster name '${CLUSTER_NAME}': must be lowercase alphanumeric/hyphens, start and end with alphanumeric, max 63 chars" >&2
    exit 1
fi

# --- DT_PORT_MAP sourcing ---
if [[ -n "${DT_PORT_MAP:-}" ]]; then
    if [[ ! -f "${DT_PORT_MAP}" ]]; then
        echo "ERROR: DT_PORT_MAP file not found: ${DT_PORT_MAP}" >&2
        exit 1
    fi
    # Validate every line is a safe variable assignment (VAR_NAME=digits)
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Skip empty lines and comments
        [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
        if [[ ! "$line" =~ ^[A-Z_][A-Z0-9_]*=[0-9]+$ ]]; then
            echo "ERROR: DT_PORT_MAP contains invalid line: ${line}" >&2
            exit 1
        fi
    done < "${DT_PORT_MAP}"
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
# convention in this script. provision_run_org() asserts the context resolves
# before it writes, and it must assert the SAME name every ${KUBECTL} call uses —
# deriving it here rather than re-forming the `kind-` prefix at the check site is
# what stops the assertion and the action drifting apart (CLAUDE.md
# single-source-of-truth). Do not inline this back into the KUBECTL string.
KUBE_CONTEXT="kind-${CLUSTER_NAME}"
KUBECTL="kubectl --context ${KUBE_CONTEXT}"

# --- Argument parsing ---
AUTO_YES=false
ONLY_SERVICE=""
SKIP_BUILD=false
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
        --only)
            if [[ -z "${2:-}" ]]; then
                echo "ERROR: --only requires a service name (ac, gc, mc, mh)" >&2
                exit 1
            fi
            case "$2" in
                ac|gc|mc|mh) ONLY_SERVICE="$2" ;;
                *)
                    echo "ERROR: Unknown service '$2'. Valid: ac, gc, mc, mh" >&2
                    exit 1
                    ;;
            esac
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --provision-org)
            # Deliberately NOT modelled on --only's check above, which tests only
            # `-z "${2:-}"` and therefore lets a LEADING-HYPHEN value through:
            # `--provision-org --skip-build` would silently consume the next flag
            # as the subdomain. The anchored regex in provision_run_org() is what
            # rejects that (a value starting with `-` cannot match), which is one
            # more reason that validation must not be collapsed as redundant.
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

# --provision-org is a terminal, container-runnable mode: it creates one row and
# exits without touching the cluster. Combining it with flags that only mean
# something on the full host-side setup path can only express a misunderstanding,
# so reject the combination rather than silently ignoring the other flag.
if [[ -n "${PROVISION_ORG_SUB}" ]] && { [[ -n "${ONLY_SERVICE}" ]] || [[ "${SKIP_BUILD}" == true ]]; }; then
    echo "ERROR: --provision-org cannot be combined with --only or --skip-build" >&2
    exit 1
fi

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

log_warn() {
    echo -e "${YELLOW}[$(_ts) WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[$(_ts) ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[$(_ts) STEP]${NC} $1"
}

# --- Cluster Postgres access (single source of truth) ------------------------
# EVERY psql call in this script goes through dt_psql(). The connection identity
# is stated once here; it was copy-pasted across three call sites before, and a
# fourth copy in scripts/layer7.sh was the alternative this function replaced.
#
# These are set unconditionally at file scope, NOT inside a function: the
# --provision-org path early-returns from main() and must not depend on anything
# the full-setup flow initialises on its way past.
#
# WHY NOT "$DATABASE_URL": inside the devloop container that variable points at
# the per-devloop UNIT-TEST database (…-db:5432/dark_tower_test), which AC and GC
# never read. That database has its own organizations table, so the wrong-target
# write SUCCEEDS silently rather than erroring (verified: `psql "$DATABASE_URL"
# -tAc "SELECT to_regclass('public.organizations')"` returns a non-null
# regclass). An org inserted there leaves R-7 looking fixed in Phase 1 and broken
# in Phase 2, surfacing as GC's org_not_provisioned discriminant — a message that
# points at provisioning which demonstrably ran.
#
# AUTH SHAPE is deliberate: in-pod `-U <user> -d <db>`, never a
# postgresql://user:pass@host URI. A URI would land in `ps` output and, when this
# script runs under Layer 7, in ${DEVLOOP_TMP}/layer-7-*.log.
DT_PG_NAMESPACE="dark-tower"
DT_PG_POD="postgres-0"
# The pod has an init container; naming the target container suppresses kubectl's
# `Defaulted container "postgres" out of: postgres, init-permissions (init)`
# banner, which would otherwise interleave with captured output.
DT_PG_CONTAINER="postgres"
DT_PG_USER="darktower"
DT_PG_DB="dark_tower"

# Run psql inside the cluster's Postgres pod.
#
#   dt_psql [--stdin] <psql args...>
#
# --stdin adds `kubectl exec -i` and is REQUIRED when the SQL arrives on stdin
# (without it psql reads an empty stdin and exits 0 — a silent no-op). It is
# opt-in rather than always-on because the `-c` call sites never read stdin, and
# forwarding an open non-TTY stdin to `kubectl exec` on those calls risks a hang.
# Never `-it`: a TTY performs CRLF translation and can corrupt SQL on stdin.
dt_psql() {
    local stdin_flag=()
    if [[ "${1:-}" == "--stdin" ]]; then
        stdin_flag=(-i)
        shift
    fi
    # NB two different `-c` flags on this line: the one before `--` is kubectl's
    # --container; any `-c` in "$@" after `--` is psql's --command. The `--` is
    # what keeps them unambiguous.
    ${KUBECTL} exec "${stdin_flag[@]}" -n "${DT_PG_NAMESPACE}" -c "${DT_PG_CONTAINER}" \
        "${DT_PG_POD}" -- psql -U "${DT_PG_USER}" -d "${DT_PG_DB}" "$@"
}

# Concurrent-meeting cap for organizations the TEST SUITES drive.
#
# CARVE-OUT — this applies to seed_test_data()'s `devtest` org and to
# provision_run_org() ONLY. It deliberately does NOT apply to seed_demo_org(),
# which takes the schema default of 10 because it is the user-facing browser
# sign-up org and the default is the right profile for it (see the rationale
# above seed_demo_org). A reader seeing two of three call sites use this constant
# is looking at a deliberate exclusion, not an unfinished refactor.
#
# The schema default of 10 is the R-7 bug: no production code marks a meeting
# ended, so an org's live-meeting count only climbs, and the browser suite's ~7
# meetings/run exhausts a cap of 10 on the second run against the same cluster.
# Config over hardcoding (CLAUDE.md), and the knob is what makes the validator below a LIVE
# check rather than a vacuous one (@code-reviewer F3): with a bare literal, `1000` can never
# fail `^[1-9][0-9]{0,8}$`, so the validation would describe a control with no reachable input.
# `readonly` is deliberately omitted — no other constant in this script uses it.
ORG_MAX_CONCURRENT_MEETINGS="${DT_ORG_MAX_CONCURRENT_MEETINGS:-1000}"

# Validate the constant at the boundary that forms SQL statements from it, so a
# bad DT_ORG_MAX_CONCURRENT_MEETINGS dies at script start with a clear message
# rather than as a psql cast error mid-provision. This is not belt-and-braces:
# seed_test_data interpolates this value directly into SQL TEXT (`psql -c "…
# max_concurrent_meetings = ${ORG_MAX_CONCURRENT_MEETINGS};"`), which is the one
# site where it does NOT go through --set + (:'max_meetings')::int — so this
# regex is the only thing standing between an env override and a SQL injection
# point. The `(:'var')::int` quoting in those statements is the
# structural backstop; this is the fail-fast, exactly as the subdomain regex is
# the fail-fast in front of the schema's own CHECK.
if [[ ! "${ORG_MAX_CONCURRENT_MEETINGS}" =~ ^[1-9][0-9]{0,8}$ ]]; then
    echo "ERROR: ORG_MAX_CONCURRENT_MEETINGS must be a positive integer with no leading zero, got '${ORG_MAX_CONCURRENT_MEETINGS}'" >&2
    exit 1
fi

# Load a container image into the Kind cluster.
# Handles podman save/load workaround vs docker direct load.
# Usage: load_image_to_kind <image-tag>
load_image_to_kind() {
    local TAG="$1"
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        local TMPFILE
        TMPFILE=$(mktemp /tmp/kind-image.XXXXXX.tar)
        podman save "$TAG" -o "${TMPFILE}"
        kind load image-archive "${TMPFILE}" --name "${CLUSTER_NAME}"
        rm -f "${TMPFILE}"
    else
        kind load docker-image "$TAG" --name "${CLUSTER_NAME}"
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

# Warn that kind extraPortMappings are fixed at cluster-create time (R-37).
# Reusing an existing cluster will NOT pick up new or changed host-port mappings
# (AC 8443, GC 8444, MC/MH WebTransport) until teardown + recreate — otherwise a
# dev "applies the change and nothing happens". Fired only on the reuse paths.
warn_port_mapping_cliff() {
    log_warn "Reusing existing cluster — kind extraPortMappings (AC 8443, GC 8444, MC/MH WebTransport)"
    log_warn "  are baked in at 'kind create cluster'. If you just pulled new/changed port mappings, they"
    log_warn "  are NOT active until you recreate the cluster:"
    log_warn "    ./infra/kind/scripts/teardown.sh && ./infra/kind/scripts/setup.sh"
    log_warn "  (On recreate, a host-port collision fails loudly with 'port is already allocated' — free"
    log_warn "   the conflicting loopback port and retry.)"
}

# Create kind cluster
create_cluster() {
    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_warn "Cluster '${CLUSTER_NAME}' already exists."
        if [[ "${AUTO_YES}" == "true" ]]; then
            log_info "Cluster exists, reusing (auto-yes defaults to non-destructive)."
            warn_port_mapping_cliff
            return 0
        fi
        read -p "Delete and recreate? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            log_info "Deleting existing cluster..."
            kind delete cluster --name "${CLUSTER_NAME}"
        else
            log_info "Using existing cluster."
            warn_port_mapping_cliff
            return 0
        fi
    fi

    log_step "Creating kind cluster '${CLUSTER_NAME}' with Calico CNI..."
    kind create cluster --config="${KIND_CONFIG}" --name="${CLUSTER_NAME}"

    # NOTE: Don't wait for nodes here - they won't be Ready until Calico is installed
    # (because we set disableDefaultCNI: true in kind-config.yaml)
    log_info "Cluster created. Installing CNI before nodes can become Ready..."
}

# Install Calico CNI
install_calico() {
    log_step "Installing Calico CNI for NetworkPolicy enforcement..."

    ${KUBECTL} create -f "https://raw.githubusercontent.com/projectcalico/calico/${CALICO_VERSION}/manifests/calico.yaml"

    log_info "Waiting for Calico pods to be created..."
    # Wait for calico-node pods to exist before trying to wait for Ready
    local max_attempts=30
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if ${KUBECTL} get pods -n kube-system -l k8s-app=calico-node 2>/dev/null | grep -q calico-node; then
            break
        fi
        attempt=$((attempt + 1))
        sleep 2
    done

    if [[ $attempt -eq $max_attempts ]]; then
        log_error "Calico pods were not created in time"
        exit 1
    fi

    log_info "Waiting for Calico to be ready..."
    ${KUBECTL} wait --for=condition=Ready pods -l k8s-app=calico-node -n kube-system --timeout=180s
    ${KUBECTL} wait --for=condition=Ready pods -l k8s-app=calico-kube-controllers -n kube-system --timeout=180s

    log_info "Calico CNI installed successfully."

    # Now wait for nodes to be Ready (requires CNI to be installed first)
    log_info "Waiting for nodes to be ready..."
    ${KUBECTL} wait --for=condition=Ready nodes --all --timeout=120s
}

# Create namespaces
create_namespaces() {
    log_step "Creating namespaces..."

    ${KUBECTL} create namespace dark-tower --dry-run=client -o yaml | ${KUBECTL} apply -f -
    ${KUBECTL} create namespace dark-tower-observability --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "Namespaces created."
}

# Fail fast BEFORE the first cold image build if the host container-storage filesystem
# lacks headroom for a 4-service release build — rather than dying mid-COPY with "no space
# left on device" after several minutes. Run-once (the first build_image call guards them
# all); naturally skipped under --skip-build (build_image isn't called).
#
# $1 = the resolved container runtime (build_image's $CONTAINER_CMD). MUST be passed, NOT
# re-derived: build_image defaults to `docker` when KIND_EXPERIMENTAL_PROVIDER is unset, so
# a re-derived `podman` default here would `df` the WRONG runtime's graphroot on an
# unset-provider host — measuring the wrong filesystem exactly when the guard matters.
#
# DEVLOOP_MIN_DISK_GB (default 15) is a fast-fail FLOOR, NOT a success guarantee: a fully
# cold build cache can exceed it and still pass-then-die, so a pass means "not obviously
# doomed", not "will succeed".
#
# Emits the relayed `PRECONDITION_FAILURE: … REASON=insufficient-disk` banner that surfaces
# in layer7's Phase-1 cluster-setup/rebuild stderr (${DEVLOOP_TMP}/layer-7.stderr.log; the
# §4 two-token convention — setup.sh is the third PRECONDITION_FAILURE emitter alongside
# layer-all.sh + layer7.sh). See docs/runbooks/devloop-validation.md §6.7.
_DT_DISK_CHECKED=false
check_build_disk_space() {
    local cmd="$1"
    [[ "${_DT_DISK_CHECKED}" == "true" ]] && return 0
    _DT_DISK_CHECKED=true
    local min_gb="${DEVLOOP_MIN_DISK_GB:-15}" graphroot avail_gb
    # ${cmd}'s container-storage root is where build layers land; fall back to the rootless
    # default, then the rootful default, then PROJECT_ROOT's fs (always resolvable).
    graphroot="$(${cmd} info --format '{{.Store.GraphRoot}}' 2>/dev/null || true)"
    [[ -n "${graphroot}" && -d "${graphroot}" ]] || graphroot="${HOME}/.local/share/containers/storage"
    [[ -d "${graphroot}" ]] || graphroot="/var/lib/containers/storage"
    [[ -d "${graphroot}" ]] || graphroot="${PROJECT_ROOT}"
    # `|| true`: this script is `set -euo pipefail`, so a non-zero df under pipefail would
    # abort on this bare assignment — match the `graphroot=…|| true` line above. The
    # downstream `[[ -n … ]] && (( … ))` guard is already set-e-safe (if-condition).
    avail_gb="$(df -Pk "${graphroot}" 2>/dev/null | awk 'NR==2{printf "%d", $4/1024/1024}' || true)"
    if [[ -z "${avail_gb}" ]]; then
        # Unmeasurable (df/info failed). Fail OPEN — never false-positive-block a healthy
        # build — but say so, so the case stays self-attributing rather than silently
        # regressing to the old buried mid-COPY error. The build's own no-space error backstops.
        echo "WARN: could not determine free space on ${graphroot}; skipping disk precondition (the build's own 'no space left on device' error remains the backstop)." >&2
        return 0
    fi
    if (( avail_gb < min_gb )); then
        echo "PRECONDITION_FAILURE: container-storage filesystem (${graphroot}) has ${avail_gb}GB free, below the ${min_gb}GB floor for a 4-service image build — a cold build would die mid-COPY ('no space left on device'). REASON=insufficient-disk" >&2
        echo "  Fix: reclaim space — 'podman image prune -f && podman builder prune -f' (add -af only if no parallel devloops are running). See docs/runbooks/devloop-validation.md §6.7." >&2
        exit 2
    fi
}

# Build a container image, removing the old image if it was replaced.
# Usage: build_image <tag> <dockerfile> <context-dir>
build_image() {
    local TAG="$1" DOCKERFILE="$2" CONTEXT="$3"
    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi
    # Fast-fail on insufficient host disk before the (multi-minute) cold build. Pass the
    # resolved runtime so the guard measures the SAME runtime's graphroot (see fn header).
    check_build_disk_space "$CONTAINER_CMD"
    local OLD_IMAGE_ID
    OLD_IMAGE_ID=$(${CONTAINER_CMD} images -q "$TAG" 2>/dev/null || true)
    ${CONTAINER_CMD} build -t "$TAG" -f "$DOCKERFILE" "$CONTEXT"
    if [ -n "$OLD_IMAGE_ID" ] && [ "$OLD_IMAGE_ID" != "$(${CONTAINER_CMD} images -q "$TAG")" ]; then
        ${CONTAINER_CMD} rmi "$OLD_IMAGE_ID" 2>/dev/null || true
    fi
}

# Pre-load third-party images into the Kind cluster.
# Derives the image list from Kustomize manifests (single source of truth),
# pulls to host cache if not present, then loads into Kind.
preload_third_party_images() {
    log_step "Pre-loading third-party images into Kind cluster..."

    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi

    # Extract third-party images from rendered Kustomize manifests,
    # qualifying Docker Hub short names for podman compatibility.
    local IMAGES
    IMAGES=$(${KUBECTL} kustomize "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/" \
        | grep -oP 'image:\s+\K\S+' \
        | grep -v '^localhost/' \
        | sort -u)

    if [ -z "$IMAGES" ]; then
        log_info "No third-party images found in manifests, skipping."
        return 0
    fi

    # Podman requires fully-qualified names; prefix docker.io/ for Docker Hub images
    if [[ "$CONTAINER_CMD" == "podman" ]]; then
        local qualified=""
        local img
        for img in $IMAGES; do
            if [[ "$img" != *"."*"/"* ]]; then
                qualified+="docker.io/$img"$'\n'
            else
                qualified+="$img"$'\n'
            fi
        done
        IMAGES=$(echo "$qualified" | sed '/^$/d')
    fi

    # Pull to host cache if not already present
    local img
    for img in $IMAGES; do
        if ${CONTAINER_CMD} image inspect "$img" >/dev/null 2>&1; then
            log_info "Cached: $img"
        else
            log_info "Pulling: $img"
            ${CONTAINER_CMD} pull "$img"
        fi
    done

    # Load into Kind one at a time — batching multiple images into a single
    # podman save archive corrupts tag-to-image mapping in kind load.
    for img in $IMAGES; do
        log_info "Loading: $img"
        load_image_to_kind "$img"
    done

    log_info "Third-party images pre-loaded."
}

# Deploy PostgreSQL
deploy_postgres() {
    log_step "Deploying PostgreSQL..."

    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/postgres/"

    log_info "Waiting for PostgreSQL to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=postgres -n dark-tower --timeout=120s
    log_info "PostgreSQL deployed successfully."
}

# Deploy Redis
deploy_redis() {
    log_step "Deploying Redis..."

    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/redis/"

    log_info "Waiting for Redis to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=redis -n dark-tower --timeout=120s
    log_info "Redis deployed successfully."
}

# Deploy observability stack (Prometheus, Loki, Promtail, kube-state-metrics, node-exporter, Grafana)
deploy_observability() {
    log_step "Deploying observability stack..."

    # Clean up legacy monolithic ConfigMap from older versions of this script
    ${KUBECTL} delete configmap grafana-dashboards -n dark-tower-observability --ignore-not-found

    # Apply entire observability stack via Kustomize overlay
    # (includes Grafana RBAC, deployment, service, and dashboard ConfigMaps via configMapGenerator)
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/observability/"

    log_info "Waiting for kube-state-metrics to be ready..."
    ${KUBECTL} wait --for=condition=available --timeout=120s \
        deployment/kube-state-metrics -n dark-tower-observability

    log_info "Waiting for node-exporter to be ready..."
    ${KUBECTL} rollout status daemonset/node-exporter -n dark-tower-observability --timeout=60s

    log_info "Waiting for Prometheus to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=prometheus -n dark-tower-observability --timeout=120s

    log_info "Waiting for Loki to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=loki -n dark-tower-observability --timeout=120s

    log_info "Waiting for Promtail to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=promtail -n dark-tower-observability --timeout=120s

    log_info "Waiting for Grafana to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=grafana -n dark-tower-observability --timeout=300s

    log_info "Observability stack deployed successfully."
}

# Deploy the dev OTel collector (R-59)
#
# Ordered AFTER deploy_observability/deploy_redis and BEFORE the AC/GC/MC/MH
# services. The readiness gate is load-bearing once R-55 wires init_otel: under
# R-54 fail-hard-at-init, a service started before the collector is Ready would
# fail init and CrashLoopBackoff. On THIS branch the gate only blocks on the
# collector's own readiness (no service calls init_otel yet) — see the
# collector-upgrade-discipline section in docs/runbooks/gc-deployment.md.
deploy_otel_collector() {
    log_step "Deploying OTel collector..."

    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/otel-collector/"

    log_info "Waiting for OTel collector to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=otel-collector -n dark-tower --timeout=120s
    log_info "OTel collector deployed successfully."
}

# Run database migrations
run_migrations() {
    log_step "Running database migrations..."

    # Start port-forward in background (use dynamic port for multi-cluster support)
    local pg_port="${POSTGRES_PORT:-5432}"
    ${KUBECTL} port-forward -n dark-tower svc/postgres "${pg_port}:5432" &
    PF_PID=$!

    # Give it a moment to establish
    sleep 3

    export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:${pg_port}/dark_tower"

    # Check if sqlx is available
    if command -v sqlx &> /dev/null; then
        (cd "${PROJECT_ROOT}" && sqlx migrate run)
        log_info "Migrations completed successfully."
    else
        log_warn "sqlx-cli not installed. Run migrations manually:"
        log_warn "  cargo install sqlx-cli --no-default-features --features postgres"
        log_warn "  export DATABASE_URL=\"postgresql://darktower:dev_password_change_in_production@localhost:${pg_port}/dark_tower\""
        log_warn "  sqlx migrate run"
    fi

    # Kill port-forward
    kill $PF_PID 2>/dev/null || true
}

# Seed test data (service credentials for development)
seed_test_data() {
    log_step "Seeding test service credentials..."

    # Pre-computed bcrypt hashes (cost factor 12) for development credentials
    # Generated with: python3 -c "import bcrypt; print(bcrypt.hashpw(b'PASSWORD', bcrypt.gensalt(rounds=12)).decode())"
    # Hashes are inlined below with escaped $ to avoid shell interpretation
    #
    # Credentials:
    #   global-controller / global-controller-secret-dev-001
    #   meeting-controller / meeting-controller-secret-dev-002
    #   media-handler / media-handler-secret-dev-003
    #   test-client / test-client-secret-dev-999

    # Insert credentials using idempotent ON CONFLICT DO UPDATE.
    # ON_ERROR_STOP=1: without it psql can exit 0 after a failed statement, so a
    # constraint violation here would report success (see provision_run_org's
    # header for the measurement). Applied to all three psql calls in this file
    # so a reader never has to wonder which flavour was intentional.
    if ! dt_psql -v ON_ERROR_STOP=1 -c "
INSERT INTO service_credentials (client_id, client_secret_hash, service_type, region, scopes, is_active)
VALUES
    ('global-controller', '\$2b\$12\$Gcm3fKCVQzVeCKBkVumWeu9MpAqayxTo08p4aS7xScQTCK8Fi6nBu', 'global-controller', 'us-west-2', ARRAY['service.write.mc', 'internal:meeting-token'], true),
    ('meeting-controller', '\$2b\$12\$BX5OkdvGLfsj6eTM89qkGe/mPpU2nf2aAXDK7v5sedsndrwUmG6dm', 'meeting-controller', 'us-west-2', ARRAY['service.write.mh', 'service.write.gc'], true),
    ('media-handler', '\$2b\$12\$DpQDslp37I3UFi.IBC24NOCnMWcPKkdiDO96FEACLVoXqVyYEhyZa', 'media-handler', 'us-west-2', ARRAY['service.write.mc', 'service.write.gc'], true),
    ('test-client', '\$2b\$12\$DpBLvWIsdO2j3a8dhx0VwOd8kLdZ4/szjsuZVm.TX.z4fxjlWzOny', 'global-controller', NULL, ARRAY['test:all'], true)
ON CONFLICT (client_id) DO UPDATE SET
    client_secret_hash = EXCLUDED.client_secret_hash,
    service_type = EXCLUDED.service_type,
    region = EXCLUDED.region,
    scopes = EXCLUDED.scopes,
    is_active = EXCLUDED.is_active,
    updated_at = NOW();
"; then
        # Reachability fix: the previous `if [ $? -eq 0 ]` AFTER the command was
        # dead code under the `set -euo pipefail` at the top of this file — a
        # failing psql aborted the script before the check could run, so the
        # error branch never printed and the failure was silent. Testing the
        # command IN the if-condition is set-e-safe (conditions are exempt), so
        # the branch is now reachable AND still fails the run. Same trap already
        # documented above seed_demo_org.
        log_error "Failed to seed test credentials."
        return 1
    fi
    log_info "Test credentials seeded successfully."

    # Seed test organization for env-tests (required by user registration flow).
    # TODO: Replace with AC org provisioning API (see docs/TODO.md)
    #
    # COUPLED: the subdomain literal below backs the DOCUMENTED MANUAL env-test
    # invocation `ENV_TEST_ORG_SUBDOMAIN=devtest cargo test -p env-tests`
    # (crates/env-tests/src/fixtures/auth_client.rs, resolve_org_subdomain()).
    # That variable is REQUIRED with no fallback, so this row is not a silent
    # default for anything — but rename it and the documented manual invocation
    # stops working with an auth error that looks unrelated to seeding. A
    # matching cross-reference lives at the auth_client.rs end.
    #
    # Layer 7 does NOT use this org: it provisions a fresh one per run via
    # provision_run_org() so repeated runs cannot accumulate against one cap.
    log_step "Seeding test organization..."
    if ! dt_psql -v ON_ERROR_STOP=1 -c "
INSERT INTO organizations (subdomain, display_name, plan_tier, max_concurrent_meetings)
VALUES ('devtest', 'Development Test Organization', 'enterprise', ${ORG_MAX_CONCURRENT_MEETINGS})
ON CONFLICT (subdomain) DO UPDATE SET max_concurrent_meetings = ${ORG_MAX_CONCURRENT_MEETINGS};
"; then
        log_error "Failed to seed test organization."
        return 1
    fi
    log_info "Test organization seeded successfully."
}

# Seed the 'demo' organization (R-38) — the default org the browser sign-up flow
# registers against. Idempotent via ON CONFLICT DO NOTHING (unlike seed_test_data's
# devtest DO UPDATE): once present, the demo org is never clobbered on re-run.
# The 2-column INSERT is valid — display_name is the only NOT-NULL column without a
# default beyond subdomain; plan_tier ('free') and max_concurrent_meetings (10) take
# their schema defaults (the right profile for a user-facing demo org), and is_active
# defaults true so the row is immediately usable for sign-up. No DB schema change.
seed_demo_org() {
    log_step "Seeding demo organization (browser sign-up default)..."
    # Direct exit-code check in the if-condition: set-e-safe (conditions are exempt),
    # so the failure branch is actually reachable — unlike a post-hoc `$?` test under
    # `set -e`, where a failed command aborts before the check.
    #
    # Disposition differs from seed_test_data on purpose, and the justification
    # is checkable rather than a matter of taste: `demo` is NOT a precondition of
    # any suite. The browser suite requires E2E_ORG_SUBDOMAIN with NO default and
    # throws at config-load if it is unset (packages/web-app/e2e/env.ts,
    # readSubdomain), deriving E2E_BASE_URL's host label from it; the Rust suite
    # uses `devtest`. So `demo` backs only a human opening the demo UI by hand
    # after a manual setup, and a failure here is logged while setup continues.
    # If a suite is ever repointed at `demo`, this must become fatal like
    # seed_test_data — continue-on-error would then be a masked precondition
    # failure reported as a suite failure, which R-7 forbids.
    # Deliberately NOT switched to ORG_MAX_CONCURRENT_MEETINGS — see the
    # carve-out on that constant.
    if dt_psql -v ON_ERROR_STOP=1 -c "
INSERT INTO organizations (subdomain, display_name)
VALUES ('demo', 'Demo Organization')
ON CONFLICT (subdomain) DO NOTHING;
"; then
        log_info "Demo organization seeded successfully."
    else
        log_error "Failed to seed demo organization."
    fi
}

# --- Per-run organization provisioning (R-7) ---------------------------------
# Creates ONE freshly generated organization per layer-7 run, so the Nth
# consecutive run against the same dev cluster reaches the same verdict as the
# first. Without it every run's meetings accumulate against a single org's
# max_concurrent_meetings — nothing in production code marks a meeting ended —
# and the browser suite's ~7 meetings/run exhaust the seeded cap on the second
# run, reported as a failure of the code under test.
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
# Args: $1 = subdomain (lowercase, DNS-label shaped)
# Env:  DT_CLUSTER_NAME (required, no fallback)
# Stdout: one line `PROVISIONED_ORG org_id=<uuid> subdomain=<sub>`
provision_run_org() {
    local sub="${1:-}"
    local display_name="Layer-7 run ${sub}"

    # (1) Cluster identity must be EXPLICIT on this path — no fallback.
    # The global default at the top of this file (${DT_CLUSTER_NAME:-dark-tower})
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
    # leading-hyphen value smuggled in by `--provision-org --skip-build`.
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

# Create AC service secrets
create_ac_secrets() {
    log_step "Creating AC service secrets..."

    # Use consistent dev master key (same as local dev scripts)
    local MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    local DB_URL="postgresql://darktower:dev_password_change_in_production@postgres.dark-tower.svc.cluster.local:5432/dark_tower"

    ${KUBECTL} create secret generic ac-service-secrets \
        --from-literal=DATABASE_URL="${DB_URL}" \
        --from-literal=AC_MASTER_KEY="${MASTER_KEY}" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "AC service secrets created."
}

# Build and deploy AC service
deploy_ac_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building AC service container image..."
        build_image localhost/ac-service:latest infra/docker/ac-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/ac-service:latest
    else
        log_warn "Skipping AC image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying AC service to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/ac-service/"

    log_info "Waiting for AC service to be ready..."
    ${KUBECTL} rollout status statefulset/ac-service -n dark-tower --timeout=180s

    log_info "AC service deployed successfully."
}

# Build and deploy Global Controller service
deploy_gc_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building Global Controller container image..."
        build_image localhost/gc-service:latest infra/docker/gc-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/gc-service:latest
    else
        log_warn "Skipping GC image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying Global Controller to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/gc-service/"

    log_info "Waiting for Global Controller to be ready..."
    ${KUBECTL} rollout status deployment/gc-service -n dark-tower --timeout=180s

    log_info "Global Controller deployed successfully."
}

# Generate TLS certificates and create MC TLS secret
create_mc_tls_secret() {
    log_step "Generating TLS certificates for MC WebTransport..."

    # Generate dev certs (idempotent — reuses CA if it already exists)
    "${PROJECT_ROOT}/scripts/generate-dev-certs.sh"

    log_step "Creating mc-service-tls Secret from generated certs..."
    ${KUBECTL} create secret tls mc-service-tls \
        --cert="${PROJECT_ROOT}/infra/docker/certs/mc-webtransport.crt" \
        --key="${PROJECT_ROOT}/infra/docker/certs/mc-webtransport.key" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "MC TLS secret created successfully."
}

# Build and deploy Meeting Controller service
deploy_mc_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building Meeting Controller container image..."
        build_image localhost/mc-service:latest infra/docker/mc-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/mc-service:latest
    else
        log_warn "Skipping MC image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying Meeting Controller to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/mc-service/"

    # Devloop mode: patch MC advertise addresses to use host-gateway IP + dynamic ports.
    # Static ConfigMaps default to localhost:4433/4435 which are unreachable from the
    # devloop container. The gateway IP is reachable from both container and host.
    # TLS SAN coverage is not required — dev-mode clients use cert validation bypass.
    if [[ -n "${DT_HOST_GATEWAY_IP:-}" ]]; then
        log_info "Patching MC-0 advertise address: https://${DT_HOST_GATEWAY_IP}:${MC_0_WEBTRANSPORT_PORT}"
        ${KUBECTL} patch configmap mc-0-config -n dark-tower \
            --type merge -p "{\"data\":{\"MC_WEBTRANSPORT_ADVERTISE_ADDRESS\":\"https://${DT_HOST_GATEWAY_IP}:${MC_0_WEBTRANSPORT_PORT}\"}}"
        log_info "Patching MC-1 advertise address: https://${DT_HOST_GATEWAY_IP}:${MC_1_WEBTRANSPORT_PORT}"
        ${KUBECTL} patch configmap mc-1-config -n dark-tower \
            --type merge -p "{\"data\":{\"MC_WEBTRANSPORT_ADVERTISE_ADDRESS\":\"https://${DT_HOST_GATEWAY_IP}:${MC_1_WEBTRANSPORT_PORT}\"}}"
        ${KUBECTL} rollout restart deployment/mc-0 deployment/mc-1 -n dark-tower
    fi

    log_info "Waiting for Meeting Controller to be ready..."
    ${KUBECTL} rollout status deployment/mc-0 -n dark-tower --timeout=180s
    ${KUBECTL} rollout status deployment/mc-1 -n dark-tower --timeout=180s

    log_info "Meeting Controller deployed successfully."
}

# Create MH service secrets
create_mh_secrets() {
    log_step "Creating MH service secrets..."

    ${KUBECTL} create secret generic mh-service-secrets \
        --from-literal=MH_CLIENT_SECRET="media-handler-secret-dev-003" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "MH service secrets created."
}

# Generate TLS certificates and create MH TLS secret
create_mh_tls_secret() {
    log_step "Generating TLS certificates for MH WebTransport..."

    # Generate dev certs (idempotent — reuses CA if it already exists)
    "${PROJECT_ROOT}/scripts/generate-dev-certs.sh"

    log_step "Creating mh-service-tls Secret from generated certs..."
    ${KUBECTL} create secret tls mh-service-tls \
        --cert="${PROJECT_ROOT}/infra/docker/certs/mh-webtransport.crt" \
        --key="${PROJECT_ROOT}/infra/docker/certs/mh-webtransport.key" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "MH TLS secret created successfully."
}

# Build and deploy Media Handler service
deploy_mh_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building Media Handler container image..."
        build_image localhost/mh-service:latest infra/docker/mh-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/mh-service:latest
    else
        log_warn "Skipping MH image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying Media Handler to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/mh-service/"

    # Devloop mode: patch MH advertise addresses (same pattern as MC above).
    if [[ -n "${DT_HOST_GATEWAY_IP:-}" ]]; then
        log_info "Patching MH-0 advertise address: https://${DT_HOST_GATEWAY_IP}:${MH_0_WEBTRANSPORT_PORT}"
        ${KUBECTL} patch configmap mh-0-config -n dark-tower \
            --type merge -p "{\"data\":{\"MH_WEBTRANSPORT_ADVERTISE_ADDRESS\":\"https://${DT_HOST_GATEWAY_IP}:${MH_0_WEBTRANSPORT_PORT}\"}}"
        log_info "Patching MH-1 advertise address: https://${DT_HOST_GATEWAY_IP}:${MH_1_WEBTRANSPORT_PORT}"
        ${KUBECTL} patch configmap mh-1-config -n dark-tower \
            --type merge -p "{\"data\":{\"MH_WEBTRANSPORT_ADVERTISE_ADDRESS\":\"https://${DT_HOST_GATEWAY_IP}:${MH_1_WEBTRANSPORT_PORT}\"}}"
        ${KUBECTL} rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower
    fi

    log_info "Waiting for Media Handler to be ready..."
    ${KUBECTL} rollout status deployment/mh-0 -n dark-tower --timeout=180s
    ${KUBECTL} rollout status deployment/mh-1 -n dark-tower --timeout=180s

    log_info "Media Handler deployed successfully."
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
    echo "    DATABASE_URL: postgresql://darktower:dev_password_change_in_production@localhost:${p_postgres}/dark_tower"
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
    echo "Restart In-Cluster Services:"
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

# Deploy a single service (used by --only flag)
deploy_only_service() {
    local svc="$1"

    # Verify cluster exists
    if ! kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_error "Cluster '${CLUSTER_NAME}' does not exist. Run full setup first (without --only)."
        exit 1
    fi

    check_prerequisites

    case "$svc" in
        ac)
            create_ac_secrets
            deploy_ac_service
            ;;
        gc)
            deploy_gc_service
            ;;
        mc)
            create_mc_tls_secret
            deploy_mc_service
            ;;
        mh)
            create_mh_secrets
            create_mh_tls_secret
            deploy_mh_service
            ;;
        otel)
            deploy_otel_collector
            ;;
        *)
            log_error "Unknown service '${svc}'"
            exit 1
            ;;
    esac

    log_info "Service '${svc}' deployed successfully."
}

# Main
main() {
    # --provision-org: terminal, CONTAINER-RUNNABLE mode. This branch is FIRST
    # and returns immediately — see EXECUTION CONTEXT in the file header. It must
    # not fall through to check_prerequisites(), which requires kind + podman or
    # docker (absent in the devloop container, so the run would die on "Neither
    # Podman nor Docker found" — the wrong problem, and unfixable from there),
    # nor reach seed_test_data()/create_ac_secrets(), which put credential
    # material into psql/kubectl arguments that would land in the layer-7 log.
    # The early return is therefore a containment control as well as a
    # correctness one. --only is NOT the precedent to copy: it routes through
    # deploy_only_service(), which calls `kind get clusters` + check_prerequisites.
    if [[ -n "${PROVISION_ORG_SUB}" ]]; then
        provision_run_org "${PROVISION_ORG_SUB}" || return 1
        return 0
    fi

    # --only: targeted single-service rebuild+redeploy
    if [[ -n "${ONLY_SERVICE}" ]]; then
        log_info "Rebuilding and redeploying service '${ONLY_SERVICE}'..."
        echo ""
        deploy_only_service "${ONLY_SERVICE}"
        return
    fi

    log_info "Setting up Dark Tower local development environment..."
    echo ""

    check_prerequisites
    create_cluster
    install_calico
    create_namespaces
    preload_third_party_images
    deploy_postgres
    deploy_redis
    deploy_observability
    deploy_otel_collector
    run_migrations
    seed_test_data
    seed_demo_org
    create_ac_secrets
    deploy_ac_service
    deploy_gc_service
    create_mc_tls_secret
    deploy_mc_service
    create_mh_secrets
    create_mh_tls_secret
    deploy_mh_service
    install_telepresence
    setup_port_forwards
    print_access_info
}

# Run main only when executed, not when sourced (the self-test sources this script to
# exercise check_build_disk_space directly). Mirrors scripts/layer7.sh's BASH_SOURCE guard.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
