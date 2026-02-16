# Semantic Guard Patterns

Successful approaches and patterns for semantic analysis in the Dark Tower codebase.

---

## Pattern: TokenRefreshCallback Safety Verification
**Added**: 2026-02-16
**Related files**: `crates/common/src/token_manager.rs`, `crates/mc-service/src/main.rs`, `crates/gc-service/src/main.rs`

When reviewing `with_on_refresh` callback wiring, verify the full data flow from `TokenRefreshEvent` through to metric recording. The `TokenRefreshEvent` struct is the security boundary -- it deliberately excludes tokens and secrets, exposing only `success: bool`, `duration: Duration`, and `error_category: Option<&'static str>`. As long as the callback only passes these fields to metrics functions, credential leaks are structurally impossible. Check `crates/common/src/token_manager.rs:131-143` to confirm the struct definition hasn't changed.

---

## Pattern: Cross-Service Metric Pattern Comparison
**Added**: 2026-02-16
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`

When a new service adds metrics that mirror an existing service (e.g., MC adding `record_token_refresh` after GC), compare the implementations side-by-side. Check: (1) function signatures match, (2) metric name prefixes differ correctly (gc_ vs mc_), (3) histogram buckets align where SLOs are shared, (4) test values use the correct service's domain (HTTP codes for GC, signaling codes for MC). The implementations should be structurally identical with only the prefix differing.

---
