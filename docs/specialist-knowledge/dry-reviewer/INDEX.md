# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)
- Cross-Boundary Ownership Model (three-tier classification, GSA, Paired flag) -> ADR-0024 §6
- GSA quartet + cross-boundary guards -> ADR-0024 §6.4; mirrors `.claude/skills/devloop/SKILL.md:116`, `.claude/skills/devloop/review-protocol.md:16`, `scripts/guards/simple/cross-boundary-ownership.yaml` (manifest; anchor-of-truth grep on "Mirror of ADR-0024 §6.4" / "Source of truth for GSA enumeration"); guards `scripts/guards/simple/validate-cross-boundary-scope.sh` + `scripts/guards/simple/validate-cross-boundary-classification.sh` share `scripts/guards/common.sh:parse_cross_boundary_table` (Pattern C precedent)
- Spin-out as third fix-or-defer path + Ownership-lens verdict field -> `.claude/skills/devloop/review-protocol.md:68-94`, `:113-119`

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
- Internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)
- MH JWKS config -> `infra/services/mh-service/configmap.yaml:AC_JWKS_URL` (shared configmap, mirrors MC pattern in `mc-service-config`)

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication); paired MC↔MH notification counters (`mc_mh_notifications_received_total` ↔ `mh_mc_notifications_total`) — different sender/receiver perspectives, not duplication
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc,mh}-alerts.yaml`; guard + conventions -> `scripts/guards/simple/validate-alert-rules.sh`, `docs/observability/alert-conventions.md` (ADR-0031); dashboards -> ADR-0029, `infra/grafana/dashboards/`
- Runbooks (per-service incident-response + deployment) -> `docs/runbooks/`; canonical post-deploy checklist owned in `docs/runbooks/mh-deployment.md`, MC addendum cross-pointers to it (DRY: thresholds owned in one place)

## Integration Test Coverage
- MC tests (join_tests T1-T15, actor_metrics, auth_layer, gc, media_coordination, orphan_metrics, redis_metrics, register_meeting, token_refresh, webtransport_accept_loop) -> `crates/mc-service/tests/`; idempotent MH-retry invariant -> `crates/mc-service/src/grpc/media_coordination.rs:tests::test_coordination_flow_connect_disconnect_round_trip`; GC -> `crates/gc-service/tests/meeting_tests.rs`
- MC shared scaffolding (MockMhAssignmentStore, MockMhRegistrationClient + `wait_for_calls`, TestStackHandles, build_test_stack, seed_meeting_with_mh, create_meeting_with_handlers) -> `crates/mc-service/tests/common/mod.rs`, `crates/mc-service/tests/join_tests.rs`
- MC accept-loop rig (`bind() → accept_loop()` + `write_self_signed_pems`) -> `crates/mc-service/tests/common/accept_loop_rig.rs:AcceptLoopRig` (near-clone of MH's; extraction candidate per ADR-0032 §Step 6 + TODO.md)
- MH tests (gc_integration, mc_client_integration, auth_layer_integration, register_meeting_integration, webtransport_integration, webtransport_accept_loop_integration, token_refresh_integration) -> `crates/mh-service/tests/`
- MH shared rigs (TestKeypair, mock_mc, jwks_rig, grpc_rig, accept_loop_rig, wt_client, tokens) -> `crates/mh-service/tests/common/`
- env-tests (cluster integration) -> `crates/env-tests/tests/`; MH QUIC E2E (R-33) -> `crates/env-tests/tests/26_mh_quic.rs`; join flow -> `:24_join_flow.rs`
- AC tests (13 cluster files) -> `crates/ac-service/tests/*_integration.rs`; in-crate scaffolding -> `crates/ac-service/tests/common/`; GC scaffolding -> `crates/gc-service/tests/common/jwt_fixtures.rs`
- Shared fixtures (cross-service) -> `crates/{ac,gc,mc}-test-utils/src/`; MetricAssertion -> `crates/common/src/observability/testing.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service); ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()`
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern in GC + MC + MH config

## gRPC Auth (Cross-Service)
- MC/MH auth layers (async JWKS, R-22; legacy `*AuthInterceptor` in same files is dead in production) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer`, `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- McAuthLayer/MhAuthLayer near-identical tower Layer/Service (extraction candidate in TODO.md); shared constant `common::jwt::MAX_JWT_SIZE_BYTES`

## MC gRPC Services (GC→MC + MH→MC + MC→MH)
- MC assignment (GC→MC) -> `crates/mc-service/src/grpc/mc_service.rs:McAssignmentService`; media coordination (MH→MC, R-15) -> `crates/mc-service/src/grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry (single MAX_ID_LENGTH source) -> `crates/mc-service/src/mh_connection_registry.rs:MhConnectionRegistry`
- MhRegistrationClient trait + async RegisterMeeting trigger (R-12) -> `crates/mc-service/src/grpc/mh_client.rs`, `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`

## MH Service (R-12..R-36)
- MH WebTransport stack (server, accept loop, connection handler) -> `crates/mh-service/src/webtransport/`; provisional-accept select extracted -> `:connection.rs:await_meeting_registration()`
- MH JWT validator + SessionManager + gRPC clients -> `crates/mh-service/src/auth/mod.rs:MhJwtValidator`, `crates/mh-service/src/session/mod.rs:SessionManager`, `crates/mh-service/src/grpc/`
- MH selection (Vec<MhAssignmentInfo> with grpc_endpoint) -> `crates/gc-service/src/services/mh_selection.rs:MhSelection`

## gRPC Clients (Cross-Service)
- MC GcClient (bounded retries, fast/comprehensive heartbeats) -> `crates/mc-service/src/grpc/gc_client.rs`; MC MhClient (per-call channels, no retries) -> `crates/mc-service/src/grpc/mh_client.rs`
- MH GcClient (unbounded retries, load reports) -> `crates/mh-service/src/grpc/gc_client.rs`; MH McClient (channel-per-call, exp backoff, auth-error short-circuit) -> `crates/mh-service/src/grpc/mc_client.rs`
- Shared `add_auth` (~10 lines, 4 call sites — extraction candidate per TODO.md)

## Redis Abstractions (MC)
- MhAssignmentStore trait -> `crates/mc-service/src/redis/client.rs:MhAssignmentStore` (testability seam for join flow)
- FencedRedisClient -> `crates/mc-service/src/redis/client.rs:FencedRedisClient` (fenced writes, implements MhAssignmentStore)

## Health Endpoints (Cross-Service Consistency)
- MC health -> `crates/mc-service/src/observability/health.rs:health_router()` | MH -> `crates/mh-service/src/observability/health.rs` (duplicated, see TODO.md)
- GC health -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- MC/MH per-pod Services + ConfigMaps -> `infra/services/{mc,mh}-service/` (port: `base + ordinal*2`)
- Network policies -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; Kind -> `infra/kind/`
- setup/teardown (ADR-0030) -> `infra/kind/scripts/{setup,teardown}.sh`; devloop -> `infra/devloop/devloop.sh`

## False Positive Boundaries
- Per-service error mapping (GcError/McError/MhError); MC vs MH GcClient (different RPCs/retry); AC vs GC rate limiting (different mechanisms); `common::jwt` vs `common::meeting_token` (JWT enums narrower)
- AC test-side `Claims { ... }` literal repetition -> false-positive on the literal; surrounding decrypt-and-sign IS extracted to `crates/ac-service/tests/common/jwt_fixtures.rs`. TODO.md tracks the literal-only repetition.

## Tech Debt / Common Crates
- Active cross-service duplication -> `docs/TODO.md`; common crate + per-service test-utils -> `crates/common/src/`, `crates/{ac,gc,mc}-test-utils/`
