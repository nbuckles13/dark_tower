#!/usr/bin/env bash
#
# Single source of truth for WHICH bash guard self-tests drive the instrumented
# `dt-guard` binary and must therefore run under the CI coverage job
# (`.github/workflows/ci.yml`). Prints the selected suites (one path per line)
# to stdout, and runs the drift checks; a drift or floor failure exits non-zero
# with diagnostics on stderr, so the caller learns before it reads the list.
#
# Deliverable-4 SSoT (no hand-maintained list):
#
#   SELECTION is a single derivation point — every `*.test.sh` ANYWHERE under
#   `scripts/` that references `DT_GUARD`. There is no second list to drift
#   against. Selection is recursive on purpose: a dt-guard-driving suite added
#   under `scripts/lang/` or `scripts/` root is still discovered and run.
#
#   FLOOR is a hard minimum of 2 (the current exact driver count:
#   counter-zero-init.test.sh + media-telemetry-deny.test.sh). It is a FLOOR,
#   deliberately NOT a count-equality assert — landing a 3rd suite must not red
#   the job, but a silent DROP below 2 (a suite renamed/removed, or refactored
#   so it stops matching) MUST. This is the primary removal detector; cf.
#   `scripts/guards/run-guards.test.sh` and the drifted "all 33"-vs-41 comment
#   at `scripts/layer3.sh` that a count-equality assert rots into.
#
#   DRIFT MARKERS are bounded to `scripts/guards/*.test.sh` — the directory
#   where a dt-guard-driving self-test belongs. Every file there that is NOT
#   selected must carry a `# coverage-exempt: <reason>` line (reason floored at
#   MIN_REASON_LEN >= 10 chars — LENGTH ONLY; see the reason_too_short note below
#   for why the ignore.rs lazy-vocabulary is deliberately NOT mirrored here).
#   A file neither selected nor exempt fails the job. The universe is bounded
#   here — NOT widened to all of `scripts/**` — because marking ~20 unrelated
#   cross-domain `*.test.sh` is annotation debt in other specialists' files;
#   the recursive selection already RUNS a driver placed elsewhere, so the
#   marker only makes NON-selection deliberate where a driver would plausibly
#   be added. Do not "complete" this by widening the universe.
#
#   SECOND GREP (defense-in-depth): the bounded marker leaves one evasion — a
#   suite that BOTH moves out of `scripts/guards/` AND stops matching
#   `DT_GUARD` (e.g. hardcodes `target/release/dt-guard`). That is itself the
#   anti-pattern (a binary invocation not routed through the overridable
#   `$DT_GUARD`), so this script also fails on any non-`$DT_GUARD` dt-guard
#   binary invocation anywhere under `scripts/`, naming file:line + the remedy.
#
# Hermeticity note: the coverage job's checkout is deliberately shallow
# (`.github/workflows/ci.yml`, no `fetch-depth: 0`). A suite that needs a git
# base ref would be pulled in by selection and fail confusingly there — keep
# coverage suites hermetic (mktemp roots, no merge-base).

set -euo pipefail
IFS=$'\n\t'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

FLOOR=2

fail() {
  printf 'list-coverage-suites: %s\n' "$1" >&2
  exit 1
}

# --- SELECTION: recursive, single derivation point -------------------------
mapfile -t ALL_TEST_SH < <(find scripts -name '*.test.sh' -type f | sort)
SELECTED=()
for f in "${ALL_TEST_SH[@]}"; do
  # Reference to DT_GUARD on a NON-comment line — a real use of the binary
  # override, not a `# coverage-exempt:` marker that merely names it in prose.
  if grep -vE '^[[:space:]]*#' "$f" | grep -qE 'DT_GUARD'; then
    SELECTED+=("$f")
  fi
done

# --- FLOOR (primary removal detector) --------------------------------------
if (( ${#SELECTED[@]} < FLOOR )); then
  fail "found ${#SELECTED[@]} DT_GUARD-driving suite(s), need >= ${FLOOR} (floor, not a count-equality). \
A suite was renamed, removed, or refactored so it stops matching \`DT_GUARD\` — restore it or, if it \
legitimately no longer drives the binary, lower the FLOOR deliberately with a note."
fi

# --- coverage-exempt reason quality: LENGTH FLOOR ONLY -----------------------
# Deliberately NOT a bash mirror of ignore.rs's lazy-VOCABULARY. The full
# guard:ignore reason bar (crates/dt-guard/src/ignore.rs::is_lazy_reason) also
# rejects a test/tmp/todo/fixme/wip vocabulary via a word-boundaried `\b` regex,
# which a portable POSIX ERE cannot express — so a hand-copied bash vocabulary is
# an unanchored cross-language fork (no validate-*-sync.sh drift guard) and it had
# already drifted: without `\b` it mis-rejects reasons like "testing harness for X".
# A coverage-exempt marker is lower-stakes than a guard:ignore suppression, so the
# LENGTH FLOOR alone is the intended bar here (DRY review, 2026-09-17). Only the
# integer floor is shared with ignore.rs; keep it equal to MIN_REASON_LEN.
REASON_MIN_LEN=10   # SSoT: crates/dt-guard/src/ignore.rs::MIN_REASON_LEN
reason_too_short() {
  local r="${1#"${1%%[![:space:]]*}"}" # ltrim
  r="${r%"${r##*[![:space:]]}"}"       # rtrim
  (( ${#r} < REASON_MIN_LEN ))
}

# --- DRIFT MARKERS, bounded to scripts/guards/*.test.sh --------------------
for f in scripts/guards/*.test.sh; do
  [[ -e "$f" ]] || continue
  is_selected=false
  for s in "${SELECTED[@]}"; do [[ "$s" == "$f" ]] && is_selected=true && break; done
  $is_selected && continue
  marker="$(grep -oE '# *coverage-exempt: *.+$' "$f" | head -n1 || true)"
  if [[ -z "$marker" ]]; then
    fail "$f is under scripts/guards/, does not drive \$DT_GUARD, and carries no \
\`# coverage-exempt: <reason>\` marker. Add one (a descriptive reason >= ${REASON_MIN_LEN} chars) \
explaining why it is not a coverage suite, or route it through \"\$DT_GUARD\" if it should be."
  fi
  reason="${marker#*coverage-exempt:}"
  reason="${reason#"${reason%%[![:space:]]*}"}"
  if reason_too_short "$reason"; then
    fail "$f coverage-exempt reason is too short: '${reason}' \
(require a descriptive reason >= ${REASON_MIN_LEN} chars)."
  fi
done

# --- SECOND GREP: non-$DT_GUARD dt-guard binary invocations ----------------
# Match a hardcoded binary path (target/.../dt-guard) or a bare `dt-guard`
# command, on lines that are NOT comments, NOT cargo invocations, and do NOT
# route through $DT_GUARD. Fail with file:line + remedy.
while IFS= read -r hit; do
  [[ -z "$hit" ]] && continue
  fail "non-\$DT_GUARD dt-guard binary invocation — route it through \"\$DT_GUARD\" so the \
coverage job can point it at the instrumented binary:
    $hit"
done < <(
  # An INVOCATION is `<path-or-bare>dt-guard <subcommand>` — the binary
  # followed by a lowercase subcommand token. This deliberately does NOT match
  # an existence check (`[ -x target/release/dt-guard ]` → followed by `]`) or
  # cargo's `-p dt-guard` package flag (followed by ` -`/newline).
  grep -rInE '(^|[[:space:];&|(/])dt-guard[[:space:]]+[a-z]' \
    --include='*.test.sh' --include='*.sh' scripts 2>/dev/null \
  | grep -vE ':[[:space:]]*#' \
  | grep -vE 'DT_GUARD' \
  | grep -vE 'cargo[[:space:]]+(build|llvm-cov|test|run)' \
  || true
)

# --- OUTPUT: the selected suites, one per line -----------------------------
printf '%s\n' "${SELECTED[@]}"
