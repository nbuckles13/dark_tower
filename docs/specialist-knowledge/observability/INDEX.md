# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating, listenAddress fix) -> ADR-0030
- Metric testability (component tests + `MetricAssertion` helper + presence guard) -> ADR-0032

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `docs/observability/metrics/gc-service.md`, `docs/observability/metrics/mc-service.md`, `docs/observability/metrics/mh-service.md`
- AC metrics -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, gauge init `services/key_management_service.rs:init_key_metrics()`, HTTP middleware `middleware/http_metrics.rs`, rate limit config `config.rs`
- GC metrics -> `crates/gc-service/src/observability/metrics.rs`, HTTP middleware `middleware/http_metrics.rs:normalize_endpoint()`, join wiring `handlers/meetings.rs:join_meeting()`, DB metrics `repositories/`; gap: `get_guest_token()` uninstrumented
- MC metrics -> `crates/mc-service/src/observability/metrics.rs`; recording sites: `webtransport/connection.rs:handle_connection()`, `server.rs:accept_loop()`, `grpc/mh_client.rs:register_meeting()`, `grpc/media_coordination.rs` (mc_mh_notifications_received_total, mc_media_connection_failures_total); bounded labels `errors.rs:error_type_label()`
- MH metrics -> `crates/mh-service/src/observability/metrics.rs`; recording sites: `webtransport/server.rs:accept_loop()`, `webtransport/connection.rs:handle_connection()`, `grpc/auth_interceptor.rs:MhAuthService`, `grpc/mc_client.rs`

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
- Port map + port-map.env (observability + WebTransport ports) -> `/tmp/devloop-{slug}/ports.json`, `crates/devloop-helper/src/commands.rs:write_port_map_shell()`
- Status command, pod health parsing, client display -> `crates/devloop-helper/src/commands.rs:cmd_status()`, `parse_pod_health()`, `infra/devloop/dev-cluster`
- Helper audit log + devloop.sh infrastructure health check (ADR-0030 Step 6) -> `crates/devloop-helper/src/logging.rs:AuditLog`, `infra/devloop/devloop.sh`
- Env-tests observability + Layer 8 validation pipeline -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`, `crates/env-tests/tests/30_observability.rs`, `.claude/skills/devloop/SKILL.md` (Layer 8)

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20, bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`
- Metric-test coverage (src emission vs test reference) -> `scripts/guards/simple/validate-metric-coverage.sh`
- MetricAssertion test helper (per-thread DebuggingRecorder; histogram-first ordering load-bearing — first `.assert_*()` of any kind drains histograms via `Snapshotter::snapshot()`, counter/gauge re-reads idempotent; `assert_unobserved` symmetric across `CounterQuery`/`GaugeQuery`/`HistogramQuery` with `ensure_no_kind_mismatch` hardening — closes ADR-0032 §F4 gauge-absence gap, histogram form must precede `assert_observation_count*` on same name+labels) -> `crates/common/src/observability/testing.rs`

## Env-Test Observability, Cluster Config & Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
- GC scenarios 8-9 + join failure triage -> `docs/runbooks/gc-incident-response.md`, `docs/observability/alerts.md`

## Test Coverage & Integration Seams
- Per-service metrics tests -> `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`
- MH accept-loop component-test pattern (ADR-0032 Step 2 canonical template — real `WebTransportServer::bind()`+`accept_loop()` byte-identical to production, rcgen-PEMs-on-disk, no `results_rx`; `#[tokio::test]` current-thread + live-connection hold + histogram-first are load-bearing; `accepted`/`rejected`/`error` via `MetricAssertion` + `SessionManagerHandle` + companion `mh_jwt_validations_total` labels; sibling JWT/MC-notify/handshake/caller-type rigs in same dir) -> `crates/mh-service/tests/common/accept_loop_rig.rs`, `crates/mh-service/tests/webtransport_accept_loop_integration.rs`; Cat B token-refresh extraction -> `crates/mh-service/src/observability/metrics.rs:record_token_refresh_metrics()`
- MC accept-loop component-test pattern (ADR-0032 Step 3 — MH template + `MockMhAssignmentStore`/`MockMhRegistrationClient` injection + `MeetingControllerActorHandle`; `mc_webtransport_connections_total` + `mc_jwt_validations_total` + `mc_session_join_failures_total` adjacency) -> `crates/mc-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/webtransport_accept_loop_integration.rs`; Cat B token-refresh -> `crates/mc-service/src/observability/metrics.rs:record_token_refresh_metrics()`
- AC cluster component-test pattern (ADR-0032 Step 4 canonical Cat C — 13 cluster integration test files; in-src `metrics.rs::tests` migrated to per-cluster `MetricAssertion`; per-failure-class adjacency via `assert_delta(0)` + `assert_unobserved`; HTTP middleware via `tower::ServiceExt::oneshot`; bcrypt cost-12 production parity for histogram buckets; audit-log seams `break_auth_events_inserts` (CHECK NOT VALID for pre-querying fns) + `break_auth_events_table` (DROP CASCADE) + `assert_only_event_type` adjacency helper; orphan/drift findings (`ac_token_validations_total` Phase-4-reserved, `ac_jwks_requests_total` only `miss` reachable, `error_category="clock_skew"` drifts vs catalog); follow-ups in `docs/TODO.md` §Observability Debt (db-query F2, HTTP 400/415 F4, orphan dispositions)) -> `crates/ac-service/tests/audit_log_failures_integration.rs`, `crates/ac-service/tests/bcrypt_metrics_integration.rs`, `crates/ac-service/tests/credential_ops_metrics_integration.rs`, `crates/ac-service/tests/db_metrics_integration.rs`, `crates/ac-service/tests/errors_metric_integration.rs`, `crates/ac-service/tests/http_metrics_integration.rs`, `crates/ac-service/tests/internal_token_metrics_integration.rs`, `crates/ac-service/tests/jwks_metrics_integration.rs`, `crates/ac-service/tests/key_rotation_metrics_integration.rs`, `crates/ac-service/tests/rate_limit_metrics_integration.rs`, `crates/ac-service/tests/token_issuance_service_integration.rs`, `crates/ac-service/tests/token_issuance_user_integration.rs`, `crates/ac-service/tests/token_validation_integration.rs`, `crates/ac-service/src/observability/metrics.rs`, `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/handlers/jwks_handler.rs`, `docs/observability/metrics/ac-service.md`, `docs/TODO.md`