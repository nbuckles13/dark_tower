#!/usr/bin/env bash
# Guard: every service image builds from ONE shared builder.
#
# WHY: the service Dockerfiles (infra/docker/*-service/Dockerfile) carry a
# byte-identical chef/planner/builder block so BuildKit computes the same cache
# keys and the workspace compiles once for all of them (CI image caching). If one
# copy drifts, nothing fails: that image silently gets its own layer-cache scope
# and recompiles the whole workspace, and CI time quietly goes back up. This
# guard makes the drift a gate failure.
#
# "Identical" is judged by the SAME derivation the build uses to pick the cache
# scope (deploy.sh image_cache_scope: the instructions before the target stage,
# comments and blank lines dropped), so the guard and the cache cannot disagree.
# The set checked is every Dockerfile that runs `cargo chef cook` — the images
# that share the builder — not a hand-kept list.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

fail() { echo "shared-image-builder: $*" >&2; exit 1; }

# deploy.sh parses "$@" when sourced and runs main only when executed.
set --
# shellcheck source=../../../infra/kind/scripts/deploy.sh
source "${root}/infra/kind/scripts/deploy.sh"

mapfile -t dockerfiles < <(grep -l 'cargo chef cook' "${root}"/infra/docker/*/Dockerfile | sort)
# Positive control: a moved directory or reworded cook step would otherwise
# leave nothing (or one file) to compare and pass vacuously.
[ "${#dockerfiles[@]}" -ge 2 ] \
    || fail "expected at least 2 Dockerfiles running 'cargo chef cook' under infra/docker/, found ${#dockerfiles[@]}"

first="" first_scope=""
for df in "${dockerfiles[@]}"; do
    name="$(basename "$(dirname "${df}")")"
    scope="$(image_cache_scope "localhost/${name}")" || fail "cannot derive the cache scope of ${name}"
    if [ -z "${first}" ]; then
        first="${name}" first_scope="${scope}"
    elif [ "${scope}" != "${first_scope}" ]; then
        echo "VIOLATION: ${name} builder differs from ${first} (${scope} != ${first_scope})"
        diff <(image_builder_instructions "localhost/${first}") \
             <(image_builder_instructions "localhost/${name}") || true
        fail "the builder stages must be identical across: ${dockerfiles[*]#"${root}/"}"
    fi
done
echo "shared-image-builder: OK (${#dockerfiles[@]} images share ${first_scope})"
