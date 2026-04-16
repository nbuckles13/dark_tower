# Semantic Guard Navigation

## Architecture & Design
- Guard methodology & principles → ADR-0015 (`docs/decisions/adr-0015-principles-guards-methodology.md`)
- Agent Teams validation pipeline → ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)
- Semantic check definitions → `scripts/guards/semantic/checks.md` | Utils → `scripts/guards/common.sh`
- Validation Layer 8 (env-tests integration) → `.claude/skills/devloop/SKILL.md` ("Layer 8" section)

## Metrics Catalogs (Label Validation)
- AC → `docs/observability/metrics/ac-service.md` | GC → `docs/observability/metrics/gc-service.md` | MC → `docs/observability/metrics/mc-service.md` | MH → `docs/observability/metrics/mh-service.md`

## Cross-Service Boundary Files
- Common JWT → `crates/common/src/jwt.rs` | Token refresh → `crates/common/src/token_manager.rs`
- Error types & JwtError mapping → `crates/gc-service/src/errors.rs`, `crates/mc-service/src/errors.rs`
- MH→MC McClient (connect/disconnect RPCs, retry, auth) → `crates/mh-service/src/grpc/mc_client.rs`
- MC notification wiring → `crates/mh-service/src/webtransport/connection.rs:spawn_notify_connected()` | Metrics → `metrics.rs:record_mc_notification()`
- MC notification integration tests → `crates/mh-service/tests/mc_client_integration.rs`

## Authentication Seams
- GC JWT validation → `crates/gc-service/src/auth/jwt.rs` | JWKS → `auth/jwks.rs` | Middleware → `middleware/auth.rs`
- MC two-layer gRPC auth (ADR-0003) → `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer` (Layer 1: scope `service.write.mc`, Layer 2: `service_type` + URI-path routing, claims injection into extensions)
- MC JWT validation (McJwtValidator) → `crates/mc-service/src/auth/mod.rs` (meeting + guest token methods)
- MC JWKS config → `crates/mc-service/src/config.rs:ac_jwks_url`
- MC WebTransport JWT check (pre-actor) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MH gRPC auth interceptor → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor`
- MH JWKS config → `infra/services/mh-service/configmap.yaml:AC_JWKS_URL`
- ADR-0003 scope definitions → `crates/ac-service/src/models/mod.rs:ServiceType::default_scopes()` | Seed SQL → `infra/kind/scripts/setup.sh`
- ADR-0003 scope contract tests (drift prevention) → `crates/ac-service/src/models/mod.rs` (`test_scope_contract_*`)
- NOTE: `McAuthInterceptor` was removed (replaced by `McAuthLayer`). Doc-only references remain in devloop-outputs.

## MC Actor Hierarchy
- Controller → `actors/controller.rs` | Meeting → `actors/meeting.rs` | Participant → `actors/participant.rs`
- Messages → `actors/messages.rs` | Metrics → `actors/metrics.rs` (all under `crates/mc-service/src/`)

## MC WebTransport Layer
- Server (accept loop, TLS, capacity gate) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer`
- Connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- Wire format: 4-byte BE length prefix + protobuf; MAX_MESSAGE_SIZE=64KB, MAX_PARTICIPANT_NAME_LEN=256
- Protobuf encoding utilities → `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`

## GC MH Selection & Assignment
- MH selection (weighted random) → `crates/gc-service/src/services/mh_selection.rs:MhSelectionService` | Types → `MhSelection`
- MC assignment with MH → `crates/gc-service/src/services/mc_assignment.rs:AssignmentWithMh`
- MH selection metrics → `crates/gc-service/src/observability/metrics.rs:record_mh_selection()`

## GC Handlers, Routes & Repositories
- Create/Join/Guest/Settings handlers → `crates/gc-service/src/handlers/meetings.rs`
- Route wiring → `crates/gc-service/src/routes/mod.rs:build_routes()`
- Meetings repo (CTE, activation) → `repositories/meetings.rs` | Participants → `repositories/participants.rs`
- Models → `crates/gc-service/src/models/mod.rs:CreateMeetingRequest`, `Participant`

## GC Join Integration Tests (`crates/gc-service/tests/meeting_tests.rs`)
- Harness: TestMeetingServer, wiremock JWKS+AC, MockMcClient, `#[sqlx::test]`
- MH assignment tests → `tests/mc_assignment_rpc_tests.rs`, `tests/meeting_assignment_tests.rs`

## MC Test Utilities & Join Integration Tests
- TestKeypair + JWKS mock → `crates/mc-test-utils/src/jwt_test.rs`
- Join tests → `crates/mc-service/tests/join_tests.rs` (TestServer, self-signed TLS, wiremock JWKS, real actors)

## Observability
- GC metrics → `crates/gc-service/src/observability/metrics.rs` | MC metrics → `crates/mc-service/src/observability/metrics.rs`
- MC Layer 2 auth metric → `mc_caller_type_rejected_total{grpc_service, expected_type, actual_type}` in `metrics.rs:record_caller_type_rejected()`
- GC dashboard → `infra/grafana/dashboards/gc-overview.json` | MC dashboard → `infra/grafana/dashboards/mc-overview.json`
- GC alerts → `infra/docker/prometheus/rules/gc-alerts.yaml` | MC alerts → `mc-alerts.yaml`
- Alerts doc → `docs/observability/alerts.md` | Dashboards doc → `docs/observability/dashboards.md`

## E2E Env-Tests (`crates/env-tests/`)
- Cluster infra → `src/cluster.rs:ClusterConnection`, `ClusterPorts::from_env()`, `parse_host_port()`
- Auth/GC fixtures → `src/fixtures/auth_client.rs`, `gc_client.rs`
- Join flow E2E → `tests/24_join_flow.rs` (Tier 1: GC-level + Tier 2: MC WebTransport)

## Kustomize, Kind & Network Policies
- Kind overlay → `infra/kubernetes/overlays/kind/` | Setup → ADR-0030, `infra/kind/scripts/setup.sh` | Teardown → `teardown.sh`
- ConfigMap patching (MC/MH advertise) → `setup.sh:deploy_mc_service()`, `deploy_mh_service()` | Helper → `crates/devloop-helper/src/commands.rs`
- Network policies → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml` | MC↔MH gRPC: MC→MH:50053, MH→MC:50052
