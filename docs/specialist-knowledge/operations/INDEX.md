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
- Kind cluster config (TCP/UDP port mappings) → `infra/kind/kind-config.yaml`
- Kind setup/iterate/teardown → `infra/kind/scripts/`
- Per-service manifests (deployment, netpol, PDB) → `infra/services/{ac,gc,mc}-service/`
- Alert rules → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
  - MC join: MCHighJoinFailureRate (>5%), MCHighWebTransportRejections (>10%), MCHighJwtValidationFailures (>10%), MCHighJoinLatency (p95 >2s info)
- Dev certs (CA + service TLS), master key, service registration → `scripts/generate-dev-certs.sh`, `generate-master-key.sh`, `register-service.sh`
- MC TLS secret + volume mount → `infra/services/mc-service/tls-secret.yaml`, `deployment.yaml` (volume `mc-tls` at `/etc/mc-tls`)
- MC WebTransport UDP NodePort + Kind mapping → `infra/services/mc-service/service.yaml` (30433), `infra/kind/kind-config.yaml` (hostPort 4433)
- MC NetworkPolicy (UDP 4433 ingress) → `infra/services/mc-service/network-policy.yaml`

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
- GC metrics + catalog + dashboard (incl. join panels) → `crates/gc-service/src/observability/metrics.rs`, `docs/observability/metrics/gc-service.md`, `infra/grafana/dashboards/gc-overview.json`
- MC metrics recorder + catalog → `crates/mc-service/src/observability/metrics.rs`, `docs/observability/metrics/mc-service.md`
- MC join flow metrics (WT connections, JWT, session join) → `crates/mc-service/src/observability/metrics.rs:record_webtransport_connection()`, `record_jwt_validation()`, `record_session_join()`
- MC overview dashboard (incl. Join Flow row, panels 26-30) + alert catalog → `infra/grafana/dashboards/mc-overview.json`, `docs/observability/alerts.md`
- MC health probes (Phase 6h, commented out) → `infra/services/mc-service/deployment.yaml:120`
- Prometheus scrape config (all services) → `infra/docker/prometheus/prometheus.yml`

## Code Locations — MC WebTransport Server
- WebTransport server (bind, accept_loop, max_connections guard) → `crates/mc-service/src/webtransport/server.rs`
- WebTransport connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs`
- Protobuf encoding utilities (encode_participant_update) → `crates/mc-service/src/webtransport/handler.rs`
- MC startup (bind-before-spawn, shutdown token chain) → `crates/mc-service/src/main.rs`

## Code Locations — MC Actor System
- Controller actor (root, capacity, join_connection) → `crates/mc-service/src/actors/controller.rs`
- Meeting actor (participants, grace period, child tokens for participants) → `crates/mc-service/src/actors/meeting.rs`
- Participant actor (per-participant, stream_tx, meeting disconnect notify) → `crates/mc-service/src/actors/participant.rs`
- Actor metrics + mailbox monitoring → `crates/mc-service/src/actors/metrics.rs`
- Session binding tokens → `crates/mc-service/src/actors/session.rs`

## Code Locations — GC Routes & Integration
- GC route definitions (public, user-auth, service-auth) → `crates/gc-service/src/routes/mod.rs`
- Meeting handlers (join, guest-token, settings, create) → `crates/gc-service/src/handlers/meetings.rs`
- Env-tests → `crates/env-tests/`
