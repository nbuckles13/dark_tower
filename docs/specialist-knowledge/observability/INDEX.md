# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Media flow — forward path, key custody §4, media-path telemetry prohibition §11 -> ADR-0036
- Validation pipeline + cross-boundary ownership model -> ADR-0024, ADR-0024 §6; polyglot layers -> ADR-0033
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028; dashboard counter-vs-rate presentation -> ADR-0029
- Host-side cluster helper -> ADR-0030; service-owned dashboards/alerts -> ADR-0031; metric testability -> ADR-0032
- Guard pipeline as Rust binary; `dt-guard <subcommand> --explain` -> ADR-0034 §7; fixtures `crates/dt-guard/tests/fixtures/`
- Story runner -> ADR-0035; dev-cluster deploy parity (config delivery §2) -> ADR-0038
- Open debt -> `docs/TODO.md` §Observability Debt, §Media Path Obligations, §Guard Coverage Gaps

## Policy Documents (authoritative; prose elsewhere points here)
- SLO targets, error budgets, open objectives — precedence over ADR-0011's table -> `docs/observability/slos.md`
- Label vocabulary, `key_custody`, media-path identity, shared labels -> `docs/observability/label-taxonomy.md`
- Dashboard conventions + inventory -> `docs/observability/dashboard-conventions.md`, `dashboards.md`; alert conventions + inventory -> `alert-conventions.md`, `alerts.md`
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `gc-service.md`, `mc-service.md`, `mh-service.md`, `client.md`

## Metrics
- AC -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, `record_meeting_display_name_outcome()`; gauges `services/key_management_service.rs:init_key_metrics()`; middleware `middleware/http_metrics.rs`; emission `handlers/internal_tokens.rs:resolve_meeting_display_name()`
- GC -> `crates/gc-service/src/observability/metrics.rs`; middleware `middleware/http_metrics.rs:normalize_endpoint()`; join wiring `handlers/meetings.rs:join_meeting()`, `get_guest_token()`; telemetry proxy + CORS `handlers/telemetry.rs`, `services/telemetry_filter.rs`, `middleware/cors_observer.rs`; refusal taxonomy `repositories/meetings.rs:MeetingRefusal::metric_label()`
- MC recorders -> `crates/mc-service/src/observability/metrics.rs` — slots/edges `record_slot_state()`, `record_send_targets()`, `record_edge_move()`, `record_connect_settle()`; KEK lifecycle `record_kek_push()`, `record_kek_rotation_failure()`, `record_kek_rotation_duration()`, `set_kek_rotation_pending_age()`, `set_sender_ids_issued_max()`; mute `record_mute_request()`, `record_server_mute_request()`, `set_server_muted_sources()`; teardown `record_end_meeting()`, `record_push_quiesce()`, `record_teardown_fence_backstop()`; bounded labels `errors.rs:error_type_label()`
- MC bounded telemetry vocabularies (`ALL` + `label()`) -> `crates/mc-service/src/media_signaling/outcome.rs`, `assignments.rs:slot_state_label()`, `crates/mc-service/src/media_admission/binding_response.rs:SenderBindingOutcome`, `crates/mc-service/src/media_routing/connectivity.rs:Unapplied` (shares three tokens with `SenderBindingOutcome` — `ANCHOR (DRY):` + pin test), `SettleOutcome`, `observability/metrics.rs:EdgeMove`
- MC emission sites -> `crates/mc-service/src/webtransport/connection.rs:handle_receive_capability()`, `reject_capability()`, `handle_mute_request()`, `handle_connection()`; `actors/meeting_media.rs:flush_one()` (every composition: send directives, send targets, slot states, slot-view emissions, unreachable and not-yet-connected senders), `install()` (handler-set divergence), `sync_routing()` (connect settles), `record_edge_moves()` (edge moves); `webtransport/server.rs:WebTransportServer::new()` (slot-cap gauge, settle-window gauge); `actors/meeting.rs:handle_join()` (+ rotation failures); `actors/participant.rs:record_outbound_drop()`; `grpc/media_coordination.rs`; KEK rotation `media_admission/rotation.rs`; teardown `media_routing/teardown.rs`, `actors/controller.rs`
- MH -> `crates/mh-service/src/observability/metrics.rs:MediaDirection`, `MediaLatencyPhase`, `MediaDropReason`, `MediaMetricHandles`, `resolve_media_handles()`, `record_media_frames_dropped()`, `MEDIA_FORWARD_OBJECTIVE_SECONDS`, `PolicyApplyOutcome`, `StreamAdmissionOutcome`, `publish_egress_admission()`, `MeetingTeardownOutcome`; dev-only per-frame facility `observability/per_frame_trace.rs`
- MH emission sites -> `crates/mh-service/src/grpc/mh_service.rs:register_meeting()` (+ end-meeting teardown), `session/admission.rs` (egress admission window), `grpc/auth_interceptor.rs`, `grpc/mc_client.rs`, `session/mod.rs`, `process.rs`; rejection reasons `routing/mod.rs:PolicyRejection`
- Shared `key_custody` / media label consts -> `crates/common/src/observability/labels.rs`; telemetry-free hot paths -> `crates/media-protocol/`, `crates/mh-service/src/media/`, `packages/sdk-core/src/media/pipeline/`
- Client SDK media metrics + allow-list label projection -> `packages/sdk-core/src/media/setup/mediaMetrics.ts`; config `packages/sdk-core/src/config/clientConfig.ts`; wire-token mapping `packages/web-app/src/lib/slotState.ts`

## Client Metrics Pipeline (browser -> GC -> collector -> Prometheus)
- SDK exporter (DELTA temporality, per-export token) -> `packages/sdk-core/src/telemetry/telemetryConfig.ts:createMetricExporter()`
- GC proxy tiered allowlist + server-stamped `org_id` -> `crates/gc-service/src/services/telemetry_filter.rs:MEDIA_DATAPOINT_EXTRA`, `filter_datapoint_attrs()`, `stamp_org_id()`
- Collector pipeline (name allowlist, clock restamp, `delta_to_cumulative`) -> `infra/services/otel-collector/collector.yaml`; scrape jobs `infra/kubernetes/observability/prometheus.yml`; acceptance harness `scripts/otel-collector/acceptance.sh`, `sdk_writers.mjs`
- Cross-hop drift guard (export set, label forwarding, keep_keys, tripwires, reject-token partition) -> `crates/dt-guard/src/client_metrics_export.rs`; shared lexer `crates/dt-guard/src/common/ts_lex.rs`; rule-file helpers `common/alert_rule_files.rs`

## Tracing & Health
- Common JWT (JwksClient, JwtValidator, verify_token, PII-redacted Debug) -> `crates/common/src/jwt.rs`; wrappers `crates/gc-service/src/auth/jwt.rs`, `crates/mc-service/src/auth/mod.rs`, `crates/mh-service/src/auth/mod.rs`; MH gRPC layer `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MC WebTransport -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs`, `handler.rs`; ParticipantActor `crates/mc-service/src/actors/participant.rs:run()`; gRPC clients `grpc/gc_client.rs`, `grpc/mh_client.rs`
- MH WebTransport + transport seam -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`, `media_transport.rs`, `crates/mh-service/src/transport/mod.rs`
- Health routers -> `crates/mc-service/src/observability/health.rs:health_router()`, `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards & Alerts
- Dashboards + provisioning + K8s wiring -> `infra/grafana/dashboards/`, `infra/grafana/provisioning/`, `infra/grafana/kustomization.yaml`; one projected volume mounting every dashboard group `infra/grafana/deployment.yaml` (ADR-0038 Implementation step 1); unmounted-group check (R-21) `crates/dt-guard/src/kustomize_content_addressing.rs`
- Per-service overviews -> `infra/grafana/dashboards/ac-overview.json`, `gc-overview.json`, `mc-overview.json`, `mh-overview.json`; template `_template-service-overview.json`
- Media + SLO boards -> `infra/grafana/dashboards/mh-media.json`, `mc-media.json`, `client-media.json`, `mh-slos.json`, `mh-logs.json`, `ac-slos.json`, `mc-slos.json`
- Alert rules -> `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`, `mh-alerts.yaml`, `client-alerts.yaml`, `otel-alerts.yaml`, template `_template-service-alerts.yaml`
- Per-service alert-rule authorship (owning specialist; supersedes ADR-0011 §Documentation Ownership row) -> ADR-0031 §Ownership split

## Observability Infrastructure
- Prometheus server config, extracted to one file both Docker and K8s load -> `infra/kubernetes/observability/prometheus.yml`, `infra/docker/prometheus/prometheus.yml`, `infra/docker/prometheus/kustomization.yaml`, `infra/kubernetes/observability/prometheus-config.yaml`, `loki-config.yaml`, `kustomization.yaml`
- MC/MH probes + scrape config (per-instance workloads) -> `infra/services/mc-service/mc-0-deployment.yaml`, `mc-1-deployment.yaml`, `infra/services/mh-service/mh-0-deployment.yaml`, `mh-1-deployment.yaml`
- Kind NodePorts + cluster lifecycle -> `infra/kind/kind-config.yaml`, `infra/kind/kind-config.yaml.tmpl`, `infra/kind/scripts/setup.sh`, `teardown.sh`
- Devloop helper (port map, status, audit log) -> `crates/devloop-helper/src/commands.rs:write_port_map_shell()`, `cmd_status()`, `parse_pod_health()`, `crates/devloop-helper/src/logging.rs:AuditLog`, `infra/devloop/dev-cluster`, `infra/devloop/devloop.sh`
- Layer-7 operator lane + per-run org provisioning -> `scripts/layer7.sh:precondition_fail()`, `__generate_org_subdomain()`, `infra/kind/scripts/setup.sh:provision_run_org()`, `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`; tokens `docs/runbooks/devloop-validation.md`

## Guards
- Metric-to-dashboard, metric-test and dashboard-to-kustomize coverage -> `scripts/guards/simple/validate-application-metrics.sh`, `validate-metric-coverage.sh`, `validate-kustomize.sh`
- Metric label + naming policies -> `crates/dt-guard/src/metric_labels.rs`, `ts_metric_naming.rs`; `MetricAssertion` helper `crates/common/src/observability/testing.rs`
- Alert-rules policy incl. on-disk ⊆ `rule_files` loading -> `crates/dt-guard/src/alert_rules.rs`, `scripts/guards/simple/validate-alert-rules.sh`; Prometheus-derived valid-label set `crates/dt-guard/src/infrastructure_metrics.rs`
- Media-path telemetry deny (ADR-0036 §11) -> `crates/dt-guard/src/media_telemetry_deny.rs`; scope config `scripts/guards/simple/media-telemetry-deny.yaml`; wrapper `media-telemetry-deny.sh`; self-test `scripts/guards/media-telemetry-deny.test.sh`
- Macro vocabulary canonical homes; scope liveness; comment/string lexer -> `crates/dt-guard/src/telemetry_macros.rs`, `metric_macros.rs`, `common/scope.rs`, `common/test_code_filter.rs`
- PII vocabulary + matcher-family split (word-boundary vs `segments()`) -> `crates/dt-guard/src/common/pii_vocabulary.rs`, `crates/dt-guard/src/ts_retained_credentials.rs:segments()`, `scripts/guards/simple/no-pii-in-logs.sh`, `scripts/guards/simple/ts/no-pii-in-logs-ts.sh`
- `instrument(skip_all)`, frame reject-reason vocabulary, cross-encoding drift -> `scripts/guards/simple/instrument-skip-all.sh`, `validate-frame-vectors.sh`, `validate-subdomain-regex-sync.sh`, `validate-slug-class-sync.sh`

## Test Coverage & Integration Seams
- Live metric-hygiene kernel incl. media-path identity + `key_custody` rules -> `crates/env-tests/src/fixtures/metric_hygiene.rs:MEDIA_PATH_PREFIXES`, `crates/env-tests/tests/32_media_metric_hygiene.rs`; client series via GC `31_gc_telemetry.rs`
- Story-2 live metric assertions (slots, egress admission + alert-join probe, teardown, KEK rotation, server mute) -> `crates/env-tests/tests/27_mc_slot_placement.rs`, `28_mh_egress_admission.rs`, `29_mh_meeting_teardown.rs`, `34_mc_kek_rotation.rs`, `35_mc_server_mute_teardown.rs`
- Cluster observability + alert-rule loading -> `crates/env-tests/tests/30_observability.rs`, `33_alert_rules_loaded.rs`, `crates/env-tests/src/fixtures/alert_rules_loaded.rs:alert_expr_in_yaml()`
- Metric polling + baseline helpers; cluster ports -> `crates/env-tests/src/fixtures/metrics.rs:instances_exceeding_baseline()`, `poll_until_any_instance_above()`, `poll_until_stable()`, `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- MH/MC accept-loop rigs (ADR-0032); MH media telemetry gates -> `crates/mh-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/common/accept_loop_rig.rs`, `crates/mh-service/tests/media_metrics_integration.rs`, `policy_apply_integration.rs`, `stream_admission_integration.rs`; AC/GC component tests `crates/ac-service/tests/`, `crates/gc-service/tests/`

## Runbooks & Story Runner
- Per-service deployment + incident response, media-path scenarios -> `docs/runbooks/`; OTLP smoke fixture `infra/smoke/empty-otlp-metrics.bin`
- Cost ledger, canary, escalation, gate-rc lanes -> `scripts/workflow/run-story.sh`; seams `scripts/workflow/run-story.test.sh`, `scripts/lang/_test_helpers.sh`, `scripts/workflow/preflight-story.sh`, `devloop-stop-hook.sh`, `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`
- Task status + devloop slug SSoT -> `crates/dt-story/src/manifest.rs`, `engine.rs`, `markdown.rs:find_manifest_block()`; records `docs/devloop-outputs/`, template `docs/devloop-outputs/_template/main.md`
