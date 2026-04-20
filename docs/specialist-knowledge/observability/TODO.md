# Observability TODOs

Follow-ups and known coverage gaps. INDEX.md is for navigation; this file is for gaps and deferred work.

## Coverage Gaps

- **MH `mh_webtransport_connections_total{status=accepted|rejected|error}` has no integration coverage.** Recording sites at `crates/mh-service/src/webtransport/server.rs:174,179,205` (inside `accept_loop`) are reachable only through the full `WebTransportServer::bind() -> accept_loop()` path. The MH integration WT rig at `crates/mh-service/tests/common/wt_rig.rs` bypasses `accept_loop` and calls `handle_connection` directly (documented justification at `wt_rig.rs:14-21` — `accept_loop` drops the per-connection `Result`, so tests cannot assert on `MhError` variants). **Resolution: ADR-0032** — Tier C `TestHooks` struct with `ConnectionOutcome` result channel lets the rig drive the real `accept_loop` and assert both handler errors and counter deltas (via Tier B `MetricAssertion`). Gauge state (`mh_active_connections` at `:202,217`) handled via Tier D recorder-snapshot reads after receiving `ConnectionOutcome`, relying on spawn-cleanup ordering rule. MH owns canonical-case first landing per ADR-0032 §Implementation Notes phasing step 2.
