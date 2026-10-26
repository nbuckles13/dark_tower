# shellcheck shell=bash
# The ONE spelling, for this package's scripts, of where `buf generate` writes
# the TS bindings. It mirrors `out:` in proto/buf.gen.yaml (relative to proto/)
# and the `codegen` target's `outputs` in ../project.json. Sourced by codegen.sh
# (the producer) and verify-codegen.sh (the oracle) so that those two cannot
# disagree. Definitions only.
CODEGEN_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CODEGEN_OUT_DIR="${CODEGEN_REPO_ROOT}/packages/sdk-core/src/proto"
