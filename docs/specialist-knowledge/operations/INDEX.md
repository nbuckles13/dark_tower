# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, Skaffold, zero-trust) → ADR-0012
- Local development environment → ADR-0013
- Environment integration tests → ADR-0014
- Guard pipeline methodology → ADR-0015
- Validation pipeline (CI gates) → ADR-0024 (Section: Validation Pipeline)
- Containerized devloop execution → ADR-0025
- Client architecture (CI workflow, deployment, canary) → ADR-0028

## Code Locations — CI & Guards
- CI pipeline → `.github/workflows/ci.yml`
- Guard runner + application metrics guard → `scripts/guards/run-guards.sh`, `scripts/guards/simple/validate-application-metrics.sh`

## Code Locations — Deployment & K8s
- Kind cluster config + scripts → `infra/kind/kind-config.yaml`, `infra/kind/scripts/`
- Per-service manifests (deployment, netpol, PDB) → `infra/services/{ac,gc,mc}-service/`
- Alert rules (MC join: failure rate, WT rejections, JWT failures, latency) → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Dev certs, master key, service registration → `scripts/generate-dev-certs.sh`, `generate-master-key.sh`, `register-service.sh`
- MC TLS secret + volume mount → `infra/services/mc-service/tls-secret.yaml`, `deployment.yaml`
- MC WebTransport UDP NodePort (30433) + NetworkPolicy → `infra/services/mc-service/service.yaml`, `network-policy.yaml`

## Runbooks
- GC incident response (Scenarios 1-9) → `docs/runbooks/gc-incident-response.md`
- GC deployment, rollback, smoke tests → `docs/runbooks/gc-deployment.md`
- MC incident response (Scenarios 1-7; 8-10 pending task 17) → `docs/runbooks/mc-incident-response.md`
- MC deployment, rollback, smoke tests → `docs/runbooks/mc-deployment.md`
- AC incident response → `docs/runbooks/ac-service-incident-response.md`
- AC deployment → `docs/runbooks/ac-service-deployment.md`

## Code Locations — Database & Migrations
- Participant tracking migration → `migrations/20260322000001_add_participant_tracking.sql`
- ParticipantsRepository → `crates/gc-service/src/repositories/participants.rs`
- Meeting activation (scheduled→active) → `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`
- Audit event logging + updated_at trigger → `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`

## Code Locations — Auth & JWT
- Common: JWKS client, JWT validator, claims types, JwtError → `crates/common/src/jwt.rs`
- GC thin wrapper (JwtError→GcError) → `crates/gc-service/src/auth/jwt.rs`, `crates/gc-service/src/errors.rs`
- MC thin wrapper (JwtError→McError) → `crates/mc-service/src/auth/mod.rs`, `crates/mc-service/src/errors.rs`
- MC JWKS config (`AC_JWKS_URL`, required) → `crates/mc-service/src/config.rs:ac_jwks_url`
- MC TLS config (`MC_TLS_CERT_PATH`, `MC_TLS_KEY_PATH`, required + file-exists) → `crates/mc-service/src/config.rs:tls_cert_path`
- Service auth design → ADR-0003

## Code Locations — Observability
- GC metrics + catalog + dashboard → `crates/gc-service/src/observability/metrics.rs`, `docs/observability/metrics/gc-service.md`, `infra/grafana/dashboards/gc-overview.json`
- MC metrics + catalog → `crates/mc-service/src/observability/metrics.rs`, `docs/observability/metrics/mc-service.md`
- MC join metrics (WT, JWT, session) → `crates/mc-service/src/observability/metrics.rs`
- MC dashboard (Join Flow row, panels 26-30) + alerts → `infra/grafana/dashboards/mc-overview.json`, `docs/observability/alerts.md`
- Prometheus scrape config → `infra/docker/prometheus/prometheus.yml`

## Code Locations — MC WebTransport Server
- WebTransport server (bind, accept_loop, max_connections guard) → `crates/mc-service/src/webtransport/server.rs`
- WebTransport connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs`
- Protobuf encoding utilities (encode_participant_update) → `crates/mc-service/src/webtransport/handler.rs`
- MC startup (bind-before-spawn, shutdown token chain) → `crates/mc-service/src/main.rs`

## Code Locations — MC Actor System
- Controller actor (root, capacity, join_connection) → `crates/mc-service/src/actors/controller.rs`
- Meeting actor (participants, grace period) → `crates/mc-service/src/actors/meeting.rs`
- Participant actor (stream_tx, disconnect notify) → `crates/mc-service/src/actors/participant.rs`
- Actor metrics → `crates/mc-service/src/actors/metrics.rs`

## Code Locations — MC Join Integration Tests
- MC join tests (11 tests: JWT, signaling, bridge) → `crates/mc-service/tests/join_tests.rs`
  - CI-safe: self-signed TLS (`Identity::self_signed`), wiremock JWKS, `127.0.0.1:0` ports, no external deps
  - `dangerous-configuration` feature scoped to `[dev-dependencies]` only
- TestKeypair (Ed25519 signing + JWKS mock) → `crates/mc-test-utils/src/jwt_test.rs` (reusable)
- Bug fix: `send_error()` stream.finish() flush → `crates/mc-service/src/webtransport/connection.rs:543`

## Code Locations — GC
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `crates/gc-service/src/handlers/meetings.rs`
- GC join tests (R-18: auth, AC-down, no-MC, success) → `crates/gc-service/tests/meeting_tests.rs`
- Env-tests → `crates/env-tests/`
