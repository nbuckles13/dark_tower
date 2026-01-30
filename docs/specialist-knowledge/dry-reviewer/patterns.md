# DRY Reviewer - Patterns That Work

This file captures successful patterns and approaches discovered during DRY reviews.

---

## Architectural Alignment vs. Harmful Duplication

**Added**: 2026-01-29
**Related files**: `crates/env-tests/src/cluster.rs`, `crates/ac-service/src/repositories/*.rs`, `crates/global-controller/src/services/*.rs`

**Pattern**: The `.map_err(|e| ErrorType::Variant(format!("context: {}", e)))` error preservation pattern appears across all services (AC, MC, GC, env-tests) with 40+ instances. This is **healthy architectural alignment**, NOT harmful duplication requiring extraction. Each crate should define its own domain-specific error types (`AcError`, `GcError`, `ClusterError`) while following the same error preservation convention. Extracting this to a macro or shared utility would add complexity without reducing maintenance burden.

**Classification per ADR-0019**: Healthy pattern replication (following a convention) vs. harmful duplication (copy-paste code needing extraction).

---
