#!/bin/bash
#
# Guard Runner: Execute all simple (pattern-based) guards
#
# Semantic guards are handled by the semantic-guard agent during devloops,
# not by this script. See .claude/agents/semantic-guard.md.
#
# Exit codes:
#   0 - All guards passed
#   1 - One or more guards found a violation (implementer lane)
#   2 - A guard hit its timeout (operator lane — PRECONDITION_FAILURE; see
#       scripts/lang/_common.sh status_to_exit_code / ADR-0033 §6), OR a script/usage error.
#       A timeout exit DOMINATES a violation exit (ladder-mirror: PRECONDITION_FAILURE > FAIL);
#       the coexisting violation is still named loudly (STATUS=FAIL guard-violations + MIXED_LANE).
#
# Usage:
#   ./run-guards.sh [options] [path]
#
# Options:
#   --verbose         Show detailed output
#   --help            Show this help message
#
# Examples:
#   ./run-guards.sh                              # Run simple guards on entire repo
#   ./run-guards.sh crates/ac-service/src/       # Run on specific directory
#

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common library for helper functions (also provides RED/YELLOW/… — tty-gated there, the
# ONE home for the guards tree; standalone use still reaches this source unconditionally under
# `set -e`, so no separate copy is kept here. ADR-0037 D8: the tty-gate keeps raw ANSI out of
# ${DEVLOOP_TMP}/layer-3.log so the runbook §6.3/§8 `FAILED:` grep and the `^FAILED` teaser anchor match).
source "$SCRIPT_DIR/common.sh"

# Default options
VERBOSE=false
SEARCH_PATH=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -25 "$0" | tail -20
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            exit 2
            ;;
        *)
            SEARCH_PATH="$1"
            shift
            ;;
    esac
done

# Default to repository root
if [[ -z "$SEARCH_PATH" ]]; then
    # Find repository root (go up until we find .git directory or file)
    # Note: In worktrees/clones, .git may be a file pointing to the main repo
    REPO_ROOT="$SCRIPT_DIR"
    while [[ ! -d "$REPO_ROOT/.git" ]] && [[ ! -f "$REPO_ROOT/.git" ]] && [[ "$REPO_ROOT" != "/" ]]; do
        REPO_ROOT="$(dirname "$REPO_ROOT")"
    done
    if [[ "$REPO_ROOT" == "/" ]]; then
        echo "Error: Could not find repository root (.git directory or file)"
        exit 2
    fi
    SEARCH_PATH="$REPO_ROOT"
fi

echo -e "${BOLD}=========================================="
echo "Guard Pipeline Runner"
echo "==========================================${NC}"
echo ""
echo "Path: $SEARCH_PATH"
echo ""

# Track results.
# FAILED_GUARDS / FAILED_GUARD_NAMES mean "found a VIOLATION" (implementer lane, exit 1).
# PRECONDITION_GUARDS / PRECONDITION_GUARD_NAMES mean "timed out / SIGKILLed" (operator lane,
# exit 2) — a machine fact, not a diff defect. Kept as SEPARATE counters so a timeout is never
# reported as a violation and vice versa (ADR-0033 §6 lane split).
TOTAL_GUARDS=0
PASSED_GUARDS=0
FAILED_GUARDS=0
PRECONDITION_GUARDS=0
declare -a FAILED_GUARD_NAMES
declare -a PRECONDITION_GUARD_NAMES

# Timer
START_TIME=$(date +%s.%N)

# -----------------------------------------------------------------------------
# Run Simple Guards
# -----------------------------------------------------------------------------
echo -e "${BOLD}Simple Guards${NC}"
echo "============="
echo ""

# Iterate simple/**/*.sh recursively (per ADR-0033 §1.5). Prune fixtures/ —
# no committed fixtures per docs/TODO.md:341 (2026-05-08 guard-self-test-
# cleanup policy); pruning structurally enforces the policy at the runner
# level so a future accidental fixture addition does not auto-execute.
SIMPLE_GUARDS_DIR="$SCRIPT_DIR/simple"
if [[ -d "$SIMPLE_GUARDS_DIR" ]]; then
    mapfile -d '' -t guards < <(
        find "$SIMPLE_GUARDS_DIR" -name "*.sh" -type f -not -path '*/fixtures/*' -print0 | sort -z
    )
    # Per-guard timeout per ADR-0034 §9 (strategy-independent hardening).
    # `GUARD_TIMEOUT_SECS` defaults to 30s, `GUARD_KILL_AFTER_SECS` to 5s.
    # Exit 124 → STATUS=PRECONDITION_FAILURE REASON=guard-timeout-<name>       (operator lane, exit 2)
    # Exit 137 → STATUS=PRECONDITION_FAILURE REASON=guard-timeout-kill-<name>  (operator lane, exit 2)
    # A timeout is a machine fact (contention/OOM), NOT a diff defect, so it lands on the
    # operator lane instead of a code-defect FAIL. REASON tokens are byte-identical to the
    # pre-2026-08-21 contract; only the STATUS enum moved FAIL → PRECONDITION_FAILURE.
    # Capture form `local guard_exit=0 || guard_exit=$?` is load-bearing
    # under `set -euo pipefail` — without the `0` initializer + `||` capture,
    # a non-zero timeout exit aborts the for-loop before the classifier runs.
    GUARD_TIMEOUT_SECS="${GUARD_TIMEOUT_SECS:-30}"
    GUARD_KILL_AFTER_SECS="${GUARD_KILL_AFTER_SECS:-5}"

    # Single classifier per @test F1 fold-in 2026-05-19. Maps the `$1`
    # exit code from the timeout-wrapped guard invocation into one of
    # four classes: 0 (PASS), 124 (timeout), 137 (timeout-kill), or
    # other (violation). 124/137 update PRECONDITION_GUARDS / _NAMES
    # (operator lane); a violation updates FAILED_GUARDS / _NAMES
    # (implementer lane) — the two counter sets are never crossed.
    # `$2` is the captured stdout+stderr for non-verbose callers; empty
    # for verbose callers (where output already streamed live). On the
    # generic-failure branch, captured output is greped for VIOLATION /
    # ERROR / WARN markers and the first 5 hits printed for triage.
    classify_guard_exit() {
        local exit_code="$1"
        local captured="$2"
        case "$exit_code" in
            0)
                echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
                ((PASSED_GUARDS++)) || true
                # Surface `^WARN ` lines from PASSING guards.
                #
                # Without this the pass arm discards $captured entirely, so a
                # guard that passes while reporting a coverage hole is silent on
                # every run. That made two contracts unenforceable: §6.3's
                # `ts-no-retained-credentials` rule that "a clean run must be a
                # WARN-free run ... do not ignore it because the layer passed",
                # and validate-frame-vectors' g14 banner, whose whole job is to
                # say the cross-language property is not yet established.
                #
                # Line-anchored `^WARN ` on this arm, deliberately narrower than
                # the failure arm's unanchored pattern: on a green run this is a
                # structured-emission contract, and surfacing any guard that
                # prints "WARN" mid-sentence would be noise — which is how people
                # learn to skim past the one line that mattered.
                #
                # `|| true` is load-bearing for the same reason as the failure
                # arm: with no match, grep exits 1, pipefail propagates, and
                # `set -e` aborts the loop, silently skipping every remaining
                # guard. Keep the sentinel even if the pattern changes.
                #
                # Deliberately NO `head -5` here, unlike the failure arm. That
                # cap exists because the failure arm's pattern is broad
                # (VIOLATION|violation|ERROR|error|WARN, unanchored) and can
                # match hundreds of lines from one guard. `^WARN ` is a narrow
                # structured-emission contract with few producers, and each line
                # is a distinct coverage hole an operator must see — truncating
                # would silently drop the fifth one, which is the same
                # empty-result-reads-as-pass failure this arm was added to fix.
                # If the pass arm ever grows noisy, the fix is to stop routine
                # text through `^WARN ` (see NOTE in no_insecure_browser_flags),
                # not to cap it.
                if [[ -n "$captured" ]]; then
                    { echo "$captured" | grep -E "^WARN "; } || true
                fi
                ;;
            124)
                # Timeout → operator lane (PRECONDITION_FAILURE, exit 2), NOT a diff defect.
                echo "STATUS=PRECONDITION_FAILURE REASON=guard-timeout-${GUARD_NAME}"
                echo -e "${YELLOW}TIMEOUT${NC}: $GUARD_NAME (timed out after ${GUARD_TIMEOUT_SECS}s — operator lane, machine too slow, not a diff defect)"
                # Line-anchored stderr banner (R-F): makes the runbook §4 one-pass triage grep
                # `^(ERROR|PRECONDITION_FAILURE):` find a guard timeout. Under layer3 this lands
                # in ${DEVLOOP_TMP}/layer-3.stderr.log via layer-all's `2>>` redirect.
                echo "PRECONDITION_FAILURE: guard ${GUARD_NAME} timed out after ${GUARD_TIMEOUT_SECS}s (operator lane; suspect concurrent machine load) REASON=guard-timeout-${GUARD_NAME}" >&2
                ((PRECONDITION_GUARDS++)) || true
                PRECONDITION_GUARD_NAMES+=("$GUARD_NAME")
                ;;
            137)
                # SIGKILL after the --kill-after grace (or an OOM kill) — also an environment
                # fact, so it shares the operator lane with 124 (ADR-0034 §9 authorizes both).
                echo "STATUS=PRECONDITION_FAILURE REASON=guard-timeout-kill-${GUARD_NAME}"
                echo -e "${YELLOW}TIMEOUT-KILL${NC}: $GUARD_NAME (SIGKILLed after ${GUARD_TIMEOUT_SECS}s timeout + ${GUARD_KILL_AFTER_SECS}s grace — operator lane, not a diff defect)"
                echo "PRECONDITION_FAILURE: guard ${GUARD_NAME} SIGKILLed after ${GUARD_TIMEOUT_SECS}s timeout + ${GUARD_KILL_AFTER_SECS}s grace (operator lane; suspect concurrent machine load or OOM) REASON=guard-timeout-kill-${GUARD_NAME}" >&2
                ((PRECONDITION_GUARDS++)) || true
                PRECONDITION_GUARD_NAMES+=("$GUARD_NAME")
                ;;
            *)
                echo -e "${RED}FAILED${NC}: $GUARD_NAME (exit $exit_code)"
                ((FAILED_GUARDS++)) || true
                FAILED_GUARD_NAMES+=("$GUARD_NAME")
                # Show failure details when caller captured output.
                #
                # `|| true` is load-bearing: under `set -euo pipefail`, if the
                # failed guard's output contains NO `VIOLATION` text (e.g.
                # validate-application-metrics emits `ERROR` instead), grep
                # exits 1, pipefail propagates, and `set -e` aborts the
                # for-loop mid-pipeline. The script then stops running the
                # remaining guards silently — turning a single failure into
                # a CI lie where downstream guard failures go unreported.
                # Keep this sentinel in place even if the pattern changes.
                #
                # `WARN` added per @team-lead F-SG-2 fold-in 2026-05-19: dt-guard
                # subcommands now emit `WARN dt-guard auxiliary skip: <path> (<err>)`
                # to stderr on IO/parse swallow sites (see common/scan.rs).
                # Surfacing them here gives oncall coverage-hole visibility in
                # non-verbose CI logs.
                if [[ -n "$captured" ]]; then
                    { echo "$captured" | grep -E "(VIOLATION|violation|ERROR|error|WARN)" | head -5; } || true
                fi
                ;;
        esac
    }

    for guard in "${guards[@]}"; do
        if [[ -x "$guard" ]]; then
            GUARD_NAME=$(basename "$guard" .sh)
            ((TOTAL_GUARDS++)) || true

            echo -e "${BLUE}Running:${NC} $GUARD_NAME"

            if $VERBOSE; then
                guard_exit=0
                timeout --kill-after="${GUARD_KILL_AFTER_SECS}s" "${GUARD_TIMEOUT_SECS}s" "$guard" "$SEARCH_PATH" || guard_exit=$?
                classify_guard_exit "$guard_exit" ""
            else
                # Capture output for non-verbose mode.
                # `OUTPUT=$(...)` swallows exit code into a separate var so we
                # can still classify timeout vs other failures.
                guard_exit=0
                OUTPUT=$(timeout --kill-after="${GUARD_KILL_AFTER_SECS}s" "${GUARD_TIMEOUT_SECS}s" "$guard" "$SEARCH_PATH" 2>&1) || guard_exit=$?
                classify_guard_exit "$guard_exit" "$OUTPUT"
            fi
            echo ""
        fi
    done
else
    echo "No simple guards found in $SIMPLE_GUARDS_DIR"
fi

# NOTE: Semantic guards are now handled by the semantic-guard agent,
# spawned during devloop validation (see .claude/agents/semantic-guard.md)

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
END_TIME=$(date +%s.%N)
ELAPSED=$(echo "$END_TIME - $START_TIME" | bc)

echo -e "${BOLD}=========================================="
echo "Summary"
echo "==========================================${NC}"
echo ""
echo "Total guards run: $TOTAL_GUARDS"
echo -e "Passed: ${GREEN}$PASSED_GUARDS${NC}"
echo -e "Failed (violations): ${RED}$FAILED_GUARDS${NC}"
echo -e "Timed out (operator lane): ${YELLOW}$PRECONDITION_GUARDS${NC}"
printf "Elapsed time: %.2f seconds\n" "$ELAPSED"
echo ""

# R-A2 (@security F2): a violating guard's generic-failure arm prints only the human
# `FAILED: <name>` line and NO `STATUS=` token — so standalone a violation would have no
# machine-readable trace. Emit one whenever any violation was found, mirroring the `FAIL`
# line run_and_emit already emits on the layer path. This is what keeps the ladder-mirror
# operator-lane exit (2, below) from MASKING a real defect when a timeout coexists.
if [[ $FAILED_GUARDS -gt 0 ]]; then
    echo "STATUS=FAIL REASON=guard-violations"
    # Machine-parseable, uncolored, comma-joined names (ADR-0037 D8). DISTINCT from the human
    # "Failed guards" block below AND from the INTEGER counter var FAILED_GUARDS (`:99`, which
    # drives the >0 test, the :277 summary and the exit ladder at :335) — same spelling, different
    # thing: this is a log-line TOKEN, that is a shell variable. Emitted from the FAILED_GUARD_NAMES
    # array (one producer, so the token and the human list cannot disagree). Plain `echo`, no color,
    # so `^FAILED_GUARD_NAMES=` anchors; a subshell scopes IF=',' without leaking. layer-all.sh
    # reads it via _common.sh::parse_failed_guard_names() to name the exact guard in the D8
    # FAILURE_TRIAGE directive. NOT a STATUS= line (does not trip the ANCHOR revisit trigger below).
    # The token string is the emitter<->parser contract, pinned by run-guards.test.sh's round-trip.
    ( IFS=','; echo "FAILED_GUARD_NAMES=${FAILED_GUARD_NAMES[*]}" )
    echo -e "${RED}Failed guards (violations — implementer lane):${NC}"
    for failed in "${FAILED_GUARD_NAMES[@]}"; do
        echo "  - $failed"
    done
    echo ""
fi

if [[ $PRECONDITION_GUARDS -gt 0 ]]; then
    echo -e "${YELLOW}Timed-out guards (operator lane — machine too slow / OOM, NOT a diff defect):${NC}"
    for timed_out in "${PRECONDITION_GUARD_NAMES[@]}"; do
        echo "  - $timed_out"
    done
    echo "  Re-run on a quiet machine. See docs/runbooks/devloop-validation.md §6.3."
    echo ""
fi

# MIXED_LANE (R-A2): when a timeout AND a violation coexist, name the violation LOUDLY so the
# ladder-mirror operator-lane exit (2) is never read as "nothing to fix". This line + the
# guard-violations STATUS trace above are what license exit-2-on-mixed (see the exit block).
if [[ $PRECONDITION_GUARDS -gt 0 && $FAILED_GUARDS -gt 0 ]]; then
    mixed_lane_line="MIXED_LANE: precondition=${PRECONDITION_GUARDS} violations=${FAILED_GUARDS} — exit 2 (operator lane); the ${FAILED_GUARDS} violation(s) above are REAL and must be fixed; re-run on a quiet machine for a clean implementer-lane verdict."
    echo "$mixed_lane_line"
    # Also to stderr (@observability): under layer3, stdout → layer-3.log but the documented
    # §4 one-pass triage reads layer-3.stderr.log — where the timeout banner lands. Without this,
    # an operator grepping stderr sees only "guard timed out, suspect machine load" and could
    # dismiss with "re-run on a quiet machine", missing the coexisting violation R-A2 exists to
    # keep legible. Putting the caveat in the SAME channel as the alarm closes that 3am shortcut.
    echo "$mixed_lane_line" >&2
    echo ""
fi

# ANCHOR (DRY): this exit precedence MIRRORS scripts/lang/_common.sh __status_rank +
# status_to_exit_code (PRECONDITION_FAILURE rank 6 > FAIL rank 5 > OK → exit 2 > 1 > 0).
# run-guards.sh sources scripts/guards/common.sh, NOT lang/_common.sh, so it cannot call
# status_to_exit_code directly — this hand-copy is the SAME ONE ladder as the layer
# aggregation (standalone exit == layer verdict == pipeline lane; no divergence). The mirror,
# AND the three hand-rolled `STATUS=` emissions in this file (the 124/137 arms + the
# guard-violations line above, all bypassing lang/_common.sh emit_status), are pinned by
# scripts/guards/run-guards.test.sh, which derives the expected exit from status_to_exit_code
# at test time so a ladder change REDS the test instead of silently desyncing. That self-test
# is the ONLY mechanical drift guard for this hand-copy. A FOURTH hand-rolled `STATUS=`
# emission is the recorded trigger to revisit routing through emit_status (which would require
# sourcing lang/_common.sh — rejected on coupling grounds, ADR-0015 §Pre-commit standalone use).
if [[ $PRECONDITION_GUARDS -gt 0 ]]; then
    echo "Run with --verbose for detailed output"
    exit 2
elif [[ $FAILED_GUARDS -gt 0 ]]; then
    echo "Run with --verbose for detailed output"
    exit 1
else
    echo -e "${GREEN}All guards passed!${NC}"
    exit 0
fi
