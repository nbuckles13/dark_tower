#!/usr/bin/env bash
# Build every first-party image ONCE and push it to a registry, writing the
# digest-pinned refs for the test job (deploy.sh DT_PREBUILT_IMAGES).
#
# Usage: infra/ci/build-push-images.sh <out-file>
#   DT_IMAGE_REGISTRY   registry prefix, e.g. ghcr.io/<owner>/dark-tower-ci (required)
#   DT_IMAGE_SOURCE_REV source revision; becomes the traceability tag git-<rev> (required)
#   DT_IMAGE_BUILD_CACHE optional layer cache backend (see deploy.sh image_build_flags)
#
# The repo list, Dockerfile convention, target stage, build-args and cache flags
# all come from deploy.sh (sourced for its functions), so what CI pushes is
# exactly what a local converge builds. Consumers pull by DIGEST; the tag is for
# humans and cleanup only and may be repointed.
set -euo pipefail

out="${1:?usage: build-push-images.sh <out-file>}"
: "${DT_IMAGE_REGISTRY:?DT_IMAGE_REGISTRY is required}"
: "${DT_IMAGE_SOURCE_REV:?DT_IMAGE_SOURCE_REV is required}"
[[ "${DT_IMAGE_SOURCE_REV}" =~ ^[0-9a-f]{40}$ ]] \
    || { echo "ERROR: DT_IMAGE_SOURCE_REV must be a full git sha" >&2; exit 1; }

# Functions only: deploy.sh runs main when executed, not when sourced. Clear the
# positional args first — it parses "$@" at source time.
set --
# shellcheck source=../kind/scripts/deploy.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../kind/scripts/deploy.sh"

cache_renders
repos="$(first_party_repos)"
: > "${out}"
while IFS= read -r repo; do
    name="${repo#localhost/}"
    mapfile -t flags < <(image_build_flags "${repo}")
    meta="$(mktemp "${TMPDIR:-/tmp}/dt-meta.XXXXXX")"
    start=$(date +%s)
    log_step "Building + pushing ${repo} -> ${DT_IMAGE_REGISTRY}/${name}"
    docker buildx build "${flags[@]}" \
        --tag "${DT_IMAGE_REGISTRY}/${name}:git-${DT_IMAGE_SOURCE_REV}" \
        --push --metadata-file "${meta}" \
        -f "${PROJECT_ROOT}/infra/docker/${name}/Dockerfile" "${PROJECT_ROOT}"
    digest="$(jq -r '."containerimage.digest" // empty' "${meta}")"
    rm -f "${meta}"
    [[ "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]] \
        || { log_error "No pushed digest for ${repo} (got '${digest}')"; exit 1; }
    printf '%s %s@%s\n' "${repo}" "${DT_IMAGE_REGISTRY}/${name}" "${digest}" >> "${out}"
    log_info "${repo}: ${digest} in $(( $(date +%s) - start ))s"
done <<< "${repos}"
