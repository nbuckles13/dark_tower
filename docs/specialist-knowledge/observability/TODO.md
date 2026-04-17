# Observability TODOs

Follow-ups and known coverage gaps. INDEX.md is for navigation; this file is for gaps and deferred work.

## Coverage Gaps

- **MH `mh_webtransport_connections_total{status=accepted|rejected|error}` has no integration coverage.** Recording sites at `crates/mh-service/src/webtransport/server.rs:174,179,205` (inside `accept_loop`) are reachable only through the full `WebTransportServer::bind() -> accept_loop()` path. The MH integration WT rig at `crates/mh-service/tests/common/wt_rig.rs` bypasses `accept_loop` and calls `handle_connection` directly (documented justification at `wt_rig.rs:14-21` — `accept_loop` drops the per-connection `Result`, so tests cannot assert on `MhError` variants). Fix options: (a) add a test that runs the real `accept_loop` and scrapes the Prometheus handle after a handshake attempt, or (b) introduce a server hook that exposes per-connection results so a single rig can drive both accept-path metric assertions and `MhError` assertions. No unit tests exist in `webtransport/server.rs` either, so these counters are currently unverified end-to-end.
