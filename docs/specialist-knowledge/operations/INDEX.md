# Operations Navigation

## Architecture & Design
- Infra (Kind, zero-trust) → ADR-0012; Local dev → ADR-0013; Env tests → ADR-0014
- Guard pipeline → ADR-0015; CI gates → ADR-0024; Containerized devloop → ADR-0025
- Host-side cluster helper → ADR-0030; Dashboard metrics (counters vs rates) → ADR-0029
- Metric testability (single presence guard, Cat A/B/C rollout, raw `/metrics` evidence, per-service SLO sub-targets) → ADR-0032

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`; runner + common → `scripts/guards/run-guards.sh`, `common.sh`
- Kustomize → `scripts/guards/simple/validate-kustomize.sh`; app metrics (metric↔dashboard) → `validate-application-metrics.sh`
- Metric-test coverage guard (`validate-metric-coverage.sh`, single presence check; lead sequences per-service backfill PRs during phasing window) → ADR-0032

## Devloop Cluster Helper
- Kind config template (envsubst, host-gateway listenAddress) → `infra/kind/kind-config.yaml.tmpl`
- Devloop wrapper → `infra/devloop/devloop.sh`, container image → `infra/devloop/Dockerfile`
- Cluster networking debate → `docs/debates/2026-04-09-devloop-cluster-networking/debate.md`; sidecar (superseded) → `docs/debates/2026-04-05-devloop-cluster-sidecar.md`
- Helper commands (setup, deploy, rebuild, teardown, status) → `crates/devloop-helper/src/commands.rs`; protocol → `crates/devloop-helper/src/protocol.rs`
- Port-map.env generation → `crates/devloop-helper/src/commands.rs:write_port_map_shell()`
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

## Runbooks & Database
- Per-service incident/deployment → `docs/runbooks/` (ac, gc, mc)
- Participant tracking + meetings → `crates/gc-service/src/repositories/participants.rs`, `meetings.rs`

## Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`
- Shared GC↔AC token types → `crates/common/src/meeting_token.rs`
- AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; Service auth → ADR-0003

## Observability
- Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/`; Alerts → `docs/observability/alerts.md`
- Per-service metrics → `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`; Prometheus → `infra/docker/prometheus/prometheus.yml`

## MH Service
- MH startup + config + health → `crates/mh-service/src/main.rs`, `config.rs`, `observability/health.rs`
- MH gRPC (service, GC client, MC client, JWKS auth) → `crates/mh-service/src/grpc/mh_service.rs`, `gc_client.rs`, `mc_client.rs`, `auth_interceptor.rs`
- MH→MC notifications (fire-and-forget) → `crates/mh-service/src/webtransport/connection.rs:spawn_notify_connected()`; tests → `tests/mc_client_integration.rs`
- MH WebTransport + session mgmt → `crates/mh-service/src/webtransport/server.rs`, `connection.rs`, `session/mod.rs`
- MH crate integration tests + shared rigs (RAII Drop, `127.0.0.1:0`) → `crates/mh-service/tests/` (`auth_layer_integration.rs`, `register_meeting_integration.rs`, `webtransport_integration.rs`, `common/`)

## MC Service
- MC startup + gRPC server wiring → `crates/mc-service/src/main.rs`; config → `crates/mc-service/src/config.rs`
- MC WebTransport → `crates/mc-service/src/webtransport/server.rs`, `connection.rs`
- MC GC client → `crates/mc-service/src/grpc/gc_client.rs`; MH client (MhRegistrationClient trait) → `crates/mc-service/src/grpc/mh_client.rs`
- Async RegisterMeeting trigger (first-participant, retry+backoff, cancel-aware) → `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- MC gRPC services (GC→MC assignments, MH→MC MediaCoordination) → `crates/mc-service/src/grpc/mc_service.rs`, `media_coordination.rs`; JWKS auth → `auth_interceptor.rs:McAuthLayer`
- MhConnectionRegistry (cleanup wired in controller.rs `remove_meeting()`) → `crates/mc-service/src/mh_connection_registry.rs`
- Idempotent MH-retry invariant (disconnect after registry-clear returns Ok, not gRPC error) → `crates/mc-service/src/grpc/media_coordination.rs:test_coordination_flow_connect_disconnect_round_trip()`
- Redis (fenced writes, MhAssignmentData, MhAssignmentStore trait) → `crates/mc-service/src/redis/client.rs`
- Actors → `crates/mc-service/src/actors/controller.rs`, `meeting.rs`, `participant.rs`
- MCMediaConnectionAllFailed alert → `infra/docker/prometheus/rules/mc-alerts.yaml`

## GC Service + Tests
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `handlers/meetings.rs`
- MC join tests → `crates/mc-service/tests/join_tests.rs`; TestKeypair → `crates/mc-test-utils/src/jwt_test.rs`
- GC join tests → `crates/gc-service/tests/meeting_tests.rs`; Env-tests → `crates/env-tests/`