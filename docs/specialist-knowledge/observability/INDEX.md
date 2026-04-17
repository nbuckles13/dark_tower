# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating, listenAddress fix) -> ADR-0030
- Metric testability (component tests + `MetricAssertion` helper + presence guard) -> ADR-0032
- Service-owned dashboards and alerts (collapsed Phase 4, observability as cross-cutting reviewer) -> ADR-0031

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `docs/observability/metrics/gc-service.md`, `docs/observability/metrics/mc-service.md`, `docs/observability/metrics/mh-service.md`
- AC metrics -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, gauge init `services/key_management_service.rs:init_key_metrics()`, HTTP middleware `middleware/http_metrics.rs`, rate limit config `config.rs`
- GC metrics -> `crates/gc-service/src/observability/metrics.rs`, HTTP middleware `middleware/http_metrics.rs:normalize_endpoint()`, join wiring `handlers/meetings.rs:{join_meeting,get_guest_token}()` (shared `gc_meeting_join_*` family discriminated by `participant=user|guest`; do NOT fork a `gc_guest_token_*` family), DB metrics `repositories/`
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
- Alert rules (GC, MC) -> `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`, template -> `_template-service-alerts.yaml`; conventions -> `docs/observability/alert-conventions.md`; docs -> `docs/observability/alerts.md`, `dashboards.md`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml` (compose), `infra/kubernetes/observability/prometheus-config.yaml` (K8s); Kind NodePorts (30090/30030/30080) -> `infra/kind/kind-config.yaml`
- Loki + observability kustomization -> `infra/kubernetes/observability/{loki-config,kustomization}.yaml`
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
- Alert-rules lint -> `scripts/guards/simple/validate-alert-rules.sh`

## Env-Test Observability, Cluster Config & Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service); GC scenarios 8-9 + join failure triage -> `gc-incident-response.md`, `docs/observability/alerts.md`

## Test Coverage & Integration Seams
- MH/MC accept-loop component-test pattern (ADR-0032 Steps 2-3 — Step 2 canonical: real `WebTransportServer::bind()`+`accept_loop()` byte-identical to production, rcgen-PEMs-on-disk, no `results_rx`; `#[tokio::test]` current-thread + live-connection hold + histogram-first are load-bearing; `accepted`/`rejected`/`error` via `MetricAssertion` + `SessionManagerHandle` + companion `mh_jwt_validations_total` labels; sibling JWT/MC-notify/handshake/caller-type rigs. Step 3 ports template + adds `MockMhAssignmentStore`/`MockMhRegistrationClient` injection + `MeetingControllerActorHandle` + `mc_webtransport_connections_total`/`mc_jwt_validations_total`/`mc_session_join_failures_total` adjacency. Cat B token-refresh `record_token_refresh_metrics()` extracted in both services) -> `crates/mh-service/tests/common/accept_loop_rig.rs`, `crates/mh-service/tests/webtransport_accept_loop_integration.rs`, `crates/mc-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/webtransport_accept_loop_integration.rs`, `crates/mh-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`
- AC cluster component-test pattern (ADR-0032 Step 4 canonical Cat C — 13 cluster integration test files; in-src `metrics.rs::tests` migrated to per-cluster `MetricAssertion`; per-failure-class adjacency via `assert_delta(0)` + `assert_unobserved`; HTTP middleware via `tower::ServiceExt::oneshot`; bcrypt cost-12 production parity for histogram buckets; audit-log seams `break_auth_events_inserts` (CHECK NOT VALID) + `break_auth_events_table` (DROP CASCADE) + `assert_only_event_type` adjacency; orphan/drift findings (`ac_token_validations_total` Phase-4-reserved, `ac_jwks_requests_total` only `miss` reachable, `error_category="clock_skew"` drifts vs catalog); follow-ups in `docs/TODO.md` §Observability Debt) -> `crates/ac-service/tests/*_integration.rs`, `crates/ac-service/src/observability/metrics.rs`, `docs/observability/metrics/ac-service.md`
- GC cluster component-test pattern (ADR-0032 Step 5 — 13 cluster integration test files closing `feature/mh-quic-mh-tests` with `validate-metric-coverage.sh` fully GREEN across all four services; in-src `metrics.rs::tests` migrated to per-cluster `MetricAssertion`; Cat B `record_token_refresh_metrics` byte-identical to MH/MC. **Reusable patterns**: (1) catalog "expected vs enforced" honesty for JWT/external-input labels without allowlist-clamp — add "Cardinality note" with emission site + trust boundary + TODO clamp (canonical: `gc_caller_type_rejected_total{actual_type}` F1 finding, clamp at `auth_layer.rs:241`); (2) shared family discriminator label — reuse family + discriminator label, NOT parallel name-prefixed families; catalog enumerates label values (canonical: `gc_meeting_join_*{participant=user|guest}`); (3) label-domain exclusivity invariant — `assert_unobserved` on OTHER discriminator branch's slot when a value is path-exclusive (canonical: `meeting_join_metrics_integration.rs` 3 exclusivity tests); (4) gauge zero-fill 4-cell adjacency — full / partial-`assert_value(0.0)` / empty / caller-short-circuit-`assert_unobserved`; cells 2+4 distinguish explicit-emit-zero from never-emitted (canonical: `registered_controllers_metrics_integration.rs`)) -> `crates/gc-service/tests/*_integration.rs`, `crates/gc-service/src/observability/metrics.rs`, `crates/gc-service/src/handlers/meetings.rs`, `docs/observability/metrics/gc-service.md`