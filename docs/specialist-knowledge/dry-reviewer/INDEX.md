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
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`; guard + conventions -> `scripts/guards/simple/validate-alert-rules.sh`, `docs/observability/alert-conventions.md` (ADR-0031); dashboards -> ADR-0029, `infra/grafana/dashboards/`

## Integration Test Coverage
- MC tests (join_tests, actor_metrics, auth_layer, gc, media_coordination, orphan_metrics, redis_metrics, register_meeting, token_refresh, webtransport_accept_loop) -> `crates/mc-service/tests/`; GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MC shared scaffolding (MockMhAssignmentStore, MockMhRegistrationClient, TestStackHandles, build_test_stack, seed_meeting_with_mh) -> `crates/mc-service/tests/common/mod.rs`
- MC accept-loop rig (`bind() → accept_loop()` + `write_self_signed_pems`) -> `crates/mc-service/tests/common/accept_loop_rig.rs:AcceptLoopRig` (near-clone of MH's; extraction candidate per ADR-0032 §Step 6 + TODO.md)
- MH tests (gc_integration, mc_client_integration, auth_layer_integration, register_meeting_integration, webtransport_integration) -> `crates/mh-service/tests/`
- MH shared rigs (TestKeypair, mock_mc, jwks_rig, grpc_rig, accept_loop_rig, wt_client, tokens) -> `crates/mh-service/tests/common/`
- AC tests (Step 4 metric backfill, 13 cluster files: http, bcrypt, token_validation, rate_limit, key_rotation, jwks, credential_ops, errors, token_issuance_service, token_issuance_user, internal_token, audit_log_failures, db) -> `crates/ac-service/tests/*_integration.rs`
- AC + GC in-crate scaffolding -> `crates/ac-service/tests/common/test_state.rs` (`make_app_state`, `seed_signing_key`, `seed_service_credential`), `crates/ac-service/tests/common/jwt_fixtures.rs` (`sign_service_token`, `sign_user_token`; MIN_BCRYPT_COST except `bcrypt_metrics_integration.rs`); `crates/gc-service/tests/common/jwt_fixtures.rs` (`TestKeypair`, `TestUserClaims`, `TestServiceClaims`; Step 5 full 3-of-3 migration; attack helpers as free fns over `&TestKeypair`). TODO.md.
- Shared fixtures (cross-service) -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/` (crypto_fixtures, server_harness, token_builders); MetricAssertion -> `crates/common/src/observability/testing.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service); ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()`
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern in GC + MC + MH config

## gRPC Auth (Cross-Service)
- MC/MH auth layers (async JWKS, R-22; legacy `*AuthInterceptor` in same files is dead in production) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer`, `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- McAuthLayer/MhAuthLayer near-identical tower Layer/Service (extraction candidate in TODO.md); shared constant `common::jwt::MAX_JWT_SIZE_BYTES`

## MC gRPC Services (GC→MC + MH→MC + MC→MH)
- MC assignment (GC→MC) -> `crates/mc-service/src/grpc/mc_service.rs:McAssignmentService`; media coordination (MH→MC, R-15) -> `:grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry (single MAX_ID_LENGTH source) -> `crates/mc-service/src/mh_connection_registry.rs:MhConnectionRegistry`
- MhRegistrationClient trait + async RegisterMeeting trigger (R-12) -> `crates/mc-service/src/grpc/mh_client.rs`, `:webtransport/connection.rs:register_meeting_with_handlers()`

## gRPC Clients (Cross-Service)
- MC GcClient (bounded retries, fast/comprehensive heartbeats) -> `crates/mc-service/src/grpc/gc_client.rs`; MC MhClient (per-call channels, no retries) -> `crates/mc-service/src/grpc/mh_client.rs`
- MH GcClient (unbounded retries, load reports) -> `crates/mh-service/src/grpc/gc_client.rs`
- Shared patterns: channel creation, `add_auth` (~10 lines, 3 call sites — extraction candidate per TODO.md), backoff constants

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
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt enums vs common::meeting_token -> JWT enums intentionally narrower
- AC test-side `Claims { ... }` literal repetition (5 axes of variation) -> false-positive on the literal itself; surrounding decrypt-and-sign boilerplate IS extracted to `tests/common/jwt_fixtures.rs` (`sign_service_token`, `sign_user_token`; 6 call sites). TODO.md tracks the literal-only repetition.

## Abstraction Lessons (DRY judgment calls)
- **Abstract the fixed mechanic, not speculative axes (AC Step 4 iter-2→iter-3, GC Step 5)**: speculative helpers pre-baking varying axes get abandoned (`sign_service_token_iat_offset` 3-of-5 axes); the fix is helpers parameterized only on the truly-fixed mechanic (`sign_service_token(pool, master_key, &Claims)`, `TestKeypair` + free-fn attack helpers). When removing dead helpers, verify the duplication is gone, not just moved inline. Mechanical extraction (incl. receiver-style → free-fn) ≠ renaming/re-shaping types.
- **Per-crate `tests/common/` 3-crate sibling (Steps 3-5)**: AC + MC + GC each own a `tests/common/` with `AppState`/`Config` builders, DB seeding, JWT fixtures; topology divergence keeps them per-service. Cross-crate fixtures (harnesses, crypto, token builders) belong in `*-test-utils`. Workspace-level promotion to `crates/test-utils-common` triggers on 4th caller. Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section). Common crate + test fixtures -> `crates/common/src/`, `crates/mc-test-utils/`, `crates/gc-test-utils/`, `crates/ac-test-utils/`.
