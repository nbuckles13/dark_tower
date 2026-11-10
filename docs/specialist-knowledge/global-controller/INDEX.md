# Global Controller Navigation

## Architecture & Design
- GC architecture, dual-server, MC/MH registry, load balancing -> ADR-0010
- API versioning, URL prefix conventions -> ADR-0004
- User auth, meeting access, join dependency chain -> ADR-0020
- Service-to-service auth (OAuth 2.0 client credentials); gRPC scopes, two-layer auth (JWKS + service_type routing) -> ADR-0003
- No-panic policy (DB row decoding must use `try_get`) -> ADR-0002
- Metric testability (four-tier pattern, MetricAssertion, per-failure-class mechanism table) -> ADR-0032
- MH replacement/migration (MC->GC AcquireMediaHandler, exclusion not veto, MC owns handler set; planned drain) -> ADR-0039 (`docs/decisions/adr-0039-mh-replacement-and-migration.md`)
- RegisterMH endpoint rules (allowlists, supersede/reject arms) -> `docs/debates/2026-10-28-mh-replacement-and-migration/gc-register-rule.txt`

## Code Locations
- Entrypoint (HTTP + gRPC dual-server startup) -> `crates/gc-service/src/main.rs`
- Route definitions and AppState -> `crates/gc-service/src/routes/mod.rs:build_routes()`
- Configuration (env vars, thresholds) -> `crates/gc-service/src/config.rs:Config::from_env()`
- Error enum: HTTP status, `error.code`, `error_type` metric label -> `crates/gc-service/src/errors.rs:GcError`
- JWT validation (validate + validate_raw for gRPC failure_reason); JWKS caching; claims -> `crates/gc-service/src/auth/jwt.rs`, `crates/gc-service/src/auth/jwks.rs:JwksClient::get_key()`, `crates/gc-service/src/auth/claims.rs`
- HTTP auth middleware (service + user token: require_auth, require_user_auth) -> `crates/gc-service/src/middleware/auth.rs`
- gRPC auth layer (Tower, two-layer: scope + service_type routing per ADR-0003); failure_reason classifier -> `crates/gc-service/src/grpc/auth_layer.rs:GrpcAuthLayer`, `classify_jwt_error()`
- Meeting handlers (create, join, guest-token, settings) -> `crates/gc-service/src/handlers/meetings.rs`
- Create (exhaustive `CreateMeetingOutcome` match, per-cause log + metric label) -> `crates/gc-service/src/handlers/meetings.rs:create_meeting()`
- Join/settings (user-auth, status allowlist, `home_org_id`, metrics) -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`, `update_meeting_settings()`
- Join response construction (shared by join + guest-token; McAssignment -> McAssignmentInfo) -> `crates/gc-service/src/handlers/meetings.rs:JoinMeetingResponse::new()`, `From<McAssignment>`
- MC gRPC service (register incl. `max_exclude_handler_ids` push, heartbeat, AcquireMediaHandler: `validate_acquire()` + single recording site) -> `crates/gc-service/src/grpc/mc_service.rs:McService`; shared handler-id rule -> `grpc/mod.rs:validate_handler_id()`. AcquireMediaHandler (ADR-0039 §3, R-23): ownership → gc_view → select; trigger projection, `classify()` (gc_view), `Rejection`/`outcome()`, `gc.mh.replacement_report` event + evidence rule -> `crates/gc-service/src/services/mh_replacement.rs`; ownership query -> `crates/gc-service/src/repositories/meeting_assignments.rs:get_active_assignment_region()`; label vocab + 73-cell emitted set -> `crates/gc-service/src/observability/metrics.rs:ReplacementTrigger`, `ReplacementOutcome`, `GcView`, `is_emitted_replacement_combination()`
- MH gRPC service (register: arm 0 + single outcome site; load report) -> `crates/gc-service/src/grpc/mh_service.rs:MhService`; RegisterMH allowlists (canonical grammar, zones, parsers) -> `crates/gc-service/src/mh_endpoint_policy.rs:MhEndpointAllowlist`, keys `config.rs:mh_endpoint_allowlist_from_vars()`, Kind render `infra/kind/scripts/deploy.sh:gc_mh_allowlist_literals()`; arms 1-3 (locked txn, `decide()`) -> `crates/gc-service/src/repositories/media_handlers.rs:MediaHandlersRepository::register_mh()`
- MC assignment + load balancing; join stickiness (reuse returns existing MC, no MH selection, `mh_selection: None`, R-6) -> `crates/gc-service/src/services/mc_assignment.rs:assign_meeting_with_mh()`, `AssignmentWithMh`
- MH selection: ONE function for initial + replacement (`select(region, count, exclude, rng)`, records no metrics; callers own their series), Rust eligibility predicate -> `crates/gc-service/src/services/mh_selection.rs:MhSelectionService::select()`, `is_selectable()`; exclude-before-LIMIT SQL -> `repositories/media_handlers.rs:get_candidate_mhs()`; the one weighted kernel (MC + MH) + `SelectionRng` test seam -> `crates/gc-service/src/services/weighted_pick.rs`
- MC gRPC client (`assign_meeting` RPC carrying per-handler `webtransport_endpoint` + `grpc_endpoint`) -> `crates/gc-service/src/services/mc_client.rs:McClientTrait`
- AC HTTP client (meeting/guest tokens) -> `crates/gc-service/src/services/ac_client.rs:AcClient`
- MC/MH repositories (register, heartbeat, staleness) -> `crates/gc-service/src/repositories/` (`meeting_controllers.rs`, `media_handlers.rs`); MH registry freshness: config (REQUIRED `GC_MH_*`, one staleness bound = k x interval) -> `crates/gc-service/src/config.rs:MhRegistryConfig`; tick (sweep -> retention -> `gc_mh_available`) -> `crates/gc-service/src/tasks/mh_health_checker.rs:mh_registry_tick()`
- Meetings repository (create with limit check, audit log) -> `crates/gc-service/src/repositories/meetings.rs:MeetingsRepository::create_meeting_with_limit_check()`
- Assignment repository (weighted select, atomic assign incl. ENDED-row revival; end + batch jobs keyed on (meeting, region)) -> `crates/gc-service/src/repositories/meeting_assignments.rs:MeetingAssignmentsRepository::atomic_assign()`, `end_assignment()`, `end_stale_assignments()`, `cleanup_old_assignments()`
- Generic health checker loop -> `crates/gc-service/src/tasks/generic_health_checker.rs`; assignment cleanup (soft/hard delete) -> `crates/gc-service/src/tasks/assignment_cleanup.rs`
- Observability metrics (incl. join metrics) -> `crates/gc-service/src/observability/metrics.rs`
- Grafana dashboard -> `infra/grafana/dashboards/gc-overview.json`

## Client telemetry proxy (OTLP ingest -> collector)
- Routes `/api/v1/telemetry/v1/{metrics,traces}` -> `crates/gc-service/src/routes/mod.rs:build_routes()`
- Ingest handlers, body limit, rate limiter -> `crates/gc-service/src/handlers/telemetry.rs:ingest_metrics()`, `ingest_traces()`, `TelemetryState`
- Attribute allowlist, media datapoint tier (`media_kind`/`content_kind` admitted at the browser trust boundary), `org_id` stamped from `UserClaims`, `key_custody` value rule -> `crates/gc-service/src/services/telemetry_filter.rs:ALLOWLIST`, `MEDIA_DATAPOINT_EXTRA`, `custody_value_ok()`, `filter_metrics()`, `filter_traces()`
- Shared label-key vocabulary (SSoT with MC recorders) -> `crates/common/src/observability/labels.rs:MEDIA_KIND_LABEL`, `CONTENT_KIND_LABEL`, `KEY_CUSTODY_LABEL`
- Upstream forwarder -> `crates/gc-service/src/services/telemetry_forwarder.rs:TelemetryForwarder`

## Meeting-refusal taxonomy
- Cause enum, metric label, SQL discriminant -> `crates/gc-service/src/repositories/meetings.rs:MeetingRefusal`, `metric_label()`, `from_discriminant()`
- Repository outcome type (Created / Refused / MeetingCodeTaken) -> `crates/gc-service/src/repositories/meetings.rs:CreateMeetingOutcome`
- Unique-violation classification by SQLSTATE + constraint -> `crates/gc-service/src/repositories/meetings.rs:classify_insert_error()`
- Row decoding (`try_get`, ADR-0002) -> `crates/gc-service/src/repositories/meetings.rs:map_row_to_meeting()`
- Cause -> HTTP mapping (single site) -> `crates/gc-service/src/errors.rs:impl From<MeetingRefusal> for GcError`
- Wire contract (error codes, statuses, retry guidance) -> `docs/API_CONTRACTS.md`
- Per-cause operator response -> `docs/runbooks/gc-incident-response.md` (Scenario 8)
- Alert rules -> `infra/docker/prometheus/rules/gc-alerts.yaml`, `docs/observability/alerts.md`

## Integration Seams
- GC <-> AC (OAuth token refresh) -> `crates/common/src/token_manager.rs:TokenReceiver`
- GC <-> AC shared types (MeetingTokenRequest, GuestTokenRequest, TokenResponse, ParticipantType, MeetingRole) -> `crates/common/src/meeting_token.rs`
- GC <-> MC (gRPC assignment RPC, requires service.write.mc per ADR-0003) -> `crates/gc-service/src/services/mc_client.rs`
- GC <-> MC / MH (gRPC registration, heartbeat, load report) -> `crates/gc-service/src/grpc/mc_service.rs`, `crates/gc-service/src/grpc/mh_service.rs`
- MhAssignmentInfo -> MhAssignment proto mapping (mh_id, webtransport_endpoint, grpc_endpoint) -> `crates/gc-service/src/services/mc_client.rs:assign_meeting()`
- GC <-> Client (HTTP API /api/v1/*) -> `crates/gc-service/src/routes/mod.rs`
- UserClaims (user JWT claims type) -> `crates/common/src/jwt.rs:UserClaims`
- env-tests GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`; org the env-test/browser suites create meetings in (`ENV_TEST_ORG_SUBDOMAIN`) -> `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- Per-run org provisioning (Layer 7 Phase 1h, operator lane) -> `scripts/layer7.sh`, `infra/kind/scripts/setup.sh --provision-org`

## Tests
- Meeting creation + the three refusal causes -> `crates/gc-service/tests/meeting_create_tests.rs`
- Meeting join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- MC/MH assignment tests -> `crates/gc-service/tests/mc_assignment_rpc_tests.rs`, `crates/gc-service/tests/meeting_assignment_tests.rs`; HTTP join stickiness -> `crates/gc-service/tests/meeting_tests.rs` (`test_join_sticky_*`); auth (incl. R-15 media_dial pin `r15_*`) -> `crates/gc-service/tests/auth_tests.rs`; AcquireMediaHandler gates -> `tests/mh_replacement_tests.rs`, label shape `tests/mh_replacement_metrics_integration.rs`, fixtures `tests/common/{mc_grpc,event_capture}.rs`
- Per-metric-cluster integration tests (ADR-0032) -> `crates/gc-service/tests/*_metrics_integration.rs`
- `error_type` label coverage -> `crates/gc-service/tests/meeting_creation_metrics_integration.rs`, `crates/gc-service/tests/errors_metric_integration.rs`
- Telemetry proxy tests -> `crates/gc-service/tests/telemetry_proxy_tests.rs`, `crates/gc-service/tests/telemetry_metrics_integration.rs`
- MH registry: gates -> `crates/gc-service/tests/mh_registry_tests.rs`, `crates/gc-service/tests/mh_health_checker_tests.rs` (fixtures `tests/common/mh_registry.rs`); series -> `crates/gc-service/tests/mh_registry_metrics_integration.rs`; shipped config for tests -> `crates/gc-test-utils/src/reference_config.rs:reference_mh_registry()`; env-test -> `crates/env-tests/tests/03_gc_deployment_config.rs`; RegisterMH rule -> `tests/mh_registration_tests.rs` (arms, concurrency), `tests/mh_registration_metrics_integration.rs` (label shape), test allowlist `crates/gc-test-utils/src/mh_endpoint_allowlist.rs`, env-test `crates/env-tests/tests/44_gc_mh_registration.rs`
- Test harness -> `crates/gc-test-utils/src/server_harness.rs`; per-crate JWT fixtures -> `crates/gc-service/tests/common/jwt_fixtures.rs`
- Metrics catalog (creation + join, `error_type` value list) -> `docs/observability/metrics/gc-service.md`
