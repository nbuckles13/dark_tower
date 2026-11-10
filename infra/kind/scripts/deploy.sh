#!/usr/bin/env bash
#
# deploy.sh — DELIVERY for the Kind dev cluster (ADR-0038 §2): converge the
# application to the tree, identically on every run.
#
#   preload missing third-party images -> build every first-party image
#   (content-tagged; loaded into Kind only when the node lacks that ref) ->
#   PostgreSQL + Redis -> migration Job (Complete) -> seeds -> OTel collector
#   (Ready-gated, R-54 ordering) -> the ONE environment root
#   (infra/kubernetes/overlays/kind/; the MH template is rendered there but stored
#   as ConfigMap mh-template, never applied) -> wait for every root workload ->
#   the MH pool (mh.sh upgrade, then top up to baseline B) -> prune.
#
# Content-tagged images and content-addressed ConfigMaps roll EXACTLY the
# workloads whose image or configuration changed; nothing is restarted
# unconditionally. It never creates the GENERATED secret material (the AC
# service secrets, the MC/MH TLS Secrets): that is provision's
# (infra/kind/scripts/provision.sh). Declarative Secrets in the tree (e.g.
# infra/services/postgres/secret.yaml) are part of the root and applied here.
#
# It first runs `provision.sh --check`: a missing, half-built or stale platform
# fails loudly here, naming `provision`, before anything is built or applied.
#
# HOST-ONLY (kind + podman/docker). Run by the devloop helper's `deploy` verb
# (ADR-0030), by setup.sh, or by hand:
#   ./infra/kind/scripts/deploy.sh [--yes]
#
# Environment: DT_CLUSTER_NAME, DT_PORT_MAP, DT_HOST_GATEWAY_IP, DT_KIND_CONFIG
# (see lib/common.sh and provision.sh).
#
# Failure lines (setup-level tokens, relayed by Layer 7 under
# REASON=cluster-deploy-failed; docs/runbooks/devloop-validation.md §6.7):
#   DEPLOY_FAILED REASON=<blueprint-changed|blueprint-missing|blueprint-unreadable|collector-rollout-failed|rollout-failed> WORKLOADS=...
#   DEPLOY_FAILED REASON=<mh-no-free-slot|mh-rollout-failed|mh-pool-failed|mh-removed-still-selectable> MH_TOKEN=<mh.sh REASON|-> SLOT=<k|-> WORKLOADS=<mh-s<k>|deployment/gc-service|->
#     — the MH pool step (converge_mh_pool), mapped from mh.sh's result token by the ONE table
#     MH_DEPLOY_REASONS: mh-no-free-slot (no slot for an upgrade or top-up; also deploy's own
#     B + 1 > K headroom check, MH_TOKEN=-), mh-rollout-failed (mh.sh mh-rollout-failed |
#     not-available-timeout: a new MH never went Available; the old MH keeps serving), and
#     mh-pool-failed (EVERY other mh.sh token — kubectl-failed, mh-pool-busy, no-template,
#     drain-timeout, stuck-terminating, ...: a pool/tool fault, never folded into a rollout).
#     mh-removed-still-selectable is deploy's own (not an mh.sh token): after a converge that
#     removed an MH, a live GC pod still counted more healthy MHs than are live past B + margin
#     (wait_removed_mh_unselectable), MH_TOKEN=-, WORKLOADS=deployment/gc-service.
#     mh.sh prerequisite-missing maps to prerequisite-missing, and every UNMAPPED token is first
#     run through classify_env_failure: a dead apiserver/runtime is apiserver-unreachable /
#     runtime-unreachable (operator lane), MH_TOKEN/SLOT kept on the line.
#   DEPLOY_FAILED REASON=<insufficient-disk|prerequisite-missing|image-ref-read-failed> WORKLOADS=-
#   DEPLOY_FAILED REASON=<runtime-unreachable|apiserver-unreachable> STEP=<main step> WORKLOADS=-
#     — from main's EXIT trap when a step failed AND the shared bounded environment probe
#     (lib/common.sh:classify_env_failure) says the runtime or the apiserver is gone;
#   DEPLOY_FAILED REASON=step-failed STEP=<main step> WORKLOADS=-  — EVERY other failure (a
#     manifest the apiserver rejects, an image build, a migration, ...), from main's EXIT trap
#     once the probe passed, so one grep for DEPLOY_FAILED finds every failed deploy.
#   The REASON is classified HERE, at the source: Layer 7 routes the environment reasons
#   (scripts/layer7.sh:CLUSTER_ENV_REASONS — one list for provision and deploy) to the
#   operator lane and every other one to the implementer lane. The step's own banner (when it has one) is printed
#   above it:
#   MIGRATION_FAILURE ... REASON=migration-failed|migration-timeout
#   PRECONDITION_FAILURE ... REASON=insufficient-disk
#
# Rollback in the dev cluster: a bad migration = recreate the cluster (no
# down-migrations); a bad image or manifest = revert, then deploy again.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
# shellcheck source=lib/common.sh
source "${SCRIPT_DIR}/lib/common.sh"
# shellcheck source=lib/cluster-db.sh
source "${SCRIPT_DIR}/lib/cluster-db.sh"
# shellcheck source=../../lib/cargo-lock-version.sh
source "${PROJECT_ROOT}/infra/lib/cargo-lock-version.sh"
# shellcheck source=../../lib/rust-toolchain.sh
source "${PROJECT_ROOT}/infra/lib/rust-toolchain.sh"
# shellcheck source=../../lib/cargo-tools.sh
source "${PROJECT_ROOT}/infra/lib/cargo-tools.sh"
dt_init_cluster_env

PROVISION_SH="${SCRIPT_DIR}/provision.sh"

print_usage() {
    sed -n '2,/^$/{ s/^# \?//; p }' "${BASH_SOURCE[0]}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        # Accepted for a uniform helper argv; deploy never prompts.
        --yes) shift ;;
        --help) print_usage; exit 0 ;;
        *)
            echo "ERROR: Unknown option '$1'" >&2
            print_usage >&2
            exit 1
            ;;
    esac
done

# The one DEPLOY_FAILED line of this run (Layer 7 lifts it into
# REASON=cluster-deploy-failed). Records that it was printed, so main's EXIT
# trap adds its generic `step-failed` line only when no specific one exists.
# Usage: deploy_failed <reason> <workloads|-> [extra fields, placed before WORKLOADS]
DEPLOY_FAILED_EMITTED=false
deploy_failed() {
    DEPLOY_FAILED_EMITTED=true
    echo "DEPLOY_FAILED REASON=$1 ${3:+$3 }WORKLOADS=$2" >&2
}

# Print every container image named in rendered YAML on stdin, one per line.
#
# ANCHORED TO A REAL YAML `image:` KEY (optionally a list item), NOT a bare
# substring. Rendered Kustomize output includes ConfigMap literals, so any comment
# or prose containing `image: <word>` was once extracted as an image name — a
# collector-config comment made setup run `podman pull docker.io/without`, the pull
# was denied, and whole-cluster bring-up failed. A comment must never be able to
# break setup: after the anchored leading whitespace only `- ` or the key itself
# may appear, so a `#`-comment line can never match. Pinned by
# scripts/setup.test.sh (emi-* cases).
extract_manifest_images() {
    grep -oP '^\s*(-\s+)?image:\s+\K\S+' || true
}

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

# The cluster's (single) Kind node name — every image lookup/removal below
# addresses it. Empty when kind cannot list the nodes.
kind_node() {
    kind get nodes --name "${CLUSTER_NAME}" 2>/dev/null | head -n1 || true
}

# True iff the Kind node already holds image <ref>. A content-addressed ref
# names exactly one image, so presence means the SAME bytes: loading it again
# is pure cost (a podman save + kind load per image, every gate).
node_has_image() {
    local ref="$1" node
    node="$(kind_node)"
    [[ -n "${node}" ]] || return 1
    "$(container_cmd)" exec "${node}" crictl inspecti -q "${ref}" >/dev/null 2>&1
}

# Fail fast BEFORE the first cold image build if the host container-storage filesystem
# lacks headroom for a 4-service release build — rather than dying mid-COPY with "no space
# left on device" after several minutes. Run-once (the first build_content_tagged_image call guards them
# all).
#
# $1 = the resolved container runtime (container_cmd). MUST be passed, NOT
# re-derived: container_cmd defaults to `docker` when KIND_EXPERIMENTAL_PROVIDER is unset, so
# a re-derived `podman` default here would `df` the WRONG runtime's graphroot on an
# unset-provider host — measuring the wrong filesystem exactly when the guard matters.
#
# DEVLOOP_MIN_DISK_GB (default 15) is a fast-fail FLOOR, NOT a success guarantee: a fully
# cold build cache can exceed it and still pass-then-die, so a pass means "not obviously
# doomed", not "will succeed".
#
# Emits the relayed `PRECONDITION_FAILURE: … REASON=insufficient-disk` banner that surfaces
# in layer7's Phase-1 deploy stderr (${DEVLOOP_TMP}/layer-7.stderr.log; the
# §4 two-token convention — deploy.sh is the third PRECONDITION_FAILURE emitter alongside
# layer-all.sh + layer7.sh). See docs/runbooks/devloop-validation.md §6.7.
_DT_DISK_CHECKED=false
check_build_disk_space() {
    local cmd="$1"
    [[ "${_DT_DISK_CHECKED}" == "true" ]] && return 0
    _DT_DISK_CHECKED=true
    local min_gb="${DEVLOOP_MIN_DISK_GB:-15}" graphroot avail_gb
    # ${cmd}'s container-storage root is where build layers land; fall back to the rootless
    # default, then the rootful default, then PROJECT_ROOT's fs (always resolvable).
    # Bounded like the shared env probe (lib/common.sh:ENV_PROBE_TIMEOUT): a HUNG runtime is
    # the environment and fails loudly here, never an unbounded wait or a silent fall-open.
    local info_rc=0
    graphroot="$(timeout "${ENV_PROBE_TIMEOUT}" ${cmd} info --format '{{.Store.GraphRoot}}' 2>/dev/null)" || info_rc=$?
    if (( info_rc == 124 )); then
        deploy_failed runtime-unreachable -
        echo "PRECONDITION_FAILURE: '${cmd} info' did not answer within ${ENV_PROBE_TIMEOUT}s — the container runtime is hung. REASON=runtime-unreachable" >&2
        exit 2
    fi
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
        deploy_failed insufficient-disk -
        echo "PRECONDITION_FAILURE: container-storage filesystem (${graphroot}) has ${avail_gb}GB free, below the ${min_gb}GB floor for a 4-service image build — a cold build would die mid-COPY ('no space left on device'). REASON=insufficient-disk" >&2
        echo "  Fix: reclaim space — 'podman image prune -f && podman builder prune -f' (add -af only if no parallel devloops are running). See docs/runbooks/devloop-validation.md §6.7." >&2
        exit 2
    fi
}

# --- Content-derived image tags (ADR-0038 §2 step 1) ------------------------
#
# THE ONE place an image tag is derived. The tag is the image's own ID (its
# content digest), so a tag can never name different bytes, and an unchanged
# build — every layer cached — yields the same ID and so the same tag, which
# makes the root apply a no-op for that workload. Nothing else (the helper, the
# overlays, a hand-written manifest) derives or types a tag.
#
# Why the ID and not a hash of the build inputs: an input hash names DIFFERENT
# bytes under the SAME tag whenever an input it does not hash changes (floating
# base images, apt packages), and `kind load` would then swap the image in the
# node with nothing rolling — stale code, green gate. The cost of the ID is one
# extra rollout after a build-cache prune (unchanged code rebuilds to a new ID).
#
# Usage: content_tag <image-id>   (`sha256:<64 hex>` or bare `<64 hex>`)
# Prints `sha-<first 16 hex>`; fails on anything that is not a sha256 image ID.
CONTENT_TAG_LEN=16
content_tag() {
    local id="${1:-}"
    id="${id#sha256:}"
    if [[ ! "${id}" =~ ^[0-9a-f]{64}$ ]]; then
        log_error "content_tag: not a sha256 image ID: '${1:-}'"
        return 1
    fi
    printf 'sha-%s\n' "${id:0:${CONTENT_TAG_LEN}}"
}

# The only tag shape a first-party image may be deployed under. Anything else —
# `:latest` from a pre-ADR-0038 cluster, the `:render-required` placeholder —
# is rejected, never reused.
is_content_ref() {
    local repo="$1" ref="$2"
    [[ "${ref}" =~ ^${repo}:sha-[0-9a-f]{${CONTENT_TAG_LEN}}$ ]]
}

# Refs built in this run, keyed by repo (`localhost/<name>`).
declare -A BUILT_REFS=()
# Refs the cluster ran BEFORE this converge (the migrations image's is the
# previous Job's, which prune_superseded_images keeps; service images keep the
# generation their rollout history names — previous_generation_refs).
declare -A DEPLOYED_REFS=()
# The refs this converge deploys (resolve_image_refs): every one built in this
# run.
declare -A IMAGE_REFS=()

# Build-args a first-party image needs; none of these Dockerfile ARGs has a default.
# Every image: RUST_VERSION, the repo's ONE Rust version (the one reader:
# infra/lib/rust-toolchain.sh over rust-toolchain.toml). db-migrate: sqlx-cli pinned
# to the workspace's `sqlx` (the one reader: infra/lib/cargo-lock-version.sh). The
# service images: CARGO_CHEF_VERSION, the exact cargo-chef they install, pinned in
# infra/cargo-tools.versions (the one reader: infra/lib/cargo-tools.sh) — unpinned,
# `cargo install` resolves the newest release, which can require a newer rustc.
image_build_args() {
    local repo="$1" v
    v="$(rust_toolchain_version "${PROJECT_ROOT}/rust-toolchain.toml")" || return 1
    printf '%s\n' "--build-arg" "RUST_VERSION=${v}"
    case "${repo}" in
        "${MIGRATE_REPO}")
            v="$(cargo_lock_version "${PROJECT_ROOT}/Cargo.lock" sqlx)" || return 1
            printf '%s\n' "--build-arg" "SQLX_CLI_VERSION=${v}"
            ;;
        *)
            v="$(cargo_tool_version "${PROJECT_ROOT}/infra/cargo-tools.versions" cargo-chef)" || return 1
            printf '%s\n' "--build-arg" "CARGO_CHEF_VERSION=${v}"
            ;;
    esac
}

# The Dockerfile stage every first-party image is built from: the hardened
# distroless `runtime` stage. Never the default (last) stage — in the service
# Dockerfiles that is `runtime-with-healthcheck`, distroless :debug with a
# busybox shell. Shared by this converge and the CI image build
# (infra/ci/build-images.sh), so Kind runs what would be published.
IMAGE_BUILD_TARGET="runtime"

# Every `docker build` flag for <repo> except output/tag flags, one per line:
# the target stage, the build-args (image_build_args), and the opt-in BuildKit
# layer cache. The ONE flag source for both build paths (local converge, CI push).
# DT_IMAGE_BUILD_CACHE: unset = no cache flags; `gha` = GitHub Actions cache,
# scoped per image (needs BUILDX_BUILDER = a docker-container builder and the
# Actions runtime token in the environment). A failed cache export never fails
# the build (ignore-error); a cache miss just builds.
image_build_flags() {
    local repo="$1" scope
    printf '%s\n' "--target" "${IMAGE_BUILD_TARGET}"
    image_build_args "${repo}" || return 1
    case "${DT_IMAGE_BUILD_CACHE:-}" in
        "") ;;
        gha)
            scope="$(image_cache_scope "${repo}")" || return 1
            printf '%s\n' "--cache-from" "type=gha,scope=${scope}" \
                "--cache-to" "type=gha,mode=max,scope=${scope},ignore-error=true"
            ;;
        *)
            log_error "DT_IMAGE_BUILD_CACHE='${DT_IMAGE_BUILD_CACHE}' is not a known cache backend (gha)"
            return 1
            ;;
    esac
}

# The layer-cache scope for <repo>: derived from its Dockerfile's build
# instructions before the `${IMAGE_BUILD_TARGET}` stage (comments and blank lines
# dropped — they do not affect BuildKit cache keys). Images whose builder stages
# are identical (the four service images) share one scope, so their dependency
# layers are stored once; a builder that drifts gets its own scope automatically.
# Fails when the Dockerfile has no `AS ${IMAGE_BUILD_TARGET}` stage.
# (scripts/guards/simple/validate-shared-image-builder.sh holds the service images
# to ONE scope through these same two functions.)
image_cache_scope() {
    local sum
    sum="$(image_builder_instructions "$1" | sha256sum)" || return 1
    printf 'builder-%s\n' "${sum:0:16}"
}

# The build instructions of <repo>'s Dockerfile before the target stage, without
# comments or blank lines: what image_cache_scope hashes.
image_builder_instructions() {
    local df="${PROJECT_ROOT}/infra/docker/${1#localhost/}/Dockerfile"
    if ! grep -qE "^FROM .* AS ${IMAGE_BUILD_TARGET}([[:space:]]|\$)" "${df}"; then
        log_error "${df} has no 'AS ${IMAGE_BUILD_TARGET}' stage to build."
        return 1
    fi
    awk -v t="${IMAGE_BUILD_TARGET}" \
        '$0 ~ ("^FROM .* AS " t "([[:space:]]|$)") { exit } !/^[[:space:]]*(#|$)/' "${df}"
}

# The content-pinned image to use for <repo> instead of building it, from
# DT_PREBUILT_IMAGES (a file of `<repo> <ref>` lines written by the CI build job,
# infra/ci/build-images.sh). <ref> is either `<registry>/<name>@sha256:<64 hex>`
# (pulled by digest) or a bare image ID `sha256:<64 hex>` (already loaded into the
# local store from a build archive; never pulled). Fails loudly when the file lacks
# <repo> or the ref is neither — a tag could be repointed between build and test.
prebuilt_image_ref() {
    local repo="$1" ref
    ref="$(awk -v r="${repo}" '$1 == r { print $2 }' "${DT_PREBUILT_IMAGES}")" || return 1
    if [[ ! "${ref}" =~ ^([a-z0-9./_-]+@)?sha256:[0-9a-f]{64}$ ]]; then
        log_error "DT_PREBUILT_IMAGES (${DT_PREBUILT_IMAGES}) has no digest-pinned ref for ${repo} (got '${ref}'); refusing to build or guess."
        return 1
    fi
    printf '%s\n' "${ref}"
}

# Produce a first-party image, tag it by its content, load it into Kind unless
# the node already holds that ref, and record it in BUILT_REFS. With
# DT_PREBUILT_IMAGES set the image is PULLED by digest, or taken by image ID
# from the local store (CI: built once by the build job, tested here); otherwise `localhost/<name>` is built from
# infra/docker/<name>/Dockerfile with the repo root as context (convention —
# first_party_repos() fails on a repo without that Dockerfile).
# Usage: build_content_tagged_image <repo>
build_content_tagged_image() {
    local repo="$1" name cmd iid_file iid tag ref src args=()
    name="${repo#localhost/}"
    cmd="$(container_cmd)"
    if [[ -n "${DT_PREBUILT_IMAGES:-}" ]]; then
        src="$(prebuilt_image_ref "${repo}")" || return 1
        if [[ "${src}" == sha256:* ]]; then
            log_step "Using prebuilt ${repo} (image ${src}) from the local store..."
            iid="$(${cmd} image inspect --format '{{.Id}}' "${src}")" \
                || { log_error "Prebuilt image ${src} for ${repo} is not in the local store; refusing to pull or build."; return 1; }
        else
            log_step "Pulling prebuilt ${repo} (${src})..."
            ${cmd} pull "${src}" || { log_error "Pull failed: ${src}"; return 1; }
            iid="$(${cmd} image inspect --format '{{.Id}}' "${src}")" || return 1
        fi
    else
        # Fast-fail on insufficient host disk before the (multi-minute) cold build.
        check_build_disk_space "${cmd}"
        local flags
        flags="$(image_build_flags "${repo}")" || return 1
        mapfile -t args <<< "${flags}"
        # A docker-container builder (cache path) keeps results in its own store;
        # --load puts the image in the daemon for the tag + kind load below.
        [[ -z "${DT_IMAGE_BUILD_CACHE:-}" ]] || args+=(--load)
        iid_file="$(mktemp "${TMPDIR:-/tmp}/dt-iid.XXXXXX")"
        log_step "Building ${repo} container image..."
        if ! ${cmd} build --iidfile "${iid_file}" "${args[@]}" \
                -f "${PROJECT_ROOT}/infra/docker/${name}/Dockerfile" "${PROJECT_ROOT}"; then
            rm -f "${iid_file}"
            log_error "Image build failed: ${repo}"
            return 1
        fi
        iid="$(cat "${iid_file}" 2>/dev/null || true)"
        rm -f "${iid_file}"
        if [[ -z "${iid}" ]]; then
            log_error "Image build of ${repo} wrote no image ID (--iidfile empty); refusing to guess a tag."
            return 1
        fi
    fi
    if [[ -z "${iid}" ]]; then
        log_error "No image ID for prebuilt ${repo}; refusing to guess a tag."
        return 1
    fi
    tag="$(content_tag "${iid}")" || return 1
    ref="${repo}:${tag}"
    ${cmd} tag "${iid}" "${ref}"
    if node_has_image "${ref}"; then
        log_info "${ref} is already in the kind node; not reloading."
    else
        log_step "Loading ${ref} into kind cluster..."
        load_image_to_kind "${ref}"
    fi
    BUILT_REFS["${repo}"]="${ref}"
}

# Pre-load third-party images into the Kind cluster.
# Derives the image list from Kustomize manifests (single source of truth),
# skips every image the node already holds (this runs on EVERY deploy, so an
# unchanged root must cost a lookup, not a reload), pulls the rest to the host
# cache if not present, then loads them into Kind.
preload_third_party_images() {
    log_step "Pre-loading third-party images missing from the Kind node..."

    local CONTAINER_CMD
    CONTAINER_CMD="$(container_cmd)"

    # Extract third-party images from rendered Kustomize manifests,
    # qualifying Docker Hub short names for podman compatibility.
    local IMAGES
    IMAGES="$(cached_render root \
        | extract_manifest_images \
        | { grep -v '^localhost/' || true; } \
        | sort -u)" || return 1

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

    local missing=""
    local img
    for img in $IMAGES; do
        if node_has_image "$img"; then
            log_info "In node: $img"
        else
            missing+="$img"$'\n'
        fi
    done
    IMAGES=$(echo "$missing" | sed '/^$/d')
    if [ -z "$IMAGES" ]; then
        log_info "Every third-party image is already in the Kind node."
        return 0
    fi

    # Pull to host cache if not already present
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

    log_info "Waiting for the PostgreSQL rollout..."
    await_statefulset_rollout postgres
    log_info "PostgreSQL deployed successfully."
}

# Deploy Redis
deploy_redis() {
    log_step "Deploying Redis..."

    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/redis/"

    log_info "Waiting for the Redis rollout..."
    await_statefulset_rollout redis
    log_info "Redis deployed successfully."
}

# `rollout status`, NOT `wait --for=condition=Ready pod -l app=<x>`: deploy runs
# against a WARM cluster on every gate, and after a changed StatefulSet is
# applied a label-selector wait matches the OLD, still-Ready pod before the
# controller replaces it — returning at once, so the migration Job would start
# while postgres-0 restarts and a platform rollout would surface as a migration
# failure. BUDGET: a pod becoming Ready (120s) plus the old one terminating
# (terminationGracePeriodSeconds, 30s for Redis), serialized.
# Usage: await_statefulset_rollout <name>   (namespace dark-tower)
await_statefulset_rollout() {
    local name="$1"
    if ! ${KUBECTL} rollout status "statefulset/${name}" -n dark-tower --timeout=180s; then
        dump_workload_diagnostics dark-tower "statefulset/${name}"
        deploy_failed rollout-failed "dark-tower/statefulset/${name}"
        return 1
    fi
}

# Deploy the dev OTel collector (R-59)
#
# Ordered BEFORE the environment-root apply that starts the AC/GC/MC/MH
# services (the collector is also part of that root; this pre-apply is a subset
# of it and exists for ordering). THE READINESS GATE IS LOAD-BEARING: AC, GC and
# MC all set OTEL_ENABLED=true in the Kind overlay and probe this collector during
# init, and under R-54 fail-hard-at-init a service started before the collector
# is Ready fails init and CrashLoopBackoffs. The gate is the `rollout status`
# below; do not shorten, skip, or collapse it back to a label-selector
# `wait --for=condition=Ready` (see the comment at the site for why). A config
# the pinned collector image rejects STALLS whole-cluster bring-up loudly at
# this gate rather than being silently ignored by an already-running pod — that
# is the intended trade, not a regression.
# It runs on EVERY deploy, so a collector change rolls — and is Ready-gated —
# BEFORE the services the same deploy rolls: the dev-cluster form of the
# separate change window (gc-deployment.md §Mitigation). A failed or timed-out
# rollout STOPS deploy here, before the root, and names the collector:
#   DEPLOY_FAILED REASON=collector-rollout-failed WORKLOADS=dark-tower/deployment/otel-collector
# (MH is the exception and deliberately so: it has no OTEL_ENABLED and no 4317
# egress rule, so it never probes. Enabling it needs the egress rule first.)
# See the collector-upgrade-discipline section in docs/runbooks/gc-deployment.md.
deploy_otel_collector() {
    log_step "Deploying OTel collector..."

    # otelcol reads `--config` once at startup with no file watcher, so a config
    # edit must roll the pod. It does, with no restart here: the config is a
    # content-addressed generator (ADR-0038 §2), so an edit changes the
    # ConfigMap's name, hence the pod template, hence a rollout.
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/otel-collector/"

    # `rollout status`, NOT `wait --for=condition=Ready pod -l app=otel-collector`.
    # deployment.yaml has `replicas: 1` and no `strategy:` block, so the default
    # RollingUpdate maxUnavailable (25% of 1) rounds to 0: a config the collector
    # rejects STALLS the rollout with the OLD pod still Ready, and a label-selector
    # wait would match that old pod and report success over a rejected config.
    # BUDGET: this timeout must exceed the single-pod readiness budget (120s, the
    # figure the replaced wait used) PLUS terminationGracePeriodSeconds (30s,
    # deployment.yaml) - after a config change it covers a new pod becoming Ready
    # AND the old one terminating, serialized. Do not "tidy" it down to 120s.
    log_info "Waiting for OTel collector rollout to complete..."
    if ! ${KUBECTL} rollout status deployment/otel-collector -n dark-tower --timeout=180s; then
        dump_workload_diagnostics dark-tower deployment/otel-collector
        deploy_failed collector-rollout-failed dark-tower/deployment/otel-collector
        echo "  The collector did not roll out within 180s; the services were NOT rolled (R-54: they would fail init against it). kubectl -n dark-tower logs deploy/otel-collector" >&2
        return 1
    fi
    log_info "OTel collector deployed successfully."
}

# --- The environment root (ADR-0038 §2) ------------------------------------
#
# `infra/kubernetes/overlays/kind/` composes every service, the OTel collector
# and the observability stack. apply_env_root() is the ONE place that applies
# it. Every ConfigMap a pod template consumes is generated with a content hash
# in its name, so applying the root rolls exactly the workloads whose
# configuration changed and nothing else — no unconditional restart.
ENV_ROOT_REL="infra/kubernetes/overlays/kind"
ENV_ROOT="${PROJECT_ROOT}/${ENV_ROOT_REL}"

# The migration Job's base (ADR-0038 §2 step 3), applied AHEAD of the root by
# run_migration_job — not one of the root's resources — and its image's repo.
MIGRATE_BASE_REL="infra/services/db-migrate"
MIGRATE_BASE="${PROJECT_ROOT}/${MIGRATE_BASE_REL}"
MIGRATE_REPO="localhost/db-migrate"
MIGRATE_NAMESPACE="dark-tower"

# ONE render of each static kustomization a converge derives from (the root and
# the migration base), taken by cache_renders — a `step` of main, before anything
# derives from them. Every derivation (preload_third_party_images,
# first_party_repos, root_repos, env_root_workloads) reads THIS snapshot, so
# preload, build, ref resolution, wait and prune all see the same root within one
# converge — do not "refresh" it mid-run. Nothing a converge writes lands under
# ENV_ROOT or MIGRATE_BASE (the wrapper and Job renders go to mktemp dirs). Shell
# variables only — the root render carries Secret manifests, so it is never
# written to a file and never printed.
ROOT_RENDER=""
MIGRATE_BASE_RENDER=""
RENDERS_CACHED=false
cache_renders() {
    ROOT_RENDER="$(kubectl kustomize "${ENV_ROOT}")" \
        || { log_error "Could not render ${ENV_ROOT_REL}."; return 1; }
    MIGRATE_BASE_RENDER="$(kubectl kustomize "${MIGRATE_BASE}")" \
        || { log_error "Could not render ${MIGRATE_BASE_REL}."; return 1; }
    RENDERS_CACHED=true
}

# The cached render of $1 (root|migrate-base). Fails loudly, naming the render
# (never its content), when cache_renders has not run — never a silent re-render.
cached_render() {
    if [[ "${RENDERS_CACHED}" != "true" ]]; then
        log_error "internal: the ${1} render is not cached; cache_renders must run first (main does)."
        return 1
    fi
    case "$1" in
        root) printf '%s\n' "${ROOT_RENDER}" ;;
        migrate-base) printf '%s\n' "${MIGRATE_BASE_RENDER}" ;;
        *) log_error "internal: no cached render named '${1}'."; return 1 ;;
    esac
}

# The per-instance MC ConfigMaps whose advertise address a devloop cluster
# overrides, DERIVED from the per-instance generator sources on disk (not a
# hand-kept list): adding mc-2 means adding mc-2-config.env, and the override
# follows. Prints `mc-0`, `mc-1`, … one per line, sorted. MC ONLY: MH has no
# fixed instances — its addresses are per hostPort slot (mh_slot_table).
advertise_instances() {
    local f base
    for f in "${PROJECT_ROOT}"/infra/services/mc-service/mc-[0-9]*-config.env; do
        [[ -e "$f" ]] || continue
        base="${f##*/}"
        printf '%s\n' "${base%-config.env}"
    done | sort
}

# THE client-facing advertise URL for <host> <port> — one formatter for the MC
# per-instance overrides and the MH slot table.
advertise_url() {
    printf 'https://%s:%s\n' "$1" "$2"
}

# --- MH hostPort slot pool (ADR-0039 §7 "Kind") ------------------------------
#
# deploy.sh PRODUCES the cluster-side interface infra/kind/scripts/mh.sh consumes
# (see mh.sh's header): ConfigMap `mh-template` (deployment.json, revision,
# baseline) and one ConfigMap per slot labelled dark-tower.io/mh-slot=<k> (keys
# MH_WEBTRANSPORT_ADVERTISE_ADDRESS, MH_HOST_PORT). TRUST BOUNDARY: write access
# to those ConfigMaps amounts to "run an arbitrary pod spec with a hostPort" (mh.sh
# instantiates them). Acceptable in Kind under today's RBAC (the same principal
# already applies every manifest); slot values come ONLY from mh_slot_table (the
# blueprint-verified kind-config), never from caller input.
# The MH pool tool. DT_MH_SH (the same variable scripts/layer7.sh exports for the
# env-tests) may name another copy; it must be an existing executable.
MH_SH="${DT_MH_SH:-${SCRIPT_DIR}/mh.sh}"
# The ONE bash spelling of the slot marker the devloop helper emits before each
# slot mapping (crates/devloop-helper/src/ports.rs:MH_SLOT_MARKER; pinned equal
# by scripts/setup.test.sh).
MH_SLOT_MARKER='# dt:mh-slot='
# The template Deployment's name in the base (infra/services/mh-service/deployment.yaml).
MH_TEMPLATE_DEPLOYMENT="mh-template"
# Docs labelled with this are template INPUTS (the placeholder advertise
# ConfigMap): rendered, NEVER applied.
MH_TEMPLATE_INPUT_LABEL="dark-tower.io/mh-template-input"
MH_SLOT_LABEL="dark-tower.io/mh-slot"
MH_TEMPLATE_CM="mh-template"
MH_FLEET_KEY="MH_BASELINE"
# B's one home (the Kind overlay; see its header).
MH_FLEET_ENV="${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/mh-service/mh-fleet.env"
MH_BASELINE_OWNER="baseline"
# The kind config the cluster was built from (check_blueprint proved it).
KIND_CONFIG_FILE="${DT_KIND_CONFIG:-${PROJECT_ROOT}/${DT_STATIC_KIND_CONFIG_REL}}"

# THE slot-table source (ADR-0039 §7; also GC-2's allowlist input): one line per
# MH hostPort slot, `<slot> <host_port> <advertise_url>`, sorted by slot. Read
# from the kind config the cluster was built from — the helper's render (from its
# port table, crates/devloop-helper/src/ports.rs, K's one home) or the static
# manual-dev file — so the advertised address is by construction where the slot
# is published: https://<listenAddress>:<hostPort>. Host port == node
# containerPort == pod hostPort == MH_HOST_PORT. Fails loudly (non-zero, one
# `mh_slot_table:` line naming the fault) on any malformed table.
mh_slot_table() {
    local cfg="${KIND_CONFIG_FILE}" rows k cp hp la proto n=0 seen_k=" " seen_p=" "
    if [[ ! -f "${cfg}" ]]; then
        log_error "mh_slot_table: kind config not found: ${cfg}"
        return 1
    fi
    if kind_config_unrendered_line "${cfg}" >/dev/null; then
        log_error "mh_slot_table: ${cfg} has an unsubstituted \${...} placeholder — it was rendered by a devloop-helper built from an older tree. Rebuild the helper on the HOST, then recreate the cluster ($(remedy "'dev-cluster recreate'" "'./infra/kind/scripts/setup.sh'"))."
        return 1
    fi
    rows="$(awk -v marker="${MH_SLOT_MARKER}" '
        function flush() { if (slot != "") print slot, (cp == "" ? "-" : cp), (hp == "" ? "-" : hp), (la == "" ? "-" : la), (pr == "" ? "TCP" : pr); slot = ""; cp = ""; hp = ""; la = ""; pr = "" }
        { line = $0; sub(/^[[:space:]]+/, "", line) }
        index(line, marker) == 1 { flush(); pending = substr(line, length(marker) + 1); gsub(/[[:space:]]/, "", pending); next }
        line ~ /^- containerPort:/ { flush(); if (pending != "") { slot = pending; pending = ""; cp = line; sub(/^- containerPort:[[:space:]]*/, "", cp) } ; next }
        slot != "" && line ~ /^hostPort:/ { hp = line; sub(/^hostPort:[[:space:]]*/, "", hp) }
        slot != "" && line ~ /^listenAddress:/ { la = line; sub(/^listenAddress:[[:space:]]*/, "", la); gsub(/"/, "", la) }
        slot != "" && line ~ /^protocol:/ { pr = line; sub(/^protocol:[[:space:]]*/, "", pr) }
        slot != "" && line !~ /^(hostPort|listenAddress|protocol):/ && line !~ /^#/ && line != "" { flush() }
        END { flush() }' "${cfg}" | sort -n -k1,1)"
    if [[ -z "${rows}" ]]; then
        log_error "mh_slot_table: no MH slot mappings ('${MH_SLOT_MARKER}<k>') in ${cfg} — a cluster built from an older tree or helper. Rebuild the helper on the HOST, then recreate the cluster ($(remedy "'dev-cluster recreate'" "'./infra/kind/scripts/setup.sh'"))."
        return 1
    fi
    while read -r k cp hp la proto; do
        if [[ ! "${k}" =~ ^(0|[1-9][0-9]{0,3})$ ]]; then
            log_error "mh_slot_table: slot marker '${MH_SLOT_MARKER}${k}' is not a decimal slot number (${cfg})"; return 1
        fi
        if [[ ! "${hp}" =~ ^[1-9][0-9]{0,4}$ ]] || (( hp > 65535 )); then
            log_error "mh_slot_table: slot ${k}: hostPort '${hp}' is not a port (${cfg})"; return 1
        fi
        if [[ "${proto}" != "UDP" ]]; then
            log_error "mh_slot_table: slot ${k}: protocol is '${proto}', not UDP (${cfg})"; return 1
        fi
        if [[ "${cp}" != "${hp}" ]]; then
            log_error "mh_slot_table: slot ${k}: containerPort ${cp} != hostPort ${hp} — the node port must equal the published port (${cfg})"; return 1
        fi
        if [[ ! "${la}" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ || "${la}" == "0.0.0.0" ]]; then
            log_error "mh_slot_table: slot ${k}: listenAddress '${la}' is not a bound IPv4 address (${cfg})"; return 1
        fi
        if [[ -n "${DT_HOST_GATEWAY_IP:-}" && "${la}" != "${DT_HOST_GATEWAY_IP}" ]]; then
            log_error "mh_slot_table: slot ${k}: listenAddress ${la} != DT_HOST_GATEWAY_IP ${DT_HOST_GATEWAY_IP} — the cluster was built for another gateway. Recreate it ($(remedy "'dev-cluster recreate'" "'./infra/kind/scripts/setup.sh'"))."
            return 1
        fi
        if [[ "${seen_k}" == *" ${k} "* ]]; then
            log_error "mh_slot_table: duplicate slot ${k} (${cfg})"; return 1
        fi
        if [[ "${seen_p}" == *" ${hp} "* ]]; then
            log_error "mh_slot_table: duplicate slot port ${hp} (${cfg})"; return 1
        fi
        seen_k+="${k} "; seen_p+="${hp} "
        printf '%s %s %s\n' "${k}" "${hp}" "$(advertise_url "${la}" "${hp}")"
        n=$((n + 1))
    done <<< "${rows}"
    (( n > 0 ))
}

# The pod subnet of the kind config the cluster was built from (GC-2's internal
# MH gRPC CIDR allowlist input). Fails loudly when absent.
kind_pod_subnet() {
    local v
    v="$(awk '/^[[:space:]]*podSubnet:/ { v = $2; gsub(/"/, "", v); print v; exit }' "${KIND_CONFIG_FILE}")"
    if [[ ! "${v}" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}/[0-9]{1,2}$ ]]; then
        log_error "kind_pod_subnet: no valid networking.podSubnet in ${KIND_CONFIG_FILE} (got '${v}')"
        return 1
    fi
    printf '%s\n' "${v}"
}

# The ONE source of GC's RegisterMH endpoint allowlist values (ADR-0039 §7 "GC
# registration rule"; GC-2), host-mode and devloop alike. Prints exactly four
# `KEY=VALUE` lines in this fixed order (values comma-joined, sorted, de-duplicated):
#   GC_MH_WT_ALLOWED_HOSTS    the distinct slot hosts        (mh_slot_table)
#   GC_MH_WT_ALLOWED_PORTS    the slot host ports            (mh_slot_table)
#   GC_MH_GRPC_ALLOWED_CIDRS  the cluster pod subnet         (kind_pod_subnet)
#   GC_MH_GRPC_ALLOWED_PORTS  the MH gRPC advertise port     (the rendered MH
#                             template's MH_GRPC_ADVERTISE_ADDRESS, `cached_render root`)
# Nothing is retyped: a slot, gateway, subnet or MH gRPC port change flows here.
# GC_MH_GRPC_ALLOWED_SCHEMES is NOT rendered: it is policy (base config.env), so an
# MH-only scheme flip fails registration closed until GC's config flips too.
# The base config.env carries the four keys EMPTY (GC refuses to start on them),
# so this render is what makes GC startable. Fails loudly (non-zero, a
# `gc_mh_allowlist_literals:` line) on any input it cannot derive. Requires
# cache_renders. I-3's drift guard reads THIS function's output.
gc_mh_allowlist_literals() {
    local slots k port addr host hosts="" ports="" subnet adv n grpc_port
    slots="$(mh_slot_table)" || return 1
    while read -r k port addr; do
        host="${addr#https://}"
        host="${host%:"${port}"}"
        # Round-trip: the host recovered from the URL must re-format to that URL
        # through THE formatter, or the slot table and this reader disagree.
        if [[ -z "${host}" || "$(advertise_url "${host}" "${port}")" != "${addr}" ]]; then
            log_error "gc_mh_allowlist_literals: slot ${k}: cannot recover the host of advertise address '${addr}' (port ${port}) through advertise_url"
            return 1
        fi
        hosts+="${host}"$'\n'
        ports+="${port}"$'\n'
    done <<< "${slots}"
    subnet="$(kind_pod_subnet)" || return 1
    adv="$(cached_render root | yaml_docs_to_json \
        | jq -r --arg n "${MH_TEMPLATE_DEPLOYMENT}" \
            'select(.kind == "Deployment" and .metadata.name == $n)
             | .spec.template.spec.containers[].env[]?
             | select(.name == "MH_GRPC_ADVERTISE_ADDRESS") | .value // ""')" || {
        log_error "gc_mh_allowlist_literals: could not read MH_GRPC_ADVERTISE_ADDRESS from the ${MH_TEMPLATE_DEPLOYMENT} Deployment in the ${ENV_ROOT_REL} render"
        return 1
    }
    n="$(grep -c . <<< "${adv}" || true)"
    if (( n != 1 )) || [[ ! "${adv}" =~ ^[a-z][a-z0-9+]*://\$\(POD_IP\):([1-9][0-9]{0,4})$ ]] || (( BASH_REMATCH[1] > 65535 )); then
        log_error "gc_mh_allowlist_literals: the ${MH_TEMPLATE_DEPLOYMENT} Deployment must carry exactly one MH_GRPC_ADVERTISE_ADDRESS of the form '<scheme>://\$(POD_IP):<port>' (got ${n}: '${adv//$'\n'/ | }')"
        return 1
    fi
    grpc_port="${BASH_REMATCH[1]}"
    printf 'GC_MH_WT_ALLOWED_HOSTS=%s\n' "$(printf '%s' "${hosts}" | sort -u | paste -sd, -)"
    printf 'GC_MH_WT_ALLOWED_PORTS=%s\n' "$(printf '%s' "${ports}" | sort -n -u | paste -sd, -)"
    printf 'GC_MH_GRPC_ALLOWED_CIDRS=%s\n' "${subnet}"
    printf 'GC_MH_GRPC_ALLOWED_PORTS=%s\n' "${grpc_port}"
}

# The first-party repos (`localhost/<name>`, tag stripped) the cached render $1
# (root|migrate-base) names — DERIVED from the render, not a hand-kept service
# list. One per line, sorted; empty when it names none; non-zero when the render
# is not cached.
first_party_repos_of() {
    cached_render "$1" | extract_manifest_images | { grep '^localhost/' || true; } \
        | sed -E 's/:[^:/]*$//' | sort -u
}

# First-party repos the environment root runs.
root_repos() {
    first_party_repos_of root
}

# Every first-party repo a converge must resolve a ref for: the root's, plus
# the migrations image (whose base is applied ahead of the root, not in it).
# Fails when none is derived (a vacuous converge) or when a repo has no
# infra/docker/<name>/Dockerfile to build it from.
first_party_repos() {
    local root mig repos repo missing=0
    # Each derivation checked on its own: a failure of either is never masked by
    # the other yielding repos.
    root="$(root_repos)" || return 1
    mig="$(first_party_repos_of migrate-base)" || return 1
    repos="$(printf '%s\n%s\n' "${root}" "${mig}" | awk 'NF' | sort -u)"
    if [[ -z "${repos}" ]]; then
        log_error "No first-party (localhost/) images derived from ${ENV_ROOT_REL} or ${MIGRATE_BASE_REL}; refusing a vacuous converge."
        return 1
    fi
    while IFS= read -r repo; do
        if [[ ! -f "${PROJECT_ROOT}/infra/docker/${repo#localhost/}/Dockerfile" ]]; then
            log_error "First-party image ${repo} has no infra/docker/${repo#localhost/}/Dockerfile to build it from."
            missing=1
        fi
    done <<< "${repos}"
    (( missing == 0 )) || return 1
    printf '%s\n' "${repos}"
}

# Every distinct image ref of <repo> the cluster runs now, one per line (may be
# empty; may be invalid — the caller judges). The ONE deployed-ref reader, keyed
# by repo: root workloads for service images, the Complete migration Job for
# db-migrate. Returns non-zero when the cluster cannot be READ — that is never
# folded into "nothing deployed".
deployed_refs() {
    local repo="$1" ns res out workloads
    if [[ "${repo}" == "${MIGRATE_REPO}" ]]; then
        out="$(${KUBECTL} get jobs -n "${MIGRATE_NAMESPACE}" -l app=db-migrate \
            -o jsonpath='{range .items[*]}{.status.succeeded}{" "}{.spec.template.spec.containers[0].image}{"\n"}{end}')" || return 1
        awk '$1 == "1" { print $2 }' <<< "${out}" | sort -u
        return 0
    fi
    workloads="$(env_root_workloads "${repo}")" || return 1
    {
        while read -r ns res; do
            [[ -n "${res}" ]] || continue
            out="$(${KUBECTL} get "${res}" -n "${ns}" --ignore-not-found \
                -o jsonpath='{range .spec.template.spec.containers[*]}{.image}{"\n"}{end}')" || return 1
            grep "^${repo}:" <<< "${out}" || true
        done <<< "${workloads}"
    } | sort -u
}

# Fill IMAGE_REFS and DEPLOYED_REFS for every first-party repo. deploy builds
# EVERY first-party image on every run (build_first_party_images), so the ref
# deployed is always the one built in this run — there is no fallback to a
# deployed ref (a repo left unbuilt is a code defect and fails loudly).
# DEPLOYED_REFS records the single content-tagged ref the cluster runs BEFORE
# this converge (the previous generation prune_superseded_images keeps); a
# cluster that cannot be READ fails loudly, never "nothing deployed".
resolve_image_refs() {
    local repos repo refs n
    repos="$(first_party_repos)" || return 1
    while IFS= read -r repo; do
        if ! refs="$(deployed_refs "${repo}")"; then
            log_error "IMAGE_REF_READ_FAILED: could not read the deployed ref of ${repo} from cluster '${CLUSTER_NAME}' (kubectl failed; see above) REASON=image-ref-read-failed"
            deploy_failed image-ref-read-failed -
            return 1
        fi
        n="$(grep -c . <<< "${refs}" || true)"
        if (( n == 1 )) && is_content_ref "${repo}" "${refs}"; then
            DEPLOYED_REFS["${repo}"]="${refs}"
        fi
        if [[ -z "${BUILT_REFS[${repo}]:-}" ]]; then
            log_error "internal: first-party image ${repo} was not built in this run; deploy builds every first-party image, so this is a defect in deploy.sh, not the environment."
            return 1
        fi
        IMAGE_REFS["${repo}"]="${BUILT_REFS[${repo}]}"
    done <<< "${repos}"
}

# Render the wrapper kustomization the root is applied through into directory
# $1. PURE given IMAGE_REFS: it writes only $1/kustomization.yaml and touches no
# cluster (setup.test.sh renders it).
#
# Always rendered (ADR-0038 §2): every first-party image's tag is a per-build
# value, set here as `images:` so the tree never carries it (the bases say
# `:render-required`, which nothing loads). A render with any root repo lacking
# a resolved ref fails.
#
# It ALWAYS carries one ConfigMap generator per MH hostPort slot,
# `mh-slot-<k>-config` (labelled dark-tower.io/mh-slot=<k>; keys
# MH_WEBTRANSPORT_ADVERTISE_ADDRESS and MH_HOST_PORT), from mh_slot_table — the
# per-slot half of mh.sh's cluster-side interface. Content-addressed like every
# generator; deploy never deletes an old generation (a live MH pod may reference
# it), it only un-labels it (retire_stale_slot_configmaps).
#
# It ALWAYS carries GC's RegisterMH endpoint allowlists (gc_mh_allowlist_literals)
# as a `behavior: merge` over gc-service-config — the base ships them empty.
#
# With DT_HOST_GATEWAY_IP set (a devloop cluster) it also carries the advertise
# address of each MC pod (host-gateway IP + the helper's per-slug port). That
# used to be `kubectl patch`ed onto the live ConfigMap after apply, which (a)
# addresses a literal ConfigMap name that no longer exists once names are
# content-addressed and (b) sits outside the render, so the next apply reverts
# it and it never enters a hash. Here it is a render INPUT: a `behavior: merge`
# generator over the root's own per-instance generator, so the value is part of
# the hash and a gateway/port change rolls exactly those pods.
#
# Returns non-zero, with a diagnostic, on any invalid input — the caller must
# not fall back to the plain root (that would run placeholder images, or
# silently advertise localhost).
render_env_overlay() {
    local dir="$1" rel inst svc idx svc_uc port_var port instances repos repo ref slots k addr allow line
    repos="$(root_repos)" || return 1
    if [[ -z "${repos}" ]]; then
        log_error "render_env_overlay: no first-party (localhost/) images in ${ENV_ROOT_REL}"
        return 1
    fi
    # kustomize rejects an absolute path in `resources:`; the default load
    # restrictor allows a relative path to a kustomization directory.
    rel="$(realpath --relative-to="${dir}" "${ENV_ROOT}")" || return 1
    {
        echo "# GENERATED by infra/kind/scripts/deploy.sh:render_env_overlay — do not edit."
        echo "apiVersion: kustomize.config.k8s.io/v1beta1"
        echo "kind: Kustomization"
        echo "resources:"
        echo "  - ${rel}"
        echo "images:"
    } > "${dir}/kustomization.yaml"
    while IFS= read -r repo; do
        ref="${IMAGE_REFS[${repo}]:-}"
        if ! is_content_ref "${repo}" "${ref}"; then
            log_error "render_env_overlay: no content-tagged ref resolved for ${repo} (got '${ref}')"
            return 1
        fi
        {
            echo "  - name: ${repo}"
            echo "    newTag: ${ref##*:}"
        } >> "${dir}/kustomization.yaml"
    done <<< "${repos}"

    slots="$(mh_slot_table)" || return 1
    echo "configMapGenerator:" >> "${dir}/kustomization.yaml"
    while read -r k port addr; do
        {
            echo "  - name: mh-slot-${k}-config"
            echo "    namespace: dark-tower"
            echo "    literals:"
            echo "      - \"MH_WEBTRANSPORT_ADVERTISE_ADDRESS=${addr}\""
            echo "      - \"MH_HOST_PORT=${port}\""
            echo "    options:"
            echo "      labels:"
            echo "        app: mh-service"
            echo "        component: media"
            echo "        ${MH_SLOT_LABEL}: \"${k}\""
        } >> "${dir}/kustomization.yaml"
    done <<< "${slots}"

    # GC's RegisterMH endpoint allowlists (GC-2), ALWAYS: the base carries them
    # empty and GC refuses to start on that. A merge over the root's own
    # generator, so the values enter gc-service-config's hash and roll GC.
    allow="$(gc_mh_allowlist_literals)" || return 1
    {
        echo "  - name: gc-service-config"
        echo "    namespace: dark-tower"
        echo "    behavior: merge"
        echo "    literals:"
        while IFS= read -r line; do
            echo "      - \"${line}\""
        done <<< "${allow}"
    } >> "${dir}/kustomization.yaml"

    [[ -n "${DT_HOST_GATEWAY_IP:-}" ]] || return 0
    if [[ ! "${DT_HOST_GATEWAY_IP}" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ || "${DT_HOST_GATEWAY_IP}" == "0.0.0.0" ]]; then
        log_error "render_env_overlay: DT_HOST_GATEWAY_IP is invalid: '${DT_HOST_GATEWAY_IP}'"
        return 1
    fi
    instances="$(advertise_instances)"
    if [[ -z "${instances}" ]]; then
        log_error "render_env_overlay: no per-instance generator sources (infra/services/mc-service/mc-N-config.env) found"
        return 1
    fi
    while IFS= read -r inst; do
        svc="${inst%%-*}"
        idx="${inst#*-}"
        svc_uc="${svc^^}"
        port_var="${svc_uc}_${idx}_WEBTRANSPORT_PORT"
        port="${!port_var:-}"
        if [[ ! "${port}" =~ ^[0-9]+$ ]] || (( port < 1 || port > 65535 )); then
            log_error "render_env_overlay: ${port_var} is unset or not a port (1..65535): '${port}' (expected from DT_PORT_MAP)"
            return 1
        fi
        {
            echo "  - name: ${inst}-config"
            echo "    namespace: dark-tower"
            echo "    behavior: merge"
            echo "    literals:"
            echo "      - \"${svc_uc}_WEBTRANSPORT_ADVERTISE_ADDRESS=$(advertise_url "${DT_HOST_GATEWAY_IP}" "${port}")\""
        } >> "${dir}/kustomization.yaml"
    done <<< "${instances}"
}

# Render the wrapper in $1 and print it as ONE JSON object per line (jq -c).
# YAML -> JSON offline via `kubectl annotate --local` (no apiserver, no file):
# the marker annotation it needs is stripped again. The render carries Secrets,
# so it only ever flows through pipes — never a file, never stdout of deploy.
render_env_root_json() {
    kubectl kustomize "$1" | yaml_docs_to_json
}

# THE one YAML -> JSON converter (rendered YAML on stdin -> one JSON object per
# line): offline `kubectl annotate --local` (no apiserver, no file), its marker
# annotation stripped again. Pipes only (a render may carry Secrets).
yaml_docs_to_json() {
    local mark="dark-tower.io/deploy-render"
    kubectl annotate --local -f - -o json "${mark}=1" \
        | jq -c --arg m "${mark}" 'del(.metadata.annotations[$m])
            | if (.metadata.annotations // {}) == {} then del(.metadata.annotations) else . end'
}

# jq predicate: is this rendered doc an MH template INPUT (never applied)? The
# template Deployment itself, and every doc labelled MH_TEMPLATE_INPUT_LABEL
# (the placeholder advertise ConfigMap).
MH_TEMPLATE_PART_JQ='def mh_template_part($n; $l): (.kind == "Deployment" and .metadata.name == $n) or (((.metadata.labels // {})[$l] // "") == "true");'

# Apply the environment root, ALWAYS through the wrapper rendered above — chosen
# HERE and nowhere else, so no caller can apply the plain root (placeholder
# images; on a devloop cluster, localhost advertise addresses). A render failure
# aborts; there is no fallback. Requires IMAGE_REFS (resolve_image_refs).
#
# The MH template is rendered in this SAME render (content-tagged image,
# hash-named refs) but EXCLUDED from the apply, together with its template
# inputs; it is stored as ConfigMap mh-template (write_mh_template) for mh.sh.
# No `--prune`: nothing here can delete an mh-s<k> Deployment or a ConfigMap
# generation a live pod references.
# The body is a subshell so the temp dir's EXIT trap fires on every exit path.
apply_env_root() (
    set -euo pipefail
    wrap="$(mktemp -d "${TMPDIR:-/tmp}/dt-env-root.XXXXXX")"
    trap 'rm -rf "${wrap}"' EXIT
    if ! render_env_overlay "${wrap}"; then
        log_error "Could not render the environment overlay; NOT applying (the plain root runs placeholder images and, on a devloop cluster, advertises localhost)."
        exit 1
    fi
    local docs
    if ! docs="$(render_env_root_json "${wrap}")"; then
        log_error "Could not render ${ENV_ROOT_REL} to JSON; NOT applying."
        exit 1
    fi
    log_step "Applying environment root ${ENV_ROOT_REL} (content-tagged images${DT_HOST_GATEWAY_IP:+; devloop advertise addresses via ${DT_HOST_GATEWAY_IP}}; MH template stored, not applied)..."
    printf '%s\n' "${docs}" \
        | jq -s -c --arg n "${MH_TEMPLATE_DEPLOYMENT}" --arg l "${MH_TEMPLATE_INPUT_LABEL}" \
            "${MH_TEMPLATE_PART_JQ}"' {apiVersion: "v1", kind: "List", items: [.[] | select(mh_template_part($n; $l) | not)]}' \
        | ${KUBECTL} apply -f -
    retire_stale_slot_configmaps "${docs}"
    write_mh_template "${docs}"
)

# Un-label (never delete) every ConfigMap labelled MH_SLOT_LABEL that this
# render did not produce: a superseded hash-named slot generation would
# otherwise be a duplicate slot to mh.sh (`no-template`). A live MH pod may still
# reference it by name, so it is kept (ConfigMap generations are never pruned).
# Args: $1 = the render (render_env_root_json output).
retire_stale_slot_configmaps() {
    local current live name
    current="$(printf '%s\n' "$1" | jq -r --arg l "${MH_SLOT_LABEL}" \
        'select(.kind == "ConfigMap" and ((.metadata.labels // {})[$l] // "") != "") | .metadata.name')"
    if [[ -z "${current}" ]]; then
        log_error "retire_stale_slot_configmaps: the render has no ConfigMap labelled ${MH_SLOT_LABEL}"
        return 1
    fi
    live="$(${KUBECTL} get configmaps -n dark-tower -l "${MH_SLOT_LABEL}" \
        -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')" || return 1
    while IFS= read -r name; do
        [[ -n "${name}" ]] || continue
        grep -qxF "${name}" <<< "${current}" && continue
        log_info "Retiring superseded slot ConfigMap ${name} (label removed; kept for any live pod that references it)."
        ${KUBECTL} label configmap "${name}" -n dark-tower "${MH_SLOT_LABEL}-" >/dev/null
    done <<< "${live}"
}

# The template revision: the first 16 hex of sha256 over the canonical
# (sorted-key, compact) rendered Deployment JSON on stdin. THE one derivation.
mh_template_revision() {
    jq -S -c . | sha256sum | cut -c1-16
}

# Store the rendered MH template as ConfigMap mh-template — the template half of
# mh.sh's cluster-side interface: deployment.json, revision, baseline (B, from
# the Kind overlay's mh-fleet.env, its one home), plus the additive
# `previous_images` (the images of the revision this one replaces; carried
# forward on an unchanged revision) that prune_superseded_images keeps for
# rollback, because per-slot Deployments have no rollout history.
# Args: $1 = the render (render_env_root_json output).
write_mh_template() {
    local docs="$1" tpl n fleet baseline rev old old_rev old_prev prev
    tpl="$(printf '%s\n' "${docs}" | jq -c --arg n "${MH_TEMPLATE_DEPLOYMENT}" \
        'select(.kind == "Deployment" and .metadata.name == $n)')"
    n="$(grep -c . <<< "${tpl}" || true)"
    if (( n != 1 )); then
        log_error "write_mh_template: expected exactly one Deployment/${MH_TEMPLATE_DEPLOYMENT} in ${ENV_ROOT_REL}, found ${n}"
        return 1
    fi
    if [[ "$(jq -r '.spec.template.metadata.labels.app // ""' <<< "${tpl}")" != "mh-service" ]]; then
        log_error "write_mh_template: Deployment/${MH_TEMPLATE_DEPLOYMENT} pod template is not labelled app=mh-service"
        return 1
    fi
    fleet="$(grep -E "^${MH_FLEET_KEY}=" "${MH_FLEET_ENV}" 2>/dev/null | sed "s/^${MH_FLEET_KEY}=//" || true)"
    if [[ "$(grep -c . <<< "${fleet}" || true)" -ne 1 || ! "${fleet}" =~ ^[1-9][0-9]{0,2}$ ]]; then
        log_error "write_mh_template: expected exactly one ${MH_FLEET_KEY}=<integer >= 1> line in ${MH_FLEET_ENV}, got '${fleet//$'\n'/,}'"
        return 1
    fi
    baseline="${fleet}"
    rev="$(mh_template_revision <<< "${tpl}")"
    old="$(${KUBECTL} get configmap "${MH_TEMPLATE_CM}" -n dark-tower --ignore-not-found -o json)" || return 1
    prev=""
    if [[ -n "${old}" ]]; then
        old_rev="$(jq -r '.data.revision // ""' <<< "${old}")"
        old_prev="$(jq -r '.data.previous_images // ""' <<< "${old}")"
        if [[ "${old_rev}" == "${rev}" ]]; then
            prev="${old_prev}"
        else
            prev="$(jq -r '.data["deployment.json"] // "{}" | fromjson | [.spec.template.spec.containers[]?.image] | unique | join(" ")' <<< "${old}")" || return 1
        fi
    fi
    log_info "MH template revision ${rev} (baseline ${baseline})."
    ${KUBECTL} create configmap "${MH_TEMPLATE_CM}" -n dark-tower --dry-run=client -o json \
        --from-literal=deployment.json="${tpl}" \
        --from-literal=revision="${rev}" \
        --from-literal=baseline="${baseline}" \
        --from-literal=previous_images="${prev}" \
        | jq -c '.metadata.labels = {"app": "mh-service", "managed-by": "dark-tower"}' \
        | ${KUBECTL} apply -f - >/dev/null
}

# Workloads the environment root owns, DERIVED from its render (so a new
# workload is waited on without editing a list): `<namespace> <kind>/<name>`,
# one per line. With $1 (a repo, e.g. `localhost/mc-service`) set, only the
# workloads whose pod template runs an image of that repo.
#
# The MH template Deployment is EXCLUDED: it is never applied (mh.sh runs the
# per-slot mh-s<k> instances; converge_mh_pool waits for those).
env_root_workloads() {
    local repo="${1:-}"
    cached_render root | awk -v img="${repo:+${repo}:}" -v tpl="${MH_TEMPLATE_DEPLOYMENT}" '
        function emit() {
            if (k ~ /^(Deployment|StatefulSet|DaemonSet)$/ && (img == "" || has_img) && !(k == "Deployment" && n == tpl)) print ns " " tolower(k) "/" n
            k = ""; n = ""; ns = ""; has_img = 0; m = 0
        }
        /^---/ { emit(); next }
        /^kind: / { k = $2 }
        /^metadata:/ { m = 1; next }
        /^[^ ]/ { m = 0 }
        m && /^  name: / { n = $2 }
        m && /^  namespace: / { ns = $2 }
        img != "" && index($0, "image: " img) { has_img = 1 }
        END { emit() }'
}

# Print what the rollout-status line cannot: the pod phase and events that
# distinguish CreateContainerConfigError (missing ConfigMap key), FailedMount
# (missing Secret/ConfigMap volume) and CrashLoopBackOff — see
# docs/runbooks/mc-deployment.md §Config-failure triage.
dump_workload_diagnostics() {
    local ns="$1" res="$2" sel
    sel="$(${KUBECTL} get "${res}" -n "${ns}" \
        -o go-template='{{range $k, $v := .spec.selector.matchLabels}}{{$k}}={{$v}},{{end}}' 2>/dev/null || true)"
    sel="${sel%,}"
    log_error "--- ${ns}/${res}: pods ---"
    ${KUBECTL} get pods -n "${ns}" -o wide ${sel:+-l "${sel}"} || true
    log_error "--- ${ns}/${res}: describe (tail) ---"
    ${KUBECTL} describe pods -n "${ns}" ${sel:+-l "${sel}"} 2>&1 | tail -n 60 || true
    log_error "--- ${ns}: recent events ---"
    ${KUBECTL} get events -n "${ns}" --sort-by=.lastTimestamp 2>&1 | tail -n 25 || true
}

# Wait for EVERY workload the root owns — never only the one a caller asked
# about: applying the root can roll any workload whose config changed, and one
# that crashloops on a new required key must fail this step, not pass green.
# Reports every failing workload (with diagnostics) before failing.
wait_for_env_root() {
    log_step "Waiting for every workload in ${ENV_ROOT_REL} to roll out..."
    local workloads ns res failed=()
    workloads="$(env_root_workloads)" || return 1
    if [[ -z "${workloads}" ]]; then
        log_error "Rendered ${ENV_ROOT_REL} contains no workloads — refusing to report a vacuous success."
        return 1
    fi
    while read -r ns res; do
        if ! ${KUBECTL} rollout status "${res}" -n "${ns}" --timeout=300s; then
            log_error "Rollout did not complete: ${ns}/${res}"
            dump_workload_diagnostics "${ns}" "${res}"
            failed+=("${ns}/${res}")
        fi
    done <<< "${workloads}"
    if (( ${#failed[@]} > 0 )); then
        log_error "Workloads not ready: ${failed[*]}"
        # ONE parseable line naming every failed workload (Layer 7 lifts it into
        # REASON=cluster-deploy-failed; the dumps above are the detail).
        local IFS=,
        deploy_failed rollout-failed "${failed[*]}"
        return 1
    fi
    log_info "All $(wc -l <<< "${workloads}") workloads rolled out."
}

# --- The MH pool (ADR-0039 §7 "On demand", §10) ------------------------------
#
# The ONE mapping from an mh.sh result token to deploy's DEPLOY_FAILED reason.
# Anything not listed is `mh-pool-failed` (a pool/tool fault — apiserver, lock,
# template, drain — never folded into a rollout failure). mh.sh's own token and
# slot ride on the line as MH_TOKEN= / SLOT=.
declare -A MH_DEPLOY_REASONS=(
    [mh-no-free-slot]=mh-no-free-slot
    [mh-rollout-failed]=mh-rollout-failed
    [not-available-timeout]=mh-rollout-failed
    [prerequisite-missing]=prerequisite-missing
)
MH_DEPLOY_REASON_DEFAULT="mh-pool-failed"
MH_OUT=""

# Run `mh.sh <args> --json`; its JSON object lands in MH_OUT (a global, NOT
# stdout: mh_run must run in THIS shell so deploy_failed's "already emitted" flag
# survives and the EXIT trap adds no second line). On failure, map its
# `.reason` token (the contract field; never stderr text, never an exit-code
# literal) to ONE DEPLOY_FAILED line and return 1. mh.sh's human stderr (and its
# final MH_RESULT line) passes through for diagnosis.
mh_run() {
    local out rc=0 token slot reason
    MH_OUT=""
    out="$("${MH_SH}" "$@" --json)" || rc=$?
    if (( rc == 0 )) && jq -e '.ok == true' >/dev/null 2>&1 <<< "${out}"; then
        MH_OUT="${out}"
        return 0
    fi
    token="$(jq -r '.reason // empty' 2>/dev/null <<< "${out}" || true)"
    slot="$(jq -r '.slot // empty' 2>/dev/null <<< "${out}" || true)"
    [[ "${token}" =~ ^[a-z][a-z-]*$ ]] || token="internal-error"
    [[ "${slot}" =~ ^[0-9]+$ ]] || slot=""
    reason="${MH_DEPLOY_REASONS[${token}]:-}"
    if [[ -z "${reason}" ]]; then
        # Unmapped (kubectl-failed, no-kubeconfig, internal-error, ...): classify the
        # ENVIRONMENT first, exactly as main's EXIT trap does for every other step,
        # so a dead apiserver lands in the operator lane, not as a pool fault.
        reason="$(classify_env_failure apiserver)"
        reason="${reason:-${MH_DEPLOY_REASON_DEFAULT}}"
    fi
    log_error "mh.sh $1 failed: REASON=${token}${slot:+ SLOT=${slot}} (exit ${rc}; see the MH_RESULT line above). Old MHs are untouched and keep serving."
    local workloads="-"
    [[ -z "${slot}" ]] || workloads="mh-s${slot}"
    deploy_failed "${reason}" "${workloads}" "MH_TOKEN=${token} SLOT=${slot:--}"
    return 1
}

# jq over a `mh.sh list --json` object: the slots baseline-owned MHs occupy that
# count toward B (starting or serving; a draining/terminating one is leaving).
MH_BASELINE_SLOTS_JQ='[.slots[] | select(.owner == $o and (.state == "serving" or .state == "starting")) | .slot]'

# The first-party repo (tag stripped) the MH template runs, from the cached root
# render (through the ONE YAML->JSON converter; the same tag strip as
# first_party_repos_of). Empty when it cannot be found.
mh_template_repo() {
    cached_render root | yaml_docs_to_json \
        | jq -r --arg n "${MH_TEMPLATE_DEPLOYMENT}" 'select(.kind == "Deployment" and .metadata.name == $n)
            | .spec.template.spec.containers[].image | select(startswith("localhost/")) | sub(":[^:/]*$"; "")' \
        | sort -u
}

# Converge the MH pool to the tree, after the root (GC must be Ready for an MH
# to register): `mh.sh upgrade` (serial start-new-then-drain-old for every MH
# whose revision != mh-template's), then top up with `mh.sh up --owner baseline`
# (or drain extra baseline MHs, highest slot first) until exactly B baseline MHs
# serve. B is read back from mh-template (`.baseline`), never retyped. On any
# failure the old MH keeps serving (mh.sh guarantees it) and ONE DEPLOY_FAILED line
# names the cause (mh_run).
converge_mh_pool() {
    log_step "Converging the MH pool (mh.sh upgrade, then baseline top-up)..."
    local list k_slots b have k i removed live
    mh_run list || return 1; list="${MH_OUT}"
    k_slots="$(jq -r '.slots | length' <<< "${list}")"
    b="$(jq -r '.baseline' <<< "${list}")"
    if [[ ! "${b}" =~ ^[1-9][0-9]{0,2}$ || ! "${k_slots}" =~ ^[1-9][0-9]*$ ]]; then
        log_error "mh.sh list returned baseline '${b}' / ${k_slots} slots: B must be an integer >= 1 and K >= 1 (a B of 0 would 'converge' to zero MHs)."
        deploy_failed mh-pool-failed - "MH_TOKEN=- SLOT=-"
        return 1
    fi
    if (( b + 1 > k_slots )); then
        log_error "MH baseline B=${b} needs B+1 <= K hostPort slots (an upgrade starts the new MH before draining the old), but K=${k_slots}. Lower MH_BASELINE (${MH_FLEET_ENV#"${PROJECT_ROOT}"/}) or raise K (crates/devloop-helper/src/ports.rs; cluster recreate)."
        deploy_failed mh-no-free-slot - "MH_TOKEN=- SLOT=-"
        return 1
    fi
    mh_run upgrade || return 1
    # The old MHs upgrade deleted (contract: `{ok, replaced: [{old, new}...]}`).
    removed="$(jq -r 'if (.replaced | type) == "array" then .replaced | length else "invalid" end' <<< "${MH_OUT}")"
    if [[ ! "${removed}" =~ ^[0-9]+$ ]]; then
        log_error "mh.sh upgrade returned OK without a 'replaced' array (${MH_OUT}); cannot tell whether it removed an MH."
        deploy_failed mh-pool-failed - "MH_TOKEN=- SLOT=-"
        return 1
    fi
    mh_run list || return 1; list="${MH_OUT}"
    have="$(jq -r --arg o "${MH_BASELINE_OWNER}" "${MH_BASELINE_SLOTS_JQ}"' | length' <<< "${list}")"
    for (( i = have; i < b; i++ )); do
        mh_run up --owner "${MH_BASELINE_OWNER}" || return 1
    done
    for (( i = have; i > b; i-- )); do
        k="$(jq -r --arg o "${MH_BASELINE_OWNER}" "${MH_BASELINE_SLOTS_JQ}"' | max' <<< "${list}")"
        mh_run drain "${k}" --owner "${MH_BASELINE_OWNER}" || return 1
        removed=$((removed + 1))
        mh_run list || return 1; list="${MH_OUT}"
    done
    mh_run list || return 1; list="${MH_OUT}"
    have="$(jq -r --arg o "${MH_BASELINE_OWNER}" '[.slots[] | select(.owner == $o and .state == "serving" and (.stale | not))] | length' <<< "${list}")"
    if (( have != b )); then
        log_error "MH pool did not converge: ${have} current-revision baseline MHs serving, expected B=${b}."
        deploy_failed mh-pool-failed - "MH_TOKEN=- SLOT=-"
        return 1
    fi
    log_info "MH pool: ${have} baseline MHs serving (B=${b}, K=${k_slots})."
    (( removed > 0 )) || return 0
    # Every live, REGISTERED MH (any owner): serving = Ready = RegisterMH accepted
    # (MH sets /ready only after register() returns Ok). A starting MH may hold no
    # row yet, so counting it would let a dead row hide behind it.
    live="$(jq -r --arg st "${MH_SETTLE_LIVE_STATE}" '[.slots[] | select(.state == $st)] | length' <<< "${list}")"
    wait_removed_mh_unselectable "${removed}" "${live}"
}

# --- A removed MH must stop being selectable before deploy returns ---------------
#
# Until story 4 MH-3, an MH sends GC no DRAINING report on SIGTERM, so GC keeps a
# removed MH's row `healthy` (and selectable for new meetings) until its heartbeat
# ages past the staleness bound B. A converge that removed an MH (upgrade's old-MH
# delete, or a baseline-shrink drain) would otherwise hand on a cluster where the
# next meeting may be placed on a dead handler. After MH-3 a DRAINING report
# excludes the row at once and this wait shrinks to about two health-checker ticks
# (the recompute control below).
#
# ANCHOR (DRY): the bash twin of
# crates/env-tests/src/fixtures/gc_fleet.rs::wait_removed_mh_unselectable (Rust
# cannot share code with bash). The shared tokens are kept LITERAL here, and an
# env-tests hermetic guard reads this file and pins them against the Rust side:
# the series gc_mh_available, the state set MH_SETTLE_STATES, the live-MH state
# MH_SETTLE_LIVE_STATE, the recompute count MH_SETTLE_RECOMPUTES, the recompute control series
# gc_db_queries_total{operation="count_available_mh",status="success"}, and the
# tick source crates/gc-service/src/tasks/generic_health_checker.rs. Guard:
# crates/env-tests (hermetic) settle_predicate_matches_deploy_sh. Deliberate
# difference: an ABSENT baseline is a failure here (a direct /metrics read of a
# zero-initialised counter, so absent = a broken pod) but is adopted at first
# appearance in Rust (a Prometheus scrape gap).
#
# Evidence is GC's OWN view (never a re-derivation of its selection rule), per
# live GC pod, never summed across pods:
#   (a) predicate: the FRESH rows that are or can become selectable,
#       sum(gc_mh_available{state=~"pending|healthy|degraded"}), number at most the
#       registered live MHs (`mh.sh list` slots in state serving: MH is Ready only
#       after RegisterMH succeeded, so a starting MH may hold no row and must not
#       be counted, or a dead row could hide behind it). `healthy` alone is NOT
#       enough: a just-registered replacement is `pending` until its first load
#       report, so a dead `healthy` row would hide behind it. `draining` is never
#       selectable (and after MH-3 a removed MH reports it); a row past B counts
#       `unhealthy`.
#   (b) positive control: the gauge was RECOMPUTED after the removal. The pod's
#       gc_db_queries_total{operation="count_available_mh",status="success"} must
#       be >= its value at the wait's first read + MH_SETTLE_RECOMPUTES (the counter is recorded
#       before the gauge is set, and one recompute may be in flight at that read,
#       so +2 guarantees a whole recompute that started after it). Absent or 0 is
#       a pod that never recomputed: its zero-initialised gauge proves nothing. A
#       counter that drops below its baseline is a restarted GC container: it is
#       re-baselined at 0, so the new process must recompute twice itself.
# Read straight from each pod's /metrics through the apiserver proxy: no
# Prometheus, no port-forward, and mh.sh never reads GC (its contract).
# Residual (shared with the fixture): an unhealthy live MH could hide the removed
# row; env-test 03's healthy >= live check covers that state.
#
# The ceiling is FAILURE-ONLY: B = the largest gc_mh_staleness_threshold_seconds
# the RUNNING GC pods publish (the process truth, which env-test 03 pins equal to
# the live ConfigMap's k x I), plus MH_SETTLE_MARGIN_SECONDS (default: two GC
# health-checker ticks, read from the source constant the GC image is built from;
# the env-test fixture adds its scrape ceiling instead, since it reads through
# Prometheus).
# Poll interval for the wait (a knob so the self-test runs instantly).
MH_SETTLE_POLL_SECONDS="${DT_MH_SETTLE_POLL_SECONDS:-2}"
# The gc_mh_available states that are or can become selectable (predicate (a)).
readonly MH_SETTLE_STATES='pending|healthy|degraded'
# Whole count_available_mh recomputes required since the first read (predicate
# (b)); +2 because one recompute may be in flight at the baseline read.
readonly MH_SETTLE_RECOMPUTES=2
# The `mh.sh list` slot state that counts as a registered live MH.
readonly MH_SETTLE_LIVE_STATE="serving"
# The GC health-checker tick's ONE home (the GC image is built from this tree).
GC_HEALTH_TICK_SRC="${PROJECT_ROOT}/crates/gc-service/src/tasks/generic_health_checker.rs"

# The GC health-checker tick in seconds, read from its source constant. Fails
# loudly when the declaration is absent or malformed.
gc_health_tick_seconds() {
    local v
    v="$(sed -nE 's/^pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = ([1-9][0-9]*);$/\1/p' "${GC_HEALTH_TICK_SRC}" 2>/dev/null)"
    if [[ ! "${v}" =~ ^[1-9][0-9]*$ ]]; then
        log_error "gc_health_tick_seconds: no 'pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = <n>;' in ${GC_HEALTH_TICK_SRC#"${PROJECT_ROOT}"/}"
        return 1
    fi
    printf '%s\n' "${v}"
}

# The live (not terminating, Ready) GC pods as `<ns> <name> <http-port>` lines.
# The Deployment AND its namespace are DERIVED from the render (the workload
# running the gc-service image); its selector and each pod's `http` port are read
# from the cluster.
live_gc_pods() {
    local workloads ns res sel
    workloads="$(env_root_workloads localhost/gc-service | awk '$2 ~ /^deployment\//')" || return 1
    if [[ "$(grep -c . <<< "${workloads}")" -ne 1 ]]; then
        log_error "live_gc_pods: expected exactly one gc-service Deployment in the root render, got: ${workloads//$'\n'/; }"
        return 1
    fi
    read -r ns res <<< "${workloads}"
    sel="$(${KUBECTL} get "${res}" -n "${ns}" \
        -o go-template='{{range $k, $v := .spec.selector.matchLabels}}{{$k}}={{$v}},{{end}}')" || return 1
    [[ -n "${sel%,}" ]] || { log_error "live_gc_pods: ${ns}/${res} has no selector"; return 1; }
    ${KUBECTL} get pods -n "${ns}" -l "${sel%,}" -o json | jq -r --arg ns "${ns}" '
        .items[]
        | select(.metadata.deletionTimestamp == null)
        | select(any(.status.conditions[]?; .type == "Ready" and .status == "True"))
        | "\($ns) \(.metadata.name) \([.spec.containers[].ports[]? | select(.name == "http") | .containerPort][0] // "-")"'
}

# One GC pod's /metrics through the apiserver proxy. Args: <ns> <pod> <port>.
gc_pod_metrics() {
    ${KUBECTL} get --raw "/api/v1/namespaces/$1/pods/$2:$3/proxy/metrics"
}

# PURE: from one /metrics body on stdin, print `<selectable> <success> <error>`:
# the sum of gc_mh_available over MH_SETTLE_STATES (ABSENT when no such series),
# and gc_db_queries_total{operation="count_available_mh"} for status success /
# error (ABSENT when missing). Label order is not assumed.
gc_settle_reading() {
    awk -v states="${MH_SETTLE_STATES}" '
        /^gc_mh_available\{/ {
            if (match($0, /state="[a-z]+"/)) {
                st = substr($0, RSTART + 7, RLENGTH - 8)
                if (st ~ ("^(" states ")$")) { sel += $NF; nsel++ }
            }
        }
        /^gc_db_queries_total\{/ && /operation="count_available_mh"/ {
            if (/status="success"/) { ok = $NF; nok = 1 }
            if (/status="error"/) { err = $NF; nerr = 1 }
        }
        END {
            printf "%s %s %s\n", (nsel ? sprintf("%d", sel) : "ABSENT"), (nok ? sprintf("%d", ok) : "ABSENT"), (nerr ? sprintf("%d", err) : "ABSENT")
        }'
}

# The ONE failure emitter of the settle wait: the ENVIRONMENT is classified first
# (exactly as mh_run and main's EXIT trap do), so an apiserver that dies mid-wait
# lands in the operator lane, never as mh-removed-still-selectable.
mh_settle_failed() {
    local reason
    reason="$(classify_env_failure apiserver)"
    deploy_failed "${reason:-mh-removed-still-selectable}" "deployment/gc-service" "MH_TOKEN=- SLOT=-"
}

# Wait until, on every live GC pod, a post-removal recompute shows no more
# selectable rows than registered live MHs. Args: <removed count> <live count>.
#   DEPLOY_FAILED REASON=mh-removed-still-selectable MH_TOKEN=- SLOT=- WORKLOADS=deployment/gc-service
# (or an environment reason, via mh_settle_failed). It fails AT ONCE, before any
# wait, when there is no Ready GC pod, no `http` port, /metrics is unreadable or
# the staleness gauge is absent; the log_error line above names which.
wait_removed_mh_unselectable() {
    local removed="$1" live="$2" pods ns name port m b bound=0 tick margin deadline over last="" sel ok err
    local -A base=()
    tick="$(gc_health_tick_seconds)" || return 1
    margin="${DT_MH_SETTLE_MARGIN_SECONDS:-$((2 * tick))}"
    # A failed cluster read is left to main's EXIT trap (environment probe first).
    pods="$(live_gc_pods)" || return 1
    if [[ -z "${pods}" ]]; then
        log_error "No live, Ready GC pod to read; cannot prove the removed MH is unselectable."
        mh_settle_failed
        return 1
    fi
    # First read: the bound, and each pod's recompute baseline (control (b)).
    while read -r ns name port; do
        if [[ ! "${port}" =~ ^[0-9]+$ ]]; then
            log_error "GC pod ${ns}/${name} has no container port named 'http' to read /metrics from."
            mh_settle_failed
            return 1
        fi
        m="$(gc_pod_metrics "${ns}" "${name}" "${port}")" || { log_error "Could not read /metrics of GC pod ${ns}/${name}."; mh_settle_failed; return 1; }
        b="$(awk '$1 == "gc_mh_staleness_threshold_seconds" { print $2; exit }' <<< "${m}")"
        if [[ ! "${b}" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
            log_error "GC pod ${ns}/${name} publishes no gc_mh_staleness_threshold_seconds (got '${b}')."
            mh_settle_failed
            return 1
        fi
        b="${b%%.*}"; (( b + 1 > bound )) && bound=$((b + 1))
        read -r _ ok _ <<< "$(gc_settle_reading <<< "${m}")"
        base["${name}"]="${ok}"
    done <<< "${pods}"
    log_step "Waiting for GC to stop selecting the ${removed} removed MH(s) (registered live MHs: ${live}; at most ${bound}s + ${margin}s)..."
    deadline=$(( $(date +%s) + bound + margin ))
    while :; do
        over=""
        while read -r ns name port; do
            m="$(gc_pod_metrics "${ns}" "${name}" "${port}")" || m=""
            read -r sel ok err <<< "$(gc_settle_reading <<< "${m}")"
            # A counter BELOW its baseline is a restarted GC container (the counter
            # is per process): re-baseline at 0, so the new process must itself
            # recompute twice — never a false red until the deadline.
            if [[ "${ok}" =~ ^[0-9]+$ && "${base[${name}]}" =~ ^[0-9]+$ ]] && (( ok < base[${name}] )); then
                base["${name}"]=0
            fi
            # Control (b): a pod whose recompute never succeeded, or has not run
            # twice since the baseline, has proven nothing yet.
            if [[ "${ok}" == "ABSENT" || "${ok}" == "0" || "${base[${name}]}" == "ABSENT" ]] \
                || (( ok < base[${name}] + MH_SETTLE_RECOMPUTES )); then
                over+="${name}=unrecomputed(success=${ok},baseline=${base[${name}]},error=${err}) "
            elif [[ "${sel}" == "ABSENT" ]] || (( sel > live )); then
                over+="${name}=${sel}(error=${err}) "
            fi
        done <<< "${pods}"
        last="${over}"
        [[ -n "${over}" ]] || { log_info "GC no longer selects the removed MH(s)."; return 0; }
        (( $(date +%s) < deadline )) || break
        sleep "${MH_SETTLE_POLL_SECONDS}"
    done
    log_error "After B (${bound}s, rounded up) + ${margin}s, GC pod(s) still count more fresh ${MH_SETTLE_STATES} MH rows than the ${live} registered live one(s), or have not recomputed the count since the removal: ${last% }. Either a removed MH is still selectable (until story 4 MH-3 a removed MH sends no DRAINING report), so the next meeting may be placed on a dead handler, or the count is FROZEN: check gc_db_queries_total{operation=\"count_available_mh\",status=\"error\"} (the error= figure above) and the GC health-checker's 'MH registry tick step failed' lines (kubectl logs -n ${ns} deploy/gc-service); ./infra/kind/scripts/mh.sh list."
    mh_settle_failed
    return 1
}

# --- Migrations as an in-cluster Job (ADR-0038 §2 step 3) -----------------
#
# Replaces the host path (port-forward + host `sqlx migrate run`, silently
# SKIPPED when sqlx was absent). The Job runs the image built from
# infra/docker/db-migrate/ (sqlx-cli + migrations/) and MUST reach Complete
# before the root is applied, so no AC/GC pod on new code meets an old schema —
# and before the seeds, which need the tables.
#
# Rollback in the dev cluster: there are no down-migrations. A bad or edited
# migration is fixed by restoring the file, or by recreating the cluster
# (`dev-cluster teardown`, then `dev-cluster provision` + `dev-cluster deploy`). Never hand-edit
# `_sqlx_migrations`.

# Poll interval for the Job wait (a knob so the self-test runs instantly).
JOB_POLL_SECONDS="${DT_JOB_POLL_SECONDS:-2}"
# Slack on top of the Job's own activeDeadlineSeconds before setup gives up
# polling (the Job normally goes Failed/DeadlineExceeded first). A knob only so
# the self-test can exercise the timeout lane without waiting a minute.
JOB_WAIT_MARGIN_SECONDS="${DT_JOB_WAIT_MARGIN_SECONDS:-60}"

# Redact URL userinfo from anything printed out of the cluster: the Job's URL
# carries none by construction, but its logs reach the Layer-7 log, so a
# `postgres://user:pass@` from any source must never pass through.
redact_db_urls() {
    sed -E 's#(postgres(ql)?://)[^@/[:space:]]*@#\1<redacted>@#g'
}

# The Job's name: `db-migrate-<first 10 hex of sha256(rendered Job, tag set)>`.
# The ONE place the name is derived. The render includes the image tag (so the
# migrations, through the image ID) and the whole spec, so an unchanged set
# re-applies as a no-op, and any change — migrations OR job.yaml — is a NEW Job
# (a Job's pod template is immutable; reusing a name could never update it).
# Reads the rendered YAML on stdin.
migration_job_name() {
    local h
    h="$(sha256sum | cut -c1-10)"
    printf 'db-migrate-%s\n' "${h}"
}

# Render the migration Job into directory $1 for image ref $2 and print its
# name. PURE (local kustomize only; setup.test.sh renders it). Two passes: the
# first sets the tag and is hashed; the second renames the Job by that hash.
render_migration_job() {
    local dir="$1" ref="$2" rel name
    if ! is_content_ref "${MIGRATE_REPO}" "${ref}"; then
        log_error "render_migration_job: not a content-tagged ${MIGRATE_REPO} ref: '${ref}'"
        return 1
    fi
    rel="$(realpath --relative-to="${dir}" "${MIGRATE_BASE}")" || return 1
    {
        echo "# GENERATED by infra/kind/scripts/deploy.sh:render_migration_job — do not edit."
        echo "apiVersion: kustomize.config.k8s.io/v1beta1"
        echo "kind: Kustomization"
        echo "resources:"
        echo "  - ${rel}"
        echo "images:"
        echo "  - name: ${MIGRATE_REPO}"
        echo "    newTag: ${ref##*:}"
    } > "${dir}/kustomization.yaml"
    name="$(kubectl kustomize "${dir}" | migration_job_name)" || return 1
    {
        echo "patches:"
        echo "  - target:"
        echo "      kind: Job"
        echo "      name: db-migrate"
        echo "    patch: |-"
        echo "      - op: replace"
        echo "        path: /metadata/name"
        echo "        value: ${name}"
    } >> "${dir}/kustomization.yaml"
    printf '%s\n' "${name}"
}

# activeDeadlineSeconds of the rendered Job on stdin — the SSoT the setup-side
# wait is derived from. Missing is an error, never a default.
job_active_deadline() {
    local v
    v="$(awk '/^kind: Job$/{j=1} j && /^  activeDeadlineSeconds: /{print $2; exit}')"
    if [[ ! "${v}" =~ ^[1-9][0-9]*$ ]]; then
        log_error "The rendered migration Job has no activeDeadlineSeconds; refusing to wait without a bound."
        return 1
    fi
    printf '%s\n' "${v}"
}

# The Job's terminal condition as `<Type> <reason>` (Complete / Failed), or
# empty while it runs. Returns non-zero when the Job cannot be read.
job_condition() {
    ${KUBECTL} get job "$1" -n "${MIGRATE_NAMESPACE}" --ignore-not-found \
        -o jsonpath='{range .status.conditions[?(@.status=="True")]}{.type}{" "}{.reason}{"\n"}{end}' \
        | awk '$1 == "Complete" || $1 == "Failed" { print; exit }'
}

# Everything an operator needs from a failed/timed-out Job, BEFORE any cleanup:
# logs of EVERY attempt (backoffLimit gives more than one pod), describe, events.
# Redacted; never `get -o yaml`, never the pod env.
dump_migration_job_diagnostics() {
    local name="$1"
    log_error "--- job/${name}: logs (all attempts) ---"
    ${KUBECTL} logs -n "${MIGRATE_NAMESPACE}" -l "job-name=${name}" --all-containers --prefix --tail=-1 2>&1 \
        | redact_db_urls || true
    log_error "--- job/${name}: describe ---"
    ${KUBECTL} describe job "${name}" -n "${MIGRATE_NAMESPACE}" 2>&1 | redact_db_urls | tail -n 40 || true
    log_error "--- ${MIGRATE_NAMESPACE}: recent events ---"
    ${KUBECTL} get events -n "${MIGRATE_NAMESPACE}" --sort-by=.lastTimestamp 2>&1 | tail -n 25 || true
}

# Run the migration Job for IMAGE_REFS[MIGRATE_REPO] and wait for it. Fails
# LOUDLY (non-zero, a REASON= token, full diagnostics) on a Failed Job or when
# the derived deadline passes; deploy then exits non-zero before the seeds and
# the root apply.
run_migration_job() (
    set -euo pipefail
    ref="${IMAGE_REFS[${MIGRATE_REPO}]:-}"
    wrap="$(mktemp -d "${TMPDIR:-/tmp}/dt-migrate.XXXXXX")"
    trap 'rm -rf "${wrap}"' EXIT
    name="$(render_migration_job "${wrap}" "${ref}")" || exit 1
    deadline="$(kubectl kustomize "${wrap}" | job_active_deadline)" || exit 1
    log_step "Running database migrations as job/${name} (${ref})..."

    if ! cond="$(job_condition "${name}")"; then
        log_error "Could not read job/${name} from cluster '${CLUSTER_NAME}'."
        exit 1
    fi
    noop=0
    case "${cond%% *}" in
        Complete)
            noop=1
            ;;
        Failed)
            # A Failed Job is not a record of anything: its failure was already
            # reported loudly by the run that saw it. Re-create = retry; a
            # deterministic failure (checksum drift) fails loudly again.
            log_warn "job/${name} exists and FAILED earlier (${cond#* }); deleting it to retry."
            ${KUBECTL} delete job "${name}" -n "${MIGRATE_NAMESPACE}" --ignore-not-found --wait=true
            ${KUBECTL} apply -k "${wrap}"
            ;;
        *)
            ${KUBECTL} apply -k "${wrap}"
            ;;
    esac

    budget=$(( deadline + JOB_WAIT_MARGIN_SECONDS ))
    start=${SECONDS}
    while :; do
        if ! cond="$(job_condition "${name}")"; then
            log_error "Could not read job/${name} while waiting for it."
            dump_migration_job_diagnostics "${name}"
            exit 1
        fi
        case "${cond%% *}" in
            Complete) break ;;
            Failed)
                dump_migration_job_diagnostics "${name}"
                # The Job's OWN deadline (activeDeadlineSeconds) ends as
                # Failed/DeadlineExceeded — the COMMON timeout path; the
                # deploy-poll-timeout below is only a backstop. A deadline means
                # the Job never finished (usually no sqlx output at all), so it
                # gets the timeout token and advice, never "read sqlx's error".
                if [[ "${cond#* }" == "DeadlineExceeded" ]]; then
                    echo "MIGRATION_FAILURE: job/${name} did not finish within its activeDeadlineSeconds ${deadline} (reason=DeadlineExceeded) REASON=migration-timeout" >&2
                    echo "  Usually Postgres is unreachable (NetworkPolicy, pod not Ready) or the image cannot start (ErrImageNeverPull = not loaded); see the describe/events above." >&2
                    exit 1
                fi
                echo "MIGRATION_FAILURE: job/${name} failed (reason=${cond#* }) REASON=migration-failed" >&2
                echo "  The logs above carry sqlx's error. 'previously applied but has been modified' = an applied migration file was edited: restore it, or recreate the dev cluster: $(remedy "'dev-cluster teardown', then 'dev-cluster provision' + 'dev-cluster deploy'" "'./infra/kind/scripts/teardown.sh', then './infra/kind/scripts/setup.sh'") — no down-migrations; never hand-edit _sqlx_migrations. See docs/runbooks/devloop-validation.md." >&2
                exit 1
                ;;
        esac
        if (( SECONDS - start >= budget )); then
            dump_migration_job_diagnostics "${name}"
            echo "MIGRATION_FAILURE: job/${name} did not finish within ${budget}s (activeDeadlineSeconds ${deadline} + ${JOB_WAIT_MARGIN_SECONDS}s; reason=deploy-poll-timeout) REASON=migration-timeout" >&2
            echo "  Usually Postgres is unreachable (NetworkPolicy, pod not Ready) or the image cannot start (ErrImageNeverPull = not loaded)." >&2
            exit 1
        fi
        sleep "${JOB_POLL_SECONDS}"
    done

    if (( noop )); then
        # Nothing ran: the Complete Job of this name IS the record, and its
        # pod's log is the run that applied the migrations. No log dump — it
        # would reprint that earlier run's output as if it were this one's —
        # and no claim that anything was applied.
        log_info "Migrations unchanged (job/${name} already Complete); nothing applied."
    else
        # The record of what ran. The Succeeded pod (and its logs) stays with
        # its Job until a later Job supersedes it; the devloop helper's status
        # exempts Job-owned Succeeded pods (parse_pod_health).
        ${KUBECTL} logs -n "${MIGRATE_NAMESPACE}" -l "job-name=${name}" --all-containers --prefix --tail=-1 2>&1 \
            | redact_db_urls || true
        log_info "Migrations applied (job/${name} Complete)."
    fi

    # Older migration Jobs are history, not records (the Complete Job of THIS
    # name is the record). Never the current one. Their pods go with them.
    ${KUBECTL} delete jobs -n "${MIGRATE_NAMESPACE}" -l app=db-migrate \
        --field-selector "metadata.name!=${name}" --ignore-not-found
)

# The image refs of the PREVIOUS generation of <repo>'s root workloads — what a
# one-step `kubectl rollout undo` reaches: the newest non-current ReplicaSet of
# each Deployment, the newest non-current ControllerRevision of each
# StatefulSet/DaemonSet. Read from the controllers' own history (the SSoT for
# rollback), never from "what ran before this deploy" — an unchanged redeploy
# would otherwise make the current ref its own "previous" and evict the real one.
# One per line; empty when there is no history. Non-zero when it cannot be read.
previous_generation_refs() {
    local repo="$1" ns res kind name sel out workloads
    workloads="$(env_root_workloads "${repo}")" || return 1
    while read -r ns res; do
        [[ -n "${res}" ]] || continue
        kind="${res%%/*}"; name="${res#*/}"
        case "${kind}" in
            deployment)
                sel="$(${KUBECTL} get "${res}" -n "${ns}" \
                    -o go-template='{{range $k, $v := .spec.selector.matchLabels}}{{$k}}={{$v}},{{end}}')" || return 1
                out="$(${KUBECTL} get replicasets -n "${ns}" -l "${sel%,}" \
                    -o jsonpath='{range .items[*]}{.metadata.annotations.deployment\.kubernetes\.io/revision}{" "}{.metadata.ownerReferences[0].name}{" "}{.spec.template.spec.containers[*].image}{"\n"}{end}')" || return 1
                ;;
            statefulset|daemonset)
                out="$(${KUBECTL} get controllerrevisions -n "${ns}" \
                    -o jsonpath='{range .items[*]}{.revision}{" "}{.metadata.ownerReferences[0].name}{" "}{.data.spec.template.spec.containers[*].image}{"\n"}{end}')" || return 1
                ;;
            *) continue ;;
        esac
        # Rows owned by this workload, newest revision first; the 2nd is the previous.
        awk -v n="${name}" '$2 == n' <<< "${out}" | sort -k1,1nr | sed -n 2p \
            | awk '{ for (i = 3; i <= NF; i++) print $i }' | grep "^${repo}:" || true
    done <<< "${workloads}"
}

# Remove first-party images this cluster no longer needs, KEEPING two
# generations per repo: the ref this converge deploys (IMAGE_REFS) and the
# previous generation (previous_generation_refs — what `kubectl rollout undo`
# reaches; for the migrations image, which has no rollback, the ref of the
# previous Job). Anything older is evicted from the Kind node (`crictl rmi`) and
# from the host store (`rmi`, never -f). ConfigMap generations are NEVER pruned
# here. Hygiene, not correctness: every failure is a WARN line
#   PRUNE_WARN REASON=image-prune-failed REF=<ref> WHERE=<node|host|history>
# and deploy continues; an unreadable history skips that repo (never evicts on
# uncertainty); kubelet image GC is the backstop in the node.
#
# CROSS-SLUG RACE, accepted: content tags are shared by content, so another
# devloop that just built identical code holds the SAME ref; removing it between
# that devloop's build and its `kind load` makes the load fail LOUDLY there —
# never wrong code. Only refs THIS cluster's node held are evicted (not every
# sha-* on the host), which keeps that window narrow.
#
# MH (ADR-0039 §10): per-slot Deployments have no rollout history, so the MH repo's
# keep set is the ref this converge deploys ∪ the images of EVERY LIVE MH pod (an
# old revision mid-upgrade must crash-restart in place) ∪ mh-template's
# `previous_images` (the revision before this one, for rollback). If any of those
# cannot be read, the MH repo is NOT pruned, loudly (PRUNE_SKIPPED). ConfigMap
# generations referenced by a live MH pod are never pruned (no ConfigMap is).
prune_superseded_images() {
    local repo ref cmd node listed keep prev mh_repo mh_tpl_images="" mh_keep="" mh_ok=true
    cmd="$(container_cmd)"
    mh_repo="$(mh_template_repo)"
    if [[ -z "${mh_repo}" ]]; then
        echo "PRUNE_SKIPPED SERVICE=all REASON=mh-repo-unknown (no image in Deployment/${MH_TEMPLATE_DEPLOYMENT} of the root render; not pruning anything)" >&2
        return 0
    fi
    node="$(kind_node)"
    if [[ -z "${node}" ]]; then
        echo "PRUNE_WARN REASON=image-prune-failed REF=- WHERE=node (kind lists no node for ${CLUSTER_NAME})" >&2
        return 0
    fi
    if ! listed="$(${cmd} exec "${node}" crictl images 2>/dev/null)"; then
        echo "PRUNE_WARN REASON=image-prune-failed REF=- WHERE=node (crictl images failed in ${node})" >&2
        return 0
    fi
    if ! mh_tpl_images="$(${KUBECTL} get configmap "${MH_TEMPLATE_CM}" -n dark-tower -o json \
            | jq -er '(.data["deployment.json"] | fromjson | [.spec.template.spec.containers[]?.image]) + ((.data.previous_images // "") | split(" ")) | .[] | select(. != "")')" \
        || [[ -z "${mh_tpl_images}" ]]; then
        echo "PRUNE_SKIPPED SERVICE=mh REASON=template-unreadable (could not read ${MH_TEMPLATE_CM}; MH images kept)" >&2
        mh_ok=false
    elif ! mh_keep="$(${KUBECTL} get pods -n dark-tower -l app=mh-service \
            -o jsonpath='{range .items[*]}{range .spec.containers[*]}{.image}{"\n"}{end}{end}')"; then
        echo "PRUNE_SKIPPED SERVICE=mh REASON=live-pods-unreadable (could not list live MH pods; MH images kept)" >&2
        mh_ok=false
    fi
    for repo in "${!IMAGE_REFS[@]}"; do
        if [[ "${repo}" == "${mh_repo}" ]]; then
            [[ "${mh_ok}" == "true" ]] || continue
            prev="$(printf '%s\n%s\n' "${mh_tpl_images}" "${mh_keep}")"
        elif [[ "${repo}" == "${MIGRATE_REPO}" ]]; then
            prev="${DEPLOYED_REFS[${repo}]:-}"
        elif ! prev="$(previous_generation_refs "${repo}")"; then
            echo "PRUNE_WARN REASON=image-prune-failed REF=- WHERE=history (could not read the rollout history of ${repo}'s workloads; not pruning it)" >&2
            continue
        fi
        keep="$(printf '%s\n%s\n' "${IMAGE_REFS[${repo}]}" "${prev}")"
        while read -r ref; do
            [[ -n "${ref}" ]] || continue
            grep -qxF "${ref}" <<< "${keep}" && continue
            log_info "Pruning ${ref} (older than the previous generation)..."
            ${cmd} exec "${node}" crictl rmi "${ref}" >/dev/null 2>&1 \
                || echo "PRUNE_WARN REASON=image-prune-failed REF=${ref} WHERE=node" >&2
            ${cmd} rmi "${ref}" >/dev/null 2>&1 \
                || echo "PRUNE_WARN REASON=image-prune-failed REF=${ref} WHERE=host (in use elsewhere, or already gone)" >&2
        done <<< "$(awk -v r="${repo}" '$1 == r && $2 ~ /^sha-[0-9a-f]+$/ { print $1 ":" $2 }' <<< "${listed}")"
    done
}

# Build EVERY first-party repo (content-tagged, loaded into Kind when the node
# lacks the ref). With a warm build cache an unchanged repo rebuilds to the same
# image ID, hence the same tag, hence no rollout.
build_first_party_images() {
    local repo repos all
    all="$(first_party_repos)" || return 1
    mapfile -t repos <<< "${all}"
    for repo in "${repos[@]}"; do
        build_content_tagged_image "${repo}"
    done
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


# Refuse to deploy onto a platform that does not match the tree's blueprint —
# missing (never provisioned, or half-built: provision records only after a
# successful build), stale (the blueprint changed since it was built, incl. a
# TLS leaf due for renewal) or unreadable. ONE render serves this check, the
# build and provision's own check: `provision.sh --check`.
check_blueprint() {
    local out rc=0
    out="$("${PROVISION_SH}" --check 2>&1)" || rc=$?
    printf '%s\n' "${out}"
    if (( rc != 0 )); then
        local reason
        reason="$(sed -n 's/^BLUEPRINT .*REASON=\([a-z-]*\).*/\1/p' <<< "${out}" | tail -n1)"
        deploy_failed "blueprint-${reason:-unreadable}" -
        echo "  The cluster's platform does not match the tree (see the BLUEPRINT line above). Provision it first: $(remedy "'dev-cluster provision'" "'./infra/kind/scripts/setup.sh'")." >&2
        return 1
    fi
}

# main's EXIT trap: a deploy that fails WITHOUT a specific DEPLOY_FAILED line
# (a manifest the apiserver rejects, an image build, a migration, the disk
# floor, ...) still ends with one, naming the step — complete by construction,
# not per call site.
DEPLOY_STEP=start
#
# The shared, bounded environment classifier (lib/common.sh:classify_env_failure)
# separates the environment from the tree: a dead container runtime is
# `runtime-unreachable`, a silent apiserver `apiserver-unreachable` (operator
# lane); once both answer, the failure is the tree's (`step-failed`, implementer
# lane) — the relayed output above carries the detail.
__deploy_exit_trap() {
    local rc=$?
    if (( rc != 0 )) && [[ "${DEPLOY_FAILED_EMITTED}" != "true" ]]; then
        local env_reason
        env_reason="$(classify_env_failure apiserver)"
        echo "DEPLOY_FAILED REASON=${env_reason:-step-failed} STEP=${DEPLOY_STEP} WORKLOADS=-" >&2
    fi
}

# check_prerequisites (lib/common.sh) with the environment classified: a missing
# tool is `prerequisite-missing`, never a step failure blamed on the tree.
deploy_check_prerequisites() {
    local missing
    if ! missing="$(missing_prerequisite)"; then
        deploy_failed prerequisite-missing -
        log_error "${missing} is not installed on this host."
        exit 1
    fi
    check_prerequisites
}

# Run one converge step, recording it for the EXIT trap.
step() {
    DEPLOY_STEP="$1"
    "$@"
}

main() {
    trap __deploy_exit_trap EXIT
    log_info "Deploying Dark Tower to kind cluster '${CLUSTER_NAME}' (converging the application to the tree)..."
    local exists_rc=0
    cluster_exists || exists_rc=$?
    if (( exists_rc == 2 )); then
        deploy_failed blueprint-unreadable -
        log_error "'kind get clusters' failed — cannot tell whether cluster '${CLUSTER_NAME}' exists. Check the host container runtime."
        exit 1
    fi
    if (( exists_rc != 0 )); then
        deploy_failed blueprint-missing -
        log_error "Cluster '${CLUSTER_NAME}' does not exist. Provision it first: $(remedy "'dev-cluster provision'" "'./infra/kind/scripts/setup.sh'")."
        exit 1
    fi
    step check_blueprint
    step deploy_check_prerequisites
    step cache_renders
    step preload_third_party_images
    step build_first_party_images
    step resolve_image_refs
    # Pre-applied ahead of the root for ORDERING only (each is a byte-identical
    # subset of the root render — scripts/setup.test.sh pins that — so the root
    # apply below is a no-op on them): the migration Job needs PostgreSQL (and
    # its NetworkPolicy admitting the Job), and R-54 fail-hard OTel init needs
    # the collector Ready before any service rolls.
    step deploy_postgres
    step deploy_redis
    # Migrations BEFORE the seeds (they need the tables) and before the root
    # (no service pod may start against an unmigrated schema). Fails loudly.
    step run_migration_job
    step seed_test_data
    step seed_demo_org
    step deploy_otel_collector
    step apply_env_root
    step wait_for_env_root
    step converge_mh_pool
    step prune_superseded_images
    log_info "Deployed: every workload in ${ENV_ROOT_REL} is rolled out."
}

# Run main only when executed, not when sourced (scripts/setup.test.sh sources
# this script to exercise its functions directly).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
