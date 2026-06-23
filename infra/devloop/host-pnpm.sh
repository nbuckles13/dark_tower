#!/usr/bin/env bash
# host-pnpm.sh — run pnpm via the devloop image from the host.
#
# Why this exists: the host doesn't have pnpm installed (all TS dev work
# happens in the devloop container). Occasionally the host needs to run pnpm
# directly — most commonly to regenerate pnpm-lock.yaml after a merge conflict
# during absorb. This wraps the podman invocation with two key choices:
#
#   1. --entrypoint pnpm — bypasses infra/devloop/entrypoint.sh, which hard-
#      fails on missing DATABASE_URL (correct for the dev container session;
#      wrong for one-off tool runs).
#   2. Shared pnpm-store + cargo caches via the same named volumes the dev
#      container uses, so dep fetches hit cache and are fast.
#
# Usage (anything you'd type after `pnpm`):
#   ./infra/devloop/host-pnpm.sh --version
#   ./infra/devloop/host-pnpm.sh install --lockfile-only         # regen lockfile, no node_modules
#   ./infra/devloop/host-pnpm.sh audit --audit-level=high
#
# The common post-absorb-conflict workflow:
#   git checkout --theirs pnpm-lock.yaml
#   git add pnpm-lock.yaml
#   git -c core.hooksPath=/dev/null cherry-pick --continue
#   ./infra/devloop/host-pnpm.sh install --lockfile-only
#   git add pnpm-lock.yaml
#   git commit --amend --no-edit --no-verify

set -euo pipefail

IMAGE="${DEVLOOP_IMAGE:-darktower-dev:latest}"
REPO_ROOT="$(git rev-parse --show-toplevel)"

# Sanity check the image exists. Loud failure beats a confusing podman error.
if ! podman image exists "$IMAGE" && ! podman image exists "localhost/$IMAGE"; then
    echo "ERROR: image '$IMAGE' not found locally." >&2
    echo "  Build it via: ./infra/devloop/build-image.sh   (or rerun devloop.sh once)" >&2
    exit 1
fi

exec podman run --rm --entrypoint pnpm \
    --userns=keep-id \
    -v "$REPO_ROOT:/work:Z" -w /work \
    -v pnpm-store:/tmp/pnpm-store \
    -e npm_config_store_dir=/tmp/pnpm-store \
    "$IMAGE" \
    "$@"
