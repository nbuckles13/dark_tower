#!/usr/bin/env bash
# Guard: the KEYFRAME-INTERVAL FLOOR and CEILING are the same numbers on both
# ends (story 3 C-A2, R-11, R-23).
#
# MC refuses to direct a `min_keyframe_interval_ms` below its floor
# (`MIN_KEYFRAME_INTERVAL_MS_FLOOR` in crates/mc-service/src/media_signaling/
# directive.rs, the lower bound of `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`), and the
# client enforces max(directed, ITS compiled floor) between any two keyframes
# (`KEYFRAME_MIN_INTERVAL_FLOOR_MS` in packages/sdk-core/src/config/
# clientConfig.ts), counting a directed value it had to raise on
# `dt_client_media_keyframe_min_interval_clamped_total`. The two literals encode
# ONE rule in two languages with no shared home, so this guard is the drift
# signal: if MC's floor moved and the SDK's did not, the clamp counter would
# start firing on a correct controller (SDK higher) or the SDK would accept a
# rate MC promises never to direct (SDK lower).
#
# The CEILING pair (`MIN_KEYFRAME_INTERVAL_MS_CEILING` / `KEYFRAME_MIN_INTERVAL_CEILING_MS`)
# is pinned the same way: the client refuses a directed value above it.
#
# Reads every literal by its EXACT declaration shape, requires exactly one
# match on each side, and fails on a literal not found, a duplicate, or
# inequality. Paths are env-overridable so the self-test
# (scripts/guards/validate-keyframe-floor-sync.test.sh) runs on copies and never
# mutates the tree.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

MC_SRC="${KEYFRAME_FLOOR_SYNC_MC_SRC:-crates/mc-service/src/media_signaling/directive.rs}"
SDK_SRC="${KEYFRAME_FLOOR_SYNC_SDK_SRC:-packages/sdk-core/src/config/clientConfig.ts}"

fail() { echo "keyframe-floor-sync: $*" >&2; exit 1; }

[ -f "$MC_SRC" ]  || fail "MC source ${MC_SRC} not found"
[ -f "$SDK_SRC" ] || fail "SDK source ${SDK_SRC} not found"

# extract <file> <sed-pattern-capturing-the-number> <description> -> the number,
# requiring EXACTLY one match and a numeric value (underscores stripped).
extract() {
  local file="$1" pattern="$2" what="$3" other="$4" raw count
  raw="$(sed -n "$pattern" "$file")"
  count="$(printf '%s' "$raw" | grep -c . || true)"
  [ "$count" -eq 1 ] \
    || fail "expected exactly 1 '${what}' in ${file}, found ${count} — the declaration moved or changed shape, so this guard is no longer reading it (compared against ${other}). Fix the extraction, don't delete the guard."
  raw="${raw//_/}"
  [[ "$raw" =~ ^[0-9]+$ ]] || fail "'${what}' in ${file} is not a number: '${raw}'"
  printf '%s' "$((10#$raw))"
}

# pair <mc-const> <sdk-const> <meaning>: the two literals must be equal.
pair() {
  local mc_name="$1" sdk_name="$2" meaning="$3" mc sdk
  mc="$(extract "$MC_SRC" "s/^pub const ${mc_name}: u32 = \([0-9_]*\);\$/\1/p" \
    "pub const ${mc_name}: u32 = <n>;" "$SDK_SRC")"
  sdk="$(extract "$SDK_SRC" "s/^export const ${sdk_name} = \([0-9_]*\);\$/\1/p" \
    "export const ${sdk_name} = <n>;" "$MC_SRC")"
  if [ "$mc" -ne "$sdk" ]; then
    fail "keyframe-interval ${meaning}s DISAGREE.
  MC  (${MC_SRC}) ${mc_name} = ${mc}
  SDK (${SDK_SRC}) ${sdk_name} = ${sdk}
MC directs MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS within [floor, ceiling]; the client
enforces max(directed, its floor), counting a raised value on
dt_client_media_keyframe_min_interval_clamped_total, and refuses a directed value
above its ceiling. Change both literals together."
  fi
}

pair MIN_KEYFRAME_INTERVAL_MS_FLOOR KEYFRAME_MIN_INTERVAL_FLOOR_MS floor
pair MIN_KEYFRAME_INTERVAL_MS_CEILING KEYFRAME_MIN_INTERVAL_CEILING_MS ceiling

exit 0
