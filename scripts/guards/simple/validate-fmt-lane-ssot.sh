#!/usr/bin/env bash
# Guard: the fmt-lane apply-vs-check decision has ONE home, and the tree-attesting
# gates force CHECK-only (ADR-0037 §D7).
#
# THE INVARIANT. Whether the Layer-2 format lanes APPLY fixes or only CHECK is
# decided in EXACTLY ONE place — `fmt_mode()` in scripts/lang/_common.sh (the SSoT,
# modeled on fail_fast_mode()). The per-language wrappers (rust, proto, ts) ACT on
# that verdict; they do not re-derive it. The moment a wrapper grows
# its own `if $CI`/`GITHUB_ACTIONS` branch, the decision has TWO homes and they will
# drift — a CI lane that applies, or a local lane that checks, exactly the split D7
# collapsed. This guard is the forcing function (§D9): a partial re-scatter reds here
# rather than shipping as a silent regression.
#
# It ALSO pins the fail-closed direction: the two tree-attesting gates (the run-story
# per-task authority gate + the full-pipeline gate) must carry DEVLOOP_FMT_CHECK_ONLY,
# and .githooks/pre-commit must be check-only. An attestation that auto-formats the
# tree it certifies records a green not reproducible from the commit.
#
# WHAT THIS GUARD DOES NOT PROVE: the PRECEDENCE inside fmt_mode() (that check-only
# out-ranks the apply opt-in, which out-ranks nothing below CI). That is a truth-table
# property, pinned by the fmt_mode cases in scripts/lang/_common.test.sh — not
# re-derivable by grep. This guard pins the STRUCTURE (one home, delegation, the gate
# prefixes); that suite pins the SEMANTICS.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

COMMON="${FMT_SSOT_COMMON:-scripts/lang/_common.sh}"
RUST="${FMT_SSOT_RUST:-scripts/lang/rust/fmt.sh}"
PROTO="${FMT_SSOT_PROTO:-scripts/lang/proto/fmt.sh}"
TS="${FMT_SSOT_TS:-scripts/lang/ts/fmt.sh}"
RUNNER="${FMT_SSOT_RUNNER:-scripts/workflow/run-story.sh}"
PRECOMMIT="${FMT_SSOT_PRECOMMIT:-.githooks/pre-commit}"
LAYER2="${FMT_SSOT_LAYER2:-scripts/layer2.sh}"

fail() { echo "fmt-lane-ssot: $*" >&2; exit 1; }

for f in "$COMMON" "$RUST" "$PROTO" "$TS" "$RUNNER" "$PRECOMMIT" "$LAYER2"; do
  [ -f "$f" ] || fail "expected file ${f} not found — a fmt-lane site moved or was deleted. Fix the path (or the seam) rather than removing this guard; a missing site is exactly the regression it exists to catch."
done

# (1) THE SSoT HOME: fmt_mode() defined exactly once in _common.sh.
home_count="$(grep -cE '^fmt_mode\(\)' "$COMMON" || true)"
[ "$home_count" -eq 1 ] \
  || fail "expected exactly 1 'fmt_mode()' definition in ${COMMON}, found ${home_count}. The apply-vs-check lane must have ONE home (ADR-0037 §D7); zero means the SSoT was removed, two means it forked."

# (2) DELEGATION: each language wrapper calls fmt_mode (acts on the verdict, does not re-derive it).
for w in "$RUST" "$PROTO" "$TS"; do
  grep -qE '\bfmt_mode\b' "$w" \
    || fail "${w} does not call fmt_mode — a fmt wrapper MUST delegate the lane decision to fmt_mode() in ${COMMON}, never decide it locally (ADR-0037 §D7)."
done

# (3) NO SSoT BYPASS: no wrapper re-derives the lane from a CI sentinel. Strip comment lines first (the
#     wrappers legitimately discuss "CHECK in CI" in prose); a CI/GITHUB_ACTIONS token in CODE means a
#     second decision home. This is the load-bearing anti-drift check.
for w in "$RUST" "$PROTO" "$TS"; do
  if grep -vE '^[[:space:]]*#' "$w" | grep -qE '\b(GITHUB_ACTIONS|CI)\b'; then
    fail "${w} references a CI sentinel (GITHUB_ACTIONS/CI) in CODE — the lane decision belongs ONLY to fmt_mode() in ${COMMON} (ADR-0037 §D7). Move the branch into fmt_mode and have the wrapper act on its verdict."
  fi
done

# (4) TREE-ATTESTING GATES force CHECK-only: both run_gate and run_full_gate carry the knob. Extract each
#     function body (opening `name()` line to the next line that is a lone `}`) and require the prefix in it.
func_body() {  # <file> <funcname> -> stdout = the function body
  awk -v fn="$2" '
    $0 ~ "^"fn"\\(\\)" {inb=1}
    inb {print}
    inb && /^\}/ {exit}
  ' "$1"
}
for fn in run_gate run_full_gate; do
  body="$(func_body "$RUNNER" "$fn")"
  [ -n "$body" ] \
    || fail "could not locate ${fn}() in ${RUNNER} — the extraction no longer matches, so this guard is not reading the gate. Fix the extraction rather than deleting the check."
  printf '%s\n' "$body" | grep -qF 'DEVLOOP_FMT_CHECK_ONLY=1' \
    || fail "${fn}() in ${RUNNER} does not force DEVLOOP_FMT_CHECK_ONLY=1 — this is a tree-attesting AUTHORITY gate and must never auto-format the tree it certifies (ADR-0037 §D7). Restore the per-invocation prefix."
done

# (5) PRE-COMMIT is check-only: the check form is present, and it never opts into apply.
grep -qF 'cargo fmt --all -- --check' "$PRECOMMIT" \
  || fail "${PRECOMMIT} does not run 'cargo fmt --all -- --check' — the commit hook attests the tree and must CHECK it, never apply (ADR-0037 §D7)."
if grep -qF 'DEVLOOP_FMT_APPLY' "$PRECOMMIT"; then
  fail "${PRECOMMIT} references DEVLOOP_FMT_APPLY — a tree-attesting hook must never opt into fmt auto-apply (ADR-0037 §D7)."
fi

# (6) THE FMT LAYER MUST DISPATCH proto (security co-sign #7). With ci-client's duplicate `buf format check`
#     removed, layer2.sh -> fmt.sh is the SOLE CI proto-format gate. `DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto`
#     would drop the proto lane entirely and `aggregate_worst_status` would still return OK — silencing the
#     gate with NO failure status. That pattern is LIVE in-tree (layer1.sh:32 uses it for the build stage),
#     so a future copy onto the fmt dispatch is a one-line silent regression. Fail-closed: the fmt layer
#     must not exclude proto, and if it sets an INCLUDE allow-list it must name proto. (Comment lines
#     stripped first — a triage note mentioning the pattern is fine.)
l2code="$(grep -vE '^[[:space:]]*#' "$LAYER2" || true)"
if printf '%s\n' "$l2code" | grep -qE 'DEVLOOP_DISPATCH_EXCLUDE_LANGS=[^[:space:]]*proto'; then
  fail "${LAYER2} (the fmt layer) excludes proto from the fmt dispatch — this is the SOLE CI proto-format gate (ci-client's duplicate was removed), so excluding proto silences it with no failure status (ADR-0037 §D7; security co-sign #7). Remove the exclude."
fi
if printf '%s\n' "$l2code" | grep -qE 'DEVLOOP_DISPATCH_INCLUDE_LANGS='; then
  printf '%s\n' "$l2code" | grep -E 'DEVLOOP_DISPATCH_INCLUDE_LANGS=' | grep -q 'proto' \
    || fail "${LAYER2} sets DEVLOOP_DISPATCH_INCLUDE_LANGS without naming proto — the fmt dispatch would skip the proto lane (the sole CI proto-format gate). Add proto to the allow-list."
fi

exit 0
