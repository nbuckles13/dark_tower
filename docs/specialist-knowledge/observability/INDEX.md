# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Media flow — forward path, key custody §4, media-path telemetry prohibition §11 -> ADR-0036
- Validation pipeline + cross-boundary ownership model -> ADR-0024, ADR-0024 §6; polyglot layers -> ADR-0033
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028; dashboard counter-vs-rate presentation -> ADR-0029
- Host-side cluster helper -> ADR-0030; service-owned dashboards/alerts + observability as cross-cutting reviewer -> ADR-0031
- Metric testability (component tests + `MetricAssertion` + presence guard) -> ADR-0032
- Guard pipeline as Rust binary; `dt-guard <subcommand> --explain` -> ADR-0034 §7; fixtures `crates/dt-guard/tests/fixtures/`
- Deterministic story runner; governance boundary ADR-0035 §11 -> ADR-0035
- Open debt -> `docs/TODO.md` §Observability Debt, §Media Path Obligations, §Guard Coverage Gaps

## Policy Documents (authoritative; prose elsewhere points here)
- SLO targets, error budgets, open objectives — precedence over ADR-0011's table -> `docs/observability/slos.md`
- Label vocabulary, `key_custody`, media-path identity, shared labels -> `docs/observability/label-taxonomy.md`
- Units, throughput, periodicity, panel shape -> `docs/observability/dashboard-conventions.md`
- Alert shape + non-zero-denominator guard -> `docs/observability/alert-conventions.md`; inventory `docs/observability/alerts.md`
- Dashboard inventory, ownership rows, K8s registration -> `docs/observability/dashboards.md`
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `gc-service.md`, `mc-service.md`, `mh-service.md`, `client.md`

## Metrics
- AC -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, `record_meeting_display_name_outcome()`; gauges `services/key_management_service.rs:init_key_metrics()`; middleware `middleware/http_metrics.rs`; emission `handlers/internal_tokens.rs:resolve_meeting_display_name()`
- GC -> `crates/gc-service/src/observability/metrics.rs`; middleware `middleware/http_metrics.rs:normalize_endpoint()`; join wiring `handlers/meetings.rs:join_meeting()`, `get_guest_token()`; telemetry proxy + CORS `handlers/telemetry.rs`, `services/telemetry_filter.rs`, `middleware/cors_observer.rs`; refusal taxonomy `repositories/meetings.rs:MeetingRefusal::metric_label()`
- MC -> `crates/mc-service/src/observability/metrics.rs:record_display_name_resolution()`, `record_participant_disconnect()`, `record_meeting_kek_generated()`, `record_join_identity_key_presence()`, `record_media_policy_push()`, `record_sender_binding_response()`, `record_receive_capability()`, `record_send_directive()`, `record_slot_state()`, `record_unmatched_plan_slots()`, `record_mute_request()`, `record_participant_outbound_dropped()`; bounded labels `errors.rs:error_type_label()`
- MC bounded telemetry vocabularies (`ALL` + `label()`) -> `crates/mc-service/src/media_signaling/outcome.rs`, `assignments.rs:slot_state_label()`, `crates/mc-service/src/media_admission/binding_response.rs:SenderBindingOutcome`
- MC emission sites -> `crates/mc-service/src/webtransport/connection.rs:handle_receive_capability()`, `reject_capability()`, `handle_mute_request()`, `compose_and_emit()`, `handle_connection()`; `actors/meeting.rs:handle_join()`; `actors/participant.rs:record_outbound_drop()`; `grpc/media_coordination.rs`
- MH -> `crates/mh-service/src/observability/metrics.rs:MediaDirection`, `MediaLatencyPhase`, `MediaDropReason`, `MediaMetricHandles`, `resolve_media_handles()`, `record_media_frames_dropped()`, `MEDIA_FORWARD_OBJECTIVE_SECONDS`, `PolicyApplyOutcome`; dev-only per-frame facility `observability/per_frame_trace.rs`
- MH emission sites -> `crates/mh-service/src/grpc/mh_service.rs:register_meeting()`, `grpc/auth_interceptor.rs`, `grpc/mc_client.rs`, `session/mod.rs`, `process.rs`; rejection reasons `routing/mod.rs:PolicyRejection`
- Shared `key_custody` / media label consts (hoisted at 2nd consumer) -> `crates/common/src/observability/labels.rs`
- Telemetry-free hot paths, by construction -> `crates/media-protocol/`, `crates/mh-service/src/media/`, `packages/sdk-core/src/media/pipeline/`
- Client SDK media metrics + allow-list label projection -> `packages/sdk-core/src/media/setup/mediaMetrics.ts`; config `packages/sdk-core/src/config/clientConfig.ts`; export cadence `packages/sdk-core/src/telemetry/telemetryConfig.ts`; wire-token mapping `packages/web-app/src/lib/slotState.ts`

## Tracing & Health
- Common JWT (JwksClient, JwtValidator, verify_token, PII-redacted Debug) -> `crates/common/src/jwt.rs`; wrappers `crates/gc-service/src/auth/jwt.rs`, `crates/mc-service/src/auth/mod.rs`, `crates/mh-service/src/auth/mod.rs`; MH gRPC layer `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MC WebTransport -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs`, `handler.rs`; ParticipantActor `crates/mc-service/src/actors/participant.rs:run()`; gRPC clients `grpc/gc_client.rs`, `grpc/mh_client.rs`
- MH WebTransport + transport seam -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`, `media_transport.rs`, `crates/mh-service/src/transport/mod.rs`
- Health routers -> `crates/mc-service/src/observability/health.rs:health_router()`, `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards & Alerts
- Dashboards + provisioning + K8s wiring -> `infra/grafana/dashboards/`, `infra/grafana/provisioning/`, `infra/grafana/kustomization.yaml`, `infra/kubernetes/observability/grafana/`
- Per-service overviews -> `infra/grafana/dashboards/ac-overview.json`, `gc-overview.json`, `mc-overview.json`, `mh-overview.json`; template `_template-service-overview.json`
- Media + SLO boards -> `infra/grafana/dashboards/mh-media.json`, `client-media.json`, `mh-slos.json`, `mh-logs.json`, `ac-slos.json`, `mc-slos.json`
- Alert rules -> `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`, `mh-alerts.yaml`, template `_template-service-alerts.yaml` — rule FILES are `operations`-owned (ADR-0011 §Documentation Ownership); the conventions and catalog docs above are mine

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
- Live metric-hygiene kernel (pure predicate + FIRE fixtures, fed by live scrape) -> `crates/env-tests/src/fixtures/metric_hygiene.rs`, `crates/env-tests/tests/32_media_metric_hygiene.rs`
- Cluster observability + alert-rule loading -> `crates/env-tests/tests/30_observability.rs`, `33_alert_rules_loaded.rs`, `crates/env-tests/src/fixtures/alert_rules_loaded.rs`
- Metric polling + baseline helpers; cluster ports -> `crates/env-tests/src/fixtures/metrics.rs:instances_exceeding_baseline()`, `poll_until_instance_above()`, `poll_until_stable()`, `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- MH/MC accept-loop rigs (ADR-0032); MH media telemetry gates -> `crates/mh-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/common/accept_loop_rig.rs`, `crates/mh-service/tests/media_metrics_integration.rs`, `policy_apply_integration.rs`
- AC/GC cluster component tests -> `crates/ac-service/tests/`, `crates/gc-service/tests/`

## Runbooks & Story Runner
- Per-service deployment + incident response, media-path scenarios -> `docs/runbooks/`; OTLP smoke fixture `infra/smoke/empty-otlp-metrics.bin`
- Cost ledger, canary, escalation, gate-rc lanes -> `scripts/workflow/run-story.sh`; seams `scripts/workflow/run-story.test.sh`, `scripts/lang/_test_helpers.sh`, `scripts/workflow/preflight-story.sh`, `devloop-stop-hook.sh`, `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`
- Task status + devloop slug SSoT -> `crates/dt-story/src/manifest.rs`, `engine.rs`, `markdown.rs:find_manifest_block()`; records `docs/devloop-outputs/`, template `docs/devloop-outputs/_template/main.md`
