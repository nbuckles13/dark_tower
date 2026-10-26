#!/usr/bin/env bash
#
# codegen.sh — THE producer of the TS proto bindings (Nx target
# `proto-gen:codegen`). It cleans, then runs `buf generate`.
#
# WHY THE CLEAN LIVES HERE AND NOT IN THE ORACLE (2026-10-25). Until then
# verify-codegen.sh deleted and regenerated these files itself. That was safe
# only while nothing ran it. Wired into `nx run-many -t test:unit` it would
# delete the gitignored bindings while sdk-core, sdk-svelte and web-app unit
# tests, which depend on `codegen` and not on the oracle, were importing them in
# parallel. Cleaning in the ONE producer keeps the guarantee the clean exists
# for: stale outputs from a prior config cannot mask a broken current config
# (@test Gate 3, 2026-05-06). It now holds for EVERY consumer, not just the
# oracle, and the oracle only reads.
set -euo pipefail

# shellcheck source=./codegen-paths.sh
source "$(dirname "${BASH_SOURCE[0]}")/codegen-paths.sh"

mkdir -p "${CODEGEN_OUT_DIR}"
find "${CODEGEN_OUT_DIR}" -type f \( -name '*_pb.ts' -o -name '*_pb.js' -o -name '*_pb.d.ts' \) -delete

cd "${CODEGEN_REPO_ROOT}/proto"
pnpm exec buf generate
