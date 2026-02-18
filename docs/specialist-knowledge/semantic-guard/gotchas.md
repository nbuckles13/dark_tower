# Semantic Guard Gotchas

Pitfalls and edge cases discovered during semantic analysis of the Dark Tower codebase.

---

## Gotcha: MC and GC Use Different Status Code Domains
**Added**: 2026-02-16
**Related files**: `crates/mc-service/src/errors.rs`, `crates/gc-service/src/errors.rs`, `crates/mc-service/src/observability/metrics.rs`

MC uses WebTransport signaling codes (2-7) for its `status_code` metric label, while GC uses HTTP status codes (400, 401, 403, 404, 429, 500, 503). Both services have a `record_error()` function with an identical signature accepting `status_code: u16`, but the semantic domain differs. Tests that copy GC patterns into MC (or vice versa) can introduce misleading test values that pass compilation but contradict the metrics catalog. When reviewing `record_error()` calls or tests, always check which service is being modified and verify the status code values match that service's documented domain.

---

## Gotcha: Integration Tests Can Drift from Dedicated Tests on Domain Values
**Added**: 2026-02-18
**Related files**: `crates/mc-service/src/observability/metrics.rs`

Prometheus integration tests (e.g., `test_prometheus_metrics_endpoint_integration`) exercise all recording functions to verify the recorder captures data, but they don't validate label values. This means they can use incorrect domain values (like HTTP `500` instead of signaling code `6`) without failing. Dedicated unit tests (e.g., `test_record_error`) may use correct values, creating a split where the integration test contradicts the unit test. When reviewing, cross-check metric arguments in integration tests against the dedicated unit tests and the metrics catalog -- don't assume the integration test's values are authoritative just because they compile and pass.

---

## Gotcha: Bash Arithmetic Under `set -e` Exits on Zero Increment
**Added**: 2026-02-18
**Related files**: `scripts/guards/simple/validate-application-metrics.sh`

In bash with `set -e`, `((errors++))` exits the script when `errors` is `0` because the arithmetic expression evaluates to `0` (falsy). The project convention is `((errors++)) || true` to suppress this. When reviewing guard scripts that count errors/warnings, verify all `((...++))` expressions have `|| true`. This has caused false-pass guard results in the past when `set -e` aborted the script before reaching the exit code check.

---
