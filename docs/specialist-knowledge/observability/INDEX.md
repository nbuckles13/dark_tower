# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating, listenAddress fix) -> ADR-0030

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `docs/observability/metrics/gc-service.md`, `docs/observability/metrics/mc-service.md`, `docs/observability/metrics/mh-service.md`
- AC metrics -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, gauge init `services/key_management_service.rs:init_key_metrics()`, HTTP middleware `middleware/http_metrics.rs`, rate limit config `config.rs`
- GC metrics -> `crates/gc-service/src/observability/metrics.rs`, HTTP middleware `middleware/http_metrics.rs:normalize_endpoint()`, join wiring `handlers/meetings.rs:join_meeting()`, DB metrics `repositories/`; gap: `get_guest_token()` uninstrumented
- MC metrics -> `crates/mc-service/src/observability/metrics.rs`; recording sites: `webtransport/connection.rs:handle_connection()`, `server.rs:accept_loop()`, `grpc/mh_client.rs:register_meeting()`, `grpc/media_coordination.rs` (mc_mh_notifications_received_total, mc_media_connection_failures_total); bounded labels `errors.rs:error_type_label()`
- MH metrics (registration, heartbeat, token refresh, gRPC, WebTransport, JWT, RegisterMeeting timeouts R-26) -> `crates/mh-service/src/observability/metrics.rs`; recording sites: `webtransport/server.rs:accept_loop()`, `webtransport/connection.rs:handle_connection()`, `webtransport/connection.rs:await_meeting_registration()` (provisional-accept select helper; `mh_register_meeting_timeouts_total` fires only on timeout arm — enforced by co-located behavioral tests), `grpc/auth_interceptor.rs:MhAuthService`

## Auth & JWT Tracing
- Common JWT (JwksClient, JwtValidator, verify_token, PII-redacted Debug) -> `crates/common/src/jwt.rs`
- GC/MC/MH auth wrappers -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`, `crates/mc-service/src/auth/mod.rs:McJwtValidator` (target: `mc.auth`), `crates/mh-service/src/auth/mod.rs:MhJwtValidator` (target: `mh.auth`)
- MH gRPC auth (JWKS-based ServiceClaims validation, tower Layer) -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer` (target: `mh.grpc.auth`)

## MC WebTransport Tracing
- Server/connection/handler (targets: `mc.webtransport`, `.connection`, `.handler`) -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs` (incl. MediaConnectionFailed), `handler.rs`
- MH coordination gRPC (target: `mc.grpc.media_coordination`) -> `crates/mc-service/src/grpc/media_coordination.rs`
- ParticipantActor (target: `mc.actor.participant`) -> `crates/mc-service/src/actors/participant.rs:run()`

## MH WebTransport Tracing
- Server/connection (targets: `mh.webtransport`, `.connection`) -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`

## gRPC Client Tracing (MC -> GC, MC -> MH)
- GcClient tracing (registration, heartbeat, re-registration) -> `crates/mc-service/src/grpc/gc_client.rs` (target: `mc.grpc.gc_client`)
- MhClient tracing (RegisterMeeting RPC) -> `crates/mc-service/src/grpc/mh_client.rs` (target: `mc.grpc.mh_client`)
- RegisterMeeting trigger tracing (async spawned task, retry lifecycle) -> `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()` (target: `mc.register_meeting.trigger`)

## Health
- MC health state -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health state (ready after GC registration) -> `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards, Alerts & Infrastructure
- Grafana dashboards (per-service overview, errors-overview, SLOs) -> `infra/grafana/dashboards/`, provisioning `infra/grafana/provisioning/`; MC MH Communication row -> `mc-overview.json` (panels: RegisterMeeting RPC Rate, Latency P50/P95/P99)
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
- Env-tests observability (URL config, validation) -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`, `crates/env-tests/tests/30_observability.rs`
- Layer 8 env-test integration (validation pipeline) -> `.claude/skills/devloop/SKILL.md` (Layer 8 section)

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20, bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Env-Test Observability, Cluster Config & Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
- GC scenarios 8-9 + join failure triage -> `docs/runbooks/gc-incident-response.md`, `docs/observability/alerts.md`

## Test Coverage & Integration Seams
- Per-service metrics tests -> `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`; Deferred MC label-value assertion convention -> `docs/TODO.md:78`