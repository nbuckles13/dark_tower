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
- Cluster networking debate → `docs/debates/2026-04-09-devloop-cluster-networking/debate.md`; sidecar (superseded) → `docs/debates/2026-04-05-devloop-cluster-sidecar.md`
- Helper commands (setup, deploy, rebuild, teardown, status) → `crates/devloop-helper/src/commands.rs`
- Helper protocol (command parsing, streaming) → `crates/devloop-helper/src/protocol.rs`
- Status command (cluster health, pod readiness, setup-in-progress) → `crates/devloop-helper/src/commands.rs:cmd_status()`
- Pod health parsing → `crates/devloop-helper/src/commands.rs:parse_pod_health()`
- Port-map.env generation (MC/MH WebTransport ports) → `crates/devloop-helper/src/commands.rs:write_port_map_shell()`
- DT_HOST_GATEWAY_IP propagation → `crates/devloop-helper/src/commands.rs:cmd_setup()`, `cmd_deploy()`
- Port registry → `~/.cache/devloop/port-registry.json` (global allocation state)
- Per-devloop runtime state → `/tmp/devloop-{slug}/` (PID, socket, auth token, ports.json, setup.pid, eager-setup.log)
- Container-side client → `infra/devloop/dev-cluster` (setup, rebuild, deploy, teardown, status)
- Health check + eager setup → `infra/devloop/devloop.sh` (Infrastructure health check section)
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`; Layer 8 → `.claude/skills/devloop/SKILL.md`

## Deployment & K8s
- Kind cluster: `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh` (ADR-0030: `load_image_to_kind()`, `deploy_only_service()`, --yes/--only/--skip-build), `infra/kind/scripts/teardown.sh`
- Kind overlay (per-service, observability) → `infra/kubernetes/overlays/kind/`
- Per-service Kustomize bases + manifests (statefulset/deployment, netpol, PDB) → `infra/services/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`
- Dockerfiles → `infra/docker/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`; PostgreSQL + Redis → `infra/services/postgres/`, `redis/`
- Dev certs → `scripts/generate-dev-certs.sh`; Alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`
- MC/MH: per-instance Deployments, per-pod NodePort Services, TLS secrets (imperative via setup.sh)
- MC/MH per-instance ConfigMaps (advertise addresses) → `infra/services/mc-service/mc-{0,1}-configmap.yaml`, `mh-service/mh-{0,1}-configmap.yaml`
- Devloop ConfigMap patching (advertise addresses) → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()` (gated on `DT_HOST_GATEWAY_IP`)
- DT_HOST_GATEWAY_IP validation → `infra/kind/scripts/setup.sh` (after DT_PORT_MAP sourcing)
- Per-pod UDP NodePorts: `base + ordinal*2` (MC: 4433/4435, MH: 4434/4436); cross-service netpol in `gc-service/network-policy.yaml`, `mc-service/network-policy.yaml`, `mh-service/network-policy.yaml`
- Downward API: `status.podIP` → `POD_IP`; WebTransport advertise from per-instance ConfigMap
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
- MH gRPC + JWKS auth layer → `crates/mh-service/src/grpc/mh_service.rs`, `auth_interceptor.rs`; JWT validation → `crates/mh-service/src/auth/mod.rs`
- MH WebTransport + session mgmt → `crates/mh-service/src/webtransport/server.rs`, `connection.rs`, `crates/mh-service/src/session/mod.rs`

## MC WebTransport + Actors
- MC WebTransport → `crates/mc-service/src/webtransport/server.rs`, `crates/mc-service/src/webtransport/connection.rs`
- MC startup → `crates/mc-service/src/main.rs`
- Actors → `crates/mc-service/src/actors/controller.rs`, `crates/mc-service/src/actors/meeting.rs`, `crates/mc-service/src/actors/participant.rs`

## GC Service
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `crates/gc-service/src/handlers/meetings.rs`

## Tests
- MC join tests → `crates/mc-service/tests/join_tests.rs`; TestKeypair (Ed25519 + JWKS mock) → `crates/mc-test-utils/src/jwt_test.rs`
- GC join tests → `crates/gc-service/tests/meeting_tests.rs`; Env-tests (Kind) → `crates/env-tests/`