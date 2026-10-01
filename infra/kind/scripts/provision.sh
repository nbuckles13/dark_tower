#!/usr/bin/env bash
#
# provision.sh — PROVISIONING for the Kind dev cluster (ADR-0038 §1): make sure a
# cluster exists that matches the BLUEPRINT, rebuilding it only when it does not.
#
# The blueprint is a deterministic render (blueprint_render) of everything
# provisioning consumes to build what cannot change in place: the kind
# configuration (the helper's per-slug render via DT_KIND_CONFIG, else
# infra/kind/kind-config.yaml), the kind version (which pins the node image),
# the container provider, the provisioning implementation itself (every file in
# PROVISION_INPUTS, hashed whole — this script, lib/common.sh, the secret/TLS
# recipe scripts/generate-dev-certs.sh, the Postgres credential the AC Secret
# is derived from) and the PUBLIC MC/MH WebTransport leaf certificates the TLS
# Secrets carry (a renewal — 14-day leaves — must reach the cluster). Never key
# bytes, never a Secret value. RULE: anything provision reads is a render
# input — sourced/executed/read files only through provision_input.
#
# Its sha256 is recorded in the cluster (kube-system/configmap/devloop-blueprint)
# after, and ONLY after, a successful build. So:
#   recorded == current           -> nothing to do (ACTION=none)
#   missing (no cluster, a half-built one, a pre-ADR-0038 one) or changed
#                                 -> kind delete + build + record (ACTION=rebuild)
#   record unreadable             -> fail loudly; never destroy on uncertainty (ACTION=refuse)
#
# Every provision FAILURE ends with ONE line, classified at the source (Layer 7
# routes on its REASON — scripts/layer7.sh:CLUSTER_ENV_REASONS):
#   PROVISION_FAILED REASON=<prerequisite-missing|blueprint-unreadable|operator-declined|port-held|runtime-incapable> STEP=<step>
#   PROVISION_FAILED REASON=<runtime-unreachable|apiserver-unreachable|step-failed> STEP=<step>
#     (from the EXIT trap: the shared bounded probes, else `step-failed` — the tree's)
# Every run prints ONE decision line:
#   BLUEPRINT ACTION=none|rebuild|refuse|check REASON=match|missing|changed|unreadable RECORDED=<hash12|none> CURRENT=<hash12> CHANGED=<sections|->
#
# NON-COLLAPSE (reciprocal with crates/devloop-helper/src/commands.rs::cmd_recreate):
# provision MUST destroy a HEALTHY cluster whose blueprint changed — the running
# cluster is stale by definition. cmd_recreate is its INVERSE: it MUST REFUSE to
# destroy a healthy cluster. Two semantically-opposite operations; do NOT let a
# DRY pass merge them.
#
# The record is an ANTI-DRIFT control, NOT integrity or tamper-evidence: this
# script lives in the container-writable devloop clone, and the container's
# cluster-admin kubeconfig can rewrite kube-system/devloop-blueprint.
#
# HOST-ONLY (kind + podman/docker). Run by the devloop helper's `provision`
# verb (ADR-0030), by setup.sh, or by hand.
#
# Usage:
#   ./infra/kind/scripts/provision.sh [--yes]   # provision (rebuild only when needed)
#   ./infra/kind/scripts/provision.sh --check   # compare only: rc 0 match, rc 1 otherwise
#   ./infra/kind/scripts/provision.sh --blueprint   # print the render (pure; no writes)
#
# --yes answers the rebuild prompt. A rebuild that must DESTROY an existing
# cluster non-interactively requires DT_CLUSTER_NAME set explicitly: it never
# falls back to the default `dark-tower` cluster.
#
# Environment: DT_CLUSTER_NAME, DT_PORT_MAP, DT_HOST_GATEWAY_IP (lib/common.sh),
# DT_KIND_CONFIG (the rendered kind config to build from).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

# The provisioning implementation's inputs, repo-relative. ONE list serves
# sourcing/executing/reading them (provision_input) AND the blueprint render,
# so nothing provision consumes can be left out of the hash.
# scripts/setup.test.sh pins that this script reaches no file except through
# provision_input (and the two accessors below).
PROVISION_SH_REL="infra/kind/scripts/provision.sh"
LIB_COMMON_REL="infra/kind/scripts/lib/common.sh"
CERTS_RECIPE_REL="scripts/generate-dev-certs.sh"
PG_SECRET_REL="infra/services/postgres/secret.yaml"
PROVISION_INPUTS=("${PROVISION_SH_REL}" "${LIB_COMMON_REL}" "${CERTS_RECIPE_REL}" "${PG_SECRET_REL}")

# Absolute path of a PROVISION_INPUT; any other path is refused.
provision_input() {
    local rel="$1" p
    for p in "${PROVISION_INPUTS[@]}"; do
        if [[ "${p}" == "${rel}" ]]; then
            printf '%s/%s\n' "${PROJECT_ROOT}" "${rel}"   # provision-input-accessor
            return 0
        fi
    done
    echo "ERROR: provision.sh read '${rel}', which is not a PROVISION_INPUT — add it to the list so the blueprint hashes it" >&2
    exit 1
}

# shellcheck source=lib/common.sh
source "$(provision_input "${LIB_COMMON_REL}")"
dt_init_cluster_env

# The kind configuration to build from (a blueprint section of its own).
KIND_CONFIG="${DT_KIND_CONFIG:-${PROJECT_ROOT}/infra/kind/kind-config.yaml}"   # provision-input-accessor
CALICO_VERSION="v3.27.0"
# sha256 of the manifest at CALICO_MANIFEST_URL, taken from the fetched artifact
# (2026-09-29). Applied with cluster-admin, so it is verified before it is
# applied: a moved tag or a substituted download fails loudly instead of changing
# the platform with no change to the blueprint (the pin lives in this file, so
# it IS part of the blueprint). Residual, accepted for a dev cluster: the
# manifest itself names its images by tag (docker.io/calico/*:v3.27.0), not
# digest.
CALICO_MANIFEST_SHA256="dbc4d6fdb5ca87978f6d67a43b09094852cd375398cad5c1650c1c462dd58d8b"
CALICO_MANIFEST_URL="https://raw.githubusercontent.com/projectcalico/calico/${CALICO_VERSION}/manifests/calico.yaml"

# The TLS leaves provision turns into Secrets (<leaf>.crt/.key under
# infra/docker/certs/, written by the recipe). Only the PUBLIC .crt enters the
# render.
TLS_LEAVES=(mc-webtransport mh-webtransport)
tls_file() {  # $1 = leaf, $2 = crt|key
    printf '%s/infra/docker/certs/%s.%s\n' "${PROJECT_ROOT}" "$1" "$2"   # provision-input-accessor
}

BLUEPRINT_NS="kube-system"
BLUEPRINT_CM="devloop-blueprint"
BLUEPRINT_FORMAT="dt-blueprint v1"
# Exit code generate-dev-certs.sh --check-renewal uses for "a leaf is due".
RENEWAL_DUE_RC=10

MODE=provision
AUTO_YES=false
[[ -t 0 ]] || AUTO_YES=true

print_usage() {
    sed -n '2,/^$/{ s/^# \?//; p }' "${BASH_SOURCE[0]}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --yes) AUTO_YES=true; shift ;;
        --check) MODE=check; shift ;;
        --blueprint) MODE=blueprint; shift ;;
        --help) print_usage; exit 0 ;;
        *)
            echo "ERROR: Unknown option '$1'" >&2
            print_usage >&2
            exit 1
            ;;
    esac
done

# --- The blueprint ----------------------------------------------------------

# PURE: reads files and `kind version`, touches no cluster, writes nothing.
# One `<label> <value>` line per input; the label is the section name a
# mismatch reports (CHANGED=). Values are sha256s of WHOLE input files — secret-
# bearing ones included (postgres/secret.yaml; this script's own AC literals): a
# digest, never content — plus the kind version string and the provider name.
# Never put a raw line of any input here.
#
# Every file is hashed by ONE `sha256sum` call (one process, not one per input)
# and each digest is taken back BY POSITION — never by parsing a file name back
# out, never by echoing sha256sum's own lines. Its rc, the digest count and each
# digest's shape are checked: an input that cannot be hashed FAILS the render
# (it must never render an empty digest).
blueprint_render() {
    local rel p leaf crt kind_version out line i
    local -a labels=() paths=() lines=() sums=()
    if [[ ! -f "${KIND_CONFIG}" ]]; then
        echo "ERROR: kind config not found: ${KIND_CONFIG}" >&2
        return 1
    fi
    kind_version="$(kind version)" || { echo "ERROR: 'kind version' failed" >&2; return 1; }
    # labels[i] names paths[i]: appended together, so they cannot drift apart.
    labels+=("kind-config"); paths+=("${KIND_CONFIG}")
    for rel in "${PROVISION_INPUTS[@]}"; do
        p="$(provision_input "${rel}")" || return 1
        labels+=("file:${rel}"); paths+=("${p}")
    done
    for leaf in "${TLS_LEAVES[@]}"; do
        crt="$(tls_file "${leaf}" crt)"
        if [[ -f "${crt}" ]]; then
            labels+=("tls-cert:${leaf}.crt"); paths+=("${crt}")
        fi
    done
    if ! out="$(sha256sum -- "${paths[@]}")"; then
        echo "ERROR: could not hash every blueprint input (sha256sum names the file above); refusing a partial render" >&2
        return 1
    fi
    mapfile -t lines <<< "${out}"
    if (( ${#lines[@]} != ${#paths[@]} )); then
        echo "ERROR: sha256sum returned ${#lines[@]} digests for ${#paths[@]} blueprint inputs; refusing a partial render" >&2
        return 1
    fi
    for line in "${lines[@]}"; do
        line="${line%% *}"
        line="${line#\\}"   # GNU marks an escaped file name with a leading backslash
        if [[ ! "${line}" =~ ^[0-9a-f]{64}$ ]]; then
            echo "ERROR: unexpected sha256sum output for ${labels[${#sums[@]}]}; refusing a partial render" >&2
            return 1
        fi
        sums+=("${line}")
    done
    echo "${BLUEPRINT_FORMAT}"
    echo "${labels[0]} sha256:${sums[0]}"
    echo "kind-version ${kind_version//$'\n'/ }"
    echo "provider ${KIND_EXPERIMENTAL_PROVIDER:?provider not detected}"
    for (( i = 1; i <= ${#PROVISION_INPUTS[@]}; i++ )); do
        echo "${labels[i]} sha256:${sums[i]}"
    done
    # TLS leaves in TLS_LEAVES order, a missing one in its own place. i = the first
    # TLS entry of labels/sums (after kind-config and the PROVISION_INPUTS).
    i=$(( 1 + ${#PROVISION_INPUTS[@]} ))
    for leaf in "${TLS_LEAVES[@]}"; do
        if [[ "${labels[i]:-}" == "tls-cert:${leaf}.crt" ]]; then
            echo "${labels[i]} sha256:${sums[i]}"
            i=$(( i + 1 ))
        else
            echo "tls-cert:${leaf}.crt missing"
        fi
    done
}

# Labels of the lines that differ between two renders ($1 recorded, $2 current),
# comma-joined, in current-render order (then recorded-only labels).
changed_sections() {
    awk 'NR == FNR { rec[$1] = $0; next }
         { seen[$1] = 1; if (!($1 in rec) || rec[$1] != $0) out = out (out ? "," : "") $1 }
         END { for (l in rec) if (!(l in seen)) out = out (out ? "," : "") l; print out }' \
        <(printf '%s\n' "$1") <(printf '%s\n' "$2")
}

# Compare the current render with the cluster's record. Sets BP_REASON
# (match|missing|changed|unreadable), BP_MANIFEST, BP_HASH, BP_RECORDED,
# BP_CHANGED, BP_DETAIL. Returns non-zero only if the render itself failed.
blueprint_decide() {
    local exists_rc=0 rec_hash rec_manifest
    BP_MANIFEST="$(blueprint_render)" || return 1
    BP_HASH="$(printf '%s\n' "${BP_MANIFEST}" | sha256sum | cut -d' ' -f1)"
    BP_RECORDED=""
    BP_CHANGED="-"
    BP_DETAIL=""
    cluster_exists || exists_rc=$?
    if (( exists_rc == 2 )); then
        BP_REASON=unreadable; BP_DETAIL="'kind get clusters' failed"; return 0
    fi
    if (( exists_rc == 1 )); then
        BP_REASON=missing; BP_DETAIL="no cluster '${CLUSTER_NAME}'"; return 0
    fi
    local errf
    errf="$(mktemp "${TMPDIR:-/tmp}/dt-blueprint-err.XXXXXX")"
    if ! rec_hash="$(${KUBECTL} get configmap "${BLUEPRINT_CM}" -n "${BLUEPRINT_NS}" --ignore-not-found -o jsonpath='{.data.hash}' 2>"${errf}")" \
        || ! rec_manifest="$(${KUBECTL} get configmap "${BLUEPRINT_CM}" -n "${BLUEPRINT_NS}" --ignore-not-found -o jsonpath='{.data.manifest}' 2>"${errf}")"; then
        BP_REASON=unreadable
        BP_DETAIL="could not read ${BLUEPRINT_NS}/${BLUEPRINT_CM}: $(tail -n 3 "${errf}" | tr '\n' ' ')"
        rm -f "${errf}"
        return 0
    fi
    rm -f "${errf}"
    if [[ ! "${rec_hash}" =~ ^[0-9a-f]{64}$ ]]; then
        BP_REASON=missing
        BP_DETAIL="cluster '${CLUSTER_NAME}' has no (parseable) blueprint record — half-built, or built before ADR-0038 step 3"
        return 0
    fi
    BP_RECORDED="${rec_hash}"
    if [[ "${rec_hash}" == "${BP_HASH}" ]]; then
        BP_REASON=match
    else
        BP_REASON=changed
        BP_CHANGED="$(changed_sections "${rec_manifest}" "${BP_MANIFEST}")"
        [[ -n "${BP_CHANGED}" ]] || BP_CHANGED="unknown"
    fi
}

# Append a cause to CHANGED= (keeping it a comma list, `-` when empty).
bp_add_changed() {
    if [[ "${BP_CHANGED}" == "-" || -z "${BP_CHANGED}" ]]; then BP_CHANGED="$1"; else BP_CHANGED="${BP_CHANGED},$1"; fi
}

# The ONE decision line (stable key=value vocabulary; grep it across runs).
bp_emit() {  # $1 = ACTION
    local rec="${BP_RECORDED:0:12}"
    echo "BLUEPRINT ACTION=$1 REASON=${BP_REASON} RECORDED=${rec:-none} CURRENT=${BP_HASH:0:12} CHANGED=${BP_CHANGED}"
    [[ -z "${BP_DETAIL}" ]] || echo "  (${BP_DETAIL})"
}

# rc 0 = no TLS leaf is due for renewal, RENEWAL_DUE_RC = one is (or none
# exists yet); anything else is a failure of the recipe itself. Read-only: the
# recipe's own reuse predicate decides, so "due" here and "renewed" by
# materialize_inputs can never disagree.
tls_renewal_due() {
    local rc=0
    "$(provision_input "${CERTS_RECIPE_REL}")" --check-renewal >/dev/null 2>&1 || rc=$?
    case "${rc}" in
        0) return 1 ;;
        "${RENEWAL_DUE_RC}") return 0 ;;
        *) log_error "${CERTS_RECIPE_REL} --check-renewal failed (rc ${rc})"; exit 1 ;;
    esac
}

# Materialize the host-side inputs the build consumes BEFORE rendering, so what
# is hashed is exactly what is built: the idempotent recipe renews a TLS leaf
# only when it is due (and mints them on a fresh host).
materialize_inputs() {
    log_step "Materializing dev TLS material (renews a leaf only when due)..."
    DT_CERTS_CALLER=provision "$(provision_input "${CERTS_RECIPE_REL}")"
}

# Record the blueprint in the cluster — the LAST step of a successful build.
record_blueprint() {
    local tmp
    tmp="$(mktemp "${TMPDIR:-/tmp}/dt-blueprint.XXXXXX")"
    printf '%s\n' "${BP_MANIFEST}" > "${tmp}"
    # The record is an anti-drift control, NOT integrity/tamper-evidence (see header).
    if ! ${KUBECTL} create configmap "${BLUEPRINT_CM}" -n "${BLUEPRINT_NS}" \
            --from-literal=hash="${BP_HASH}" --from-file=manifest="${tmp}" >/dev/null; then
        rm -f "${tmp}"
        log_error "Could not record the blueprint in ${BLUEPRINT_NS}/${BLUEPRINT_CM}; the next provision will rebuild."
        return 1
    fi
    rm -f "${tmp}"
    log_info "Recorded blueprint ${BP_HASH:0:12} in ${BLUEPRINT_NS}/${BLUEPRINT_CM}."
}

# --- The build --------------------------------------------------------------

# Export the kind-<cluster> context into the active kubeconfig (#2). SINGLE source
# of the host-kubeconfig write, run on EVERY provision that ends with a cluster —
# after a build AND on the unchanged (no-op) path.
#
# `kind create cluster` writes this context as a side effect — but the no-op path
# never creates, so a caller whose active kubeconfig lacks the kind-<cluster>
# context would get kubectl falling back to localhost:8080 -> "connection
# refused" -> a healthy cluster mis-read as broken (a masked failure). Making the
# write explicit + idempotent on every path closes that.
#
# `kind export kubeconfig` MERGES into the target ($KUBECONFIG, else ~/.kube/config):
# it adds/updates the kind-<cluster> context and switches current-context toward the
# kind cluster (deliberate — every subsequent kubectl here is explicit-context anyway,
# and the switch is the operator convenience), and it does NOT clobber the operator's
# other contexts. This is the HOST $KUBECONFIG — a deliberately DISTINCT artifact from
# the devloop-helper's container kubeconfig (crates/devloop-helper's
# generate_container_kubeconfig writes /tmp/devloop/kubeconfig); do not collapse them.
#
# LOUD on failure: a swallowed failure here is byte-identical to the bug being fixed.
write_kubeconfig() {
    # Report the actual KUBECONFIG state, not the selection RULE — the person who hits
    # the reuse bug is the one with KUBECONFIG set to something they've forgotten. Name
    # the variable's value rather than computing a single path: KUBECONFIG may be a
    # colon-separated list and `kind export` writes only its FIRST entry, so a computed
    # "~/.kube/config" fallback would be confidently wrong in that case.
    log_info "Exporting kubeconfig context 'kind-${CLUSTER_NAME}' (KUBECONFIG=${KUBECONFIG:-<unset, using ~/.kube/config>})..."
    if ! kind export kubeconfig --name "${CLUSTER_NAME}"; then
        log_error "Failed to export kubeconfig for cluster '${CLUSTER_NAME}' (KUBECONFIG=${KUBECONFIG:-<unset, using ~/.kube/config>})."
        log_error "  Without the kind-${CLUSTER_NAME} context, kubectl falls back to localhost:8080"
        log_error "  and a healthy cluster reads as unreachable. Aborting."
        exit 1
    fi
}


# Create the kind cluster from the rendered config. Provision only ever builds
# a FRESH cluster: an existing one was deleted first (or never existed).
create_cluster() {
    log_step "Creating kind cluster '${CLUSTER_NAME}' with Calico CNI..."
    if ! kind create cluster --config="${KIND_CONFIG}" --name="${CLUSTER_NAME}"; then
        if node_runtime_incapable; then
            provision_failed runtime-incapable
            log_error "The container runtime answers but cannot start a kind node container here (a privileged container with its own hostname failed on the node image). Host-side fix — see docs/TODO.md §G (e.g. runc instead of crun, the nested provider's userns/capabilities, or kind on the host)."
        fi
        return 1
    fi
    write_kubeconfig

    # NOTE: Don't wait for nodes here - they won't be Ready until Calico is installed
    # (because we set disableDefaultCNI: true in kind-config.yaml)
    log_info "Cluster created. Installing CNI before nodes can become Ready..."
}

# Install Calico CNI
install_calico() {
    log_step "Installing Calico CNI for NetworkPolicy enforcement..."

    local manifest actual
    manifest="$(mktemp "${TMPDIR:-/tmp}/dt-calico.XXXXXX")"
    if ! curl -fsSL --proto '=https' --tlsv1.2 -o "${manifest}" "${CALICO_MANIFEST_URL}"; then
        rm -f "${manifest}"
        log_error "Could not download the Calico manifest (${CALICO_MANIFEST_URL})."
        return 1
    fi
    actual="$(sha256sum < "${manifest}" | cut -d' ' -f1)"
    if [[ "${actual}" != "${CALICO_MANIFEST_SHA256}" ]]; then
        rm -f "${manifest}"
        log_error "Calico manifest integrity check FAILED: expected sha256 ${CALICO_MANIFEST_SHA256}, got ${actual} (${CALICO_MANIFEST_URL}). Refusing to apply it. A deliberate Calico change updates CALICO_VERSION and CALICO_MANIFEST_SHA256 together."
        return 1
    fi
    if ! ${KUBECTL} create -f "${manifest}"; then
        rm -f "${manifest}"
        return 1
    fi
    rm -f "${manifest}"

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

# Create AC service secrets. The DATABASE_URL is DERIVED from the Postgres
# Secret (a PROVISION_INPUT), so a password change there changes the blueprint
# and rebuilds AC's copy with it — never a second, drifting literal.
create_ac_secrets() {
    log_step "Creating AC service secrets..."

    # Use consistent dev master key (same as local dev scripts)
    local MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    local pg_user pg_pass pg_db DB_URL
    pg_user="$(pg_secret_value POSTGRES_USER "$(provision_input "${PG_SECRET_REL}")")" || return 1
    pg_pass="$(pg_secret_value POSTGRES_PASSWORD "$(provision_input "${PG_SECRET_REL}")")" || return 1
    pg_db="$(pg_secret_value POSTGRES_DB "$(provision_input "${PG_SECRET_REL}")")" || return 1
    # Embedded in a URL unencoded: refuse anything that would need encoding
    # rather than build a URL that silently means something else.
    if [[ ! "${pg_user}${pg_pass}${pg_db}" =~ ^[A-Za-z0-9._~-]+$ ]]; then
        log_error "${PG_SECRET_REL}: POSTGRES_USER/PASSWORD/DB must be URL-safe ([A-Za-z0-9._~-]) to be embedded in AC's DATABASE_URL"
        return 1
    fi
    DB_URL="postgresql://${pg_user}:${pg_pass}@postgres.dark-tower.svc.cluster.local:5432/${pg_db}"

    ${KUBECTL} create secret generic ac-service-secrets \
        --from-literal=DATABASE_URL="${DB_URL}" \
        --from-literal=AC_MASTER_KEY="${MASTER_KEY}" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "AC service secrets created."
}

# Create a WebTransport TLS Secret from the materialized leaf.
# Usage: create_tls_secret <mc|mh>
create_tls_secret() {
    local svc="$1"
    log_step "Creating ${svc}-service-tls Secret from generated certs..."
    ${KUBECTL} create secret tls "${svc}-service-tls" \
        --cert="$(tls_file "${svc}-webtransport" crt)" \
        --key="$(tls_file "${svc}-webtransport" key)" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -
    log_info "${svc^^} TLS secret created successfully."
}

# Delete the existing cluster so it can be rebuilt. Destroys exactly
# CLUSTER_NAME; a non-interactive destroy requires the caller to have NAMED it
# (DT_CLUSTER_NAME), so no automated path can ever delete the default cluster.
destroy_for_rebuild() {
    if [[ "${AUTO_YES}" == "true" ]]; then
        if [[ "${CLUSTER_NAME_EXPLICIT}" != "true" ]]; then
            provision_failed operator-declined
            log_error "Refusing to delete cluster '${CLUSTER_NAME}' non-interactively: DT_CLUSTER_NAME is not set, and a destroy never falls back to the default name. Set DT_CLUSTER_NAME=${CLUSTER_NAME} to confirm, or run interactively."
            exit 1
        fi
    else
        read -p "Cluster '${CLUSTER_NAME}' does not match the blueprint (${BP_REASON}). Delete and rebuild it? [y/N] " -n 1 -r
        echo
        if [[ ! ${REPLY:-} =~ ^[Yy]$ ]]; then
            provision_failed operator-declined
            log_error "Not rebuilding. The cluster does not match the tree's blueprint; deploy.sh refuses a stale platform."
            exit 1
        fi
    fi
    log_info "Deleting cluster '${CLUSTER_NAME}' to rebuild it..."
    kind delete cluster --name "${CLUSTER_NAME}"
}

# After `kind create` failed: can this host's runtime start a kind NODE container at
# all? kind's node is a privileged container with its own hostname; under nested
# podman crun fails `sethostname` (docs/TODO.md §G) while `<runtime> info` still
# answers. Reproduced structurally — the EXIT STATUS of the same container shape on
# the node image kind just pulled, never the runtime's wording. True (0) only when
# the probe RAN and FAILED; no local node image, or a hung probe (124 — the EXIT
# trap's runtime probe names that), leaves the failure the tree's (`step-failed`).
NODE_PROBE_IMAGE_RE='(^|/)kindest/node:'
node_runtime_incapable() {
    local runtime image rc=0
    runtime="$(container_cmd)"
    image="$("${runtime}" images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null \
        | grep -m1 -E "${NODE_PROBE_IMAGE_RE}")" || image=""
    if [[ -z "${image}" ]]; then
        log_warn "No local kind node image to probe the runtime with; not classifying the kind create failure."
        return 1
    fi
    timeout "${ENV_PROBE_TIMEOUT}" "${runtime}" run --rm --pull=never --privileged \
        --hostname dt-node-probe --entrypoint /bin/true "${image}" >/dev/null 2>&1 || rc=$?
    (( rc != 0 && rc != 124 ))
}

# Host ports the kind config maps (`hostPort` with its `listenAddress`) that
# something ALREADY holds, checked right before `kind create` (the old cluster, if
# any, has released its ports by then). A held port is the environment
# (`port-held`), not the tree. TCP: a bounded connect's EXIT STATUS. UDP (no
# connect to observe): the host's bound-socket table, /proc/net/udp{,6} — the
# table `ss -lun` reads, without the iproute2 dependency — keyed on a row's
# PRESENCE. Without /proc (not Linux) the UDP check WARNs and is skipped, and a
# UDP conflict surfaces as `kind create` failing.
# Usage: udp_port_bound <ipv4-addr> <port>. Held = a bound row on that port whose
# address overlaps ours: the same address, or a wildcard on either side (an IPv6
# wildcard socket is dual-stack). The table stores an IPv4 address as
# little-endian hex (127.0.0.1 -> 0100007F).
UDP_SOCKET_TABLES=(/proc/net/udp /proc/net/udp6)
udp_port_bound() {
    local want port o1 o2 o3 o4 tables=() t
    # Only the tables that exist: an IPv4-only host has no udp6, and mawk aborts on an
    # unopenable file (which would read as "not held").
    for t in "${UDP_SOCKET_TABLES[@]}"; do [[ -r "${t}" ]] && tables+=("${t}"); done
    IFS=. read -r o1 o2 o3 o4 <<< "$1"
    want="$(printf '%02X%02X%02X%02X' "${o4}" "${o3}" "${o2}" "${o1}")"
    port="$(printf '%04X' "$2")"
    awk -v want="${want}" -v port="${port}" '
        FNR > 1 {
            split($2, a, ":"); addr = toupper(a[1])
            if (toupper(a[2]) != port) next
            if (addr ~ /^0+$/ || addr == want || want == "00000000") found = 1
        }
        END { exit !found }' "${tables[@]}"
}
check_host_ports() {
    local held=() addr port proto udp_table=true
    [[ -r /proc/net/udp ]] || {
        udp_table=false
        log_warn "No /proc/net/udp on this host; UDP host ports are not checked before kind create."
    }
    while read -r proto addr port; do
        [[ -n "${port}" ]] || continue
        if [[ "${proto}" == "UDP" ]]; then
            # Not a dotted IPv4 address (unset, `::`): compare as a wildcard — overlaps anything.
            [[ "${addr}" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ ]] || addr="0.0.0.0"
            if [[ "${udp_table}" == "true" ]] && udp_port_bound "${addr}" "${port}"; then
                held+=("${addr}:${port}/udp")
            fi
        else
            [[ "${addr}" != "-" && "${addr}" != "0.0.0.0" ]] || addr="127.0.0.1"
            timeout 2 bash -c 'exec 3<>"/dev/tcp/$1/$2"' _ "${addr}" "${port}" 2>/dev/null || continue
            held+=("${addr}:${port}")
        fi
    done < <(awk '
        function flush() { if (hp != "") print (proto == "" ? "TCP" : proto), (la == "" ? "-" : la), hp; hp = ""; la = ""; proto = "" }
        /^[[:space:]]*- containerPort:/ { flush() }
        /^[[:space:]]*hostPort:/ { hp = $2; gsub(/"/, "", hp) }
        /^[[:space:]]*listenAddress:/ { la = $2; gsub(/"/, "", la) }
        /^[[:space:]]*protocol:/ { proto = $2 }
        END { flush() }' "${KIND_CONFIG}")
    if (( ${#held[@]} > 0 )); then
        provision_failed port-held
        log_error "Host port(s) already in use by another process: ${held[*]} (from ${KIND_CONFIG}). Free them (another cluster or a stray port-forward) and re-run provision."
        return 1
    fi
}

# The one PROVISION_FAILED line of this run (Layer 7 routes on its REASON:
# scripts/layer7.sh:CLUSTER_ENV_REASONS). Records that it was printed, so the EXIT
# trap adds a classified one only when no specific one exists.
PROVISION_FAILED_EMITTED=false
PROVISION_STEP=start
PROVISION_CLUSTER_CREATED=false
provision_failed() {
    PROVISION_FAILED_EMITTED=true
    echo "PROVISION_FAILED REASON=$1 STEP=${PROVISION_STEP}" >&2
}

# EXIT trap (provision mode): a failure with no specific line still ends with one,
# classified by the shared bounded probes (lib/common.sh:classify_env_failure) —
# the apiserver is probed only once this run created the cluster — else
# `step-failed` (the tree's).
__provision_exit_trap() {
    local rc=$?
    if (( rc != 0 )) && [[ "${PROVISION_FAILED_EMITTED}" != "true" ]]; then
        local scope=no-apiserver env_reason
        [[ "${PROVISION_CLUSTER_CREATED}" != "true" ]] || scope=apiserver
        env_reason="$(classify_env_failure "${scope}")"
        echo "PROVISION_FAILED REASON=${env_reason:-step-failed} STEP=${PROVISION_STEP}" >&2
    fi
}

# Run one provision step, recording it for the EXIT trap.
pstep() {
    PROVISION_STEP="$1"
    "$@"
}

# check_prerequisites (lib/common.sh) with a missing tool classified.
provision_check_prerequisites() {
    local missing
    if ! missing="$(missing_prerequisite)"; then
        provision_failed prerequisite-missing
        log_error "${missing} is not installed on this host."
        exit 1
    fi
    check_prerequisites
}

main() {
    case "${MODE}" in
        blueprint)
            detect_container_runtime >&2
            blueprint_render
            return
            ;;
        check)
            detect_container_runtime >/dev/null
            blueprint_decide
            if tls_renewal_due; then
                [[ "${BP_REASON}" != "match" ]] || BP_REASON=changed
                bp_add_changed tls-renewal-due
            fi
            bp_emit check
            [[ "${BP_REASON}" == "match" ]]
            return
            ;;
    esac

    trap __provision_exit_trap EXIT
    log_info "Provisioning Dark Tower kind cluster '${CLUSTER_NAME}'..."
    pstep provision_check_prerequisites
    local renewal=false
    PROVISION_STEP=tls_renewal_due
    if tls_renewal_due; then renewal=true; fi
    pstep materialize_inputs
    pstep blueprint_decide
    [[ "${renewal}" != "true" ]] || bp_add_changed tls-renewal-due

    case "${BP_REASON}" in
        match)
            bp_emit none
            pstep write_kubeconfig
            log_info "Platform unchanged; nothing to provision."
            return 0
            ;;
        unreadable)
            bp_emit refuse
            provision_failed blueprint-unreadable
            log_error "Cannot tell whether cluster '${CLUSTER_NAME}' matches the blueprint (${BP_DETAIL}); refusing to destroy on uncertainty. Check the host container runtime and 'kind get clusters'. If the control plane is dead, recreate the cluster: $(remedy "'dev-cluster recreate' (it re-confirms before destroying)" "'./infra/kind/scripts/teardown.sh' then './infra/kind/scripts/setup.sh'")."
            return 1
            ;;
    esac

    bp_emit rebuild
    local exists_rc=0
    cluster_exists || exists_rc=$?
    if (( exists_rc == 0 )); then
        pstep destroy_for_rebuild
    fi
    local built_hash="${BP_HASH}"
    pstep check_host_ports
    pstep create_cluster
    PROVISION_CLUSTER_CREATED=true
    pstep install_calico
    pstep create_namespaces
    pstep create_ac_secrets
    pstep create_tls_secret mc
    pstep create_tls_secret mh
    # Record only what was built: a tree edit during the build fails loudly
    # rather than recording a blueprint nobody built.
    pstep blueprint_decide
    if [[ "${BP_HASH}" != "${built_hash}" ]]; then
        PROVISION_STEP=verify_blueprint
        log_error "The blueprint changed DURING the build (${built_hash:0:12} -> ${BP_HASH:0:12}); not recording it. Re-run provision."
        return 1
    fi
    pstep record_blueprint
    if [[ "${DT_CALLER:-}" != setup.sh ]]; then
        log_info "Platform provisioned. Next: deploy the application ($(remedy "'dev-cluster deploy'" "'./infra/kind/scripts/deploy.sh'"))."
    fi
}

# Run main only when executed, not when sourced (scripts/setup.test.sh sources
# this script to exercise its functions directly).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
