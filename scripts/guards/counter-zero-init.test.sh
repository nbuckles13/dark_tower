#!/usr/bin/env bash
#
# Self-test for scripts/guards/simple/validate-counter-zero-init.sh
# (dt-guard subcommand `counter-zero-init`, ADR-0036 story-1 counter-visibility).
#
# ---------------------------------------------------------------------------
# WHY THIS FILE EXISTS
# ---------------------------------------------------------------------------
# The guard PASSES on every real run — the four services are fully zero-init'd
# and meant to stay that way. A passing guard exercises none of its failure
# branches, and those branches are its entire value: a check that walks a
# renamed/absent root and reports clean forever is the exact "guard lies"
# failure this guard was written to prevent. So this drives, against synthetic
# roots, the positive control (a planted lazy counter MUST fire), the pass
# case, each exemption path, the slot()-witness predicate, the
# entrypoint-not-called check, and the scope-liveness tokens.
#
# ---------------------------------------------------------------------------
# WHY IT IS NOT UNDER guards/simple/  — `run-guards.sh` discovers with
# `find … -name '*.sh'`, which matches `*.test.sh`, so a self-test there would
# be auto-run AS A PRODUCTION GUARD. Same reasoning as media-telemetry-deny.test.sh.
#
# NO TEST SEAM: `--root` is a production clap flag; this suite builds throwaway
# roots and points the real binary at them. Hermetic: mktemp + EXIT trap, no
# cluster/network/cargo. A missing binary is a LOUD failure here, never a skip.
set -euo pipefail
IFS=$'\n\t'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

DT_GUARD="${DT_GUARD:-${REPO_ROOT}/target/release/dt-guard}"
if [[ ! -x "$DT_GUARD" ]]; then
  printf '  - [precondition] dt-guard binary missing or not executable at %s\n' "$DT_GUARD"
  printf '\n%s: 0 passed, 1 failed\n' "$0"
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Build a synthetic repo root with one `mc-service` (a CANONICAL_SERVICES member)
#   $1 = root dir
#   $2 = metrics.rs body
#   $3 = catalog markdown body
#   $4 = main.rs body
make_root() {
  local root="$1" metrics="$2" catalog="$3" main="$4"
  mkdir -p "${root}/crates/mc-service/src/observability"
  mkdir -p "${root}/docs/observability/metrics"
  printf '%s\n' "$metrics" > "${root}/crates/mc-service/src/observability/metrics.rs"
  printf '%s\n' "$catalog" > "${root}/docs/observability/metrics/mc-service.md"
  printf '%s\n' "$main" > "${root}/crates/mc-service/src/main.rs"
}

CATALOG_ONE='# MC Metrics
### `mc_widget_total`
- **Type**: Counter'

MAIN_CALLS='fn main() { zero_initialize_counters(); }'
MAIN_NOCALL='fn main() { let _ = 1; }'

# --- Case 1: POSITIVE CONTROL — planted lazy counter MUST fire ---------------
R1="${WORK}/fires"
make_root "$R1" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }' \
"$CATALOG_ONE" "$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R1" 2>&1 || true)"
assert_status "positive-control: catalogued+emitted counter with no zero-init entrypoint fires" "STATUS=FAIL" "$out"
assert_status "positive-control fires the missing_zero_init rule" "missing_zero_init" "$out"

# --- Case 2: PASS — counter touched in a marked entrypoint -------------------
R2="${WORK}/passes"
make_root "$R2" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { counter!("mc_widget_total", "k" => "v").increment(0); }' \
"$CATALOG_ONE" "$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R2" 2>&1 || true)"
assert_status "zero-initialized counter passes" "STATUS=OK" "$out"

# --- Case 3: EXEMPT — catalog marks it exempt, not zero-init'd ---------------
R3="${WORK}/exempt"
make_root "$R3" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { let _ = 1; }' \
'# MC Metrics
### `mc_widget_total`
- **Type**: Counter
- **Zero-init**: exempt — status_code is a raw runtime u16, an unbounded label domain' \
"$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R3" 2>&1 || true)"
assert_status "catalog-exempt counter passes without a zero-init touch" "STATUS=OK" "$out"

# --- Case 4: MALFORMED exempt reason fails closed ---------------------------
R4="${WORK}/malformed"
make_root "$R4" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { let _ = 1; }' \
'# MC Metrics
### `mc_widget_total`
- **Type**: Counter
- **Zero-init**: exempt — todo' \
"$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R4" 2>&1 || true)"
assert_status "blank/lazy exempt reason fails closed" "STATUS=FAIL" "$out"
assert_status "malformed exempt reason fires its own token" "malformed_exempt_reason" "$out"

# --- Case 5: EXEMPT AND zero-init'd (mutual exclusion) fires -----------------
R5="${WORK}/both"
make_root "$R5" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { counter!("mc_widget_total", "k" => "v").increment(0); }' \
'# MC Metrics
### `mc_widget_total`
- **Type**: Counter
- **Zero-init**: exempt — status_code is a raw runtime u16, an unbounded label domain' \
"$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R5" 2>&1 || true)"
assert_status "a counter both exempt AND zero-init'd fires" "STATUS=FAIL" "$out"
assert_status "mutual-exclusion token fires" "exempt_and_zero_init" "$out"

# --- Case 6: slot() witness with a catch-all arm fires ----------------------
R6="${WORK}/wildcard"
make_root "$R6" \
'const fn slot_widget(v: T) -> usize { match v { A => 0, _ => 1 } }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { counter!("mc_widget_total").increment(0); }' \
"$CATALOG_ONE" "$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R6" 2>&1 || true)"
assert_status "slot() witness with a _ => catch-all fires" "STATUS=FAIL" "$out"
assert_status "wildcard witness token fires" "slot_witness_wildcard" "$out"

# --- Case 7: marked entrypoint never called from main.rs fires ---------------
R7="${WORK}/uncalled"
make_root "$R7" \
'// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { counter!("mc_widget_total").increment(0); }' \
"$CATALOG_ONE" "$MAIN_NOCALL"
out="$("$DT_GUARD" counter-zero-init --root "$R7" 2>&1 || true)"
assert_status "a marked entrypoint never called from main.rs fires" "STATUS=FAIL" "$out"
assert_status "uncalled-entrypoint token fires" "entrypoint_not_called_from_main" "$out"

# --- Case 8: scope-liveness — empty root (no services) fails distinctly ------
R8="${WORK}/empty"
mkdir -p "$R8"
out="$("$DT_GUARD" counter-zero-init --root "$R8" 2>&1 || true)"
assert_status "an empty root (no service metrics.rs) fails, not silently passes" "STATUS=FAIL" "$out"

# --- Case 9: G2 — a live crate whose metrics.rs was renamed/moved HARD-errors -
# The whole crate dir is present (workspace member), but the scan target is
# gone. This is the silent-quarter-drop hole: with three sibling services still
# scanning, a bare `continue` would keep STATUS=OK. It must fail closed with the
# root-absent token instead. (Whole-crate absence, by contrast, is a legitimate
# partial root — every other case above builds exactly one service and passes.)
R9="${WORK}/renamed"
make_root "$R9" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { counter!("mc_widget_total", "k" => "v").increment(0); }' \
"$CATALOG_ONE" "$MAIN_CALLS"
# Rename the scan target out from under a crate that still exists.
mv "${R9}/crates/mc-service/src/observability/metrics.rs" \
   "${R9}/crates/mc-service/src/observability/metrics_v2.rs"
out="$("$DT_GUARD" counter-zero-init --root "$R9" 2>&1 || true)"
assert_status "a renamed metrics.rs in a live crate fails, not silently drops scope" "STATUS=FAIL" "$out"
assert_status "renamed-scan-target fires the root-absent token" "scan_root_absent" "$out"

# --- Case 10: SCOPE-BOUNDARY — a lazy counter in a NON-canonical sibling dir
# stays GREEN (TEST-F2). The guard scopes by CANONICAL_SERVICES (ac/gc/mc/mh); an
# `xc-service` dir is outside that set, so its catalogued+emitted lazy counter must
# be invisible. This catches a walk-root widening — if the guard ever started
# resolving non-canonical service dirs, `xc-service` would be scanned and surface.
# Under correct scoping the passing mc-service is the only thing scanned → OK.
R10="${WORK}/scope-boundary"
make_root "$R10" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() { counter!("mc_widget_total", "k" => "v").increment(0); }' \
"$CATALOG_ONE" "$MAIN_CALLS"
mkdir -p "${R10}/crates/xc-service/src/observability"
printf '%s\n' 'pub fn record_gadget() { counter!("xc_gadget_total", "k" => "v").increment(1); }' \
  > "${R10}/crates/xc-service/src/observability/metrics.rs"
printf '%s\n' '# XC Metrics
### `xc_gadget_total`
- **Type**: Counter' > "${R10}/docs/observability/metrics/xc-service.md"
out="$("$DT_GUARD" counter-zero-init --root "$R10" 2>&1 || true)"
assert_status "a lazy counter in a non-canonical sibling dir stays GREEN (scope not widened)" "STATUS=OK" "$out"
assert_absent "the out-of-scope sibling is never scanned (its metric never surfaces)" "xc_gadget_total" "$out"

# --- Case 11: OUTPUT-SHAPE — no VIOLATION:/ERROR: on a continuation line (TEST-F2).
# `run-guards.sh` greps findings and `head -5`s them; a finding whose message
# embedded a newline would split into a continuation line and be mis-attributed.
# Every finding must be exactly ONE line: `VIOLATION: …`. Assert that no line
# CONTAINING `VIOLATION:`/`ERROR:` is anything other than a line STARTING with
# `VIOLATION:` (drive a failing root so there is at least one finding to shape-check).
R11="${WORK}/shape"
make_root "$R11" \
'pub fn record_widget() { counter!("mc_widget_total", "k" => "v").increment(1); }' \
"$CATALOG_ONE" "$MAIN_CALLS"
out="$("$DT_GUARD" counter-zero-init --root "$R11" 2>&1 || true)"
assert_status "shape precondition: the failing root actually emits a VIOLATION line" "VIOLATION:" "$out"
leak="$(printf '%s\n' "$out" | { grep -E 'VIOLATION:|ERROR:' || true; } | { grep -vE '^VIOLATION:' || true; })"
shape="SHAPE_OK"; [[ -n "$leak" ]] && shape="SHAPE_LEAK: ${leak}"
assert_status "no VIOLATION:/ERROR: on a continuation line (run-guards head-5 safe)" "SHAPE_OK" "$shape"

report_results "$0"
