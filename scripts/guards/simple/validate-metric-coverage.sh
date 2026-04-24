#!/bin/bash
#
# Metric Test-Coverage Validation Guard (ADR-0032)
#
# Enforces the ADR-0032 principle: every metric emitted by
# `crates/{service}/src/observability/metrics.rs` MUST be referenced by at
# least one test in `crates/{service}/tests/**/*.rs`.
#
# Why this is separate from validate-application-metrics.sh:
#   - validate-application-metrics.sh: metric <-> dashboard/catalog coverage
#   - this guard:                      metric <-> component-test coverage
#
# Mechanism:
#   1. For each service crate, scan `src/observability/metrics.rs` for metric
#      emission sites: `metrics::counter!`, `histogram!`, `gauge!` macro calls
#      whose first argument is a string literal. Dark Tower convention
#      (enforced by validate-application-metrics.sh) keeps all macro sites in
#      `metrics.rs`; everywhere else uses `record_*()` wrappers that bottom
#      out on those macros.
#   2. Post-filter extracted names to `^[a-z][a-z0-9_]+$` to discard any stray
#      captures from comments or string literals.
#   3. For each remaining metric name, scan
#      `crates/{service}/tests/**/*.rs` for any string occurrence. Fail the
#      guard if zero occurrences.
#
# ADR-0032 §Enforcement explicitly rejects:
#   - baseline / allowlist files
#   - per-PR ratchet mechanisms
#   - multi-line macro invocations (none currently exist in source tree;
#     the regex is single-line by design)
#
# Exit codes:
#   0 - all emitted metrics are referenced by at least one test
#   1 - one or more metrics lack a test reference (the expected interim
#       state until per-service backfill PRs, per ADR-0032 phasing)
#   2 - script error (missing source file, CANONICAL_SERVICES lookup miss)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Source common library for CANONICAL_SERVICES + color vars.
source "$SCRIPT_DIR/../common.sh"

CRATES_DIR="$REPO_ROOT/crates"

# Parse the canonical mapping into arrays indexed by metric prefix.
declare -A SERVICE_DIRS
for prefix in "${!CANONICAL_SERVICES[@]}"; do
    IFS=':' read -r dir _ <<< "${CANONICAL_SERVICES[$prefix]}"
    SERVICE_DIRS[$prefix]="$dir"
done

# -----------------------------------------------------------------------------
# Per-service scan
# -----------------------------------------------------------------------------

errors=0

echo -e "${BOLD}=========================================="
echo -e "Metric Test-Coverage Validation (ADR-0032)"
echo -e "==========================================${NC}"
echo ""

for prefix in $(printf '%s\n' "${!SERVICE_DIRS[@]}" | sort); do
    service_dir="${SERVICE_DIRS[$prefix]}"
    metrics_file="$CRATES_DIR/${service_dir}/src/observability/metrics.rs"
    tests_dir="$CRATES_DIR/${service_dir}/tests"

    echo -e "${BLUE}${prefix} (${service_dir})${NC}"

    if [[ ! -f "$metrics_file" ]]; then
        echo -e "${YELLOW}  WARNING: expected $metrics_file but it does not exist — skipping service${NC}"
        echo ""
        continue
    fi

    # Extract emitted metric names from the macro sites.
    # Matches both `metrics::counter!(...)` and bare `counter!(...)` forms.
    # Single-line macro invocations only.
    emitted=$(grep -oP '(?:metrics::)?(?:counter|histogram|gauge)!\s*\(\s*"[^"]+"' "$metrics_file" \
                 | grep -oP '"[^"]+"' \
                 | tr -d '"' \
                 | grep -E '^[a-z][a-z0-9_]+$' \
                 | sort -u || true)

    if [[ -z "$emitted" ]]; then
        echo -e "${YELLOW}  WARNING: no metric emissions found in $metrics_file — verify CANONICAL_SERVICES mapping${NC}"
        echo ""
        continue
    fi

    emitted_count=$(echo "$emitted" | wc -l)
    echo "  Scanning $emitted_count emitted metric name(s)..."

    if [[ ! -d "$tests_dir" ]]; then
        # Tests dir missing entirely: every metric is uncovered.
        while IFS= read -r metric; do
            [[ -z "$metric" ]] && continue
            echo -e "${RED}  ❌ ERROR${NC}: ${service_dir}: metric '${metric}' emitted in"
            echo "     ${metrics_file#$REPO_ROOT/}"
            echo "     but ${tests_dir#$REPO_ROOT/} does not exist (no component tests)"
            echo "     Remediation: add a test under that directory that references the metric name,"
            echo "     ideally via:"
            echo "       let snap = MetricAssertion::snapshot();"
            echo "       // ...run code under test..."
            echo "       snap.counter(\"${metric}\").assert_delta(N);"
            ((errors++)) || true
        done <<< "$emitted"
        echo ""
        continue
    fi

    # Assemble file list once per service (portable find + read loop).
    test_files=()
    while IFS= read -r f; do
        [[ -n "$f" ]] && test_files+=("$f")
    done < <(find "$tests_dir" -type f -name '*.rs' 2>/dev/null)

    while IFS= read -r metric; do
        [[ -z "$metric" ]] && continue

        # Fixed-string match across the test directory. `grep -F -q` returns
        # 0 on first match and 1 on none; we treat non-zero as uncovered.
        if [[ ${#test_files[@]} -eq 0 ]] || ! grep -F -q -r -- "$metric" "${test_files[@]}" 2>/dev/null; then
            echo -e "${RED}  ❌ ERROR${NC}: ${service_dir}: metric '${metric}' emitted in"
            echo "     ${metrics_file#$REPO_ROOT/}"
            echo "     but not referenced in any test under ${tests_dir#$REPO_ROOT/}"
            echo "     Remediation: add a test that calls:"
            echo "       let snap = MetricAssertion::snapshot();"
            echo "       // ...run code under test..."
            echo "       snap.counter(\"${metric}\").assert_delta(N);"
            ((errors++)) || true
        fi
    done <<< "$emitted"

    echo ""
done

echo -e "${BOLD}=========================================="
echo -e "Summary"
echo -e "==========================================${NC}"

if [[ $errors -gt 0 ]]; then
    echo -e "${RED}✗ Found $errors uncovered metric(s) across services${NC}"
    echo ""
    echo "Per ADR-0032 §Enforcement, this guard is intentionally blunt:"
    echo "no baseline file, no ratchet. Per-service component-test backfill"
    echo "PRs (ADR-0032 §Implementation Notes phasing steps 2-5) drain the"
    echo "uncovered set. See docs/decisions/adr-0032-metric-testability.md."
    echo ""
    exit 1
fi

echo -e "${GREEN}✓ All emitted metrics are referenced by at least one test${NC}"
echo ""
exit 0
