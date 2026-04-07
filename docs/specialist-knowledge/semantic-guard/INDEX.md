# Semantic Guard Navigation

## Architecture & Design
- Guard methodology & principles → ADR-0015 (`docs/decisions/adr-0015-principles-guards-methodology.md`)
- Agent Teams validation pipeline → ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)
- Semantic check definitions → `scripts/guards/semantic/checks.md` | Utils → `scripts/guards/common.sh`

## Metrics Catalogs (Label Validation)
- AC → `docs/observability/metrics/ac-service.md` | GC → `docs/observability/metrics/gc-service.md` | MC → `docs/observability/metrics/mc-service.md`

## Cross-Service Boundary Files
- Common JWT (types, JWKS, validator, errors, HasIat) → `crates/common/src/jwt.rs`
- Token refresh → `crates/common/src/token_manager.rs`
- GC error types & JwtError mapping → `crates/gc-service/src/errors.rs`
- MC error types & JwtError mapping → `crates/mc-service/src/errors.rs`

## Authentication Seams
- GC JWT validation → `crates/gc-service/src/auth/jwt.rs` | JWKS → `auth/jwks.rs` | Middleware → `middleware/auth.rs`
- MC JWT validation (McJwtValidator) → `crates/mc-service/src/auth/mod.rs` (meeting + guest token methods)
- MC JWKS config → `crates/mc-service/src/config.rs:ac_jwks_url`
- MC WebTransport JWT check (pre-actor) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`

## MC Actor Hierarchy
- Controller → `actors/controller.rs` | Meeting → `actors/meeting.rs` | Participant → `actors/participant.rs`
- Messages → `actors/messages.rs` | Metrics → `actors/metrics.rs` (all under `crates/mc-service/src/`)

## MC WebTransport Layer
- Server (accept loop, TLS, capacity gate) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer`
- Connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- Wire format: 4-byte BE length prefix + protobuf; MAX_MESSAGE_SIZE=64KB, MAX_PARTICIPANT_NAME_LEN=256
- Protobuf encoding utilities → `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`

## GC Handlers, Routes & Repositories
- Create/Join/Guest/Settings handlers → `crates/gc-service/src/handlers/meetings.rs`
- Route wiring → `crates/gc-service/src/routes/mod.rs:build_routes()`
- Meetings repo (CTE, activation) → `repositories/meetings.rs` | Participants → `repositories/participants.rs`
- Models → `crates/gc-service/src/models/mod.rs:CreateMeetingRequest`, `Participant`

## GC Join Integration Tests (`crates/gc-service/tests/meeting_tests.rs`)
- Harness: TestMeetingServer (spawn/spawn_with_ac_failure), wiremock JWKS+AC, MockMcClient, `#[sqlx::test]`
- Join success: scheduled+active, host+member, cross-org | Denied: not found, cancelled, ended, cross-org forbidden
- Auth: missing/invalid/expired, service token rejected, HS256 confusion, wrong key, tampered → 401
- Guest: success, not found, forbidden, display_name validation, captcha, concurrency (20 parallel)
- Settings: host allow_guests/external/waiting_room, partial+multi, non-host 403, not found 404

## MC Test Utilities & Join Integration Tests
- TestKeypair (Ed25519 seed, JWK, signing) → `crates/mc-test-utils/src/jwt_test.rs`
- JWKS mock → `jwt_test.rs:mount_jwks_mock()` | Claims → `make_meeting_claims()`, `make_expired_*`, `make_host_*`
- Note: `mc-service/src/auth/mod.rs` `#[cfg(test)]` still has private TestKeypair copy (dedup candidate)
- **Tests** (`crates/mc-service/tests/join_tests.rs`): TestServer (self-signed TLS, wiremock JWKS, real actors)
- Happy path: JoinResponse fields, empty roster | JWT: expired, garbage, wrong meeting_id, wrong key → Unauthorized
- Protocol: invalid protobuf → drop, wrong first msg → InvalidRequest | Validation: name too long → InvalidRequest
- Actor-level: join success, meeting not found, roster visibility | Bridge: ParticipantJoined (timeout-tolerant)

## Observability
- GC metrics → `crates/gc-service/src/observability/metrics.rs` | MC metrics → `crates/mc-service/src/observability/metrics.rs`
- GC dashboard → `infra/grafana/dashboards/gc-overview.json` | MC dashboard → `infra/grafana/dashboards/mc-overview.json`
- GC alerts → `infra/docker/prometheus/rules/gc-alerts.yaml` | MC alerts → `mc-alerts.yaml`
- Alerts doc → `docs/observability/alerts.md` | Dashboards doc → `docs/observability/dashboards.md`

## E2E Env-Tests (`crates/env-tests/`)
- Cluster infra → `src/cluster.rs:ClusterConnection`, `ClusterPorts::from_env()`, `parse_host_port()`
- Auth/GC fixtures → `src/fixtures/auth_client.rs`, `gc_client.rs`
- Join flow E2E → `tests/24_join_flow.rs` (Tier 1: GC-level + Tier 2: MC WebTransport)
- Wire format helpers: `encode_framed` / `read_server_message` (4-byte BE prefix)

## Kustomize & Kind
- Kind overlay → `infra/kubernetes/overlays/kind/` | Setup (DT_CLUSTER_NAME, DT_PORT_MAP, --yes/--only/--skip-build, `load_image_to_kind()`, `deploy_only_service()`) → ADR-0030, `infra/kind/scripts/setup.sh`
- Teardown (DT_CLUSTER_NAME-aware) → `infra/kind/scripts/teardown.sh`
- Observability base + Grafana → `infra/kubernetes/observability/kustomization.yaml`, `grafana/kustomization.yaml`
- Service bases → `infra/services/{ac-service,gc-service,mc-service,postgres,redis}/kustomization.yaml`
- Kustomize CI guard (R-15 builds, R-16 orphans, R-17 kubeconform, R-18 secctx, R-19 secrets, R-20 dashboards) → `scripts/guards/simple/validate-kustomize.sh`

## Runbooks
- GC incident response → `docs/runbooks/gc-incident-response.md`; GC deployment → `docs/runbooks/gc-deployment.md`
