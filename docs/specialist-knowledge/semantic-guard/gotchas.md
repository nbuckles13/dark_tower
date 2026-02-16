# Semantic Guard Gotchas

Pitfalls and edge cases discovered during semantic analysis of the Dark Tower codebase.

---

## Gotcha: MC and GC Use Different Status Code Domains
**Added**: 2026-02-16
**Related files**: `crates/mc-service/src/errors.rs`, `crates/gc-service/src/errors.rs`, `crates/mc-service/src/observability/metrics.rs`

MC uses WebTransport signaling codes (2-7) for its `status_code` metric label, while GC uses HTTP status codes (400, 401, 403, 404, 429, 500, 503). Both services have a `record_error()` function with an identical signature accepting `status_code: u16`, but the semantic domain differs. Tests that copy GC patterns into MC (or vice versa) can introduce misleading test values that pass compilation but contradict the metrics catalog. When reviewing `record_error()` calls or tests, always check which service is being modified and verify the status code values match that service's documented domain.

---
