#!/usr/bin/env bash
# Guard: numeric anchors the SDK must share with a server, read from BOTH homes
# and required equal. One extraction kernel (`extract`), one row per pair; each
# row carries its own files, declaration shapes, meaning and failure text, so no
# pair inherits another's explanation. (The script keeps its original name; it
# now covers TWO rule families.)
#
# 1. The KEYFRAME-INTERVAL FLOOR and CEILING (story 3 C-A2, R-11, R-23).
#    MC refuses to direct a `min_keyframe_interval_ms` below its floor
#    (`MIN_KEYFRAME_INTERVAL_MS_FLOOR` in crates/mc-service/src/media_signaling/
#    directive.rs, the lower bound of `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`), and the
#    client enforces max(directed, ITS compiled floor) between any two keyframes
#    (`KEYFRAME_MIN_INTERVAL_FLOOR_MS` in packages/sdk-core/src/config/
#    clientConfig.ts), counting a directed value it had to raise on
#    `dt_client_media_keyframe_min_interval_clamped_total`. The CEILING pair
#    (`MIN_KEYFRAME_INTERVAL_MS_CEILING` / `KEYFRAME_MIN_INTERVAL_CEILING_MS`) is
#    pinned the same way: the client refuses a directed value above it.
#
# 2. The MH KEEPALIVE FLOOR (story 4 C1, ADR-0039 §4). The client's first redial
#    of a lost media handler waits at least one MH keepalive interval
#    (`MH_KEEPALIVE_FLOOR_MS` in clientConfig.ts), because a redial before MH
#    idles out the old session meets BINDING_CONFLICT. MH's interval is
#    `MH_KEEPALIVE_INTERVAL_MS` in infra/services/mh-service/config.env, which the
#    browser cannot read. The guard reads the BASE config.env; the deployed value
#    is the rendered root, so it also FAILS CLOSED if any Kubernetes overlay
#    mentions the key (an overlay patch would leave the base green while the
#    cluster drifts) — teach the guard the overlay before adding one.
#
# 3. The CLIENT TOKEN-LENGTH BOUND (story 4 C1, ADR-0039 §5). The SDK refuses
#    a user token or `media_dial` token longer than `MAX_USER_TOKEN_LENGTH`
#    (packages/sdk-core/src/validation/limits.ts; `MAX_MEDIA_DIAL_TOKEN_LENGTH`
#    is derived from it). The relation is client bound >= the server's JWT
#    verify bound `MAX_JWT_SIZE_BYTES` (crates/common/src/jwt.rs, a GSA path
#    this guard only READS), or the client would drop a token the server
#    accepts; equality is the simplest way to hold it, so equality is pinned.
#
# Reads every literal by its EXACT declaration shape (anchored on the whole
# line), requires exactly one match on each side, and fails on a literal not
# found, a duplicate, or inequality. Paths are env-overridable so the self-test
# (scripts/guards/validate-keyframe-floor-sync.test.sh) runs on copies and never
# mutates the tree.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

MC_SRC="${KEYFRAME_FLOOR_SYNC_MC_SRC:-crates/mc-service/src/media_signaling/directive.rs}"
SDK_SRC="${KEYFRAME_FLOOR_SYNC_SDK_SRC:-packages/sdk-core/src/config/clientConfig.ts}"
MH_ENV="${KEYFRAME_FLOOR_SYNC_MH_ENV:-infra/services/mh-service/config.env}"
JWT_SRC="${KEYFRAME_FLOOR_SYNC_JWT_SRC:-crates/common/src/jwt.rs}"
LIMITS_SRC="${KEYFRAME_FLOOR_SYNC_LIMITS_SRC:-packages/sdk-core/src/validation/limits.ts}"
OVERLAYS="${KEYFRAME_FLOOR_SYNC_OVERLAYS:-infra/kubernetes/overlays}"

fail() { echo "sdk-server-anchor-sync (validate-keyframe-floor-sync.sh): $*" >&2; exit 1; }

[ -f "$MC_SRC" ]  || fail "MC source ${MC_SRC} not found"
[ -f "$SDK_SRC" ] || fail "SDK source ${SDK_SRC} not found"
[ -f "$MH_ENV" ]  || fail "MH config ${MH_ENV} not found"
[ -f "$JWT_SRC" ] || fail "JWT source ${JWT_SRC} not found"
[ -f "$LIMITS_SRC" ] || fail "SDK limits ${LIMITS_SRC} not found"
[ -d "$OVERLAYS" ] || fail "overlays root ${OVERLAYS} not found"

# extract <file> <sed-pattern-capturing-the-number> <description> <other-file>
# -> the number, requiring EXACTLY one match and a numeric value (underscores
# stripped). THE one kernel; every row goes through it.
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

# pair <a-file> <a-pattern> <a-what> <b-file> <b-pattern> <b-what> <meaning> <why>:
# the two literals must be equal. Everything a row says comes from the row.
pair() {
  local a_file="$1" a_pat="$2" a_what="$3" b_file="$4" b_pat="$5" b_what="$6"
  local meaning="$7" why="$8" a b
  a="$(extract "$a_file" "$a_pat" "$a_what" "$b_file")"
  b="$(extract "$b_file" "$b_pat" "$b_what" "$a_file")"
  if [ "$a" -ne "$b" ]; then
    fail "${meaning} DISAGREE.
  ${a_file}: ${a_what} -> ${a}
  ${b_file}: ${b_what} -> ${b}
${why}"
  fi
}

rust_u32() { printf 's/^pub const %s: u32 = \\([0-9_]*\\);$/\\1/p' "$1"; }
# `pub const NAME: usize = <n>;` with an optional trailing `// comment`.
rust_usize() { printf 's/^pub const %s: usize = \\([0-9_]*\\);\\( *\\/\\/.*\\)\\{0,1\\}$/\\1/p' "$1"; }
ts_const() { printf 's/^export const %s = \\([0-9_]*\\);$/\\1/p' "$1"; }
# The whole line, digits only: a quoted value, trailing text or a commented-out
# key does not match (MH's u64 parse would reject an underscored value anyway).
env_key() { printf 's/^%s=\\([0-9]*\\)$/\\1/p' "$1"; }

KEYFRAME_WHY="MC directs MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS within [floor, ceiling]; the client
enforces max(directed, its floor), counting a raised value on
dt_client_media_keyframe_min_interval_clamped_total, and refuses a directed value
above its ceiling. Change both literals together."

pair "$MC_SRC" "$(rust_u32 MIN_KEYFRAME_INTERVAL_MS_FLOOR)" \
  "pub const MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = <n>;" \
  "$SDK_SRC" "$(ts_const KEYFRAME_MIN_INTERVAL_FLOOR_MS)" \
  "export const KEYFRAME_MIN_INTERVAL_FLOOR_MS = <n>;" \
  "keyframe-interval floors" "$KEYFRAME_WHY"

pair "$MC_SRC" "$(rust_u32 MIN_KEYFRAME_INTERVAL_MS_CEILING)" \
  "pub const MIN_KEYFRAME_INTERVAL_MS_CEILING: u32 = <n>;" \
  "$SDK_SRC" "$(ts_const KEYFRAME_MIN_INTERVAL_CEILING_MS)" \
  "export const KEYFRAME_MIN_INTERVAL_CEILING_MS = <n>;" \
  "keyframe-interval ceilings" "$KEYFRAME_WHY"

pair "$MH_ENV" "$(env_key MH_KEEPALIVE_INTERVAL_MS)" \
  "MH_KEEPALIVE_INTERVAL_MS=<n>" \
  "$SDK_SRC" "$(ts_const MH_KEEPALIVE_FLOOR_MS)" \
  "export const MH_KEEPALIVE_FLOOR_MS = <n>;" \
  "MH keepalive interval and the client first-redial floor" \
  "The client's first redial of a lost media handler waits one MH keepalive interval
(ADR-0039 §4): earlier, it meets BINDING_CONFLICT on the not-yet-idled session.
Change MH_KEEPALIVE_INTERVAL_MS and MH_KEEPALIVE_FLOOR_MS together."

pair "$JWT_SRC" "$(rust_usize MAX_JWT_SIZE_BYTES)" \
  "pub const MAX_JWT_SIZE_BYTES: usize = <n>;" \
  "$LIMITS_SRC" "$(ts_const MAX_USER_TOKEN_LENGTH)" \
  "export const MAX_USER_TOKEN_LENGTH = <n>;" \
  "server JWT size bound and the client token-length bound" \
  "The client token-length bound must EQUAL the server JWT verify bound. Below it,
the client drops tokens the server accepts. Above it, the client holds and sends
bytes no server accepts. ADR-0039 §5 dial tokens derive their cap from
MAX_USER_TOKEN_LENGTH. Change MAX_JWT_SIZE_BYTES and MAX_USER_TOKEN_LENGTH together."

# Fail closed on an overlay patching the keepalive: the base read above would
# no longer be the deployed value. grep's rc is branched three ways so a read
# error (rc 2) can never pass as "no match" (rc 1).
set +e
overlay_hits="$(grep -rl -- 'MH_KEEPALIVE_INTERVAL_MS' "$OVERLAYS" 2>&1)"
overlay_rc=$?
set -e
case "$overlay_rc" in
  0)
    fail "a Kubernetes overlay under ${OVERLAYS} mentions MH_KEEPALIVE_INTERVAL_MS:
${overlay_hits}
This guard reads only the base ${MH_ENV}. Teach the guard the overlay (compare the
rendered value) before patching the keepalive there."
    ;;
  1) ;;
  *)
    fail "overlay scan failed (grep rc ${overlay_rc}) — cannot prove no overlay under ${OVERLAYS} patches MH_KEEPALIVE_INTERVAL_MS:
${overlay_hits}"
    ;;
esac

exit 0
