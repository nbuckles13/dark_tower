# shellcheck shell=bash
# The CI image namespace: ONE definition for the scripts that push to it
# (build-push-images.sh) and prune it (prune-ci-images.sh). Separate from any
# release namespace; consumers pull by digest, never by tag.
#
# DT_IMAGE_REGISTRY, when set, wins (a fork or a test points elsewhere);
# otherwise it is ghcr.io/<repository owner>/dark-tower-ci.
if [[ -z "${DT_IMAGE_REGISTRY:-}" ]]; then
    : "${GITHUB_REPOSITORY_OWNER:?set DT_IMAGE_REGISTRY, or run under GitHub Actions (GITHUB_REPOSITORY_OWNER)}"
    DT_IMAGE_REGISTRY="ghcr.io/${GITHUB_REPOSITORY_OWNER,,}/dark-tower-ci"
fi
