# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating, listenAddress fix) -> ADR-0030
- gRPC auth scopes (two-layer auth, caller_type_rejected_total metric, MH metrics gap fix) -> ADR-0003

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `docs/observability/metrics/gc-service.md`, `docs/observability/metrics/mc-service.md`, `docs/observability/metrics/mh-service.md`
- AC metrics -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, gauge init `services/key_management_service.rs:init_key_metrics()`, HTTP middleware `middleware/http_metrics.rs`, rate limit config `config.rs`
- GC metrics -> `crates/gc-service/src/observability/metrics.rs` (incl. `record_jwt_validation()` / `record_caller_type_rejected()` for ADR-0003), HTTP middleware `middleware/http_metrics.rs:normalize_endpoint()`, join wiring `handlers/meetings.rs:join_meeting()`, DB metrics `repositories/`; gap: `get_guest_token()` uninstrumented
- MC metrics -> `crates/mc-service/src/observability/metrics.rs`; recording sites: `webtransport/connection.rs:handle_connection()`, `server.rs:accept_loop()`, `grpc/mh_client.rs:register_meeting()`, `grpc/media_coordination.rs` (mc_mh_notifications_received_total, mc_media_connection_failures_total), `grpc/auth_interceptor.rs` (mc_caller_type_rejected_total); bounded labels `errors.rs:error_type_label()`
- MH metrics (registration, heartbeat, token refresh, gRPC, WebTransport, JWT w/ failure_reason, MC notifications, caller_type_rejected) -> `crates/mh-service/src/observability/metrics.rs`; recording sites: `webtransport/server.rs:accept_loop()`, `webtransport/connection.rs:handle_connection()`, `grpc/auth_interceptor.rs:MhAuthService` (classify_jwt_error, scope_mismatch, Layer 2 caller_type), `grpc/mc_client.rs:send_with_retry()`

## Auth & JWT Tracing
- Common JWT (JwksClient, JwtValidator, verify_token, PII-redacted Debug) -> `crates/common/src/jwt.rs`
- GC/MC/MH auth wrappers -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`, `crates/mc-service/src/auth/mod.rs:McJwtValidator` (target: `mc.auth`), `crates/mh-service/src/auth/mod.rs:MhJwtValidator` (target: `mh.auth`)
- Two-layer gRPC auth (ADR-0003): Layer 1 JWKS+scope (`jwt_validations_total{result, token_type, failure_reason}`), Layer 2 service_type routing (`caller_type_rejected_total{grpc_service, expected_type, actual_type}` — any non-zero is a bug); `classify_jwt_error()` maps `JwtError` -> bounded failure_reason in GC, MC, and MH
- GC gRPC auth (target: `gc.grpc.auth`) -> `crates/gc-service/src/grpc/auth_layer.rs:GrpcAuthLayer` (Layer 1+2, `classify_jwt_error()`, claims injection); MC (target: `mc.grpc.auth`) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer` (Layer 1+2, claims injection); MH (target: `mh.grpc.auth`) -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer` (Layer 1+2, claims injection, PERMISSION_DENIED for Layer 2); legacy MhAuthInterceptor removed
- ADR-0003 scope alignment: `default_scopes()` -> `crates/ac-service/src/models/mod.rs:ServiceType`; seed SQL -> `infra/kind/scripts/setup.sh`; scope contract tests -> `crates/ac-service/src/models/mod.rs:tests::test_scope_contract_*`

## MC WebTransport Tracing
- Server/connection/handler (targets: `mc.webtransport`, `.connection`, `.handler`) -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs` (incl. MediaConnectionFailed), `handler.rs`
- MH coordination gRPC (target: `mc.grpc.media_coordination`) -> `crates/mc-service/src/grpc/media_coordination.rs`
- ParticipantActor (target: `mc.actor.participant`) -> `crates/mc-service/src/actors/participant.rs:run()`

## MH WebTransport Tracing
- Server/connection (targets: `mh.webtransport`, `.connection`) -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`

## gRPC Client Tracing (MC -> GC, MC -> MH, MH -> MC)
- GcClient tracing (registration, heartbeat, re-registration) -> `crates/mc-service/src/grpc/gc_client.rs` (target: `mc.grpc.gc_client`)
- MhClient tracing (RegisterMeeting RPC) -> `crates/mc-service/src/grpc/mh_client.rs` (target: `mc.grpc.mh_client`)
- McClient tracing (NotifyParticipantConnected/Disconnected) -> `crates/mh-service/src/grpc/mc_client.rs` (target: `mh.grpc.mc_client`)

## Health
- MC health state -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health state (ready after GC registration) -> `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards, Alerts & Infrastructure
- Grafana dashboards (per-service overview, errors-overview, SLOs) -> `infra/grafana/dashboards/`, provisioning `infra/grafana/provisioning/`; GC gRPC Auth row -> `gc-overview.json` (JWT Validations by Result & Type, Caller Type Rejections ADR-0003); MC MH Communication row -> `mc-overview.json` (panels: RegisterMeeting RPC Rate, Latency P50/P95/P99, Caller Type Rejections); MH Client Connections row -> `mh-overview.json` (JWT Validations by Result w/ failure_reason, Caller Type Rejections ADR-0003)
- Grafana K8s (configMapGenerator, sidecar, RBAC) -> `infra/kubernetes/observability/grafana/`
- Alert rules (GC, MC) -> `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml` (incl. MCMediaConnectionAllFailed); docs -> `docs/observability/alerts.md`, `docs/observability/dashboards.md`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml` (compose), `infra/kubernetes/observability/prometheus-config.yaml` (K8s)
- Loki + observability kustomization -> `infra/kubernetes/observability/{loki-config,kustomization}.yaml`
- Kind observability NodePorts (30090/30030/30080) -> `infra/kind/kind-config.yaml`
- MC/MH K8s health probes + metrics scrape -> `infra/services/{mc,mh}-service/deployment.yaml`

## Kind Cluster Setup (Observability)
- Setup (deploy_observability, setup_port_forwards, deploy_only_service, DT_CLUSTER_NAME/DT_PORT_MAP) -> `infra/kind/scripts/setup.sh`
- Devloop ConfigMap patching (MC/MH advertise addresses, DT_HOST_GATEWAY_IP guard) -> `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- Teardown (parameterized cluster name, scoped pkill) -> `infra/kind/scripts/teardown.sh`

## Devloop Cluster Helper (Observability)
- Kind config template (listenAddress: ${HOST_GATEWAY_IP}, dynamic observability ports) -> `infra/kind/kind-config.yaml.tmpl`
- Port map (prometheus, grafana, loki port discovery) -> `/tmp/devloop-{slug}/ports.json`
- Port-map.env (observability + WebTransport port shell vars) -> `crates/devloop-helper/src/commands.rs:write_port_map_shell()`
- Status command (cluster health, pod readiness, checked_at) -> `crates/devloop-helper/src/commands.rs:cmd_status()`
- Pod health parsing (pure function, unit-testable) -> `crates/devloop-helper/src/commands.rs:parse_pod_health()`
- Status client display (health summary from result data) -> `infra/devloop/dev-cluster` (status post-command section)
- Helper audit log (JSONL, all commands including status) -> `crates/devloop-helper/src/logging.rs:AuditLog`
- Devloop.sh infrastructure health check (re-entry) -> `infra/devloop/devloop.sh` (ADR-0030 Step 6 section)
- Eager setup background log -> `/tmp/devloop-{slug}/eager-setup.log`
- Env-test observability URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- Env-tests observability validation -> `crates/env-tests/tests/30_observability.rs`
- Layer 8 env-test integration (validation pipeline) -> `.claude/skills/devloop/SKILL.md` (Layer 8 section)

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20, bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Env-Test Observability, Runbooks & Test Coverage
- Runbooks + alerts -> `docs/runbooks/`, `docs/observability/alerts.md`; Metrics tests -> per-service `observability/metrics.rs`