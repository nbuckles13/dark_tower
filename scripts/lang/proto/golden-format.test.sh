#!/usr/bin/env bash
# golden-format.test.sh — behavioral pin for `pnpm exec buf format` (ADR-0037 §D7; @security control).
#
# WHY THIS EXISTS: `_buf.sh`'s version assertion goes GREEN on a @bufbuild/buf bump (the running buf
# matches the newly-bumped lockfile pin), so it structurally CANNOT notice that the formatter now BEHAVES
# differently. This test is the disjoint control: the pinned buf must reproduce the committed golden bytes
# EXACTLY, so a bump that reformats reds HERE and the diff is the review evidence of what changed.
#
# THE GOLDEN-POISONING HAZARD (@security — the load-bearing gate). The golden check runs ONLY AFTER
# `proto_buf_preflight` passes (the EXACT version assertion). If it ran before, a STALE node_modules would
# red as "formatter behavior changed", the natural remedy is "regenerate the golden", and that would bake
# stale bytes into the committed golden PERMANENTLY — strictly worse than no fixture. So a stale install
# MUST red as `buf-version-mismatch` (remedy: `pnpm install --frozen-lockfile`), NEVER as golden drift, and
# the golden's failure message says regeneration is valid ONLY under a preflight-verified buf.
#
# WHY THE VERSION LIVES HERE, NOT IN THE GOLDEN (@security): the golden is PURE formatter output. If its
# generating version were a comment inside it, a bump would change the golden for TWO reasons (version
# string + formatting delta) and the reviewer could not read the formatting change cleanly — and that clean
# diff IS the fixture's whole purpose (P-2-bump-ceremony evidence). The version is a constant here instead.
#
# FIXTURES:
#   golden-format.messy.proto — the deliberately-messy raw input (from @security). It MUST contain the
#     option-block-then-leading-comment construct — the shape buf 1.50 and 1.72 format DIFFERENTLY; a
#     fixture without it passes clean across that version gap and teaches nothing (asserted below).
#   golden-format.proto        — that raw, run through the pinned buf: the committed golden.
# REGENERATION (buf-bump ceremony ONLY, never a blind reset): after `pnpm install --frozen-lockfile` brings
#   the new pinned buf AND this test's preflight passes, run
#     pnpm exec buf format scripts/lang/proto/fixtures/golden-format.messy.proto > .../golden-format.proto
#   bump BUF_GOLDEN_VERSION below, and REVIEW the golden diff as the record of what the new formatter changed.
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
__repo_root="$(cd "${__here}/../../.." && pwd)"
cd "$__repo_root"                                    # pnpm exec resolves the workspace from cwd
source "${__here}/../_common.sh"                     # emit helpers (unused directly; parity with lane)
source "${__here}/../_test_helpers.sh"               # PASS/FAIL, assert_status, report_results
source "${__here}/_buf.sh"                            # proto_buf_preflight (toolchain + EXACT version pin)

GOLDEN="scripts/lang/proto/fixtures/golden-format.proto"
MESSY="scripts/lang/proto/fixtures/golden-format.messy.proto"
# The buf version that produced the committed golden. Sidecar constant, NOT inside the golden (see header).
# Bump this AS PART OF regenerating the golden in the buf-bump ceremony.
BUF_GOLDEN_VERSION="1.72.0"

# (0) FAIL-CLOSED PREFLIGHT FIRST — the golden-poisoning gate. A stale/absent toolchain or a running buf !=
#     the lockfile pin reds HERE with the version token, so the golden check below only ever runs under a
#     verified-correct buf. (proto_buf_preflight emit_status FAILs itself.)
if ! proto_buf_preflight; then
  echo "  - [preflight] toolchain/version gate failed BEFORE the golden check (correct precedence — a stale buf must not be read as golden drift)." >&2
  printf '\n%s: 0 passed, 1 failed (preflight)\n' "scripts/lang/proto/golden-format.test.sh"
  exit 1
fi
for f in "$GOLDEN" "$MESSY"; do
  [ -f "$f" ] || { echo "  - [precondition] fixture missing at ${f}" >&2; \
    printf '\n%s: 0 passed, 1 failed\n' "scripts/lang/proto/golden-format.test.sh"; exit 1; }
done

# (1) VERSION SIDECAR AGREES WITH THE PIN (belt beyond preflight; documents which buf produced the golden).
#     If they diverge, the golden was regenerated without bumping the constant (or vice versa) — a bump
#     ceremony half-done. Derive the pin the same fail-closed way _buf.sh does.
__pin="$(jq -er '.devDependencies["@bufbuild/buf"] // empty' package.json 2>/dev/null || true)"
if [ "$BUF_GOLDEN_VERSION" = "$__pin" ]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[golden-version-sidecar] BUF_GOLDEN_VERSION=${BUF_GOLDEN_VERSION} != @bufbuild/buf pin ${__pin:-<underivable>}. The golden and its recorded version disagree — a buf-bump ceremony was left half-done. Regenerate the golden under the pinned buf AND set BUF_GOLDEN_VERSION to match.")
fi

# (2) IDEMPOTENCY (the bump control): pinned buf reproduces the golden byte-for-byte.
formatted="$(pnpm exec buf format "$GOLDEN" 2>/dev/null || true)"
if [ "$formatted" = "$(cat "$GOLDEN")" ]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[golden-idempotent] the pinned buf (preflight-verified == lockfile) REFORMATTED the golden — a genuine formatter behavior change, NOT a stale install. Review this diff (it is the buf-bump ceremony evidence); regenerate the golden ONLY now that the version is verified correct:
$(diff <(printf '%s\n' "$formatted") "$GOLDEN" | head -30)")
fi

# (3) CONSTRUCT-PRESENCE (anti-vacuity, @security "MUST contain"): the golden must still hold an option line
#     IMMEDIATELY followed by a leading `//` comment — the 1.50↔1.72 differentiator. If a future edit drops
#     it, the fixture stops controlling a buf bump, so red here.
if awk '/^option / { opt = NR } opt && NR == opt + 1 && /^\/\/ / { found = 1 } END { exit(found ? 0 : 1) }' "$GOLDEN"; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[golden-has-sensitive-construct] the golden no longer has an option line IMMEDIATELY followed by a leading comment — the version-sensitive construct is gone, so it no longer controls a buf bump. Restore it (regenerate from the messy fixture, which contains it).")
fi

# (4) NORMALIZATION CONTROL (non-vacuity): the committed MESSY raw must format to EXACTLY the golden. Proves
#     buf ACTUALLY RAN and normalizes (idempotency alone could hold vacuously if buf were a no-op), and pins
#     that the golden is the true canonical output of that raw — the two committed files stay in lockstep.
#     Read-only: formats the committed raw in place, never writes the tree (S-9).
renormalized="$(pnpm exec buf format "$MESSY" 2>/dev/null || true)"
if [ "$renormalized" = "$(cat "$GOLDEN")" ]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[golden-normalizes-messy] pinned buf did NOT normalize the messy raw to the golden — the two fixtures drifted (the golden was hand-edited, or regenerated from a different raw). Regenerate: pnpm exec buf format ${MESSY} > ${GOLDEN}:
$(diff <(printf '%s\n' "$renormalized") "$GOLDEN" | head -20)")
fi

report_results "scripts/lang/proto/golden-format.test.sh"
