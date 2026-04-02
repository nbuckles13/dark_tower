# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, Skaffold, zero-trust) → ADR-0012
- Local development environment → ADR-0013
- Environment integration tests → ADR-0014
- Guard pipeline methodology → ADR-0015
- Validation pipeline (CI gates) → ADR-0024
- Containerized devloop execution → ADR-0025
- Dashboard metric presentation (counters vs rates) → ADR-0029

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
- Alert rules → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Dev certs, master key, service registration → `scripts/generate-dev-certs.sh`, `generate-master-key.sh`, `register-service.sh`
- MC TLS secret (imperative, setup.sh) + UDP NodePort (30433) → `infra/services/mc-service/`

## Runbooks
- AC incident/deployment → `docs/runbooks/ac-service-incident-response.md`, `ac-service-deployment.md`
- GC incident/deployment → `docs/runbooks/gc-incident-response.md`, `gc-deployment.md`
- MC incident/deployment → `docs/runbooks/mc-incident-response.md`, `mc-deployment.md`

## Code Locations — Database & Migrations
- Participant tracking → `migrations/20260322000001_add_participant_tracking.sql`, `crates/gc-service/src/repositories/participants.rs`
- Meeting activation + audit → `crates/gc-service/src/repositories/meetings.rs`

## Code Locations — Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`; GC wrapper → `gc-service/src/auth/jwt.rs`; MC wrapper → `mc-service/src/auth/mod.rs`
- MC JWKS + TLS config → `crates/mc-service/src/config.rs` (`AC_JWKS_URL`, `MC_TLS_CERT_PATH`, `MC_TLS_KEY_PATH`)
- AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; Service auth → ADR-0003

## Code Locations — Observability
- Observability Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/`
- GC metrics → `crates/gc-service/src/observability/metrics.rs`; MC metrics → `crates/mc-service/src/observability/metrics.rs`
- MC dashboard + alerts → `infra/grafana/dashboards/mc-overview.json`, `docs/observability/alerts.md`
- Prometheus scrape config → `infra/docker/prometheus/prometheus.yml`

## Code Locations — MH Service (Stub)
- MH startup (bind-before-spawn, shutdown, GC registration) → `crates/mh-service/src/main.rs`
- MH config (`MH_` prefix, SecretString, TLS fail-fast) → `crates/mh-service/src/config.rs`
- MH health (`/health`, `/ready`, `/metrics` on port 8083) → `crates/mh-service/src/observability/health.rs`
- MH GC client (RegisterMH, SendLoadReport, NOT_FOUND re-reg) → `crates/mh-service/src/grpc/gc_client.rs`
- MH gRPC stubs → `crates/mh-service/src/grpc/mh_service.rs`
- MH auth interceptor → `crates/mh-service/src/grpc/auth_interceptor.rs`
- MH metrics + errors → `crates/mh-service/src/observability/metrics.rs`, `errors.rs`
- Port map: AC=8082, GC=8080/50051, MC=8081/50052/4433, MH=8083/50053/4434

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
- Cluster infra + join flow E2E → `crates/env-tests/src/cluster.rs`, `tests/24_join_flow.rs`
