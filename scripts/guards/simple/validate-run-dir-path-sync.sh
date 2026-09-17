#!/usr/bin/env bash
# Guard: the story-runner RUN-DIR path encodings shared across the host↔container
# boundary agree, character for character — STATIC extract-and-compare of source text
# only, NO runtime resolution (a guard whose verdict depends on the environment it
# runs in has the same defect it exists to catch). Precedents:
# validate-slug-class-sync.sh, validate-subdomain-regex-sync.sh, validate-gsa-sync.sh.
#
# NAMED for the LEDGER-BASE coupling (its larger, newer half — grep run-dir/ledger,
# not just marker): it binds the ADR-0037 D5 ledger-base path AND the pre-existing
# in-flight-marker path. Both are RUN-DIR path agreements between devloop.sh (host)
# and run-story.sh (in-container).
#
# WHY (ADR-0037 D5 two-key decomposition): `infra/devloop/devloop.sh` (host) and
# `scripts/workflow/run-story.sh` (in-container) each encode paths the OTHER relies
# on across the container boundary. They agreed before only because the image user
# happens to be `dev`; nothing mechanical bound them, and this loop nearly shipped a
# marker-path move that would have silently broken skip-CLI-update-during-a-run (no
# error — the CLI updates under a live run and the story later dies on
# SUBSTRATE-CHANGED, the WRONG cause). This guard makes that drift a gate failure.
#
# SCOPE — three statements, deliberately separated (read before "unifying" anything):
#   (1) The LEDGER-base pair AGREES: devloop.sh's CONTAINER_LEDGER_BASE (mount target
#       + the DEVLOOP_STORY_RUN_BASE carrier value) == run-story.sh's H2 seam literal;
#       and devloop.sh's STORY_RUNS_HOST == run-story's resolve_run_base run-dir HOME
#       default. This is the D5 binding: if they diverge, RUN_DIR silently becomes
#       container-local and the ledger dies with the container while the run reports
#       success — the feature not existing, with no error.
#   (2) The MARKER pair AGREES: devloop.sh's INFLIGHT_MARKER and run-story's INFLIGHT
#       both compose `story-runner/.run-in-flight`, and the marker base is /tmp/devloop
#       on both sides (devloop.sh mounts HELPER_RUNTIME_DIR there; resolve_run_base
#       marker defaults to it). Drift here silently disables the CLI-update skip.
#   (3) The MARKER base and the LEDGER base are DELIBERATELY NOT in agreement with
#       each other — that is the whole two-key decomposition (marker per-CONTAINER on
#       /tmp/devloop; ledger per-STORY on ~/.cache/devloop/story-runs). A reader who
#       sees (1) and (2) must NOT "fix" the fact that the marker and ledger bases
#       differ — unifying them re-collapses the two keys and reintroduces the
#       ledger-dies-with-the-container failure D5 exists to fix.
#
# BLIND SPOTS, named not assumed-total:
#   - STORY_RUNNER_ALLOW_HOST=1 (preflight-story.sh) runs OUTSIDE the container against
#     the host $HOME, so CONTAINER_LEDGER_BASE (/home/dev/...) is not the production
#     base on that lane; the H2 seam clause OR's in a ${HOME}-derived literal for it,
#     but this static guard only pins the container-side literals.
#   - A carrier deliberately redirected at runtime escapes any static literal — a test
#     process cannot know another (live) process's redirected path. Fundamental.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

DEVLOOP="${RUN_DIR_PATH_SYNC_DEVLOOP:-infra/devloop/devloop.sh}"
RUNNER="${RUN_DIR_PATH_SYNC_RUNNER:-scripts/workflow/run-story.sh}"

fail() { echo "run-dir-path-sync: $*" >&2; exit 1; }

# [ -f ] on both sources is this guard's OWN positive control: a renamed/moved file
# would otherwise make the extraction find nothing and pass vacuously — green through
# exactly the drift it exists to catch.
[ -f "$DEVLOOP" ] || fail "devloop.sh not found at ${DEVLOOP} — the guard cannot bind the pair. Fix the path rather than deleting the guard."
[ -f "$RUNNER" ]  || fail "run-story.sh not found at ${RUNNER} — the guard cannot bind the pair. Fix the path rather than deleting the guard."

extract_one() {
  local label="$1" file="$2" sed_expr="$3" val n
  val="$(sed -n "$sed_expr" "$file")"
  [ -n "$val" ] || fail "could not extract ${label} from ${file} — the declaration moved or changed shape, so the guard is no longer reading it. Fix the extraction rather than deleting the check."
  n="$(printf '%s\n' "$val" | grep -c . || true)"
  [ "$n" -eq 1 ] || fail "expected exactly 1 ${label} in ${file}, found ${n} — two declarations are two homes, which is what this guard prevents."
  printf '%s' "$val"
}

# --- (1) LEDGER-base pair --------------------------------------------------------
dl_container="$(extract_one 'CONTAINER_LEDGER_BASE' "$DEVLOOP" 's/^CONTAINER_LEDGER_BASE="\(.*\)"$/\1/p')"
dl_host="$(extract_one 'STORY_RUNS_HOST' "$DEVLOOP" 's/^STORY_RUNS_HOST="\(.*\)"$/\1/p')"
rs_container="$(extract_one '__ledger_base_container (H2 seam literal)' "$RUNNER" 's/^  __ledger_base_container="\(.*\)"$/\1/p')"
# run-story's resolve_run_base run-dir HOME default tail: the ${HOME}/... path inside
# the ${DEVLOOP_STORY_RUN_BASE:-...} default. Pinned by the leading marker.
rs_host="$(extract_one 'resolve_run_base run-dir HOME default' "$RUNNER" 's|.*DEVLOOP_STORY_RUN_BASE:-\(${HOME}/[^}]*\)}}/story-runner.*|\1|p')"

[ "$dl_container" = "$rs_container" ] || fail "LEDGER-base container literals DISAGREE.
  devloop.sh CONTAINER_LEDGER_BASE : ${dl_container}
  run-story.sh H2 seam literal     : ${rs_container}
These are the mount target and the seam's production-base floor; if they diverge the
H2 guard defends a path nothing uses and RUN_DIR can silently become container-local."

[ "$dl_host" = "$rs_host" ] || fail "LEDGER-base host defaults DISAGREE.
  devloop.sh STORY_RUNS_HOST            : ${dl_host}
  run-story.sh resolve_run_base default : ${rs_host}
devloop.sh mounts STORY_RUNS_HOST; run-story defaults RUN_DIR there when the carrier
is absent. Divergence means the mount and the default point at different dirs and the
ledger dies with the container on the host-hatch/no-carrier path."

# The container literal must be the host default with $HOME resolved to the image
# user's home — the one place the two sides' agreement rested on "the user is dev".
[ "$dl_container" = "/home/dev/${dl_host#\$\{HOME\}/}" ] || fail "LEDGER-base container vs host MISMATCH.
  CONTAINER_LEDGER_BASE : ${dl_container}
  expected              : /home/dev/${dl_host#\$\{HOME\}/}  (from STORY_RUNS_HOST with \$HOME=/home/dev)
The container mount target must equal the host default with the image user's \$HOME."

# --- (2) MARKER pair -------------------------------------------------------------
grep -q 'INFLIGHT_MARKER="\${HELPER_RUNTIME_DIR}/story-runner/\.run-in-flight"' "$DEVLOOP" \
  || fail "devloop.sh no longer composes INFLIGHT_MARKER as \${HELPER_RUNTIME_DIR}/story-runner/.run-in-flight — the marker pair may have drifted; run-story reads this exact path to skip the mid-run CLI update."
grep -q 'INFLIGHT="\$RUN_BASE/\.run-in-flight"' "$RUNNER" \
  || fail "run-story.sh no longer composes INFLIGHT as \$RUN_BASE/.run-in-flight — the marker pair may have drifted from devloop.sh's INFLIGHT_MARKER."
# Marker base is /tmp/devloop on both sides: devloop.sh mounts HELPER_RUNTIME_DIR
# there, run-story's resolve_run_base marker defaults to it.
grep -q 'HELPER_RUNTIME_DIR:/tmp/devloop:Z' "$DEVLOOP" \
  || fail "devloop.sh no longer mounts HELPER_RUNTIME_DIR at container /tmp/devloop — the marker base mapping broke."
grep -q 'marker)  printf .%s. "\${DEVLOOP_TMP:-/tmp/devloop}/story-runner"' "$RUNNER" \
  || fail "run-story.sh resolve_run_base marker no longer defaults to /tmp/devloop/story-runner — the marker base drifted from devloop.sh's mount target."

exit 0
