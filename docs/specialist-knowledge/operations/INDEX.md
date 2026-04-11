# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, zero-trust) → ADR-0012; Local dev → ADR-0013
- Env integration tests → ADR-0014; Guard pipeline → ADR-0015; CI gates → ADR-0024
- Containerized devloop → ADR-0025; Host-side cluster helper (incl. cluster networking) → ADR-0030
- Dashboard metric presentation (counters vs rates) → ADR-0029

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`
- Guard runner → `scripts/guards/run-guards.sh`, common → `scripts/guards/common.sh`
- Kustomize guard → `scripts/guards/simple/validate-kustomize.sh`
- Application metrics guard → `scripts/guards/simple/validate-application-metrics.sh`

## Devloop Cluster Helper
- Kind config template (envsubst, host-gateway listenAddress) → `infra/kind/kind-config.yaml.tmpl`
- Devloop wrapper → `infra/devloop/devloop.sh`, container image → `infra/devloop/Dockerfile`
- Cluster networking debate → `docs/debates/2026-04-09-devloop-cluster-networking/debate.md`
- Sidecar design doc (superseded by ADR-0030) → `docs/debates/2026-04-05-devloop-cluster-sidecar.md`
- Host state directory → `~/.cache/devloop/` (port-registry.json, per-slug state)
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- URL parsing for health checks → `crates/env-tests/src/cluster.rs:parse_host_port()`

## Deployment & K8s
- Kind cluster: `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh` (ADR-0030: `load_image_to_kind()`, `deploy_only_service()`, --yes/--only/--skip-build), `infra/kind/scripts/teardown.sh`
- Kind overlay (per-service, observability) → `infra/kubernetes/overlays/kind/`
- Per-service Kustomize bases + manifests (statefulset/deployment, netpol, PDB) → `infra/services/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`
- Dockerfiles → `infra/docker/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`; PostgreSQL + Redis → `infra/services/postgres/`, `redis/`
- Dev certs → `scripts/generate-dev-certs.sh`; Alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`
- MC/MH: StatefulSets, per-pod NodePort Services (`statefulset.kubernetes.io/pod-name`), headless Service, TLS secrets (imperative via setup.sh)
- Per-pod UDP NodePorts: `base + ordinal*2` (MC: 4433/4435, MH: 4434/4436); cross-service netpol in `gc-service/network-policy.yaml`, `mc-service/network-policy.yaml`
- Downward API: `status.podIP` → `POD_IP`; WebTransport advertise from HOSTNAME ordinal
- Port map: AC=8082, GC=8080/50051, MC=8081/50052/4433, MH=8083/50053/4434; scaling requires per-pod Services + Kind port mappings

## Runbooks
- Per-service incident/deployment → `docs/runbooks/` (ac, gc, mc)

## Database & Migrations
- Participant tracking + meetings → `crates/gc-service/src/repositories/participants.rs`, `meetings.rs`

## Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`
- Shared GC↔AC token types → `crates/common/src/meeting_token.rs`
- AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; Service auth → ADR-0003

## Observability
- Observability Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/`; Alerts → `docs/observability/alerts.md`
- Per-service metrics → `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`; Prometheus → `infra/docker/prometheus/prometheus.yml`

## MH Service
- MH startup + config + health → `crates/mh-service/src/main.rs`, `crates/mh-service/src/config.rs`, `crates/mh-service/src/observability/health.rs`
- MH GC client → `crates/mh-service/src/grpc/gc_client.rs`
- MH gRPC + auth → `crates/mh-service/src/grpc/mh_service.rs`, `auth_interceptor.rs`

## MC WebTransport + Actors
- MC WebTransport → `crates/mc-service/src/webtransport/server.rs`, `crates/mc-service/src/webtransport/connection.rs`
- MC startup → `crates/mc-service/src/main.rs`
- Actors → `crates/mc-service/src/actors/controller.rs`, `crates/mc-service/src/actors/meeting.rs`, `crates/mc-service/src/actors/participant.rs`

## GC Service
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `crates/gc-service/src/handlers/meetings.rs`

## Tests
- MC join tests → `crates/mc-service/tests/join_tests.rs`; TestKeypair (Ed25519 + JWKS mock) → `crates/mc-test-utils/src/jwt_test.rs`
- GC join tests → `crates/gc-service/tests/meeting_tests.rs`; Env-tests (Kind) → `crates/env-tests/`