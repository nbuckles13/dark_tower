# DRY Review: Wire GC Observability Metrics

**Task**: Wire GC observability metrics - instrument MC assignment, DB queries, and token refresh code paths.
**Date**: 2026-02-09
**Reviewer**: DRY Reviewer Specialist

## Files Reviewed

### Modified Files (Global Controller)
| File | Methods Instrumented |
|------|---------------------|
| `crates/global-controller/src/services/mc_assignment.rs` | 1 (`assign_meeting_with_mh`) |
| `crates/global-controller/src/repositories/meeting_assignments.rs` | 7 (`get_healthy_assignment`, `get_candidate_mcs`, `atomic_assign`, `get_current_assignment`, `end_assignment`, `end_stale_assignments`, `cleanup_old_assignments`) |
| `crates/global-controller/src/repositories/meeting_controllers.rs` | 4 (`register_mc`, `update_heartbeat`, `mark_stale_controllers_unhealthy`, `get_controller`) |
| `crates/global-controller/src/repositories/media_handlers.rs` | 5 (`register_mh`, `update_load_report`, `mark_stale_handlers_unhealthy`, `get_candidate_mhs`, `get_handler`) |
| `crates/global-controller/src/observability/metrics.rs` | N/A (metric definitions) |

### Cross-Service Check
| Service | Location | Checked |
|---------|----------|---------|
| AC Service | `crates/ac-service/` | Yes |
| Meeting Controller | `crates/meeting-controller/` | Yes |
| Common | `crates/common/` | Yes |

## Findings

### BLOCKER (blocks approval): 0

No code exists in `crates/common/` that should have been imported. Each service correctly maintains its own metrics module with service-specific prefixes as required by ADR-0011.

### TECH_DEBT (document for future): 0

**Pattern Analysis**:

1. **Metrics Modules per Service (Acceptable)**:
   - AC: `crates/ac-service/src/observability/metrics.rs` with `ac_` prefix
   - GC: `crates/global-controller/src/observability/metrics.rs` with `gc_` prefix
   - MC: `crates/meeting-controller/src/actors/metrics.rs` with `mc_` prefix

   Each service requires its own metrics module per ADR-0011 naming conventions. The metric prefixes (`ac_`, `gc_`, `mc_`) are deliberately different for Prometheus cardinality and dashboard organization.

2. **Similar `record_db_query` Functions (Not Tech Debt)**:
   - AC: `record_db_query(operation, table, status, duration)` - 4 params
   - GC: `record_db_query(operation, status, duration)` - 3 params

   The signatures differ because AC tracks per-table metrics while GC tracks per-operation. Extracting to common would require either:
   - A generic signature that both services adapt to (over-engineering)
   - Separate functions anyway

   **Decision**: Not tech debt. Service-specific metric labels are intentional.

3. **Timing Pattern (`Instant::now()` + `elapsed()`)**:

   Both services use:
   ```rust
   let start = Instant::now();
   // ... operation ...
   metrics::record_*(status, start.elapsed());
   ```

   This is idiomatic Rust timing. Creating a shared abstraction would add complexity without benefit. The pattern is 3 lines and extremely readable.

   **Decision**: Not tech debt. Standard Rust idiom.

4. **Status Determination Logic**:

   Both services use match patterns to categorize results as "success"/"error". These are inline and context-specific.

   **Decision**: Not tech debt. Domain-specific logic.

## Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 0
checkpoint_exists: true
summary: No duplication issues found. GC metrics instrumentation follows ADR-0011 patterns correctly. Each service maintains its own metrics module with service-specific prefixes (ac_, gc_, mc_) as designed. The record_db_query function signatures intentionally differ between services due to different labeling requirements. No shared code exists in common that was ignored.
```

## Patterns Verified

1. **ADR-0011 Compliance**: GC metrics use `gc_` prefix throughout
2. **Histogram + Counter Pattern**: All instrumentation records both duration (histogram) and count (counter)
3. **SLO Alignment**: Comments reference ADR-0010/ADR-0011 SLO targets
4. **Cardinality Bounded**: Labels are bounded (status: success/error, operation: finite set)
5. **No Common Extraction Needed**: Services have different metric requirements

## Recommendation

Proceed with implementation. The metrics instrumentation follows established patterns without introducing problematic duplication. The intentional service separation aligns with ADR-0011's multi-service observability architecture.

---

## Iteration 2 Review (2026-02-09)

### Files Reviewed in Iteration 2

| File | Change Type | Description |
|------|-------------|-------------|
| `crates/global-controller/src/services/mh_selection.rs` | NEW metrics | Added `record_mh_selection` calls for MH selection timing |
| `crates/global-controller/src/observability/metrics.rs` | Token refresh removed | Removed `record_token_refresh*` functions (see tech debt note) |

### Iteration 2 Findings

#### BLOCKER: 0

No code exists in `crates/common/` that was ignored:

1. **CSPRNG Usage**: The `mh_selection.rs` uses `ring::rand::SystemRandom` directly. Checked `crates/common/src/` - no shared CSPRNG utilities exist. This is appropriate since CSPRNG usage is localized to MH selection.

2. **Weighted Random Selection**: The `weighted_random_select` function is unique to MH selection. No similar utility exists in common. The algorithm is specific to load-ratio-based selection and doesn't generalize.

3. **Metrics**: The `record_mh_selection` function follows GC metrics patterns with `gc_` prefix. No metrics infrastructure exists in common (confirmed by grep). Each service maintains its own metrics per ADR-0011.

#### TECH_DEBT: 0

**Analysis of New Code**:

1. **`record_mh_selection` Function**:
   - Uses same pattern as `record_mc_assignment`: histogram + counter
   - Labels: `status`, `has_backup` - bounded cardinality
   - No duplication with other services (unique to GC)
   - **Decision**: Not tech debt

2. **Token Refresh Metrics Removal**:
   - Correctly removed with explanatory comment (lines 174-183)
   - References tech debt TD-GC-001 for tracking
   - Architectural constraint documented (TokenManager in common crate)
   - **Decision**: Not tech debt (properly documented limitation)

3. **MH Selection Service Instrumentation**:
   - Two `record_mh_selection` calls: error path (line 76), success path (line 136)
   - Timing uses standard `Instant::now()` + `elapsed()` pattern
   - **Decision**: Standard idiom, not tech debt

### Iteration 2 Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 0
checkpoint_exists: true
summary: Iteration 2 changes reviewed. New MH selection metrics follow established patterns. CSPRNG and weighted selection logic are appropriately localized (no common utilities exist). Token refresh metrics correctly removed with tech debt documentation. No duplication issues found.
```

### Patterns Verified (Iteration 2)

1. **MH Selection Metrics**: `gc_mh_selection_*` follows naming conventions
2. **Error Path Instrumented**: Both success and error paths record metrics
3. **has_backup Label**: Useful for monitoring backup availability rates
4. **No Common Code Ignored**: Confirmed no CSPRNG or weighted selection utilities exist in common
