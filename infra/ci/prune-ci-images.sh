#!/usr/bin/env bash
# Delete CI image versions older than a retention window from the CI package
# namespace (infra/ci/ci-images.sh).
#
# Usage: infra/ci/prune-ci-images.sh
#   DT_IMAGE_REGISTRY      ghcr.io/<owner>/<namespace> (default: infra/ci/ci-images.sh)
#   DT_CI_IMAGE_RETENTION_DAYS  versions created before now minus this are deleted (required)
#   DT_PRUNE_DELETE        1 = delete; anything else lists what would be deleted
#                          and deletes nothing (the safe default)
#   GH_TOKEN               a token with packages:write (delete) on those packages
#
# The package list is the build's own (deploy.sh first_party_repos), so a new
# image is pruned without anyone remembering to add it.
#
# The CI namespace holds only `git-<40 hex>` tags (build-images.sh --push) and the
# untagged child manifests each push creates (platform image, attestation).
# Age is the whole policy: a child is as old as its parent, so an expired
# untagged version belongs to an expired parent. That reasoning only holds while
# nothing else is tagged here, so any other tag fails the run loudly instead of
# being pruned around (its children could be deleted out from under it).
set -euo pipefail

# shellcheck source=./ci-images.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ci-images.sh"
: "${DT_CI_IMAGE_RETENTION_DAYS:?DT_CI_IMAGE_RETENTION_DAYS is required}"
[[ "${DT_CI_IMAGE_RETENTION_DAYS}" =~ ^[1-9][0-9]*$ ]] \
    || { echo "ERROR: DT_CI_IMAGE_RETENTION_DAYS must be a positive integer" >&2; exit 1; }
[[ "${DT_IMAGE_REGISTRY}" =~ ^ghcr\.io/([^/]+)/([^/]+)$ ]] \
    || { echo "ERROR: DT_IMAGE_REGISTRY must be ghcr.io/<owner>/<namespace>" >&2; exit 1; }
owner="${BASH_REMATCH[1]}" namespace="${BASH_REMATCH[2]}"

set --
# shellcheck source=../kind/scripts/deploy.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../kind/scripts/deploy.sh"

case "$(gh api "users/${owner}" --jq .type)" in
    User) base="users/${owner}/packages/container" ;;
    Organization) base="orgs/${owner}/packages/container" ;;
    *) log_error "Cannot tell whether ${owner} is a user or an organization."; exit 1 ;;
esac

cutoff="$(date -u -d "-${DT_CI_IMAGE_RETENTION_DAYS} days" +%Y-%m-%dT%H:%M:%SZ)"
dry=" (dry run)"; [[ "${DT_PRUNE_DELETE:-}" == "1" ]] && dry=""
log_info "Pruning ${DT_IMAGE_REGISTRY} versions created before ${cutoff}${dry}"

cache_renders
repos="$(first_party_repos)"
deleted=0 kept=0
while IFS= read -r repo; do
    pkg="${namespace}/${repo#localhost/}"
    enc="${pkg//\//%2F}"
    versions="$(gh api --paginate "${base}/${enc}/versions?per_page=100" \
        --jq '.[] | [.id, .created_at, ((.metadata.container.tags // []) | join(","))] | @tsv')" \
        || { log_error "Cannot list versions of ${pkg}."; exit 1; }
    while IFS=$'\t' read -r id created tags; do
        [[ -n "${id}" ]] || continue
        if [[ -n "${tags}" ]] && grep -qvE '^git-[0-9a-f]{40}$' <<< "${tags//,/$'\n'}"; then
            log_error "${pkg} version ${id} carries tags '${tags}'; the CI namespace holds only git-<sha> tags. Refusing to prune ${pkg}."
            exit 1
        fi
        if [[ "${created}" < "${cutoff}" ]]; then
            log_info "delete ${pkg} ${id} (${created}) ${tags:-<untagged>}"
            [[ -n "${dry}" ]] || gh api --method DELETE "${base}/${enc}/versions/${id}" >/dev/null
            deleted=$((deleted + 1))
        else
            kept=$((kept + 1))
        fi
    done <<< "${versions}"
done <<< "${repos}"
log_info "${deleted} version(s) ${dry:+would be }deleted, ${kept} kept.${dry}"
