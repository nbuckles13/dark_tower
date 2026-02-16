# Semantic Guard Integration Notes

How the semantic guard interacts with other components and specialists in Dark Tower.

---

## Integration: Metrics Catalog as Source of Truth for Label Validation
**Added**: 2026-02-16
**Related files**: `docs/observability/metrics/mc.md`, `docs/observability/metrics/gc.md`

The metrics catalog docs (`docs/observability/metrics/{service}.md`) define the contract for label values, cardinality bounds, and value domains. When reviewing metric recording code or tests, cross-reference these catalogs to verify that label values used in code/tests match the documented bounded sets. The catalog is the source of truth -- if code and catalog disagree, flag it.

---
