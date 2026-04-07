# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, Skaffold, zero-trust) → ADR-0012
- Local development environment → ADR-0013
- Environment integration tests → ADR-0014
- Guard pipeline methodology → ADR-0015
- Validation pipeline (CI gates) → ADR-0024
- Containerized devloop execution → ADR-0025
- Dashboard metric presentation (counters vs rates) → ADR-0029
- Host-side cluster helper for integration testing → ADR-0030

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`
- Guard runner → `scripts/guards/run-guards.sh`, common → `scripts/guards/common.sh`
- Kustomize guard → `scripts/guards/simple/validate-kustomize.sh`
- Application metrics guard → `scripts/guards/simple/validate-application-metrics.sh`

## Devloop Cluster Helper
- Cluster helper binary (new) → `crates/devloop-helper/src/main.rs`
- Dev-cluster client CLI (new) → `infra/devloop/dev-cluster`
- Kind config template (new) → `infra/kind/kind-config.yaml.tmpl`
- Devloop wrapper → `infra/devloop/devloop.sh`, container image → `infra/devloop/Dockerfile`
- Host state directory → `~/.cache/devloop/` (port-registry.json, per-slug state)
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- URL parsing for health checks → `crates/env-tests/src/cluster.rs:parse_host_port()`

## Deployment & K8s
- Kind cluster config + setup script → `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh`
- Kind cluster teardown → `infra/kind/scripts/teardown.sh`
- Kind overlay (top-level, per-service, observability) → `infra/kubernetes/overlays/kind/`
- Per-service Kustomize bases → `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- Per-service manifests (statefulset/deployment, netpol, PDB) → `infra/services/{ac,gc,mc,mh}-service/`
- Dockerfiles → `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`
- PostgreSQL + Redis → `infra/services/postgres/`, `infra/services/redis/`
- Alert rules → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Dev certs (AC, MC, MH WebTransport) → `scripts/generate-dev-certs.sh`
- MC/MH TLS secrets (imperative, setup.sh); per-pod UDP NodePorts via Kind port formula: `base + ordinal*2` (MC: 4433/4435, MH: 4434/4436) in `kind-config.yaml`
- setup.sh parameterization (ADR-0030) → `load_image_to_kind()`, `deploy_only_service()`, DT_CLUSTER_NAME/DT_PORT_MAP, --yes/--only/--skip-build
- Cross-service netpol: GC allows MH on 50051, MC allows MH on 50053 → `gc-service/network-policy.yaml`, `mc-service/network-policy.yaml`
- MC/MH are StatefulSets with per-pod NodePort Services (`statefulset.kubernetes.io/pod-name` selector) + headless Service (`clusterIP: None`)
- Downward API: `status.podIP` → `POD_IP` for gRPC advertise; WebTransport advertise computed from HOSTNAME ordinal via `*_WEBTRANSPORT_ADVERTISE_HOST` configmap key
- Port map: AC=8082, GC=8080/50051, MC=8081/50052/4433, MH=8083/50053/4434
- Scaling replicas requires: add/remove per-pod Services, update Kind port mappings (no code changes needed)

## Runbooks
- Per-service incident/deployment → `docs/runbooks/` (ac, gc, mc)

## Database & Migrations
- Participant tracking + meetings → `crates/gc-service/src/repositories/participants.rs`, `meetings.rs`

## Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`
- Shared GC↔AC token types → `crates/common/src/meeting_token.rs`
- AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; Service auth → ADR-0003

## Observability
- Observability Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/`
- Per-service metrics → `crates/{gc,mc,mh}-service/src/observability/metrics.rs`
- Prometheus scrape config → `infra/docker/prometheus/prometheus.yml`

## MH Service
- MH startup + config + health → `crates/mh-service/src/{main,config,observability/health}.rs`
- MH GC client → `crates/mh-service/src/grpc/gc_client.rs`
- MH gRPC + auth → `crates/mh-service/src/grpc/mh_service.rs`, `auth_interceptor.rs`

## MC WebTransport + Actors
- MC WebTransport → `crates/mc-service/src/webtransport/{server,connection}.rs`
- MC startup → `crates/mc-service/src/main.rs`
- Actors → `crates/mc-service/src/actors/{controller,meeting,participant}.rs`

## GC Service
- GC routes + handlers → `crates/gc-service/src/{routes/mod,handlers/meetings}.rs`

## Tests
- MC join tests → `crates/mc-service/tests/join_tests.rs`
- TestKeypair → `crates/mc-test-utils/src/jwt_test.rs`
- GC join tests → `crates/gc-service/tests/meeting_tests.rs`
- Env-tests (Kind) → `crates/env-tests/`