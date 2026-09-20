#!/usr/bin/env bash
# Guard: generated protobuf TS stays OUT of prettier's file set (ADR-0037 §D7; @dry-reviewer standing
# assertion; policy specified by @paired-client, machinery infrastructure).
#
# THE HAZARD (why a one-time probe is not enough). The TS fmt lane formats from the REPO ROOT, so the ROOT
# `.prettierignore` is what excludes `packages/sdk-core/src/proto/**` (buf codegen output). That exclusion
# holds only while TWO preconditions both hold:
#   (i)  the root .prettierignore actually carries the proto-exclusion line, AND
#   (ii) no per-package `format` target sets a `cwd` — a cwd=package makes the root .prettierignore
#        (which is cwd-relative) STOP applying, so prettier's scope silently expands to the generated tree.
# If either slips, under APPLY `prettier --write` rewrites generated protobuf TS → it diverges from what
# `buf generate` produces → surfaces LATER as a misattributed `verify-codegen.sh` mismatch. This guard pins
# both preconditions so the regression reds HERE, at the edit, instead of days later in a different tool.
#
# It does NOT reformat, run prettier, or need the codegen present — pure grep + jq over the config.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

PRETTIERIGNORE="${TSFMT_PRETTIERIGNORE:-.prettierignore}"
PKGROOT="${TSFMT_PKGROOT:-packages}"
# The exact ignore line that must be present (seam-overridable for the self-test).
PROTO_LINE="${TSFMT_PROTO_LINE:-packages/sdk-core/src/proto/}"

fail() { echo "ts-fmt-proto-excluded: $*" >&2; exit 1; }

command -v jq >/dev/null 2>&1 || fail "jq not found — this guard parses project.json format targets with jq (fail-closed; jq is at infra/devloop/Dockerfile and on ubuntu-latest)."
[ -f "$PRETTIERIGNORE" ] || fail "root prettier-ignore ${PRETTIERIGNORE} not found — the proto-exclusion's single home is gone. It is what keeps buf codegen out of prettier's set (ADR-0037 §D7)."

# (i) The root .prettierignore carries the proto-exclusion line (exact, whole-line — a substring elsewhere
#     would not exclude the path). `grep -x` pins the whole line; a leading/trailing edit reds.
grep -qxF "$PROTO_LINE" "$PRETTIERIGNORE" \
  || fail "'${PROTO_LINE}' is not a line in ${PRETTIERIGNORE} — generated protobuf TS is no longer excluded from prettier. Under APPLY, prettier --write would rewrite buf codegen and diverge from \`buf generate\`. Restore the exclusion line."

# (ii) No per-package `format` target defines a cwd (a cwd=package defeats the root, cwd-relative ignore).
#      Also a vacuity guard: at least one format target must exist, or the check is asserting nothing.
found_format=0
shopt -s nullglob
for p in "${PKGROOT}"/*/project.json; do
  jq -e '.targets.format' "$p" >/dev/null 2>&1 || continue   # package without a format target
  found_format=$((found_format + 1))
  cwd="$(jq -r '.targets.format.options.cwd // empty' "$p" 2>/dev/null || true)"
  [ -z "$cwd" ] \
    || fail "${p}'s format target sets cwd='${cwd}'. A per-package cwd makes the ROOT .prettierignore (cwd-relative) stop applying, so prettier's scope silently expands to include buf codegen. Remove the cwd — the TS fmt lane runs from the repo root by design (ADR-0037 §D7)."
  # (dry addition) no `--ignore-path` in the format command(s): it redirects prettier's ignore resolution
  # AWAY from the root .prettierignore even with cwd correct — the one path around clauses (1)+(2). Covers
  # both the single `command` and the `commands[]` array form.
  cmds="$(jq -r '(.targets.format.options.command // empty), (.targets.format.options.commands[]? | if type=="string" then . else (.command // empty) end)' "$p" 2>/dev/null || true)"
  if printf '%s\n' "$cmds" | grep -qF -- '--ignore-path'; then
    fail "${p}'s format command carries --ignore-path. That redirects prettier's ignore resolution away from the root .prettierignore (defeating the proto-exclusion even with cwd at the repo root). Remove --ignore-path — the root .prettierignore is the single home (ADR-0037 §D7)."
  fi
done
shopt -u nullglob

[ "$found_format" -gt 0 ] \
  || fail "no 'format' target found in any ${PKGROOT}/*/project.json — the TS fmt lane's single home is gone (or PKGROOT is wrong). A guard that checks zero targets asserts nothing; this is exactly the vacuity it must refuse. Restore the format targets."

exit 0
