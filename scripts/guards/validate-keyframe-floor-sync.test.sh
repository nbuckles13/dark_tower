#!/usr/bin/env bash
# coverage-exempt: checks keyframe-floor drift between two source files, does not drive the dt-guard binary via $DT_GUARD
# Self-test for validate-keyframe-floor-sync.sh (story 3 C-A2).
#
# A sync guard without a DRIFT CASE cannot fail, and is indistinguishable from
# no guard at all. The drift fixtures are COPIES OF THE REAL FILES with one
# literal edited (never hand-written one-liners), so a future reformat of either
# declaration is exercised against the real file shape.
#
# Deliberately NOT under guards/simple/: run-guards.sh discovers with
# `find -name '*.sh'`, so a self-test there would be auto-run AS A GUARD. Same
# reasoning as validate-slug-class-sync.test.sh. Invoked from scripts/layer3.sh.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

GUARD="scripts/guards/simple/validate-keyframe-floor-sync.sh"
MC_REAL="crates/mc-service/src/media_signaling/directive.rs"
SDK_REAL="packages/sdk-core/src/config/clientConfig.ts"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
rc_total=0

check() { # check <name> <expected-rc> <actual-rc>
  if [ "$2" -ne "$3" ]; then
    echo "FAIL ${1}: expected rc ${2}, got ${3}" >&2
    rc_total=1
  else
    echo "ok   ${1}"
  fi
}

run_guard() { # run_guard <mc-src> <sdk-src>
  set +e
  KEYFRAME_FLOOR_SYNC_MC_SRC="$1" KEYFRAME_FLOOR_SYNC_SDK_SRC="$2" "$GUARD" >/dev/null 2>&1
  local rc=$?
  set -e
  printf '%s' "$rc"
}

# The REAL values, derived (this self-test encodes no number of its own).
num() { sed -n "$2" "$1" | tr -d '_'; }
mc_floor="$(num "$MC_REAL" 's/^pub const MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = \([0-9_]*\);$/\1/p')"
sdk_floor="$(num "$SDK_REAL" 's/^export const KEYFRAME_MIN_INTERVAL_FLOOR_MS = \([0-9_]*\);$/\1/p')"
mc_ceil="$(num "$MC_REAL" 's/^pub const MIN_KEYFRAME_INTERVAL_MS_CEILING: u32 = \([0-9_]*\);$/\1/p')"
sdk_ceil="$(num "$SDK_REAL" 's/^export const KEYFRAME_MIN_INTERVAL_CEILING_MS = \([0-9_]*\);$/\1/p')"

# POSITIVE CONTROL: each real declaration extracts to a number, and the pairs
# agree (not merely "the guard exited 0").
if [[ "$mc_floor" =~ ^[0-9]+$ && "$mc_ceil" =~ ^[0-9]+$ ]] \
  && [ "$mc_floor" = "$sdk_floor" ] && [ "$mc_ceil" = "$sdk_ceil" ]; then
  echo "ok   real-tree-extracts-equal-numbers (floor ${mc_floor}, ceiling ${mc_ceil})"
else
  echo "FAIL real-tree-extracts: floor MC='${mc_floor}' SDK='${sdk_floor}', ceiling MC='${mc_ceil}' SDK='${sdk_ceil}'" >&2
  rc_total=1
fi

# fixture <src> <out> <sed-expr> <grep-applied>: an EDITED COPY of a real file,
# failing the self-test if the edit did not apply (no vacuous fixture).
fixture() {
  sed "$3" "$1" > "$2"
  grep -q -- "$4" "$2" || { echo "FAIL fixture $(basename "$2"): edit did not apply" >&2; rc_total=1; }
}

MCF="pub const MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 ="
SDF="export const KEYFRAME_MIN_INTERVAL_FLOOR_MS ="
MCC="pub const MIN_KEYFRAME_INTERVAL_MS_CEILING: u32 ="
SDC="export const KEYFRAME_MIN_INTERVAL_CEILING_MS ="
mc_floor_lit="$(sed -n "s/^${MCF} \([0-9_]*\);$/\1/p" "$MC_REAL")"
sdk_floor_lit="$(sed -n "s/^${SDF} \([0-9_]*\);$/\1/p" "$SDK_REAL")"
sdk_ceil_lit="$(sed -n "s/^${SDC} \([0-9_]*\);$/\1/p" "$SDK_REAL")"
drift=$((mc_floor + 50))

# (a) The REAL tree is in sync.
set +e
"$GUARD" >/dev/null 2>&1
real_rc=$?
set -e
check "real-tree-in-sync" 0 "$real_rc"

# (b) MC floor drift -> fail.
fixture "$MC_REAL" "$WORK/mc-drift.rs" "s/^${MCF} ${mc_floor_lit};$/${MCF} ${drift};/" "${MCF} ${drift};"
check "mc-floor-drift-fails" 1 "$(run_guard "$WORK/mc-drift.rs" "$SDK_REAL")"

# (c) SDK floor drift -> fail.
fixture "$SDK_REAL" "$WORK/sdk-drift.ts" "s/^${SDF} ${sdk_floor_lit};$/${SDF} ${drift};/" "${SDF} ${drift};"
check "sdk-floor-drift-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-drift.ts")"

# (d) Both floors moved TOGETHER -> pass (the guard compares; it pins no value).
check "both-moved-together-passes" 0 "$(run_guard "$WORK/mc-drift.rs" "$WORK/sdk-drift.ts")"

# (e) Underscored literal on one side is the same number -> pass.
under="$(printf '%s' "$mc_floor" | sed 's/^\(.\)/\1_/')"
fixture "$MC_REAL" "$WORK/mc-underscore.rs" "s/^${MCF} ${mc_floor_lit};$/${MCF} ${under};/" "${MCF} ${under};"
check "underscored-literal-passes" 0 "$(run_guard "$WORK/mc-underscore.rs" "$SDK_REAL")"

# (f) MC floor literal NOT FOUND (declaration reshaped) -> fail loudly.
fixture "$MC_REAL" "$WORK/mc-missing.rs" "s/^${MCF} ${mc_floor_lit};$/${MCF}\n    ${mc_floor_lit};/" "^    ${mc_floor_lit};"
check "mc-literal-missing-fails" 1 "$(run_guard "$WORK/mc-missing.rs" "$SDK_REAL")"

# (g) SDK floor literal NOT FOUND (renamed) -> fail.
fixture "$SDK_REAL" "$WORK/sdk-missing.ts" "s/^${SDF} ${sdk_floor_lit};$/export const KEYFRAME_FLOOR = ${sdk_floor_lit};/" "export const KEYFRAME_FLOOR ="
check "sdk-literal-missing-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-missing.ts")"

# (h) A DUPLICATE declaration is two homes -> fail.
{ cat "$SDK_REAL"; printf '%s %s;\n' "$SDF" "$sdk_floor_lit"; } > "$WORK/sdk-dup.ts"
check "sdk-duplicate-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-dup.ts")"

# (i) CEILING drift on the SDK side -> fail.
cdrift=$((sdk_ceil + 1))
fixture "$SDK_REAL" "$WORK/sdk-ceil.ts" "s/^${SDC} ${sdk_ceil_lit};$/${SDC} ${cdrift};/" "${SDC} ${cdrift};"
check "sdk-ceiling-drift-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-ceil.ts")"

# (j) CEILING missing on the MC side -> fail.
fixture "$MC_REAL" "$WORK/mc-ceil-missing.rs" "s/^${MCC} /pub const RENAMED_CEILING: u32 = /" "RENAMED_CEILING"
check "mc-ceiling-missing-fails" 1 "$(run_guard "$WORK/mc-ceil-missing.rs" "$SDK_REAL")"

# (k) A missing file -> fail.
check "missing-file-fails" 1 "$(run_guard "$WORK/nope.rs" "$SDK_REAL")"

exit "$rc_total"
