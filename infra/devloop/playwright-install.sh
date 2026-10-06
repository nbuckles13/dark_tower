#!/usr/bin/env bash
# playwright-install.sh — bake Playwright's Chromium into the devloop image, BOUNDED.
#
# Run by infra/devloop/Dockerfile (COPYed in; build context = infra/devloop/) as
#   playwright-install.sh "${PLAYWRIGHT_VERSION}"
# where PLAYWRIGHT_VERSION is derived from pnpm-lock.yaml by devloop.sh.
#
# Why bounded: Playwright's downloader has no timeout. On 2026-10-06 this step hung ~40 min
# AFTER the Chromium download, its CDN connections open but idle. Each attempt therefore
# runs under `timeout --kill-after` (timeout signals the whole process group, so the
# downloader child dies too; --kill-after SIGKILLs anything that ignores SIGTERM), and the
# step is retried a small fixed number of times, then fails loudly naming itself.
#
# Knobs (env, defaults below; devloop.sh does not pass them — edit the defaults to change
# them). Worst case ≈ ATTEMPTS × (TIMEOUT + KILL_AFTER) + (ATTEMPTS − 1) × RETRY_SLEEP
# = 3 × (600 + 30) + 2 × 10 ≈ 32 min, versus an unbounded hang.
#
# The "running npx playwright install without first installing your project's
# dependencies" warning is EXPECTED: the image has no repo at build time, so npx fetches
# the pinned package on its own.
#
# Tested hermetically (PATH-stub npx, tiny timeouts) in scripts/setup.test.sh group (F).

set -euo pipefail

PLAYWRIGHT_INSTALL_TIMEOUT_SECS="${PLAYWRIGHT_INSTALL_TIMEOUT_SECS:-600}"
PLAYWRIGHT_INSTALL_ATTEMPTS="${PLAYWRIGHT_INSTALL_ATTEMPTS:-3}"
PLAYWRIGHT_INSTALL_KILL_AFTER_SECS="${PLAYWRIGHT_INSTALL_KILL_AFTER_SECS:-30}"
PLAYWRIGHT_INSTALL_RETRY_SLEEP_SECS="${PLAYWRIGHT_INSTALL_RETRY_SLEEP_SECS:-10}"

# ANCHOR (DRY): byte-identical copy of infra/lib/pnpm-lock-version.sh __PNPM_LOCK_VERSION_RE (the
# build context, infra/devloop/, cannot source infra/lib); scripts/setup.test.sh group (F) fails on drift.
__PLAYWRIGHT_VERSION_RE='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'
version="${1:-}"
if [[ ! "$version" =~ ${__PLAYWRIGHT_VERSION_RE} ]]; then
    echo "ERROR: playwright-install: version '${version}' is not X.Y.Z[-pre] (expected the PLAYWRIGHT_VERSION build arg, derived from pnpm-lock.yaml by devloop.sh)" >&2
    exit 1
fi
: "${PLAYWRIGHT_BROWSERS_PATH:?PLAYWRIGHT_BROWSERS_PATH must be set (the Dockerfile ENV)}"

step="npx playwright@${version} install chromium"
rc=0
for (( attempt = 1; attempt <= PLAYWRIGHT_INSTALL_ATTEMPTS; attempt++ )); do
    echo "playwright-install: attempt ${attempt}/${PLAYWRIGHT_INSTALL_ATTEMPTS}: ${step} (timeout ${PLAYWRIGHT_INSTALL_TIMEOUT_SECS}s)" >&2
    rc=0
    timeout --kill-after="${PLAYWRIGHT_INSTALL_KILL_AFTER_SECS}" "${PLAYWRIGHT_INSTALL_TIMEOUT_SECS}" \
        npx -y "playwright@${version}" install chromium || rc=$?
    if [[ "$rc" -eq 0 ]]; then
        chmod -R a+rX "${PLAYWRIGHT_BROWSERS_PATH}"
        exit 0
    fi
    echo "playwright-install: attempt ${attempt}/${PLAYWRIGHT_INSTALL_ATTEMPTS} rc=${rc} (124=timeout, 137=killed after the grace period)" >&2
    # A killed attempt can leave a half-extracted browser dir (which a later run may take as
    # installed) and Playwright's __dirlock; start every retry from an empty tree.
    rm -rf "${PLAYWRIGHT_BROWSERS_PATH:?}"
    if (( attempt < PLAYWRIGHT_INSTALL_ATTEMPTS )); then
        sleep "${PLAYWRIGHT_INSTALL_RETRY_SLEEP_SECS}"
    fi
done
echo "ERROR: devloop image step '${step}' failed after ${PLAYWRIGHT_INSTALL_ATTEMPTS} attempt(s) (timeout ${PLAYWRIGHT_INSTALL_TIMEOUT_SECS}s each, last rc=${rc})" >&2
exit 1
