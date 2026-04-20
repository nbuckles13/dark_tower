# Metric Recording-Site Test Coverage Audit

**Date**: 2026-04-20
**Trigger**: MH WebTransport `accept_loop` gap (recorded in `docs/specialist-knowledge/observability/TODO.md`) appearing as a recurring pattern across devloops.
**Purpose**: Size the testability gap before debating a generalizable pattern.

## Summary

| | Count | % |
|---|---|---|
| Total recording sites (AC+GC+MC+MH) | 778 | 100% |
| Covered (unit + integration) | ~520 | 67% |
| Wrapper-only (wrapper unit-tested, call site not exercised end-to-end) | ~40 | 5% |
| Uncovered (no test reaches the code path) | ~218 | 28% |

The MH `accept_loop` gap is **not an outlier** — it is a representative case of a structural pattern present in every service.

## Per-service breakdown

### AC (Auth Controller) — 189 sites
- ~130 covered, ~18 wrapper-only, ~41 uncovered
- Concentrations:
  - `services/key_management_service.rs`, `services/registration_service.rs`, `handlers/admin_handler.rs` — error branches in admin/key rotation
  - `main.rs:114-117` — token refresh callback (closure inside `TokenManager::on_refresh`)
  - `record_audit_log_failure` (19 call sites) wrapper-only — wrapper tested, no test exercises the failure paths that call it

### GC (Global Controller) — 186 sites
- ~140 covered, ~25 wrapper-only, ~21 uncovered
- Concentrations:
  - `main.rs:127` — token refresh callback
  - `handlers/meetings.rs` — record_meeting_creation error paths (bad request / forbidden / internal); tests focus on happy path
  - `services/mh_selection.rs` — MH candidate filtering metrics (deeply nested in repo layer)
  - `grpc/auth_layer.rs:250` — `record_caller_type_rejected` wrapper-only

### MC (Meeting Controller) — 270 sites (highest)
- ~180 covered, ~25 wrapper-only, ~65 uncovered
- Concentrations:
  - `webtransport/server.rs:178,183,209` — connection-status counters (rejected/accepted/error). **Same `accept_loop` bypass as MH** — `join_tests.rs` calls `handle_connection` directly to assert on errors.
  - `grpc/mh_client.rs` — `record_mh_notification` retry-loop fire-and-forget spawns
  - `record_register_meeting` in MC→MH client

### MH (Media Handler) — 133 sites (smallest)
- ~95 covered, ~24 wrapper-only, ~14 uncovered
- Concentrations:
  - `webtransport/server.rs:174,179,205` — connection counters; `set_active_connections` at 202,217. Documented in `docs/specialist-knowledge/observability/TODO.md`.
  - `connection.rs:138` — handshake duration (only on success; early returns on timeout/failure don't record)

## Cross-cutting patterns

1. **Accept loops are systematically uncovered (MH + MC).** Same root cause in both services: production `accept_loop` spawns connection handlers as fire-and-forget tasks and drops `Result`. Tests need the `MhError`/`McError` variants, so they bypass the loop entirely (`WtRig` for MH, `join_tests.rs` for MC). The accept-loop metrics (rejected/accepted/error counters, active-connections gauge) are unreachable.

2. **Token manager refresh callbacks are untested (AC, GC, MC).** Each `main.rs` registers a closure with `TokenManager::on_refresh` that records refresh-success/failure metrics. No test drives a full token refresh cycle, so the callback's metric calls never fire in CI.

3. **Repository error branches have wrapper-only coverage (GC).** DB query metrics record on both success and error. Happy paths hit by integration tests; error cases need DB state corruption or mock failures and aren't exercised.

4. **Fire-and-forget spawns hide metrics (MC, MH).** Spawned task records a metric, returns `Err`, the `Err` is dropped — no test can observe the metric without scraping Prometheus.

5. **Capacity-rejection paths require load (MH, MC).** `record_*("rejected")` only fires when `max_connections` is hit; tests don't push load.

## Testability patterns that DO work today

- **Database query metrics** (GC repositories): direct call + mocked DB + read metric. ✓
- **gRPC auth validation** (all services): direct call to validator + wiremock JWKS + read counter. ✓
- **Single-function error handling**: direct call + injected error + read counter. ✓

## Testability patterns that DO NOT work today

- **Accept-loop metrics**: would require running the loop and accepting connections, OR scraping Prometheus. Current pattern: bypass.
- **Fire-and-forget spawn metrics**: same.
- **Startup/shutdown lifecycle metrics**: would require driving full lifecycle in test harness; current pattern: bypass.

## Candidate patterns for `/debate`

1. **Scrape Prometheus in integration tests** — non-invasive; tests get a metrics handle and assert on counter deltas. Cost: slower tests, exposes registry handle. Universal.
2. **Expose per-connection result channels on servers** — `WebTransportServer::with_result_rx()` etc. Tests get both `MhError` assertions AND real accept-loop metric coverage. Cost: API surface in production code. Per-server work.
3. **Move accept-loop metrics to connection startup** — record after `handle_connection` returns `Ok`, not before spawn. Cost: loses pre-handler accept stats. Cheapest.
4. **Defer to load tests / canaries** — accept that some sites are too structural to unit-test. Cost: metrics not validated on every CI run.

The choice should apply uniformly across all four services since the gap pattern is uniform.

## Open questions for the debate

- Is the 5% wrapper-only + 28% uncovered band acceptable in steady state, or do we want to drive uncovered toward zero?
- Do we tolerate per-service variation in the chosen pattern, or mandate uniformity?
- Do we add a metric-coverage guard (akin to `validate-application-metrics.sh`) that flags new uncovered sites going forward?
