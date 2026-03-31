# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, Skaffold, zero-trust) → ADR-0012
- Local development environment → ADR-0013
- Environment integration tests → ADR-0014
- Guard pipeline methodology → ADR-0015
- Validation pipeline (CI gates) → ADR-0024
- Containerized devloop execution → ADR-0025

## Code Locations — CI & Guards
- CI pipeline → `.github/workflows/ci.yml`
- Guard runner → `scripts/guards/run-guards.sh`, common library → `scripts/guards/common.sh`
- Kustomize guard (R-15–R-20: build, orphans, kubeconform, secctx, secrets, dashboards) → `scripts/guards/simple/validate-kustomize.sh`
- Application metrics guard → `scripts/guards/simple/validate-application-metrics.sh`

## Code Locations — Deployment & K8s
- Kind cluster config + setup script → `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh`
- Kind overlay (top-level, per-service, observability) → `infra/kubernetes/overlays/kind/`
- Per-service Kustomize bases → `infra/services/{ac,gc,mc}-service/kustomization.yaml`
- Per-service manifests (deployment, netpol, PDB) → `infra/services/{ac,gc,mc}-service/`
- PostgreSQL + Redis Kustomize bases → `infra/services/postgres/`, `infra/services/redis/`
- Alert rules (MC join: failure rate, WT rejections, JWT failures, latency) → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Dev certs, master key, service registration → `scripts/generate-dev-certs.sh`, `generate-master-key.sh`, `register-service.sh`
- MC TLS secret (imperative, setup.sh) + UDP NodePort (30433) → `infra/services/mc-service/`

## Runbooks
- AC incident/deployment → `docs/runbooks/ac-service-incident-response.md`, `ac-service-deployment.md`
- GC incident/deployment → `docs/runbooks/gc-incident-response.md`, `gc-deployment.md`
- MC incident/deployment → `docs/runbooks/mc-incident-response.md`, `mc-deployment.md`

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
- Observability Kustomize base → `infra/kubernetes/observability/kustomization.yaml`
- Grafana manifests + dashboard configMapGenerator → `infra/kubernetes/observability/grafana/`
- Grafana dashboard JSON files → `infra/grafana/dashboards/`
- GC metrics + catalog + dashboard → `crates/gc-service/src/observability/metrics.rs`, `docs/observability/metrics/gc-service.md`, `infra/grafana/dashboards/gc-overview.json`
- MC metrics + catalog + join metrics (WT, JWT, session) → `crates/mc-service/src/observability/metrics.rs`, `docs/observability/metrics/mc-service.md`
- MC dashboard (Join Flow row) + alerts → `infra/grafana/dashboards/mc-overview.json`, `docs/observability/alerts.md`
- Prometheus scrape config → `infra/docker/prometheus/prometheus.yml`

## Code Locations — MC WebTransport + Actors
- WT server (bind, accept_loop, max_connections) → `crates/mc-service/src/webtransport/server.rs`
- WT connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs`
- Protobuf encoding utilities → `crates/mc-service/src/webtransport/handler.rs`
- MC startup (bind-before-spawn, shutdown chain) → `crates/mc-service/src/main.rs`
- Actors: controller → `actors/controller.rs`, meeting → `actors/meeting.rs`, participant → `actors/participant.rs`

## Code Locations — MC Join Integration Tests
- MC join tests (11 tests: JWT, signaling, bridge) → `crates/mc-service/tests/join_tests.rs`
  - CI-safe: self-signed TLS, wiremock JWKS, `dangerous-configuration` in `[dev-dependencies]` only
- TestKeypair (Ed25519 signing + JWKS mock) → `crates/mc-test-utils/src/jwt_test.rs`

## Code Locations — GC
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `crates/gc-service/src/handlers/meetings.rs`
- GC join tests (R-18: auth, AC-down, no-MC, success) → `crates/gc-service/tests/meeting_tests.rs`

## Code Locations — Env-Tests (Kind Cluster)
- Cluster infra (ports, health) → `crates/env-tests/src/cluster.rs` (AC 8082, GC 8080, MC WT 4433/UDP 30433)
- Join flow E2E (Tier 1 GC + Tier 2 MC WT) → `crates/env-tests/tests/24_join_flow.rs`
- Rate limit: AC caps 5 registrations/IP/hour — all env-tests share 127.0.0.1 via port-forward
- TLS: `with_no_cert_validation()` for dev certs; Feature gates: `--features flows` for join tests
