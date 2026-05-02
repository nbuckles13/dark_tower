# Code Reviewer Navigation

## Architecture & Design
- Actor handle/task separation → ADR-0001 (Section: Pattern)
- No-panic policy, `#[expect]` over `#[allow]` → ADR-0002; AC Step 4 + MC Step 3 + MH iter-2 metric-test files use `#![allow(clippy::unwrap_used, clippy::expect_used)]` (tech-debt drift originated in MC Step 3 and was matched by AC Step 4 for consistency); cross-step batched-cleanup TODO tracks the migration to `#![expect(..., reason = "...")]` → `docs/TODO.md` §Observability Debt
- Error handling, service-layer wrapping → ADR-0003; DRY cross-service duplication → ADR-0019
- Observability naming, label cardinality, SLO targets → ADR-0011; Dashboard presentation (counters vs rates, $__rate_interval) → ADR-0029
- Guard pipeline methodology → ADR-0015; Agent teams validation pipeline → ADR-0024
- User auth, three-tier token architecture → ADR-0020
- Infrastructure architecture, K8s manifests → ADR-0012; Local dev → ADR-0013; Host-side cluster helper → ADR-0030
- Cross-boundary ownership (classification, Guarded Shared Areas, `Approved-Cross-Boundary:` trailer) → ADR-0024 §6, `.claude/skills/devloop/SKILL.md` §Cross-Boundary Edits, `.claude/skills/devloop/review-protocol.md` §sed-Test Worked Example
- Metric testability: component tests + `MetricAssertion` helper + presence guard; per-failure-class table as review heuristic → ADR-0032; disposition rule for "wrapper-only" metrics — "Phase-N marker present (annotated `#[allow(dead_code)] // Will be used in Phase N ...`, project-wide convention with 9+ hits across `crates/ac-service/src/`, established by `docs/PROJECT_STATUS.md`)" → defer disposition + WRAPPER-CAT-C invocation in `tests/` with framing comment + TODO.md entry; "no marker, zero production callers" → inline-remove (MC iter-2 precedent at `docs/devloop-outputs/2026-04-25-adr-0032-step-3-mc-metric-test-backfill/main.md:200,233,249`); WRAPPER-CAT-C framing comment shape → mirrors MC `media_connection_failed` and AC `record_token_validation` at `crates/ac-service/src/observability/metrics.rs:427-436` (references production sites, lists forward-looking reservations, points at production-path test file, ends with `docs/TODO.md` pointer)

## Code Locations — AC Service
- Clippy deny list → `Cargo.toml:34-42`
- Config (rate limits, defense-in-depth) → `crates/ac-service/src/config.rs:from_vars()`, constants at `:32-61`
- Crypto (EdDSA, AES-256-GCM, bcrypt) → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`; token-validation `clock_skew` emission sites → `crypto/mod.rs:284,439`
- Error type → `crates/ac-service/src/errors.rs:AcError`; `ErrorCategory` enum + `From<&AcError>` (4 bounded categories) → `crates/ac-service/src/observability/mod.rs:78-115`
- Handlers/routes → `handlers/auth_handler.rs:handle_service_token()`, `routes/mod.rs:build_routes()`
- Metrics → `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`; per-cluster in-src `MetricAssertion`-backed tests (replaces 14 legacy no-op smoke tests) at `:#[cfg(test)] mod tests`; WRAPPER-CAT-C framing comment for `record_token_validation` (Phase-N reservation) → `crates/ac-service/src/observability/metrics.rs:427-436`
- Repository + service layers → `repositories/signing_keys.rs`, `services/key_management_service.rs`
- Integration tests (ADR-0032 Step 4, 13 cluster files, all `#[sqlx::test]` implicit current_thread runtime + per-cluster file-header load-bearing pin comment) → `crates/ac-service/tests/audit_log_failures_integration.rs`, `bcrypt_metrics_integration.rs`, `credential_ops_metrics_integration.rs`, `db_metrics_integration.rs`, `errors_metric_integration.rs`, `http_metrics_integration.rs`, `internal_token_metrics_integration.rs`, `jwks_metrics_integration.rs`, `key_rotation_metrics_integration.rs`, `rate_limit_metrics_integration.rs`, `token_issuance_service_integration.rs`, `token_issuance_user_integration.rs`, `token_validation_integration.rs`; shared test fixtures → `crates/ac-service/tests/common/test_state.rs` (`make_app_state`, `seed_signing_key`, `seed_service_credential`)
- Audit-log fault-injection seam (`ALTER TABLE auth_events ADD CONSTRAINT block_inserts CHECK (...) NOT VALID` — surgical: preserves pre-INSERT SELECT path at `services/token_service.rs:54-59`; companion `DROP TABLE auth_events CASCADE` for fns that don't pre-query) → `crates/ac-service/tests/audit_log_failures_integration.rs:66-88`; partial-label adjacency helper `assert_only_event_type` → `:90-106`
- 12-cell adjacency-matrix factor pattern (factor `assert_only_cell(snap, op, status, expected_delta)` once, invoke uniformly across all 11 tests in the (operation × status) matrix; label-swap-bug catcher per ADR-0032 §Pattern #3) → `crates/ac-service/tests/credential_ops_metrics_integration.rs`
- Per-`ErrorCategory` variant production-driven coverage (4 tests, one per variant; `Internal` carve-out via `NotFound` + transitive From-impl unit test at `observability/mod.rs::tests::test_error_category_database_variant`; `ALL_CATEGORIES` constant drives `assert_delta(0)` adjacency on the 3 non-target siblings) → `crates/ac-service/tests/errors_metric_integration.rs`
- K8s wiring → `infra/services/ac-service/configmap.yaml`, `statefulset.yaml`

## Code Locations — GC Service
- Error type, `From<JwtError>` → `crates/gc-service/src/errors.rs:GcError`
- Auth (JWT/JWKS, middleware) → `auth/jwt.rs`, `jwks.rs`, `middleware/auth.rs:require_user_auth()`
- Meeting handlers (`participant=user|guest` parity-note + COVERAGE GAP blocks at `:512-528, 577-585, 605-613`) → `handlers/meetings.rs:create_meeting()`, `join_meeting()`, `get_guest_token()`, `JoinMeetingResponse::new()`; repositories → `repositories/meetings.rs`, `participants.rs`; AC/MC clients → `services/ac_client.rs:AcClient`, `mc_client.rs:McClientTrait`
- MH selection (flat `handlers: Vec<MhAssignmentInfo>` with `grpc_endpoint`; up to 2 peer MHs by load/AZ) → `services/mh_selection.rs:MhSelection`, `MhSelectionService::select()`
- Metrics + tests → `observability/metrics.rs` (Cat B byte-1:1 w/ MH/MC at `:295-305`), `docs/observability/metrics/gc-service.md`, `infra/grafana/dashboards/gc-overview.json`; Step 5 13 cluster files (4-cell gauge adjacency at `registered_controllers_metrics_integration.rs`, wiring-only annotation at `meeting_join_metrics_integration.rs:132-146`, JWT fixtures via `#[path = "common/mod.rs"]` from 3 in-place tests at `tests/common/jwt_fixtures.rs`) → `crates/gc-service/tests/*_metrics_integration.rs`

## Code Locations — MC Service
- Error type (McError, bounded labels, From<JwtError>, MhAssignmentMissing) → `crates/mc-service/src/errors.rs`
- Auth: JWT validator + token type enforcement → `crates/mc-service/src/auth/mod.rs:McJwtValidator`; interceptor → `grpc/auth_interceptor.rs:McAuthInterceptor`; auth layer (async JWKS, no scope — deferred to handlers) → `grpc/auth_interceptor.rs:McAuthLayer`
- MH gRPC client (Channel-per-call, RegisterMeeting RPC) → `grpc/mh_client.rs:MhClient`; trait → `mh_client.rs:MhRegistrationClient`
- MediaCoordinationService (MH→MC notifications, R-15) → `grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry (participant→MH tracking, RwLock) → `mh_connection_registry.rs:MhConnectionRegistry`
- Config (ac_jwks_url, advertise addresses, ordinal parsing) → `crates/mc-service/src/config.rs:Config`, `parse_statefulset_ordinal()`
- Startup wiring (JwksClient, McJwtValidator, McAuthLayer, MediaCoordinationService, registry) → `crates/mc-service/src/main.rs`
- Redis (MhAssignmentData, MhAssignmentStore trait, FencedRedisClient) → `crates/mc-service/src/redis/client.rs`
- WebTransport: server (accept loop, redis+mh_client injection) → `webtransport/server.rs:WebTransportServer::accept_loop()`; join flow → `connection.rs:handle_connection()`, `build_join_response()`; async RegisterMeeting trigger (first participant, retry+backoff) → `connection.rs:register_meeting_with_handlers()`; post-join (MediaConnectionFailed R-20) → `connection.rs:handle_client_message()`
- MC metrics (join, WebTransport, JWT, register_meeting, MH notifications, media failures, init) → `crates/mc-service/src/observability/metrics.rs`; catalog → `docs/observability/metrics/mc-service.md`; dashboard + alerts → `infra/grafana/dashboards/mc-overview.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`
- Integration tests + accept-loop rig (real `bind()+accept_loop()`, `rcgen`+`tempfile` PEMs, byte-identical to `main.rs:376-388`, `current_thread` flavor load-bearing for `MetricAssertion`+`tokio::spawn` capture) → `tests/{actor_metrics,auth_layer,gc,heartbeat_tasks,join,media_coordination,orphan_metrics,redis_metrics,register_meeting,token_refresh,webtransport_accept_loop}*.rs`, `tests/common/{mod,accept_loop_rig}.rs`; Cat B token-refresh extraction + matrix harness → `observability/metrics.rs:record_token_refresh_metrics()`
- Health probes + K8s (8081, per-pod NodePort) → `observability/health.rs:health_router()`, `infra/services/mc-service/`

## Code Locations — MH Service
- Config (ac_jwks_url, max_connections, register_meeting_timeout) → `config.rs:Config`
- Error type (thiserror, bounded labels) → `errors.rs:MhError`
- Auth: JWT validator → `auth/mod.rs:MhJwtValidator`; interceptor → `grpc/auth_interceptor.rs:MhAuthInterceptor`; auth layer (async JWKS, scope `service.write.mh`) → `grpc/auth_interceptor.rs:MhAuthLayer`
- GC client → `grpc/gc_client.rs:GcClient`; MC client (MH→MC notify, per-call channel, retry) → `grpc/mc_client.rs:McClient`
- gRPC stub service (MC→MH: RegisterMeeting) → `grpc/mh_service.rs:MhMediaService`; Session manager → `session/mod.rs:SessionManager`
- WebTransport: server → `webtransport/server.rs:WebTransportServer`; connection (JWT, provisional, MC notify) → `webtransport/connection.rs:handle_connection()`; provisional-accept select extracted to `await_meeting_registration()` returning `RegistrationOutcome { Registered, Timeout, Cancelled }` (timeout-arm-only metric fire — see behavioral tests at `:#[cfg(test)] mod tests`)
- Startup wiring → `main.rs`; Metrics → `observability/metrics.rs`; catalog → `docs/observability/metrics/mh-service.md`
- Integration tests → `tests/{gc,mc_client,auth_layer,register_meeting,webtransport,webtransport_accept_loop,token_refresh}_integration.rs`; shared rigs → `tests/common/{grpc_rig,jwks_rig,mock_mc,accept_loop_rig,wt_client,tokens}.rs`
- Cat B metric extraction (stateless pure fn next to sibling `record_*` wrappers) → `observability/metrics.rs:record_token_refresh_metrics()`; matrix test harness (success + every `error_category` variant) at same file `mod tests`
- Accept-loop component rig (real `WebTransportServer::bind()+accept_loop()`, runtime `rcgen` PEMs → `tempfile::TempDir`, byte-identical to `main.rs:258-260`) → `tests/common/accept_loop_rig.rs:AcceptLoopRig`; `current_thread` runtime is load-bearing for `MetricAssertion` + `tokio::spawn` capture (see file-header comment)
- Dual-signal invariant test pattern (metric-label + session-manager state catches call-site refactor) → `tests/webtransport_integration.rs:wrong_token_type_guest_rejected_on_wt_accept_path` and its inline comment
- Health + K8s → `observability/health.rs`, `infra/services/mh-service/`, `infra/docker/mh-service/Dockerfile`

## Code Locations — Common
- JWT (errors, claims, validator, JWKS, HasIat) → `crates/common/src/jwt.rs`; SecretString/SecretBox → `secret.rs`; TokenManager → `token_manager.rs:spawn_token_manager()`; Meeting token shared types (GC↔AC contract, ADR-0020) → `meeting_token.rs`
- `MetricAssertion` (ADR-0032; thread-local `DebuggingRecorder` per snapshot, `!Send`) → `crates/common/src/observability/testing.rs`; `assert_unobserved` on all three query types (counter hard-vs-soft form, gauge gap-fill for §F4, histogram with drain-on-read caveat — call BEFORE any `assert_observation_count*` on same name+labels) at `:CounterQuery::assert_unobserved`, `GaugeQuery::assert_unobserved`, `HistogramQuery::assert_unobserved`; kind-mismatch hardening (`ensure_no_kind_mismatch`) covers negative-assertion path; histogram drain-on-read proof-of-trap test → `:histogram_assert_unobserved_after_assert_observation_count_falsely_passes`; gated behind `common` `test-utils` feature (consumer Cargo.toml needs `common = { path = "../common", features = ["test-utils"] }` in `[dev-dependencies]`)

## Infrastructure & Guards
- Standard health endpoints (`/health`, `/ready`) → ADR-0012 (Section: Standard Operational Endpoints)
- MH QUIC story runbooks (R-33 env-tests, R-34 incident scenarios, R-36 post-deploy) → `crates/env-tests/tests/26_mh_quic.rs`; `docs/runbooks/mh-incident-response.md` (Sc 13/14), `docs/runbooks/mc-incident-response.md` (Sc 11/12/13), `docs/runbooks/mh-deployment.md` Post-Deploy Monitoring Checklist
- MC+MH TLS cert generation → `scripts/generate-dev-certs.sh`
- Env-tests cluster module → `crates/env-tests/src/cluster.rs`
- Kind cluster (ADR-0030): `kind-config.yaml.tmpl`, `setup.sh` (`deploy_only_service()`, `DT_HOST_GATEWAY_IP`), `{mc,mh}-{0,1}-configmap.yaml`
- Devloop helper → `crates/devloop-helper/src/commands.rs`; client → `infra/devloop/dev-cluster`; Service bases → `infra/services/*/kustomization.yaml`
- Guards: runner → `scripts/guards/run-guards.sh`; Kustomize (R-15–R-20) → `validate-kustomize.sh`; App metrics → `validate-application-metrics.sh`; Alert rules (ADR-0031) → `validate-alert-rules.sh`, conventions → `docs/observability/alert-conventions.md`
- Review heuristics: before drafting Option-1/Option-2 framings on a scope-fidelity finding, `git rev-parse HEAD` + spot-grep against the named file:line — checkout-skew presents identically to silent partial migration but resolves to no-op (GC Step 5 F1(d) precedent). Genuine partial-migration recovery pattern (AC iter-2): bundle gaps with file:line, complete-or-name-the-friction triage, re-load each gap in batched cleanup. Source-of-truth disagreement (handler comment vs catalog) IS the bug. `assert_value(0.0)` (zero-fill) vs `assert_unobserved` (untouched) distinction load-bearing for gauges. Wiring-only cells get per-cell annotation not a const split.
