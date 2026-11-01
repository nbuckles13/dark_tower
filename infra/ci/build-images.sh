#!/usr/bin/env bash
# Build every first-party image ONCE for the test jobs, writing the refs file
# deploy.sh DT_PREBUILT_IMAGES reads.
#
# Usage: infra/ci/build-images.sh --push <refs-file>
#        infra/ci/build-images.sh --archive <dir>
#   --push     push to the CI registry; <refs-file> gets `<repo> <registry>/<name>@sha256:<digest>`
#              (main tier: the build job holds the package-write token, the test jobs pull)
#   --archive  write <dir>/<name>.tar docker archives and <dir>/image-refs.txt with
#              `<repo> sha256:<image id>` (PR tier: pushes nothing; the test jobs
#              `docker load` the archives from this run's artifact)
#   DT_IMAGE_REGISTRY   registry prefix for --push (default: infra/ci/ci-images.sh)
#   DT_IMAGE_SOURCE_REV source revision; becomes the --push traceability tag git-<rev> (required)
#   DT_IMAGE_BUILD_CACHE optional layer cache backend (see deploy.sh image_build_flags)
#
# The repo list, Dockerfile convention, target stage, build-args and cache flags
# all come from deploy.sh (sourced for its functions), so what CI builds is
# exactly what a local converge builds. Consumers use the image by DIGEST or ID;
# the --push tag is for humans and cleanup only and may be repointed.
set -euo pipefail

usage="usage: build-images.sh --push <refs-file> | --archive <dir>"
mode="${1:?${usage}}"
dest="${2:?${usage}}"
case "${mode}" in
    --push)
        # shellcheck source=./ci-images.sh
        source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ci-images.sh"
        : "${DT_IMAGE_SOURCE_REV:?DT_IMAGE_SOURCE_REV is required}"
        [[ "${DT_IMAGE_SOURCE_REV}" =~ ^[0-9a-f]{40}$ ]] \
            || { echo "ERROR: DT_IMAGE_SOURCE_REV must be a full git sha" >&2; exit 1; }
        out="${dest}"
        ;;
    --archive)
        mkdir -p "${dest}"
        out="${dest}/image-refs.txt"
        ;;
    *) echo "ERROR: ${usage}" >&2; exit 1 ;;
esac

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
    if [[ "${mode}" == "--push" ]]; then
        log_step "Building + pushing ${repo} -> ${DT_IMAGE_REGISTRY}/${name}"
        docker buildx build "${flags[@]}" \
            --tag "${DT_IMAGE_REGISTRY}/${name}:git-${DT_IMAGE_SOURCE_REV}" \
            --push --metadata-file "${meta}" \
            -f "${PROJECT_ROOT}/infra/docker/${name}/Dockerfile" "${PROJECT_ROOT}"
        # The manifest digest: what a pull by digest verifies.
        key='containerimage.digest'
    else
        log_step "Building ${repo} -> ${dest}/${name}.tar"
        docker buildx build "${flags[@]}" \
            --output "type=docker,dest=${dest}/${name}.tar" --metadata-file "${meta}" \
            -f "${PROJECT_ROOT}/infra/docker/${name}/Dockerfile" "${PROJECT_ROOT}"
        # The config digest: the image ID `docker load` (classic image store) gives it,
        # which deploy.sh requires to be present before it deploys the image.
        key='containerimage.config.digest'
    fi
    digest="$(jq -r --arg k "${key}" '.[$k] // empty' "${meta}")"
    rm -f "${meta}"
    [[ "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]] \
        || { log_error "No ${key} for ${repo} (got '${digest}')"; exit 1; }
    if [[ "${mode}" == "--push" ]]; then
        printf '%s %s@%s\n' "${repo}" "${DT_IMAGE_REGISTRY}/${name}" "${digest}" >> "${out}"
    else
        printf '%s %s\n' "${repo}" "${digest}" >> "${out}"
    fi
    log_info "${repo}: ${digest} in $(( $(date +%s) - start ))s"
done <<< "${repos}"
