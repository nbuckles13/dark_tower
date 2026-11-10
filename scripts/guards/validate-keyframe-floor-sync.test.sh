#!/usr/bin/env bash
# coverage-exempt: checks numeric-anchor drift (keyframe floor/ceiling MC<->SDK, MH keepalive config.env<->SDK, JWT size bound common<->SDK), does not drive the dt-guard binary via $DT_GUARD
# Self-test for validate-keyframe-floor-sync.sh: the keyframe floor/ceiling pairs
# (story 3 C-A2) and the MH keepalive first-redial floor pair (story 4 C1).
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
ENV_REAL="infra/services/mh-service/config.env"
JWT_REAL="crates/common/src/jwt.rs"
LIMITS_REAL="packages/sdk-core/src/validation/limits.ts"
OVERLAYS_REAL="infra/kubernetes/overlays"
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

run_guard() { # run_guard <mc-src> <sdk-src> [<mh-env> [<overlays-root>]]
  set +e
  KEYFRAME_FLOOR_SYNC_MC_SRC="$1" KEYFRAME_FLOOR_SYNC_SDK_SRC="$2" \
    KEYFRAME_FLOOR_SYNC_MH_ENV="${3:-$ENV_REAL}" KEYFRAME_FLOOR_SYNC_OVERLAYS="${4:-$OVERLAYS_REAL}" \
    "$GUARD" >/dev/null 2>&1
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

# ---------------------------------------------------------------------------
# The MH keepalive pair (story 4 C1): config.env MH_KEEPALIVE_INTERVAL_MS <->
# clientConfig.ts MH_KEEPALIVE_FLOOR_MS. Every number derived from the files.
# ---------------------------------------------------------------------------
ENVK="MH_KEEPALIVE_INTERVAL_MS="
SDKK="export const MH_KEEPALIVE_FLOOR_MS ="
env_ka="$(sed -n "s/^${ENVK}\([0-9]*\)$/\1/p" "$ENV_REAL")"
sdk_ka_lit="$(sed -n "s/^${SDKK} \([0-9_]*\);$/\1/p" "$SDK_REAL")"
sdk_ka="$(printf '%s' "$sdk_ka_lit" | tr -d '_')"

# (l) POSITIVE CONTROL: both real declarations extract, and agree.
if [[ "$env_ka" =~ ^[0-9]+$ ]] && [ "$env_ka" = "$sdk_ka" ]; then
  echo "ok   real-keepalive-extracts-equal (${env_ka})"
else
  echo "FAIL real-keepalive-extracts: env='${env_ka}' SDK='${sdk_ka}'" >&2
  rc_total=1
fi
ka_drift=$((env_ka + 1))

# (m) env drift -> fail.
fixture "$ENV_REAL" "$WORK/env-drift.env" "s/^${ENVK}${env_ka}$/${ENVK}${ka_drift}/" "^${ENVK}${ka_drift}$"
check "env-keepalive-drift-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$WORK/env-drift.env")"

# (n) SDK drift -> fail.
fixture "$SDK_REAL" "$WORK/sdk-ka-drift.ts" "s/^${SDKK} ${sdk_ka_lit};$/${SDKK} ${ka_drift};/" "${SDKK} ${ka_drift};"
check "sdk-keepalive-drift-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-ka-drift.ts")"

# (o) Both moved together -> pass.
check "keepalive-both-moved-passes" 0 "$(run_guard "$MC_REAL" "$WORK/sdk-ka-drift.ts" "$WORK/env-drift.env")"

# (p) env key commented out (declaration moved) -> fail loudly, never vacuous.
fixture "$ENV_REAL" "$WORK/env-commented.env" "s/^${ENVK}${env_ka}$/# ${ENVK}${env_ka}/" "^# ${ENVK}${env_ka}$"
check "env-keepalive-commented-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$WORK/env-commented.env")"

# (q) env key renamed -> fail.
fixture "$ENV_REAL" "$WORK/env-renamed.env" "s/^${ENVK}${env_ka}$/MH_KEEPALIVE_MS=${env_ka}/" "^MH_KEEPALIVE_MS=${env_ka}$"
check "env-keepalive-renamed-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$WORK/env-renamed.env")"

# (r) quoted value -> fail (whole-line anchor).
fixture "$ENV_REAL" "$WORK/env-quoted.env" "s/^${ENVK}${env_ka}$/${ENVK}\"${env_ka}\"/" "^${ENVK}\"${env_ka}\"$"
check "env-keepalive-quoted-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$WORK/env-quoted.env")"

# (s) duplicate env key -> fail (two homes).
{ cat "$ENV_REAL"; printf '%s%s\n' "$ENVK" "$env_ka"; } > "$WORK/env-dup.env"
check "env-keepalive-duplicate-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$WORK/env-dup.env")"

# (s2) SDK keepalive literal missing (renamed) -> fail.
fixture "$SDK_REAL" "$WORK/sdk-ka-missing.ts" "s/^${SDKK} ${sdk_ka_lit};$/export const MH_KEEPALIVE_FLOOR = ${sdk_ka_lit};/" "^export const MH_KEEPALIVE_FLOOR = "
check "sdk-keepalive-literal-missing-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-ka-missing.ts")"

# (s3) SDK keepalive literal duplicated -> fail.
{ cat "$SDK_REAL"; printf '%s %s;\n' "$SDKK" "$sdk_ka_lit"; } > "$WORK/sdk-ka-dup.ts"
check "sdk-keepalive-duplicate-fails" 1 "$(run_guard "$MC_REAL" "$WORK/sdk-ka-dup.ts")"

# (t) missing env file -> fail.
check "env-missing-file-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$WORK/nope.env")"

# (u) an overlay that mentions the key -> fail closed; an empty overlay root passes.
mkdir -p "$WORK/overlays-empty" "$WORK/overlays-patched/kind"
printf 'MH_KEEPALIVE_INTERVAL_MS=%s\n' "$env_ka" > "$WORK/overlays-patched/kind/mh-patch.env"
check "overlay-mentions-keepalive-fails" 1 \
  "$(run_guard "$MC_REAL" "$SDK_REAL" "$ENV_REAL" "$WORK/overlays-patched")"
check "overlay-empty-passes" 0 "$(run_guard "$MC_REAL" "$SDK_REAL" "$ENV_REAL" "$WORK/overlays-empty")"

# (v) A missing overlays root -> fail.
check "overlays-root-missing-fails" 1 "$(run_guard "$MC_REAL" "$SDK_REAL" "$ENV_REAL" "$WORK/nope-overlays")"

# (w) An UNREADABLE overlay file -> fail (a grep read error must never pass as
# "no match"). Unexercisable as root (chmod 000 does not deny root reads): say
# so LOUDLY rather than skip silently.
mkdir -p "$WORK/overlays-unreadable"
printf 'MH_KEEPALIVE_INTERVAL_MS=%s\n' "$env_ka" > "$WORK/overlays-unreadable/p.env"
chmod 000 "$WORK/overlays-unreadable/p.env"
if [ -r "$WORK/overlays-unreadable/p.env" ]; then
  echo "NOTE overlay-unreadable-fails: NOT EXERCISED — running as a user that can read a mode-000 file (root?); the rc-2 branch is unverified on this host" >&2
else
  check "overlay-unreadable-fails" 1 \
    "$(run_guard "$MC_REAL" "$SDK_REAL" "$ENV_REAL" "$WORK/overlays-unreadable")"
fi
chmod 600 "$WORK/overlays-unreadable/p.env"

# ---------------------------------------------------------------------------
# The token-length pair (story 4 C1): crates/common/src/jwt.rs
# MAX_JWT_SIZE_BYTES <-> limits.ts MAX_USER_TOKEN_LENGTH.
# ---------------------------------------------------------------------------
run_tok() { # run_tok <jwt-src> <limits-src>
  set +e
  KEYFRAME_FLOOR_SYNC_JWT_SRC="$1" KEYFRAME_FLOOR_SYNC_LIMITS_SRC="$2" "$GUARD" >/dev/null 2>&1
  local rc=$?
  set -e
  printf '%s' "$rc"
}
JWTD="pub const MAX_JWT_SIZE_BYTES: usize ="
LIMD="export const MAX_USER_TOKEN_LENGTH ="
jwt_lit="$(sed -n "s/^${JWTD} \([0-9_]*\);.*$/\1/p" "$JWT_REAL")"
lim_lit="$(sed -n "s/^${LIMD} \([0-9_]*\);$/\1/p" "$LIMITS_REAL")"
if [[ "$jwt_lit" =~ ^[0-9_]+$ ]] && [ "${jwt_lit//_/}" = "${lim_lit//_/}" ]; then
  echo "ok   real-token-bound-extracts-equal (${jwt_lit})"
else
  echo "FAIL real-token-bound-extracts: jwt='${jwt_lit}' limits='${lim_lit}'" >&2
  rc_total=1
fi
tdrift=$(( ${jwt_lit//_/} + 1 ))
fixture "$JWT_REAL" "$WORK/jwt-drift.rs" "s/^${JWTD} ${jwt_lit};/${JWTD} ${tdrift};/" "^${JWTD} ${tdrift};"
check "jwt-bound-drift-fails" 1 "$(run_tok "$WORK/jwt-drift.rs" "$LIMITS_REAL")"
fixture "$LIMITS_REAL" "$WORK/limits-drift.ts" "s/^${LIMD} ${lim_lit};$/${LIMD} ${tdrift};/" "^${LIMD} ${tdrift};"
check "limits-bound-drift-fails" 1 "$(run_tok "$JWT_REAL" "$WORK/limits-drift.ts")"
check "token-bound-both-moved-passes" 0 "$(run_tok "$WORK/jwt-drift.rs" "$WORK/limits-drift.ts")"
fixture "$JWT_REAL" "$WORK/jwt-renamed.rs" "s/^${JWTD} /pub const MAX_JWT_BYTES: usize = /" "^pub const MAX_JWT_BYTES: usize ="
check "jwt-bound-renamed-fails" 1 "$(run_tok "$WORK/jwt-renamed.rs" "$LIMITS_REAL")"
check "jwt-missing-file-fails" 1 "$(run_tok "$WORK/nope-jwt.rs" "$LIMITS_REAL")"

exit "$rc_total"
