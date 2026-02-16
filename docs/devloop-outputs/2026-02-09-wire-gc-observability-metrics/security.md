# Security Review: Wire GC Observability Metrics

**Reviewer**: Security Specialist
**Date**: 2026-02-09
**Task**: Wire GC observability metrics - instrument MC assignment, DB queries, and MH selection code paths

---

## Iteration 2 Review

**Iteration 2 Changes Reviewed**:
- `crates/global-controller/src/services/mh_selection.rs` (NEW - added metrics instrumentation)
- `crates/global-controller/src/observability/metrics.rs` (UPDATED - removed token refresh functions, added MH selection metrics)

### Iteration 2 Analysis

#### 1. MH Selection Service (`mh_selection.rs`) - NEW

**Lines 65-76, 136**: Added `record_mh_selection()` metric calls.

**Security Assessment**: PASS

- **Cardinality**: Labels are bounded:
  - `status`: Fixed values (`"success"`, `"error"`)
  - `has_backup`: Boolean (`"true"`, `"false"`)
- **No PII**: Only operational status and boolean flag recorded
- **Privacy-by-default**: Uses `#[instrument(skip_all, fields(region = %region))]` - only safe fields exposed
- **Error semantics preserved**: Metrics recorded at appropriate points without masking errors

**Positive observations**:
- Line 76: Error case records metrics BEFORE returning error - correct pattern
- Line 136: Success case records metrics with `has_backup` flag - useful for capacity planning without exposing IDs

#### 2. Metrics Module (`metrics.rs`) - UPDATED

**Lines 173-183**: Removed `record_token_refresh()` and `record_token_refresh_failure()` functions.

**Security Assessment**: PASS (IMPROVED)

- **Rationale documented**: Comment explains cross-crate dependency issue (TokenManager in `common` crate)
- **No dead code risk**: Functions removed rather than left as `#[allow(dead_code)]`
- **Tech debt tracked**: Reference to TD-GC-001 for future implementation

**Lines 231-250**: Added `record_mh_selection()` function.

**Security Assessment**: PASS

- **Cardinality bounded**: Only `status` and `has_backup` labels (2 x 2 = 4 unique combinations max)
- **Metric naming**: Follows ADR-0011 convention (`gc_mh_selection_duration_seconds`, `gc_mh_selections_total`)
- **SLO alignment**: Duration histogram aligns with operational monitoring needs

---

## Complete File Review Summary

### Files Reviewed (All Iterations)

| File | Lines | Iteration | Status |
|------|-------|-----------|--------|
| `metrics.rs` | 431 | 1, 2 | PASS |
| `mc_assignment.rs` | 322 | 1 | PASS |
| `mh_selection.rs` | 321 | 2 (NEW) | PASS |
| `meeting_assignments.rs` | 776 | 1 | PASS |
| `meeting_controllers.rs` | 450 | 1 | PASS |
| `media_handlers.rs` | 476 | 1 | PASS |

### Security Controls Verified

| Control | Status | Notes |
|---------|--------|-------|
| No PII in metric labels | PASS | Only operational identifiers used |
| Cardinality bounded | PASS | Dynamic segments normalized; boolean/enum labels only |
| Error semantics preserved | PASS | Metrics recorded before error propagation |
| Privacy-by-default (`#[instrument(skip_all)]`) | PASS | All instrumented functions use skip_all |
| No unbounded string labels | PASS | All labels use fixed/bounded values |
| Status code properly categorized | PASS | Mapped to success/error/timeout |
| CSPRNG used for weighted selection | PASS | Uses `ring::rand::SystemRandom` |
| SQL injection prevention | PASS | All queries use parameterized statements |

---

## Findings

### No Security Vulnerabilities Identified

After thorough review of both Iteration 1 and Iteration 2 changes, no security vulnerabilities were identified.

### Tech Debt (Non-Blocking)

1. **TECH_DEBT**: Consider adding metrics cardinality monitoring
   - While the implementation correctly bounds cardinality, there's no runtime monitoring for label value explosion
   - Recommendation: Add a gauge `gc_metric_cardinality` to track unique label combinations
   - Reference: ADR-0011 Section 8 (Security Controls - Cardinality enforcement)

2. **TECH_DEBT**: Token refresh metrics require architectural changes (TD-GC-001)
   - Functions were correctly removed from metrics.rs
   - Implementation requires callback mechanism, feature flag, or metrics trait in common crate
   - Properly documented in code comments (lines 173-183)

---

## Verdict

**APPROVED**

The metric instrumentation implementation is secure across all iterations. Iteration 2 correctly:
- Added MH selection metrics with bounded cardinality labels
- Removed dead token refresh functions with proper documentation
- Maintained privacy-by-default patterns throughout

The implementation follows ADR-0011 requirements for privacy-by-default, properly bounds label cardinality to prevent DoS, and does not expose any PII through metrics.

---

## Finding Summary

| Severity | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0 |
| TECH_DEBT | 2 |

---

*Security Specialist Review Complete - Iteration 2*
